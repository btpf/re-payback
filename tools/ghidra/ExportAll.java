// Headless export: decompiled C for every function, plus defined strings with
// their referencing functions. Output dir is the first script argument.
// @category Payback
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.address.*;
import ghidra.program.model.data.StringDataInstance;
import java.io.*;

public class ExportAll extends GhidraScript {
    @Override
    public void run() throws Exception {
        File out = new File(getScriptArgs()[0]);
        out.mkdirs();
        DecompInterface di = new DecompInterface();
        di.openProgram(currentProgram);
        FunctionManager fm = currentProgram.getFunctionManager();
        try (PrintWriter w = new PrintWriter(new FileWriter(new File(out, "decompiled.c")));
             PrintWriter idx = new PrintWriter(new FileWriter(new File(out, "functions.tsv")))) {
            for (Function f : fm.getFunctions(true)) {
                if (monitor.isCancelled()) break;
                DecompileResults r = di.decompileFunction(f, 300, monitor);
                String c = r.decompileCompleted() ? r.getDecompiledFunction().getC() : "/* decompile failed */\n";
                w.printf("// ==== %s @ %s size=%d\n%s\n", f.getName(), f.getEntryPoint(), f.getBody().getNumAddresses(), c);
                idx.printf("%s\t%s\t%d\n", f.getEntryPoint(), f.getName(), f.getBody().getNumAddresses());
            }
        }
        ReferenceManager rm = currentProgram.getReferenceManager();
        try (PrintWriter w = new PrintWriter(new FileWriter(new File(out, "strings.tsv")))) {
            for (Data d : currentProgram.getListing().getDefinedData(true)) {
                StringDataInstance s = StringDataInstance.getStringDataInstance(d);
                if (s == null || s == StringDataInstance.NULL_INSTANCE) continue;
                String val = s.getStringValue();
                if (val == null) continue;
                StringBuilder refs = new StringBuilder();
                for (Reference ref : rm.getReferencesTo(d.getAddress())) {
                    Function f = fm.getFunctionContaining(ref.getFromAddress());
                    refs.append(f == null ? ref.getFromAddress().toString() : f.getName()).append(',');
                }
                w.printf("%s\t%s\t%s\n", d.getAddress(), refs, val.replace("\n", "\n").replace("\t", "\t"));
            }
        }
    }
}
