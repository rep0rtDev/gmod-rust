# Ghidra headless post script: decompile the functions whose demangled names contain any of the
# needles in DECOMPILE_NEEDLES (comma separated, env) into DECOMPILE_OUT, one file per function.
# Output lands under .local/re and never enters the repository; only behavioural notes do.
import os
from ghidra.app.decompiler import DecompInterface
from ghidra.util.task import ConsoleTaskMonitor

needles = [n for n in os.environ.get("DECOMPILE_NEEDLES", "").split(",") if n]
out = os.environ.get("DECOMPILE_OUT", "/tmp/decompiled")
if not os.path.isdir(out):
    os.makedirs(out)
iface = DecompInterface()
iface.openProgram(currentProgram)
monitor = ConsoleTaskMonitor()
count = 0
for f in currentProgram.getFunctionManager().getFunctions(True):
    name = f.getName(True)
    if not any(n in name for n in needles):
        continue
    res = iface.decompileFunction(f, 120, monitor)
    if not res.decompileCompleted():
        print("failed: " + name)
        continue
    safe = "".join(c if c.isalnum() or c in "_:" else "_" for c in name).replace("::", "__")
    with open(os.path.join(out, safe + ".c"), "w") as fh:
        fh.write(res.getDecompiledFunction().getC())
    count += 1
print("decompiled %d functions" % count)
