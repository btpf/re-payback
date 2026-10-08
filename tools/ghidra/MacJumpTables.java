// Recover CodeWarrior PowerPC (PEF/CFM) switch jump tables that Ghidra leaves
// unresolved ("Could not recover jumptable ... Too many branches"), so the case
// bodies become part of their functions and decompile. Run it as a postScript
// before ExportAll.java on the Mac builds (`Payback (OS X)`, `Payback (OS 9)`).
//
// CodeWarrior reaches a switch table through the TOC (r2), in one of two forms:
//
//     cmplwi  rI, N-1            ; bound, followed by bgt default
//     lwz     rB, d(r2)          ; table pointer stored in the TOC   (form 1)
//     addi    rB, r2, d          ; table inline in the TOC/data area (form 2)
//     slwi    rX, rI, 2
//     lwzx    rT, rB, rX         ; absolute case address
//     [add    rT, rT, rB]        ; (relative tables; not seen in Payback)
//     mtctr   rT
//     bctr
//
// The script scans every executable block for `bctr`, decodes that sequence
// from the raw words (no dependence on Ghidra's mnemonic spelling), reads r2
// from the program context (the PEF loader sets it) or from the optional
// `r2=0x...` argument, reads the N absolute targets, and then does what
// Ghidra's own SwitchOverride.java does: disassembles the targets, adds
// COMPUTED_JUMP references, writes a JumpTable override, and recomputes the
// function body. Tables nested inside case bodies only belong to a function
// once their parent switch is fixed, so it runs in passes until nothing changes,
// re-running auto-analysis in between so callees reached only from case bodies
// become functions too.
//
// Arguments (all optional): `r2=0x10183440` (TOC value), `tsv=<path>` (write a
// table list: bctr, function, form, count, targets).
// @category Payback
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.lang.Register;
import ghidra.program.model.listing.*;
import ghidra.program.model.mem.*;
import ghidra.program.model.pcode.JumpTable;
import ghidra.program.model.symbol.*;
import java.io.*;
import java.math.BigInteger;
import java.util.*;

public class MacJumpTables extends GhidraScript {
    static final int BCTR = 0x4E800420;
    static final int WINDOW = 40; // instructions to look back from bctr

    static class Table {
        Address bctr;
        Address base;
        String form;
        List<Address> targets = new ArrayList<>();
    }

    Long r2Arg = null;
    String tsvPath = null;

