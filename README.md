# gmod-rust

Garry's Mod sandbox, rebuilt from scratch in Rust.

Same maps, same props, same physgun, same feel. New engine: multithreaded simulation, Jolt physics, wgpu rendering, own networking, own addon system (Luau actors and WebAssembly components). Not compatible with the original game: it does not join GMod servers and does not run GLua addons.

**This repository contains no Garry's Mod, Half-Life 2 or Source engine files.** You need your own copy of Garry's Mod installed through Steam. The launcher reads the game's content (BSP maps, MDL models, VTF textures, VPK archives, sound and scripts) from that installation and builds its own cache on your machine. Nothing is downloaded or redistributed.

Status: foundation. See [docs/design.md](docs/design.md) for the full design and [docs/status.md](docs/status.md) for what the parity harnesses currently cover.

## How "1 to 1" is checked

Every subsystem is compared with the real game by differential harnesses: the same inputs are fed to GMod (through a probe addon running inside a real server or client) and to gmod-rust, and the outputs must match within a stated tolerance. A subsystem is not done until its harnesses are green. See [docs/design.md, section 4](docs/design.md#4-харнессы-паритета-главный-инструмент-проекта).

## Repository layout

```
crates/        engine and game crates (gm-core, gm-formats, gm-content, ...)
apps/          launcher, client, server binaries
tools/parity   parity harnesses: GLua probe, runner, compare
tools/lab      local research workspace tooling (nothing from it is committed)
docs/          design, behaviour specs, status
```

## Rules

- No game files, decompiled code or leaked source in the tree, ever. `tools/check_public_tree.py` runs on every commit and in CI.
- Behaviour is written down in `docs/spec/` as numbers, formulas and state machines, with the harness that confirms each statement. Code is written from the spec, not from any Valve or Facepunch source.
- Source SDK 2013 and the public GMod Lua base are reference material for behaviour only. Leaked Source code is not used in any form.

## Building

```
cargo build --workspace
cargo test --workspace
```

Integration tests that need the game set `GMOD_DIR` to your Garry's Mod install directory and are skipped otherwise.

## Running

```
cargo run --release -p gm-client                       # gm_construct on the embedded server
cargo run --release -p gm-client -- gm_flatgrass --addon addons/examples/hud_lua
cargo run --release -p gm-dedicated -- --map gm_construct --bind 0.0.0.0:27015
cargo run --release -p gm-client -- --connect 192.168.1.10:27015
```

The client finds the game through Steam (`GMOD_DIR` overrides). With `--connect` the server
names the map in its Welcome (5 s to answer), the embedded server stays idle and every packet
goes over UDP; `addons` given on the command line load into the embedded server only, a remote
server's addons download as usual.

## License

MIT. Garry's Mod, Half-Life 2, Source and Steam are trademarks of their respective owners. This is an unofficial fan project, not affiliated with Facepunch Studios or Valve Corporation.
