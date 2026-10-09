// Pre-script for raw 68k images produced by `tools/extract/hunk.py --flat`.
// Script args: <code_start_hex> <code_end_hex>. Disassembles the entry point
// and every LINK A5 (0x4E55) in the code range and creates functions there,
// since a raw binary gives Ghidra nothing to start from.
// @category Payback
import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.mem.Memory;

public class SeedFlat68k extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] args = getScriptArgs();
        Address start = toAddr(Long.parseLong(args[0], 16));
        Address end = toAddr(Long.parseLong(args[1], 16));
        Memory mem = currentProgram.getMemory();
        int seeds = 0;
        seed(start);
        for (Address a = start; a.compareTo(end) < 0; a = a.add(2)) {
            if (mem.getShort(a) == 0x4E55) {
                seed(a);
                seeds++;
            }
        }
        println("seeded " + seeds + " LINK A5 functions");
    }

    private void seed(Address a) {
        if (getInstructionAt(a) == null) {
            new DisassembleCommand(a, null, true).applyTo(currentProgram, monitor);
        }
        if (getFunctionAt(a) == null) {
            createFunction(a, null);
        }
    }
}
