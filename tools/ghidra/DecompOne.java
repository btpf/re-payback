// Decompile specific functions with a long timeout. Args: outfile addr1 addr2 ...
// @category Payback
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import java.io.*;

public class DecompOne extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] a = getScriptArgs();
        DecompInterface di = new DecompInterface();
        DecompileOptions o = new DecompileOptions();
        o.setMaxPayloadMBytes(512);
        o.setMaxInstructions(500000);
        di.setOptions(o);
        di.openProgram(currentProgram);
        try (PrintWriter w = new PrintWriter(new FileWriter(a[0]))) {
            for (int i = 1; i < a.length; i++) {
                Function f = getFunctionAt(toAddr(a[i]));
                if (f == null) { w.printf("// no function at %s\n", a[i]); continue; }
                DecompileResults r = di.decompileFunction(f, 1800, monitor);
                String c = r.decompileCompleted() ? r.getDecompiledFunction().getC() : "/* decompile failed: " + r.getErrorMessage() + " */\n";
                w.printf("// ==== %s @ %s size=%d\n%s\n", f.getName(), f.getEntryPoint(), f.getBody().getNumAddresses(), c);
                w.flush();
            }
        }
    }
}
