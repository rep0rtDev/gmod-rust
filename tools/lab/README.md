# Research lab

Local-only workspace for studying the real game's binaries. Everything it produces lives under `.local/` at the repository root, which is gitignored and refused by `tools/check_public_tree.py`.

What is allowed to leave the lab: human-written notes in `notes/` with behaviour, numbers and formulas, each tied to a parity harness. What is not: binaries, decompiler output, symbol dumps, or anything pasted from them.

```
python3 tools/lab/prepare.py --gmod ~/path/to/GarrysMod --srcds ~/path/to/srcds   # copies bin/ trees into .local/gmod/<build>/
python3 tools/lab/symbols.py                                                       # nm inventory of the unstripped Linux server binaries into .local/re/<build>/
```

`symbols.py` prints only counts and the list of modules; the symbol tables themselves stay in `.local/`.


Per-subsystem binaries: a subsystem entry's keys name the binaries it is cut from (`server`, `client` = client.dll + engine.dll, `vphysics`, `luashared`), each with its own needles. Float/double constants that the pseudo-C shows as `DAT_xxx` carry their value as a trailing comment (`DAT_1234 /* =0.5f */`) when the bytes read as a sane float or double.
