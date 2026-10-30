# Behaviour specifications

One file per subsystem. A spec states behaviour as numbers, formulas, state machines and edge cases, each with the parity harness that confirms it. Specs are written from public documentation, from measurements on the real game, and from reference material read for behaviour only (Source SDK 2013, the GMod Lua base, the research lab notes). They never contain code from any of those sources.

A statement without a harness is a hypothesis and is marked `(unconfirmed)`.

| Spec | Harness group | State |
|---|---|---|
| [content-mounting.md](content-mounting.md) | `fmt_*` | draft |
| [convars-defaults.md](convars-defaults.md) | `fmt_convars_defaults` | measured on build 260917 |
| [movement.md](movement.md) | `move_*` | 31 harnesses bit-exact on 2026.09.17, footsteps and landings (tick, volume, wave set) in the eleven `move_step_*` |
| [physgun.md](physgun.md) | `physgun_*` | 6 / 9 within tolerance on 2026.09.17 (hold, yaw, pitch, freeze, rotate; walking lag and the landing after release open; `physgun_sound_*` re-record hold and freeze with the server's emitted sounds, see [audio.md](audio.md)) |
| [physics.md](physics.md) | `phys_*`, `constraint_*` | 1 / 5 within tolerance on 2026.09.17 (free flight and contact tick exact, contact response open) |
- [sandbox.md](sandbox.md): prop spawning placement, undo, cleanup, limits, +use pickup (`use_*` 15 / 22 on 2026.10.31)
- [toolgun.md](toolgun.md): tool gun trace, weld and rope stools, counts and undo
- [audio.md](audio.md): client sound: WAV / soundscript formats, viewmodel event → entry table, measured prop impact sounds (surface property, `(speed/320)^2` volume law), mixer approximations
- [server.md](server.md): the server tick composing movement, sandbox and physics
- [net.md](net.md): own protocol, packets, commands, delta snapshots, measured loopback numbers
- [script.md](script.md): Luau actor runtime, phases, budgets, host API v1
