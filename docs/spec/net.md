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

Packet budget and fragments: a packet stays under 1200 bytes (`Udp` receives up to 1400). The
reliable queue goes out oldest first, as many messages as fit next to the unreliable payloads
(always at least one); a reliable message longer than 1024 bytes is split into fragments of that
size (flag bit 2 = "the next reliable message continues this one") and reassembled by the
receiver. Reliable messages are delivered in id order: one after a lost one waits for the resend
(duplicates are dropped by id, as before). `Channel::build_burst` makes up to 8 packets per tick
when reliable messages are left that none of the tick's packets carried, so a large transfer
moves ~8 KiB per tick per client; the server sends bursts, the client one packet per tick.
Measured with the gm-net unit test: a 50 000-byte message plus two small ones around it arrive
whole and in order over 20 % loss, every packet under the budget.
The fresh unreliable payload (snapshot, usercmds) always goes in the tick's first packet; reliable
messages fill what is left and a burst's later packets carry the rest (found when a 1.4 KB
reliable undo list rode along with a snapshot: 1 559 bytes).
Ack of nothing: a header with `ack = 0, ack_bits = 0` means "nothing received yet", never "packet
0 received" (that one is acked by the bits of a later packet; found with a loopback seed that lost
the first server packet: the welcome was dropped as acked and the client never joined).

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
bounce; so `net_interp_drop_crate` at 33 Hz is DIFF against the game by that one tick (2.13 u at
tol 2 after the impact-time shift in gm-physics, contact tick 35 vs 36), and `net_interp_drop_crate_66hz` with a snapshot every tick is
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

Addon distribution: the server publishes `.gmr` packages (`ServerNet::publish_addon`, the
dedicated server does it for every `--addon` whose manifest lists the `client` realm, packing a
directory on start) and announces them right after `Welcome` as `ServerMsg::Addons { list }`
(tag 7: id, version, sha256, size). A client without the package in its cache asks with
`ClientMsg::WantAddon { sha }` (tag 6) and the server streams it as `ServerMsg::AddonChunk { sha,
offset, total, data }` (tag 8), 16 KiB per chunk, one chunk queued per tick while fewer than 24
reliable fragments are in flight, so events and cvars keep flowing between chunks; unknown shas are
ignored, a chunk out of order or a total above 64 MiB drops the download. `gm-bot --cache <dir>`
keeps `<sha256>.gmr`, verifies the sha before loading, and runs the package's client actors.
Measured (loopback, 1 tick delay, 10 % loss): a 100 KB package is downloaded, verified and running
on the client in under 200 ticks with replicated cvars arriving alongside
(`published_client_addon_is_downloaded_over_a_lossy_loopback_and_runs_on_the_client`).
Over real UDP (two processes on one machine): a 300 571-byte package is downloaded, verified and
its client actor running 74 ticks after welcome (~4 KB per tick); a second connect with the
package in `--cache` loads it at once without asking.
Hot reload: a server with `--watch` that reloads an addon with the `client` realm repacks it
(`ServerHooks::drain_published`), publishes the new sha and sends every connected client a fresh
`Addons` list (latest package per addon id; older ones stay downloadable so a transfer in flight
finishes); the client downloads the new package and hot-reloads its actors, `state` kept
(`a_hot_reloaded_client_addon_is_republished_and_the_client_reloads_it_keeping_state`).

## Interest and the snapshot budget

