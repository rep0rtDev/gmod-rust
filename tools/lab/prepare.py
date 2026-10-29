#!/usr/bin/env python3
"""Copy the game's binary trees into the gitignored .local/ workspace for study.

Nothing is modified in the Steam install. The build id is read from steam.inf.
"""
from __future__ import annotations

import argparse
import re
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
LOCAL = ROOT / ".local" / "gmod"


def build_id(game_dir: Path) -> str:
    inf = game_dir / "garrysmod" / "steam.inf"
    if inf.is_file():
        m = re.search(r"PatchVersion=([\w.]+)", inf.read_text(errors="replace"))
        if m:
            return m.group(1)
    for acf in [game_dir / "steamapps" / "appmanifest_4000.acf", game_dir / "steamapps" / "appmanifest_4020.acf"]:
        if acf.is_file():
            m = re.search(r'"buildid"\s+"(\d+)"', acf.read_text(errors="replace"))
            if m:
                return m.group(1)
    return "unknown"


def copy_tree(src: Path, dst: Path, patterns: tuple[str, ...]) -> int:
    n = 0
    for p in src.rglob("*"):
        if p.is_file() and p.suffix.lower() in patterns:
            rel = p.relative_to(src)
            out = dst / rel
            out.parent.mkdir(parents=True, exist_ok=True)
            if not out.exists() or out.stat().st_size != p.stat().st_size:
                shutil.copy2(p, out)
            n += 1
    return n


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--gmod", type=Path, help="client install dir (contains garrysmod/, bin/)")
    ap.add_argument("--srcds", type=Path, help="dedicated server dir (contains srcds_run)")
    args = ap.parse_args()
    exts = (".dll", ".so", ".pdb", ".exe", ".txt", ".inf")
    for label, d in (("client", args.gmod), ("server", args.srcds)):
        if not d:
            continue
        if not (d / "garrysmod").is_dir():
            raise SystemExit(f"{d} does not look like a game directory")
        bid = build_id(d)
        dst = LOCAL / bid / label
        n = copy_tree(d / "bin", dst / "bin", exts)
        n += copy_tree(d / "garrysmod" / "bin", dst / "garrysmod" / "bin", exts)
        (dst / "SOURCE.txt").write_text(f"{d}\nbuild {bid}\n")
        print(f"{label}: build {bid}, {n} files -> {dst.relative_to(ROOT)}")
    print("Reminder: .local/ never leaves this machine.")


if __name__ == "__main__":
    main()