    @Override
    public void run() throws Exception {
        for (String a : getScriptArgs()) {
            if (a.startsWith("r2=")) r2Arg = Long.decode(a.substring(3));
            else if (a.startsWith("tsv=")) tsvPath = a.substring(4);
        }
        Memory mem = currentProgram.getMemory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();
        Listing listing = currentProgram.getListing();

        // 1. Find all candidate tables in the raw code.
        List<Table> tables = new ArrayList<>();
        int skipped = scan(tables);
        println(String.format("MacJumpTables: %d tables found (%d other bctr skipped: glue, save/restore, calls)",
            tables.size(), skipped));

        // 2. Apply overrides in passes.
        Set<Address> done = new HashSet<>();
        Map<Address, String> owner = new HashMap<>();
        for (int pass = 0; pass < 12 && !monitor.isCancelled(); pass++) {
            int n = 0;
            for (Table t : tables) {
                if (done.contains(t.bctr)) continue;
                Instruction bi = listing.getInstructionAt(t.bctr);
                Function f = fm.getFunctionContaining(t.bctr);
                if (bi == null || f == null) continue;
                ArrayList<Address> dl = new ArrayList<>();
                for (Address tg : t.targets) {
                    Function g = fm.getFunctionAt(tg);
                    if (g != null && !g.equals(f)) fm.removeFunction(tg);
                    if (listing.getInstructionAt(tg) == null) disassemble(tg);
                    rm.addMemoryReference(t.bctr, tg, RefType.COMPUTED_JUMP, SourceType.USER_DEFINED, 0);
                    if (!dl.contains(tg)) dl.add(tg);
                }
                if (bi.getFlowOverride() != FlowOverride.NONE) bi.setFlowOverride(FlowOverride.NONE);
                new JumpTable(t.bctr, dl, true, 0).writeOverride(f);
                fixBody(f);
                done.add(t.bctr);
                owner.put(t.bctr, f.getName());
                n++;
            }
            println(String.format("MacJumpTables: pass %d applied %d", pass, n));
            if (n == 0) break;
            analyzeChanges(currentProgram);
            // Analysis can create functions at case targets again; fold them back.
            for (Table t : tables) {
                if (!done.contains(t.bctr)) continue;
                Function f = fm.getFunctionContaining(t.bctr);
                if (f == null) continue;
                boolean changed = false;
                for (Address tg : t.targets) {
                    Function g = fm.getFunctionAt(tg);
                    if (g != null && !g.equals(f)) { fm.removeFunction(tg); changed = true; }
                }
                if (changed || !f.getBody().contains(t.targets.get(0))) fixBody(f);
            }
        }
        int missing = 0;
        for (Table t : tables) if (!done.contains(t.bctr)) {
            println("MacJumpTables: NOT APPLIED (no function) " + t.bctr);
            missing++;
        }
        println(String.format("MacJumpTables: %d applied, %d not applied", done.size(), missing));

        if (tsvPath != null) {
            try (PrintWriter w = new PrintWriter(new FileWriter(tsvPath))) {
                for (Table t : tables) {
                    StringBuilder sb = new StringBuilder();
                    for (Address tg : t.targets) sb.append(tg).append(' ');
                    Function f = fm.getFunctionContaining(t.bctr);
                    w.printf("%s\t%s\t%s\t%s\t%d\t%s\n", t.bctr, f == null ? "-" : f.getName(), t.form, t.base,
                        t.targets.size(), sb.toString().trim());
                }
            }
        }
    }

    int scan(List<Table> tables) {
        Memory mem = currentProgram.getMemory();
        int skipped = 0;
        for (MemoryBlock b : mem.getBlocks()) {
            if (!b.isExecute() || !b.isInitialized()) continue;
            Address s = b.getStart();
            long len = b.getSize();
            for (long off = 0; off + 4 <= len; off += 4) {
                Address a = s.add(off);
                int w;
                try { w = mem.getInt(a); } catch (MemoryAccessException e) { continue; }
                if (w != BCTR) continue;
                Table t = decode(a);
                if (t == null) { skipped++; continue; }
                tables.add(t);
            }
        }
        return skipped;
    }

    /** Recompute f's body from its flow; remove other functions that start inside it. */
    void fixBody(Function f) throws Exception {
        FunctionManager fm = currentProgram.getFunctionManager();
        AddressSetView body = CreateFunctionCmd.getFunctionBody(currentProgram, f.getEntryPoint(), monitor);
        if (body == null || body.isEmpty()) return;
        List<Address> clash = new ArrayList<>();
        for (Iterator<Function> it = fm.getFunctionsOverlapping(body); it.hasNext();) {
            Function g = it.next();
            if (g.equals(f)) continue;
            if (body.contains(g.getEntryPoint())) clash.add(g.getEntryPoint());
        }
        for (Address a : clash) fm.removeFunction(a);
        try {
            f.setBody(body);
        } catch (Exception e) {
            // Some other function's body overlaps without starting inside ours.
            CreateFunctionCmd.fixupFunctionBody(currentProgram, f, monitor);
        }
    }

    // ---- raw PowerPC decoding -------------------------------------------

    int word(Address a) {
        try { return currentProgram.getMemory().getInt(a); } catch (Exception e) { return 0; }
    }