A client is sent the entities and other players within `ServerNet::interest_radius` (8 192 u by
default) of its own player; what leaves the radius is listed in `removed` for that client and what
enters is sent whole, so the client's table is exactly its interest. Each snapshot is budgeted
(`snapshot_budget`, 1 000 bytes): the local player first, then the nearest other players
(53 bytes each), then the nearest changed entities (58 bytes + model name each); what does not fit
waits, and because the delta is taken against the baseline the client acked, an unsent change is
simply still different next time. The server remembers per client what it actually sent, not the
whole world. The player's undo list left the snapshot (60 entries were 1.4 KB in every one): it
goes as `ServerMsg::Undos { seq, entries }` (tag 9, reliable) whenever it changes and the client
keeps it across snapshots like cvars. Measured
(`snapshots_carry_only_the_props_in_the_players_interest_within_the_byte_budget`): 60 crates in a
line, radius 2 000, budget 400: the client converges on exactly the crates in range, follows a
3 000 u teleport (far crates in, old ones out), every packet under 1 200 bytes. Interest by PVS: `gm_collision::World` keeps the leaf clusters and the decompressed visibility
lump as one bitset per cluster (`cluster_at`, `visible`); an entity or player also has to be in a
cluster visible from the player's eye cluster (origin + 64 u), `ServerNet::use_pvs` turns it off.
A point in solid (cluster -1) is never visible, a map without a visibility lump is all visible.
Measured (`a_crate_out_of_the_players_pvs_is_not_sent_until_the_player_can_see_it`): on
gm_construct a crate within 2 500 u of the spawn but outside its PVS is not sent until the player
stands next to it.
Stress (`stress_64_clients_and_2000_props_over_the_loopback`, release, one thread): 64 clients in
an 8 x 8 grid and 2 000 frozen crates over the same 3 000 x 2 400 u area, all within everyone's
interest: server `tick` 1.3 ms, `send` 3.5 ms per tick (the per-client filter and delta over
2 000 entities dominates), 607 bytes per tick per client (40 KB/s, 2.5 MB/s upload for the server),
no snapshot dropped. First finding: with 63 other players nearest-first, players alone exceeded the
budget and entities got one per snapshot (a client learned ~100 entities in 200 ticks); now other
players take at most half the budget and both lists are ordered by distance / (1 + ticks since
that player or entity was last sent to this client), so far and stale ones rotate in.
Measured after both (model table below, half-budget players, staleness order): 554 bytes per tick
per client (36 KB/s), every client knows 798-800 of the 2 000 crates after 200 ticks (~265 per
second, 8x), `send` 7.0 ms per tick for 64 clients, no snapshot dropped.

Quantization: snapshot positions go as zigzag varints of 1/32 u (3 bytes per axis within 4 096 u
of the origin), angles as i16 of 1/64 degree wrapped to [-180, 180), velocities as i16 of 1/8 u/s
(clamped at 4 096) and angular velocities as i16 of 1/4 degree/s; the local player's own state
stays f32 for the prediction. An entity is at most 44 bytes (was 59), a player at most 44 (was
53). The default snapshot budget is 750 bytes, which at the snapshot rate is the 25 KB/s per
client of the design. Measured (stress, 64 clients x 2 000 crates): 385 bytes per tick per
client (25.1 KB/s) and every client knows 897-900 crates after 200 ticks, against 554 bytes
(36 KB/s) for 798-800 crates with the f32 fields and the 1 000 byte budget; `net_` parity 9/11
as before.

Authentication (v1, own accounts, no Steam AppID): a client's identity is an ed25519 public key
(`gm-bot --key <file>`, the same key format as package signing). After `Hello` the server sends
`ServerMsg::Challenge { nonce }` (32 random bytes, tag 11); a client with a key answers
`ClientMsg::Auth { pubkey, sig }` (tag 7) with the signature over `"gmr-auth1" ++ nonce`. A valid
answer sets the connection's identity (`Player::identity`, the stable id for bans, ownership and
storage); `gm-dedicated --require-auth` holds the `Welcome` (and the addon list and cvars) until
it arrives, so an anonymous client never enters. The signed message also covers the hash of the
connection's packet encryption key (channel binding), so a relaying server cannot reuse the answer
on its own connection. Not yet: a registration/name service and Steam OpenID binding on top of
the keys.

Encryption: `Hello` carries the client's ephemeral X25519 public key and `Challenge` the
server's; both derive `SHA-256("gmr-key1" ++ shared ++ client_pub ++ server_pub)` and seal every
packet after that with ChaCha20-Poly1305 (outer byte 1, 8-byte counter as nonce with a direction
word so one key serves both ways, 16-byte tag; the channel fills 1 175 bytes so the sealed
packet stays within the 1 200 wire budget).
Outer byte 0 is plaintext and only the handshake travels that way: each side keeps answering in
plaintext until it has opened one sealed packet from the peer (a lost `Challenge` is re-sent
readably) and refuses plaintext from then on; a packet that does not authenticate is dropped
before the channel sees it. Master-server packets (magic `MG`) stay plaintext. Not yet: replay
protection beyond the channel's sequence handling, key rotation, and the client checking a server
identity (the master could publish server keys).


