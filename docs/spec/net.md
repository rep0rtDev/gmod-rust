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
reliable resends, no snapshot skipped for a lost baseline. Bytes: client 49 per packet with the usercmds delta coded (a change mask per usercmd against the
previous one in the batch, only the first tick; 8 identical usercmds cost 10 bytes plus the first
one in full), 3.2 kB/s at 66 Hz; server 135 per packet on this near-empty map.

## Client prediction

The client runs every usercmd through `gm-move` the tick it sends it (`Prediction`) and keeps the
last 128 predicted states; every snapshot carries the server's full `PlayerState` after the
client's last run usercmd (`LocalPlayer`, all fields: duck timers, jump held, boost, surface
friction, so a replay is bit-exact), and when it differs from what was predicted for that usercmd
the client replaces it and replays the later usercmds from it. The server runs the usercmds in
order, catching up a backlog after a lost packet with up to 3 per tick (the extra ones move the
player only) and letting the player stand when none is due, like the engine.

Measured (`net_predict_*`: the movement recordings sent as usercmds through the loopback at 2
ticks one way): walk, jump, stairs and airstrafe predicted bit for bit equal to the real server's
recording with zero corrections, and the final predicted state equals the server's. With 20 % loss
(52 and 41 packets dropped on walk and jump) there is still no correction: the 8 usercmds in every
packet and the backlog catch-up on the server mean no usercmd is ever missing when its tick comes,
so the server never has to stand still.

## Interpolation

Other players and props are drawn `interp_ticks` behind the newest snapshot (two snapshot
intervals: 4 ticks, 60 ms at 33 Hz) between the two snapshots around that render time, positions
linearly, angles by quaternion slerp (`Interpolator`); ahead of the newest snapshot (a late one)
the pose is extrapolated from the snapshot's velocity, which entity states carry for that.

Measured (`net_interp_*`: a prop falls on the server, the client interpolates, the client's pose
indexed by render tick is compared with the real game's recording like `phys_*`): in flight the
interpolated crate is within 0.07 u of our server's truth (the linear lerp of a parabola over
30 ms), but at the impact tick it is 2.5 u off, because the two snapshots around it straddle the
bounce; so `net_interp_drop_crate` at 33 Hz is DIFF against the game by that one tick (2.50 u at
tol 2, contact tick 35 vs 36), and `net_interp_drop_crate_66hz` with a snapshot every tick is
equal (1.19 u, the same as `phys_drop_crate` itself). The tossed can is within 0.20 u and 6
degrees of our server at 33 Hz (it spins at about 400 degrees per second) and DIFF against the
game for the physics reason `phys_toss_can` is.

## Transports and the dedicated server

`Transport` is one object per endpoint (`send(to, packet)`, `recv() -> (from, packet)`): the
loopback hub hands out endpoints for harnesses, `Udp` wraps a non-blocking `std` socket (the
server numbers peers 1.. as they first appear, a client talks to endpoint 0; 1 400 byte packets).
`gm-dedicated --map gm_construct --bind 0.0.0.0:27015` loads the map from the local game copy
(brushes and displacements into both worlds, 0.07 s for gm_construct), ticks at 66.67 Hz with a
monotonic schedule (sleeps to the next tick, resyncs after a 200 ms stall) and loads models on
first spawn from MDL/PHY. `gm-bot --server host:port` is a headless client: it walks, spawns a
crate and prints what its snapshots say.

Measured on this machine, two processes over UDP on localhost: welcome after 1 client tick, first
snapshot after 2; the bot walking forward at 200 u/s for 100 ticks moved 309 u as the server
simulated it; its `gm_spawn` landed a crate at rest in front of it (count 1, one undo entry) and
the client saw it in the next snapshots; 155 packets received over 400 client ticks (33 Hz
snapshots), 26 kB down and 19 kB up; the server's tick took 0.012 ms idle and 0.044 ms with one
client and one prop (the physics world is already loaded).

## Not yet

Bit packing of snapshots, interest management (PVS + radius), snapshot compression
above 1 200 bytes, lag
compensation, encryption and authentication, QUIC transport, voice, a server browser / master server.
