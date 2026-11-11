# Ghidra headless post script for stripped MSVC binaries (client.dll, engine.dll). Selects:
#  1. functions referencing a defined string matching DECOMPILE_PATTERNS (comma separated regexes),
#  2. every virtual function of classes whose RTTI name contains a needle from DECOMPILE_CLASSES
#     (comma separated; "CWeaponPhysCannon" also matches "C_WeaponPhysCannon"),
#  3. direct callees of 2 (one level) and direct callers of 1,
# and decompiles them into DECOMPILE_OUT, one file per function, first line = why it was selected.
import io
import os
import re
import struct
from ghidra.app.decompiler import DecompInterface
from ghidra.util.task import ConsoleTaskMonitor

patterns = [re.compile(p) for p in os.environ.get("DECOMPILE_PATTERNS", "").split(",") if p]
classes = [c for c in os.environ.get("DECOMPILE_CLASSES", "").split(",") if c]
out = os.environ.get("DECOMPILE_OUT", "/tmp/decompiled")
limit = int(os.environ.get("DECOMPILE_LIMIT", "2500"))
if not os.path.isdir(out):
    os.makedirs(out)
listing = currentProgram.getListing()
fm = currentProgram.getFunctionManager()
refman = currentProgram.getReferenceManager()
symtab = currentProgram.getSymbolTable()
mem = currentProgram.getMemory()
ptr = currentProgram.getDefaultPointerSize()
selected = {}


def add(f, why):
    selected.setdefault(f.getEntryPoint(), (f, set()))[1].add(why)


def class_matches(name):
    plain = name.replace("C_", "C", 1) if name.startswith("C_") else name
    return any(c in name or c in plain for c in classes)


# 1. string references
if patterns:
    for data in listing.getDefinedData(True):
        if not data.hasStringValue():
            continue
        s = str(data.getValue())
        if not any(p.search(s) for p in patterns):
            continue
        for ref in refman.getReferencesTo(data.getAddress()):
            f = fm.getFunctionContaining(ref.getFromAddress())
            if f is not None:
                add(f, "string: " + s[:80])

# 2. RTTI vtables
vt_classes = 0
if classes:
    for sym in symtab.getAllSymbols(True):
        if "vftable" not in sym.getName():
            continue
        ns = sym.getParentNamespace()
        cname = ns.getName(True) if ns is not None else ""
        if not class_matches(cname):
            continue
        vt_classes += 1
        addr = sym.getAddress()
        slot = 0
        while slot < 400:
            try:
                target = mem.getInt(addr) & 0xffffffff if ptr == 4 else mem.getLong(addr)
            except Exception:
                break
            taddr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(target)
            f = fm.getFunctionAt(taddr)
            if f is None:
                break
            add(f, "vtable %s slot %d" % (cname, slot))
            addr = addr.add(ptr)
            slot += 1

# 3. neighbours
base = list(selected.items())
for entry, (f, whys) in base:
    if any(w.startswith("vtable") for w in whys):
        for ref in refman.getReferencesFrom(entry):
            pass
        body = f.getBody()
        for ref in refman.getReferenceIterator(body.getMinAddress()):
            if ref.getFromAddress().compareTo(body.getMaxAddress()) > 0:
                break
            if ref.getReferenceType().isCall() and body.contains(ref.getFromAddress()):
                c = fm.getFunctionAt(ref.getToAddress())
                if c is not None and c.getEntryPoint() not in selected:
                    selected.setdefault(c.getEntryPoint(), (c, set()))[1].add("callee of " + f.getName())
    else:
        for ref in refman.getReferencesTo(entry):
            if ref.getReferenceType().isCall():
                c = fm.getFunctionContaining(ref.getFromAddress())
                if c is not None and c.getEntryPoint() not in selected:
                    selected.setdefault(c.getEntryPoint(), (c, set()))[1].add("caller of " + f.getName())


# float / double constants: the pseudo-C shows them as data refs (DAT_xxx); every ref whose
# bytes read as a sane float or double gets the value appended as a comment (/* =0.5f */)
_mem = mem
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
        except Exception:
            pass
        try:
            d = struct.unpack("<d", struct.pack("<Q", _mem.getLong(addr) & 0xFFFFFFFFFFFFFFFF))[0]
            if _sane(d, 1e-5, 1e7):
                parts.append("%g" % d)
        except Exception:
            pass
        if parts:
            note = " /* =" + " | ".join(parts) + " */"
    except Exception:
        pass
    _cache[hexaddr] = note
    return note


def annotate(c):
    return _dat.sub(lambda m: m.group(0) + _value_comment(m.group(1)), c)

iface = DecompInterface()
iface.openProgram(currentProgram)
monitor = ConsoleTaskMonitor()
count = 0
for entry, (f, whys) in sorted(selected.items(), key=lambda kv: str(kv[0])):
    if count >= limit:
        print("limit reached")
        break
    res = iface.decompileFunction(f, 90, monitor)
    if not res.decompileCompleted():
        print("failed: " + f.getName())
        continue
    with io.open(os.path.join(out, "%s_%s.c" % (f.getName(), entry)), "w", encoding="utf-8", errors="replace") as fh:
        fh.write(u"// selected by: " + u" | ".join(sorted(whys)[:12]) + u"\n")
        fh.write(unicode(annotate(res.getDecompiledFunction().getC())))
    count += 1
print("decompiled %d functions (%d selected, %d vtable classes)" % (count, len(selected), vt_classes))