    static int op(int w) { return (w >>> 26) & 0x3f; }
    static int rD(int w) { return (w >>> 21) & 31; }
    static int rA(int w) { return (w >>> 16) & 31; }
    static int rB(int w) { return (w >>> 11) & 31; }
    static int xo(int w) { return (w >>> 1) & 0x3ff; }
    static boolean isMtctr(int w) { return (w & 0xFC1FFFFF) == 0x7C0903A6; }
    static boolean isLwzx(int w) { return op(w) == 31 && xo(w) == 23; }
    static boolean isAdd(int w) { return op(w) == 31 && (xo(w) & 0x1ff) == 266; }
    /** Instruction writes GPR r (approximation good enough for these short sequences). */
    static boolean writes(int w, int r) {
        int o = op(w);
        if (o == 10 || o == 11 || o == 16 || o == 18 || o == 19 || o == 36 || o == 37 || o == 38 || o == 39
            || o == 44 || o == 45 || o == 52 || o == 53 || o == 54 || o == 55 || o == 47) return false; // cmp, branch, stores
        if (o == 24 || o == 25 || o == 26 || o == 27 || o == 28 || o == 29 || o == 21 || o == 20 || o == 23)
            return rA(w) == r; // ori/oris/xori/xoris/andi./andis./rlwinm/rlwimi/rlwnm write rA
        if (o == 31) {
            int x = xo(w);
            if (x == 0 || x == 32 || x == 151 || x == 215 || x == 407 || x == 150 || x == 662 || x == 918
                || x == 183 || x == 247 || x == 439 || x == 247 || x == 467 || x == 86 || x == 54 || x == 278 || x == 470)
                return false; // cmp/cmpl/stores/mtspr/cache ops
            if (x == 28 || x == 60 || x == 124 || x == 284 || x == 316 || x == 412 || x == 444 || x == 476
                || x == 24 || x == 536 || x == 792 || x == 824 || x == 954 || x == 922 || x == 26)
                return rA(w) == r; // logical / shifts / extsX / cntlzw write rA
            return rD(w) == r;
        }
        return rD(w) == r;
    }

