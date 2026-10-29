# Network protocol (`gm-net`)

Own protocol, no compatibility with the original game's. Decision for the core: a thin layer of
our own rather than lightyear (it pulls Bevy ECS, which we do not use) with the transport behind a
trait, so harnesses run on a deterministic in-memory loopback (fixed one-way delay in ticks,
seeded loss) and the launcher can bring UDP or QUIC (`quinn`) later without touching the protocol.

## Packets

`[magic u16][seq u16][ack u16][ack_bits u32]` then messages `[flags u8][id varint if reliable]
[len varint][bytes]`. `ack` is the newest sequence seen from the other side and `ack_bits` the 32
before it (Source's scheme). Reliable messages (hello, welcome, sandbox commands, their acks,
events) are repeated in every packet until a packet that carried them is acked; the receiver drops
duplicates by the message id. Everything else (usercmds, snapshots) is unreliable.

## Client -> server

- `Cmds`: the last 8 usercmds (tick, buttons, view angles, moves, mouse, weapon), every tick at
  66 Hz. The server runs at most one new usercmd per tick in order and repeats the last one when
  none arrived (the engine's command deficit).
- `Command` (reliable, sequenced): `gm_spawn <model>`, `gmod_undo`, `gmod_cleanup`, tool gun click
  (tool, left/right/reload), `sbox_*` cvar. Executed once, in sequence order, after the movement
  and physics of the tick they arrive in; acked with `CommandAck`.
- `AckSnapshot`: the newest snapshot tick the client holds.

## Server -> client

- `Welcome` (reliable): player id, tick interval, map, server tick.
- `Snapshot` every `snapshot_every` ticks (2 = 33 Hz): tick, baseline tick (0 = full), the
  client's last run usercmd tick, all players (origin, velocity, view, ducked, held prop), the
  entities that changed against the baseline (id, model, pos, angles, frozen, owner), the ids
  removed since, and the client's own sandbox view (count, constraints, undo stack, maxprops). The
  baseline is the newest snapshot the client acked; the server keeps the last 64 it sent, the
  client the last 64 it applied, and a snapshot whose baseline the client lost is skipped until a
  later one has a known baseline.
- `Event` (reliable): LimitHit and the like.

## Measured

The three sandbox and tool gun recordings replayed as `net_*` through the loopback at 2 ticks one
way (30 ms) and 20 % loss, with the recorded state being what the client rebuilt from snapshots:
`net_spawn_basic` 16 ops within 1.72 u / 4.4 degrees (the client sees the prop a snapshot interval
plus the delay later than the recording's sample, so a prop still settling reads slightly off),
`net_weld_rope` 20 ops within 0.12 u, `net_axis` 8 ops within 0.02 u. A command's round trip is
4 to 7 ticks (60 to 105 ms) including retries after loss; 107 of 534 packets dropped, 125
reliable resends, no snapshot skipped for a lost baseline. Bytes: client 354 per packet (8 usercmds
of 41 bytes, uncompressed: 23 kB/s at 66 Hz, far above the budget; the usercmd delta encoding of
the design is the next step), server 135 per packet on this near-empty map.

## Not yet

Usercmd delta coding and bit packing, interest management (PVS + radius), snapshot compression
above 1 200 bytes, client prediction and reconciliation of the local player, interpolation, lag
compensation, encryption and authentication, UDP/QUIC transports, voice.
