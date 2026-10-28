// Export reproducible function decompilations and cross references for the original PE.
// @category VirtualLife
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;

public class ExportAnalysis extends GhidraScript {
    public void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length != 1) throw new IllegalArgumentException("Output directory required");
        Path root = Paths.get(args[0]);
        Files.createDirectories(root.resolve("functions"));
        DecompInterface decompiler = new DecompInterface();
        decompiler.openProgram(currentProgram);
        int exported = 0;
        try (BufferedWriter index = Files.newBufferedWriter(root.resolve("functions.tsv"), StandardCharsets.UTF_8)) {
            index.write("address\tname\tsize\tstatus\n");
            for (Function function : currentProgram.getFunctionManager().getFunctions(true)) {
                if (monitor.isCancelled()) break;
                if (function.isExternal() || function.isThunk()) continue;
                DecompileResults result = decompiler.decompileFunction(function, 30, monitor);
                String address = function.getEntryPoint().toString();
                String name = function.getName();
                String code = result.decompileCompleted() && result.getDecompiledFunction() != null
                    ? result.getDecompiledFunction().getC() : "/* " + result.getErrorMessage() + " */";
                Files.writeString(root.resolve("functions").resolve(address + ".c"), code, StandardCharsets.UTF_8);
                index.write(address + "\t" + name + "\t" + function.getBody().getNumAddresses()
                    + "\t" + result.decompileCompleted() + "\n");
                if (++exported % 100 == 0) println("Exported " + exported + " functions");
            }
        } finally { decompiler.dispose(); }
        try (BufferedWriter strings = Files.newBufferedWriter(root.resolve("strings.tsv"), StandardCharsets.UTF_8)) {
            strings.write("address\tvalue\treferences\n");
            for (Data data : currentProgram.getListing().getDefinedData(true)) {
                if (!data.hasStringValue()) continue;
                String value = String.valueOf(data.getValue()).replace("\t", " ").replace("\r", "\\r").replace("\n", "\\n");
                StringBuilder refs = new StringBuilder();
                for (Reference ref : getReferencesTo(data.getAddress())) {
                    Function owner = getFunctionContaining(ref.getFromAddress());
                    if (refs.length() > 0) refs.append(',');
                    refs.append(ref.getFromAddress()).append(':').append(owner == null ? "?" : owner.getEntryPoint());
                }
                strings.write(data.getAddress() + "\t" + value + "\t" + refs + "\n");
            }
        }
        println("Exported " + exported + " functions to " + root);
    }
}
