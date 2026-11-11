# Ghidra headless post script: decompile the functions whose demangled names contain any of the
# needles in DECOMPILE_NEEDLES (comma separated, env) into DECOMPILE_OUT, one file per function.
# Output lands under .local/re and never enters the repository; only behavioural notes do.
import io
import os
import re
import struct
from ghidra.app.decompiler import DecompInterface
from ghidra.util.task import ConsoleTaskMonitor

needles = [n for n in os.environ.get("DECOMPILE_NEEDLES", "").split(",") if n]
out = os.environ.get("DECOMPILE_OUT", "/tmp/decompiled")
if not os.path.isdir(out):
    os.makedirs(out)

# float / double constants: the pseudo-C shows them as data refs (DAT_xxx); every ref whose
# bytes read as a sane float or double gets the value appended as a comment (/* =0.5f */)
_mem = currentProgram.getMemory()
_space = currentProgram.getAddressFactory().getDefaultAddressSpace()
_dat = re.compile(r"(?<![A-Za-z0-9])_?DAT_([0-9a-fA-F]+)\b")
_cache = {}


def _sane(v, lo, hi):
    return v == v and abs(v) != float("inf") and (v == 0.0 or lo <= abs(v) <= hi)


def _value_comment(hexaddr):
    if hexaddr in _cache:
        return _cache[hexaddr]
    note = ""
    try:
        addr = _space.getAddress(hexaddr)
        parts = []
        # Memory reads honour the program's byte order, so the integers are re-packed as
        # little-endian IEEE bit patterns
        try:
            f = struct.unpack("<f", struct.pack("<I", _mem.getInt(addr) & 0xFFFFFFFF))[0]
            if _sane(f, 1e-5, 1e7):
                parts.append("%gf" % f)
        except:  # noqa: E722 - Jython: Java MemoryAccessException is not a Python Exception
            pass
        try:
            d = struct.unpack("<d", struct.pack("<Q", _mem.getLong(addr) & 0xFFFFFFFFFFFFFFFF))[0]
            if _sane(d, 1e-5, 1e7):
                parts.append("%g" % d)
        except:  # noqa: E722 - Jython: Java MemoryAccessException is not a Python Exception
            pass
        if parts:
            note = " /* =" + " | ".join(parts) + " */"
    except:  # noqa: E722
        pass
    _cache[hexaddr] = note
    return note


def annotate(c):
    return _dat.sub(lambda m: m.group(0) + _value_comment(m.group(1)), c)

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
    with io.open(os.path.join(out, "%s_%s.c" % (safe[:150], f.getEntryPoint())), "w", encoding="utf-8", errors="replace") as fh:
        fh.write(unicode(annotate(res.getDecompiledFunction().getC())))
    count += 1
print("decompiled %d functions" % count)
