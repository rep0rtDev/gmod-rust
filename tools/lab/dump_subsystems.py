#!/usr/bin/env python3
"""Cut the analysed Ghidra projects into per-subsystem pseudo-C tarballs under .local/re/dump.

server_srv.so (symbols) is cut by demangled-name needles with decompile.py; the stripped Windows
binaries (client.dll, engine.dll) by referenced-string patterns with decompile_strings.py.
Usage: python3 tools/lab/dump_subsystems.py [subsystem ...]
"""
import json
import os
import subprocess
import sys
import tarfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
GHIDRA = os.path.expanduser("~/tools/ghidra_11.1.2_PUBLIC/support/analyzeHeadless")
PROJECTS = os.path.join(ROOT, ".local/re/ghidra")
DUMP = os.path.join(ROOT, ".local/re/dump")
LAB = os.path.join(ROOT, "tools/lab")
BINARIES = {
    # label: (project, program name in the project, script, env var)
    "server": ("server", "server_srv.so", "decompile.py", "DECOMPILE_NEEDLES"),
    "client": ("client", "client.dll", "decompile_strings.py", "DECOMPILE_PATTERNS"),
    "engine": ("engine", "engine.dll", "decompile_strings.py", "DECOMPILE_PATTERNS"),
    "vphysics": ("vphysics", "vphysics_srv.so", "decompile.py", "DECOMPILE_NEEDLES"),
    "luashared": ("luashared", "lua_shared_srv.so", "decompile.py", "DECOMPILE_NEEDLES"),
}
# which binaries each subsystem key is cut from, with the key of subsystems.json to use
PLAN = {
    "server": ["server"],
    "client": ["client", "engine"],
}
EXTRA = {"vphysics": [("vphysics", "server")], "luabind": [("luashared", "server")]}


ONLY = [l for l in os.environ.get("DUMP_ONLY_LABELS", "").split(",") if l]


def run(label, name, selector, classes=None):
    project, program, script, var = BINARIES[label]
    out = os.path.join(DUMP, name, label)
    if ONLY and label not in ONLY:
        return out if os.path.isdir(out) else None
    if not os.path.exists(os.path.join(PROJECTS, project + ".gpr")) or os.path.exists(os.path.join(PROJECTS, project + ".lock")):
        print(f"  skip {label}: no project or still being analysed")
        return None
    out = os.path.join(DUMP, name, label)
    os.makedirs(out, exist_ok=True)
    env = dict(os.environ, DECOMPILE_OUT=out, DECOMPILE_LIMIT="2500")
    env[var] = selector
    if script == "decompile_strings.py":
        env["DECOMPILE_CLASSES"] = classes or ""
    log = os.path.join(DUMP, name, f"{label}.log")
    with open(log, "w") as fh:
        subprocess.run(
            [GHIDRA, PROJECTS, project, "-process", program, "-noanalysis", "-readOnly",
             "-scriptPath", LAB, "-postScript", script],
            env=env, stdout=fh, stderr=subprocess.STDOUT, check=False,
        )
    n = len(os.listdir(out))
    print(f"  {label}: {n} functions")
    return out


def main():
    subsystems = json.load(open(os.path.join(LAB, "subsystems.json")))
    wanted = sys.argv[1:] or list(subsystems)
    for name in wanted:
        sel = subsystems[name]
        print(name)
        dirs = []
        for key, labels in PLAN.items():
            if not sel.get(key):
                continue
            for label in labels:
                d = run(label, name, sel[key], sel.get("server"))
                if d:
                    dirs.append(d)
        for label, key in EXTRA.get(name, []):
            d = run(label, name, sel[key])
            if d:
                dirs.append(d)
        # index: one line per function file with its selecting strings / name
        idx = os.path.join(DUMP, name, "INDEX.txt")
        with open(idx, "w") as fh:
            for d in dirs:
                for f in sorted(os.listdir(d)):
                    first = open(os.path.join(d, f), errors="replace").readline().strip()
                    fh.write(f"{os.path.basename(d)}/{f}\t{first[:200]}\n")
        tgz = os.path.join(DUMP, f"{name}.tar.gz")
        with tarfile.open(tgz, "w:gz") as tar:
            for d in dirs:
                tar.add(d, arcname=f"{name}/{os.path.basename(d)}")
            tar.add(idx, arcname=f"{name}/INDEX.txt")
        print(f"  -> {tgz} ({os.path.getsize(tgz) // 1024} KiB)")


if __name__ == "__main__":
    main()
