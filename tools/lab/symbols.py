#!/usr/bin/env python3
"""Inventory symbols of the Linux server binaries copied by prepare.py.

Writes `<module>.symbols.txt` (demangled nm output) under .local/re/<build>/ and
prints only per-module counts. The Linux srcds binaries ship unstripped, so this
gives class and method names for free; those names stay in .local/.
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
LOCAL = ROOT / ".local"

MODULES = [
    "bin/vphysics_srv.so",
    "bin/engine_srv.so",
    "bin/materialsystem_srv.so",
    "bin/datacache_srv.so",
    "bin/dedicated_srv.so",
    "bin/libvstdlib_srv.so",
    "bin/libtier0_srv.so",
    "garrysmod/bin/server_srv.so",
    "bin/linux64/vphysics_srv.so",
    "bin/linux64/engine_srv.so",
    "garrysmod/bin/linux64/server_srv.so",
]


def main() -> int:
    builds = sorted((LOCAL / "gmod").glob("*/server")) if (LOCAL / "gmod").is_dir() else []
    if not builds:
        print("run tools/lab/prepare.py --srcds <dir> first", file=sys.stderr)
        return 1
    for server in builds:
        build = server.parent.name
        out_dir = LOCAL / "re" / build
        out_dir.mkdir(parents=True, exist_ok=True)
        print(f"build {build}")
        for rel in MODULES:
            mod = server / rel
            if not mod.is_file():
                continue
            try:
                text = subprocess.check_output(["nm", "-C", "--defined-only", str(mod)], stderr=subprocess.DEVNULL, text=True)
            except subprocess.CalledProcessError:
                continue
            lines = [l for l in text.splitlines() if l.strip()]
            funcs = sum(1 for l in lines if " T " in l or " t " in l or " W " in l)
            out = out_dir / (rel.replace("/", "_") + ".symbols.txt")
            out.write_text(text)
            print(f"  {rel:<40} {len(lines):>7} symbols, {funcs:>7} functions")
    return 0


if __name__ == "__main__":
    sys.exit(main())
