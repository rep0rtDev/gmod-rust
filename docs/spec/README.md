# Behaviour specifications

One file per subsystem. A spec states behaviour as numbers, formulas, state machines and edge cases, each with the parity harness that confirms it. Specs are written from public documentation, from measurements on the real game, and from reference material read for behaviour only (Source SDK 2013, the GMod Lua base, the research lab notes). They never contain code from any of those sources.

A statement without a harness is a hypothesis and is marked `(unconfirmed)`.

| Spec | Harness group | State |
|---|---|---|
| [content-mounting.md](content-mounting.md) | `fmt_*` | draft |
| [convars-defaults.md](convars-defaults.md) | `fmt_convars_defaults` | measured on build 260917 |
| [movement.md](movement.md) | `move_*` | 17 harnesses bit-exact on 2026.09.17 |
| [physgun.md](physgun.md) | `physgun_*` | 4 / 7 within tolerance on 2026.09.17 (hold, yaw, pitch, freeze exact; rotate axis and walking lag just outside) |
| [physics.md](physics.md) | `phys_*` | 1 / 5 within tolerance on 2026.09.17 (free flight and contact tick exact, contact response open) |