    Table decode(Address bctr) {
        Address[] win = new Address[WINDOW];
        int[] w = new int[WINDOW];
        int n = 0;
        Address a = bctr;
        for (int i = 0; i < WINDOW; i++) {
            try { a = a.subtract(4); } catch (Exception e) { break; }
            int x = word(a);
            if (x == 0x4E800020 || x == BCTR) break; // blr/bctr: previous block
            win[n] = a; w[n] = x; n++;
        }
        // mtctr rT
        int k = 0;
        if (n == 0 || !isMtctr(w[0])) return null;
        int rT = rD(w[0]);
        k = 1;
        // optional add rT, rT, rBase (relative table)
        boolean relative = false;
        int lw = -1;
        for (; k < n; k++) {
            if (isLwzx(w[k]) && rD(w[k]) == rT) { lw = k; break; }
            if (isAdd(w[k]) && rD(w[k]) == rT && !relative) { relative = true; continue; }
            if (writes(w[k], rT)) return null;
        }
        if (lw < 0) return null;
        // lwzx rT, rB, rX: either operand may be the base
        int ra = rA(w[lw]), rb = rB(w[lw]);
        Long base = null; int idx = -1; String form = null;
        for (int pick = 0; pick < 2 && base == null; pick++) {
            int baseReg = pick == 0 ? ra : rb, idxReg = pick == 0 ? rb : ra;
            Long bv = null; String fm = null;
            for (int j = lw + 1; j < n; j++) {
                if (!writes(w[j], baseReg)) continue;
                int o = op(w[j]);
                if (o == 32 && rA(w[j]) == 2) { // lwz rB, d(r2)
                    Long r2 = r2At(win[j]);
                    if (r2 == null) return null;
                    long slot = (r2 + (short) (w[j] & 0xffff)) & 0xffffffffL;
                    bv = readPtr(slot); fm = "lwz";
                } else if (o == 14 && rA(w[j]) == 2) { // addi rB, r2, d
                    Long r2 = r2At(win[j]);
                    if (r2 == null) return null;
                    bv = (r2 + (short) (w[j] & 0xffff)) & 0xffffffffL; fm = "addi";
                }
                break;
            }
            if (bv != null) { base = bv; idx = idxReg; form = fm; }
        }
        if (base == null) return null;
        // index: slwi rX, rI, 2 (rlwinm rX,rI,2,0,29)
        int src = -1, sl = -1;
        for (int j = lw + 1; j < n; j++) {
            if (!writes(w[j], idx)) continue;
            int x = w[j];
            if (op(x) == 21 && rA(x) == idx && rB(x) == 2 && ((x >>> 6) & 31) == 0 && ((x >>> 1) & 31) == 29) {
                src = rD(x); sl = j;
            }
            break;
        }
        if (src < 0) return null;
        // bound: cmplwi/cmpwi on the source register
        int count = -1;
        for (int j = sl + 1; j < n; j++) {
            int x = w[j];
            if ((op(x) == 10 || op(x) == 11) && rA(x) == src) {
                int imm = op(x) == 10 ? (x & 0xffff) : (short) (x & 0xffff);
                count = imm + 1;
                break;
            }
            if (writes(x, src)) {
                // index adjusted between compare and use (e.g. addi rI,rI,-0x80 before cmplwi): keep looking
                if (op(x) == 14 && rA(x) == src && rD(x) == src) continue;
                break;
            }
        }
        if (count <= 0 || count > 1024) return null;
        Table t = new Table();
        t.bctr = bctr;
        t.form = form + (relative ? "+rel" : "");
        t.base = toAddr(base);
        AddressSpace sp = bctr.getAddressSpace();
        for (int e = 0; e < count; e++) {
            Long v = readPtr(base + 4L * e);
            if (v == null) return null;
            long tv = relative ? (v + base) & 0xffffffffL : v;
            Address ta = sp.getAddress(tv);
            MemoryBlock mb = currentProgram.getMemory().getBlock(ta);
            if (mb == null || !mb.isExecute() || (tv & 3) != 0) return null;
            t.targets.add(ta);
        }
        return t;
    }

    Long readPtr(long addr) {
        try {
            return currentProgram.getMemory().getInt(toAddr(addr)) & 0xffffffffL;
        } catch (Exception e) {
            return null;
        }
    }

    Long r2At(Address a) {
        if (r2Arg != null) return r2Arg;
        Register r2 = currentProgram.getRegister("r2");
        if (r2 == null) return null;
        BigInteger v = currentProgram.getProgramContext().getValue(r2, a, false);
        if (v == null) {
            Function f = currentProgram.getFunctionManager().getFunctionContaining(a);
            if (f != null) v = currentProgram.getProgramContext().getValue(r2, f.getEntryPoint(), false);
        }
        if (v != null) return v.longValue() & 0xffffffffL;
        return defaultR2();
    }

    Long r2Default = null;
    boolean r2Searched = false;

    /** Code outside any function (case bodies not yet claimed) has no r2 context; the
     *  game is one PEF fragment with one TOC, so use the value most functions carry. */
    Long defaultR2() {
        if (r2Searched) return r2Default;
        r2Searched = true;
        Register r2 = currentProgram.getRegister("r2");
        Map<Long, Integer> votes = new HashMap<>();
        for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
            BigInteger v = currentProgram.getProgramContext().getValue(r2, f.getEntryPoint(), false);
            if (v != null) votes.merge(v.longValue() & 0xffffffffL, 1, Integer::sum);
        }
        int best = 0;
        for (Map.Entry<Long, Integer> e : votes.entrySet())
            if (e.getValue() > best) { best = e.getValue(); r2Default = e.getKey(); }
        println(String.format("MacJumpTables: default r2 = %s (%d functions)",
            r2Default == null ? "none" : Long.toHexString(r2Default), best));
        return r2Default;
    }
}