Lag compensation: every usercmd carries `seen_tick`, the server tick the client was rendering
when it made the command (`NetCmd::seen_tick`, delta-coded like the other fields; 0 = unknown).
The server keeps the last 64 ticks of (origin, ducked) per player and resolves a shot of that
command against the other players as they stood at `seen_tick` (interpolated between the two
recorded ticks), so a hit on what the shooter saw lands even though the target has moved since.
`seen_tick` 0 or older than the history resolves against the current poses (the parity runner and
the recordings `hit_*` are unchanged). Measured (`lag_compensation_hits_the_target_where_the_
shooter_saw_it`): a target running at 300 u/s moved 27 u in 6 ticks, the shot at its old pose
lands only with the rewind.

Model table: entity states carry a model index (varint, 0 = none) instead of the name; the server
keeps one table for all connections and sends each client the new entries as `ServerMsg::Models {
from, names }` (tag 10, reliable) right before the first snapshot that uses them. A snapshot that
names an id the client does not have yet (the `Models` message was deferred or lost) is dropped
like one against a lost baseline; the resend fixes it. An entity is 59 bytes on the wire
(ENTITY_SNAP_SIZE) instead of 58 + the model name.

## Server browser: master and query

Master packets carry their own magic (`0x4D47`), so a game server tells them from channel packets
before any client slot is made. `gm-dedicated --master host:27010` heartbeats every 10 s
(`MasterMsg::Heartbeat(ServerInfo)`: name, map, players, max players, game port, version, ids of
the published addons); the master (`gm-master`) keys servers by the sender's host and the game port,
drops one silent for 90 s, and answers `List { offset }` with `Servers { total, offset, entries }`
pages under 1100 bytes (entry: `host:port`, info, seconds since the heartbeat). A browser then
sends `Query { nonce }` straight to each server and gets `Info { nonce, info }` back with the live
player count; the round trip is the ping. `gm-bot --list host:27010` does all of it and prints
the table. Measured: 201 servers list in budget-sized pages; a server answers queries while a
player is connected and counts it (`a_game_server_answers_browser_queries_and_heartbeats_to_the_master`).

## Not yet

Bit packing of snapshots, interest management (PVS + radius), snapshot compression
above 1 200 bytes, lag
compensation, encryption and authentication, QUIC transport, voice, NAT traversal for the browser.

Admin commands: `Command::Reload { addon }` (tag 6) hot-reloads an addon on the server for players in `ServerNet::admins`; the answer is an event string on the player's reliable channel.

## Player life in the snapshot

`PlayerSnap` carries `alive`, `health` (i16), `armor` (i16), `weapons` (a bit per entry of `gm_server::LOADOUT`, the weapon registry until scripted weapons bring a string table) and `active` (registry index, 255 none): 7 bytes per player per snapshot. Harness `net_player_lifecycle` runs the `player_lifecycle` recording through the loopback (2 ticks each way, no loss): the client's view of life, health, armour and weapons matches the game's recording within the snapshot lag (max 2 ticks), the four respawns are seen within 0.07 u of a spawn point. The death tick's transient (weapons still listed for one tick, stripped the next) cannot appear in a snapshot two ticks later; the comparison accepts the next tick's list there.

Addon messages: `ServerMsg::Addon { seq, name, data }` and `ClientMsg::Addon { name, data }` (tag 5 both ways) are reliable and ordered like `Event`; `data` is the gm-script `netval` encoding of one Lua value (64 KiB cap). Server actors queue them through `ServerHooks::drain_net`, sent after a tick's events; client messages reach `ServerHooks::net_message` as the packet is read.

Replicated cvars: `ServerMsg::Cvar { seq, name, value }` (tag 6, reliable) carries every replicated variable right after `Welcome`, then each creation and change after the tick's script hooks; `Command::Cvar` from a client sets any registered variable through `Server::set_cvar` (clamped to min/max, side effects on the sandbox limits).
