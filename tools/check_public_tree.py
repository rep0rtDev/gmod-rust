#!/usr/bin/env python3
"""Refuse any Valve/Facepunch game file, binary, decompiled or leaked code in the tree.

Runs on the working tree by default. With --history it also walks every blob
reachable from HEAD so nothing can hide in an old commit. Exit code 1 on any hit.
"""
from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys

MAGIC = {
    b"\x34\x12\xaa\x55": "VPK archive",
    b"VBSP": "BSP map",
    b"VTF\x00": "VTF texture",
    b"IDST": "MDL model",
    b"IDSV": "VVD vertex data",
    b"IDAG": "MDL aggregate",
    b"GMAD": "GMA addon",
    b"MZ": "Windows PE binary",
    b"\x7fELF": "ELF binary",
    b"RIFF": "RIFF/WAV audio",
    b"ID3": "MP3 audio",
    b"\xff\xfb": "MP3 audio",
    b"OggS": "Ogg audio",
    b"PK\x03\x04": "zip archive",
    b"<!-- dmx": "DMX/PCF particle file",
}

# Tokens that only appear in leaked Source trees or in decompiler output. Keep this list
# narrow: every entry must be absent from Source SDK 2013, the GMod Lua base and public docs.
FORBIDDEN_TOKENS = [
    re.compile(rb"src_main[\\/]"),
    re.compile(rb"hl2_src[\\/]"),
    re.compile(rb"\bFUN_[0-9a-f]{6,}\b"),  # Ghidra default function names
    re.compile(rb"\bDAT_[0-9a-f]{6,}\b"),  # Ghidra default data names
    re.compile(rb"\bsub_[0-9A-F]{6,}\b"),  # IDA default function names
    re.compile(rb"\b__thiscall\b"),
]

FORBIDDEN_PATHS = [
    re.compile(r"(^|/)\.local(/|$)"),
    re.compile(r"\.(vpk|bsp|vtf|vmt|mdl|vvd|vtx|phy|ani|gma|wav|mp3|ogg|pcf|dll|so|exe|dylib|pdb)$", re.I),
]

MAX_FILE_BYTES = 2 * 1024 * 1024
TEXT_EXT = {".rs", ".py", ".lua", ".luau", ".md", ".toml", ".txt", ".json", ".yml", ".yaml", ".wgsl", ".wit", ".c", ".h", ".cpp", ".hpp", ".sh", ".cfg", ".res"}


def git(*args: str) -> bytes:
    return subprocess.check_output(["git", *args])


def check_blob(path: str, data: bytes, problems: list[str]) -> None:
    for pat in FORBIDDEN_PATHS:
        if pat.search(path):
            problems.append(f"{path}: forbidden path or extension")
            break
    if len(data) > MAX_FILE_BYTES:
        problems.append(f"{path}: {len(data)} bytes, larger than {MAX_FILE_BYTES}")
    head = data[:16]
    for magic, what in MAGIC.items():
        if head.startswith(magic):
            problems.append(f"{path}: looks like a {what}")
            break
    ext = os.path.splitext(path)[1].lower()
    if ext in TEXT_EXT or not ext:
        for pat in FORBIDDEN_TOKENS:
            m = pat.search(data)
            if m:
                problems.append(f"{path}: forbidden token {m.group(0)!r}")
                break


def check_worktree(problems: list[str]) -> int:
    files = git("ls-files", "-z", "--cached", "--others", "--exclude-standard").split(b"\0")
    n = 0
    for f in files:
        if not f:
            continue
        path = f.decode("utf-8", "replace")
        if path == "tools/check_public_tree.py":
            continue
        try:
            with open(path, "rb") as fh:
                data = fh.read()
        except (FileNotFoundError, IsADirectoryError):
            continue
        check_blob(path, data, problems)
        n += 1
    return n


def check_history(problems: list[str]) -> int:
    seen: set[bytes] = set()
    n = 0
    commits = git("rev-list", "HEAD").split()
    for c in commits:
        tree = git("ls-tree", "-r", "-z", c.decode()).split(b"\0")
        for line in tree:
            if not line:
                continue
            meta, path = line.split(b"\t", 1)
            _mode, kind, sha = meta.split(b" ")
            if kind != b"blob" or sha in seen:
                continue
            seen.add(sha)
            p = path.decode("utf-8", "replace")
            if p == "tools/check_public_tree.py":
                continue
            data = git("cat-file", "blob", sha.decode())
            check_blob(f"{c[:10].decode()}:{p}", data, problems)
            n += 1
    return n


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--history", action="store_true", help="also scan every blob reachable from HEAD")
    args = ap.parse_args()
    problems: list[str] = []
    n = check_worktree(problems)
    if args.history:
        n += check_history(problems)
    if problems:
        print(f"check_public_tree: {len(problems)} problem(s) in {n} blobs")
        for p in problems:
            print("  " + p)
        return 1
    print(f"check_public_tree: ok, {n} blobs clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
