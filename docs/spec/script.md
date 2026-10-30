# Addon runtime (`gm-script`)

Luau (mlua, vendored Luau, `send`), one VM per actor: own globals, own GC, own memory limit
(256 MB default via `set_memory_limit`), own CPU budget. Nothing reaches the process: `os`, `io`,
`require`, `loadstring`, `dofile`, `loadfile`, `package`, `debug`, `getfenv`, `setfenv` are gone;
Luau's own sandbox mode is not used because it freezes every global table (`state` included).

## Tick

- Parallel phase: every actor as one job on a fixed worker pool (`available_parallelism`, at
  most 16 threads; a lone actor runs inline), all with the same immutable `Snapshot` (entities:
  id, model, pos, angles, frozen, owner; players: origin, velocity, view, held prop; counts).
  `world.get(id)`, `world.find(model?)`, `world.players()`, `world.count()`, `game.tick()` read
  it; `phys.trace(start, end)` traces the world (brushes, displacements) and the prop hulls like
  the tool gun (`hit`, `pos`, `normal`, `ent` or 0, `fraction`); `world.queue(op, ent, arg)` queues a write (`set_pos`, `set_angles`, `freeze`, `remove`,
  `set_velocity`, `weld`, `nocollide`, see Writes);
  actors without the `world-write` capability get an error from `queue`.
- Serial phase: queued writes are applied in addon priority, then actor id, then enqueue order;
  a write to an entity that is gone is rejected and counted.
- Cancelable events (`prop.spawn_request`) run serially in priority order; the first handler
  returning `false` cancels, later actors never see the request.
- Events in v1: `tick.pre`, `tick.post`, `prop.spawn_request` (player, model), `entity.spawned`
  (player, entity). `events.on(name, fn, priority?)` orders handlers inside an actor the same way.
- The server hooks (`gm_server::ServerHooks`) are called by `ServerNet` around the tick and the
  spawn command; `gm-dedicated --addon file.luau` loads actors (file name = actor name).

## Scripted stools

`tool.register(name, { left = fn(ctx), right = fn(ctx), reload = fn(ctx) })`. A click of a stool
the Rust sandbox does not know (`ToolClick::Unknown`) goes to the first actor in priority order
that registered it; `ctx` carries `player`, `stage`, `hit` (`ent` or 0 for the world, `pos`,
`normal`) and `objects` (the stored tool objects in world space). The handler returns an array of
ops applied at once with the same bookkeeping as the Rust stools: `set_object`, `clear`, `weld`
(`ent1`, `ent2`, `forcelimit`, `nocollide`, `undo`), `nocollide`, `rope` and `axis` (the two
stored objects, the measured constraint rules in Rust: length between the stored points, axis
alignment with the 0.2 u offset and the hinge); limits, undo entries, counts and cleanup lists
follow. `addons/examples/weld_lua` holds `weld_lua`, `rope_lua` and `axis_lua` written this way; the
harnesses `script_weld_rope` and `script_axis` replay the tool gun recordings with the Rust stools
replaced by the Luau ones through the server loop and are equal (20 ops within 0.12 u, 8 ops within
0.02 u), the same results as the Rust stools.

## Package and hot reload

`gmr pack <dir> <out.gmr>` zips `manifest.toml`, `src/**/*.luau`, `wasm/`, `assets/` (deflate,
entries sorted, fixed timestamps: the same tree packs to the same bytes), `gmr inspect` prints
manifest, actors, files and the sha256 that identifies the package. `Runtime::load_addon` takes a
directory or a `.gmr`; `gm-dedicated --addon x.gmr` too. `Runtime::reload_addon(path)` is the hot
reload: every actor of the addon keeps its id and `state` (serialised as on restart) and gets the
new source in a fresh VM, actors new to the manifest are loaded, removed ones dropped; the test
`hot_reload_keeps_state` carries `state.n = 3` into a new source that reads it.
Over the network: `Command::Reload { addon }` (reliable, tag 6) from a player in
`ServerNet::admins` re-reads the addon from where it was loaded and answers with an event
(`Reload <id> ok <id> <version> actors n -> m`, `... denied`, `... failed <why>`); the test
`reload_over_the_network` edits an addon on disk, reloads it as admin and the counter keeps its
value and continues with the new code.

## Client realm

`Runtime::client()` loads the `client` (and `shared`) actors of a manifest; `gm-bot --addon` runs
them. Every client frame `client_tick(world, new_snapshot)` builds the snapshot from what the
client assembled (`ClientWorld`: entities with pose/model/frozen/owner, players, own prop count),
dispatches `snapshot.received` (payload `tick`) when a new server tick arrived and `tick.client`
every frame, in parallel and read-only: `world-write` is never granted on the client, write queues
are dropped, `phys.trace` always misses (no world on the client yet). Test
`client_realm_sees_the_assembled_world`.

## Manifest

An addon is a directory with `manifest.toml` (`id`, `version`, `priority`, `realms`,
`capabilities`, `[[actors]]` with `name`, `source`, `realm`, `cpu_ms`) and its Luau sources;
sources may not leave the directory. `world-write` in `capabilities` is what lets an actor queue
writes. `addons/examples/freeze_crates` refuses cans and freezes spawned crates; the integration
test `gm-dedicated/tests/addon_hooks.rs` runs it through map, server loop, loopback network and
hooks: the can never appears, the crate is frozen in the sandbox, in Jolt and in the client's
snapshot.

## Worker pool

The parallel phase used to spawn a thread per actor per dispatch; measured in release on the 8-core
VM with trivial `tick.client` handlers: 48 us per tick for one actor, 125 for 8, 559 for 32. With
the fixed pool (`pool.rs`: mpsc jobs, a latch counted down by a drop guard so a panicking job
still completes the run, jobs borrow `&mut Actor` for the run's duration) the same ticks take
1.6, 46 and 138 us. A job that panics is logged as `actor thread panicked` for that actor only.

## Budgets

The VM's interrupt callback (Luau interrupt points: loop back-edges, calls) checks a deadline of
`cpu_budget` (2 ms default) after the dispatch started; past it the run aborts with
`cpu budget exceeded` and counts an overrun. Five overruns in a row throttle the actor to one run
in two ticks until a run finishes in budget. An actor that errors is restarted: `state` (numbers,
strings, booleans, nested tables) is serialised and set before the source runs again; other actors
are untouched.

Measured (`cargo test -p gm-script`): an actor spinning forever is aborted within its budget 5
times (total CPU under 100 ms for 5 ticks at a 5 ms budget), throttled, and a second actor keeps
counting ticks; an actor calling `error` is restarted once with `state.count` carried over; a
priority 0 actor's `set_pos` is applied before a priority 10 actor's `freeze`; a priority 0 actor
refusing cans stops the request before the priority 10 actor sees it.

## Writes

`world.queue(op, ent, arg)` ops: `set_pos` / `set_angles` (`{x, y, z}`, the other half of the
transform is kept), `freeze` (`false` thaws), `remove`, `set_velocity` (`{x, y, z, angular =
{x, y, z}?}` in u/s and deg/s, through the physics body), `weld` (`{ent2, forcelimit?, nocollide?}`;
`ent2` 0 welds to the world) and `nocollide` (`{ent2}`). Script constraints are owned by the server
(owner 0): no `sbox_maxconstraints` check, no undo entry, not touched by a player's cleanup, so
an addon cannot spend a player's limits and a player cannot undo an addon's weld. A duplicate
weld of the same pair, a self weld or a write to an entity that is gone is rejected and counted in
`Applied.rejected`. After a constraint write the server resyncs the physics constraints the same
way as after a scripted stool click.

Measured (`cargo test -p gm-dedicated --test addon_hooks script_writes`, needs the game install):
an actor queueing weld(e1, e2), nocollide(e2, world), set_velocity(e1, 120 u/s up) and a weld to
a missing entity gets `writes` 3, `rejected` 1, two sandbox constraints and e1 moving up at 120
u/s before the next server tick; the constraints survive the tick.

## Players

`world.players()` and `world.player(id)` (nil when gone) read the snapshot: `id`, `origin`,
`velocity`, `view_angles`, `held` (prop id or nil), `alive`, `health`, `armor`, `max_health`,
`model`, `weapons` (sorted class list), `active_weapon`, `frags`, `deaths`. On the client the
same table is filled from the snapshot message (health and armor as sent, weapons decoded from
the loadout mask, `max_health` 100, model/frags/deaths empty until the protocol carries them).

Player writes go through `world.queue("player.<op>", id, arg)` and need `world-write`:
`teleport` (`{x, y, z}`), `set_health` / `set_armor` (number), `damage` (`{amount, type}`, the
world as attacker, armor absorbs as in the damage spec), `kill`, `give_weapon` / `select_weapon`
(class string). Rejected and counted: any op on an unknown player, `kill` of a dead player,
`give_weapon` of a class the player does not carry (the loadout is fixed until scripted weapons),
`select_weapon` of a class not carried or already out.

Measured (`cargo test -p gm-dedicated --test addon_hooks script_reads`): one `tick.pre` queueing
set_health 42, set_armor 25, damage 10, give rpg, select 357, teleport, set_health of player 7 and
give of an unknown class gets `writes` 6, `rejected` 2; the next tick's read sees the damaged
health, `weapon_357` out and the same 13 weapons.

## Net

Addon messages are our own reliable, ordered channel messages (`ServerMsg::Addon` /
`ClientMsg::Addon`, tag 5, see net.md), not the sandbox's `net` bit stream: a message is a name
(1..64 bytes) and one Lua value (nil, boolean, number, string, table with string or integer keys,
nested up to 16 deep, at most 64 KiB on the wire; functions, userdata and other keys are an
error in the sender). `net.send(name, value, player?)` queues from any actor: on the server
`player` nil or 0 means every connected client, an unknown player id goes nowhere; on the client
the message goes to the server. At most 256 messages per actor run. `net.receive(name, fn,
priority?)` is `events.on("net.message", ...)` filtered by `name`; the payload has `name`, `data`
and, on the server, `player`. Server actors see a client message as a serial event right when the
packet is read (before that tick's `tick.pre`, writes applied at once in priority order); the
server sends what the actors queued after `post_tick`, after the sandbox events, in actor order
then send order. The client runtime gets them through `Runtime::client_net` after `client_tick`
(the bot and the headless client do this every frame) and hands its own with `drain_net` to
`ClientNet::send_addon`.

Measured (`cargo test -p gm-dedicated --test addon_hooks addon_net`): a client actor sends
`ping {n = 7, list = {'a', 'b', nil, 3}}` on its first snapshot, the server actor answers the
sender with `pong {echo, list, from, half = 0.5}` and sends a second pong to player 99; within 40
loopback frames the client has exactly one pong with echo 7, from = its id, half 0.5, list[2] 'b',
list[4] 3; `netval_round_trips_and_caps` covers nested tables, integer keys, the 64 KiB cap, the
depth cap, functions and truncated input.

## Cvars

The server keeps every console variable in `Server::cvars` (`Cvar`: value, default, replicated,
archive, min, max, help), seeded with the 24 `sbox_*` variables and their measured defaults
(convars-defaults.md), all replicated. `cvar.create(name, default, {replicated?, archive?, min?,
max?, help?})` queues a `CreateConVar` (any actor, no capability: a variable is not world state);
an existing name is rejected and keeps its value and flags. `cvar.set(name, value)` needs
`world-write`; numbers are clamped to min/max, an unknown name or an unchanged value is rejected
and counted. `cvar.get(name)` (string or nil), `cvar.get_number` (0 for a non number),
`cvar.get_bool` read the snapshot: on the server every variable, on the client the replicated
ones. The server reads `sbox_maxprops`, `sbox_maxconstraints`, `sbox_maxropeconstraints` and
`sbox_playershurtplayers` from the registry on every change; a client's `Command::Cvar` goes
through the same `set_cvar`. Replication is `ServerMsg::Cvar` (tag 6): every replicated variable
on welcome, a created one and every change after that tick's `post_tick`. `cvar.changed` (`name`,
`old`, `value`) fires on the server for every logged change (before and after `tick.post`, old =
the previous string) and on the client for every difference against the last replicated set, so a
joining client sees the whole set as changes from `""`.

Measured (`cargo test -p gm-dedicated --test addon_hooks cvars_`): a server actor creating
`myaddon_speed` 10 (replicated, 0..100) and a hidden `myaddon_secret`, then on tick 5 setting
250, 100, an unknown name and creating `myaddon_speed` again, logs exactly `10>100`; the client's
`Command::Cvar sbox_maxprops 50` sets `limits.max_props` 50 and logs `200>50`; the client reads
`myaddon_speed` "100", no `myaddon_secret`, `sbox_maxprops` 50, `sbox_playershurtplayers` true
and its change list is the welcome set, the creation, `10>100`, `200>50`.

## Storage

An addon with the `storage` capability gets one persistent key/value store per realm, shared by
its actors and kept across reloads and restarts (it lives in the runtime, not the VM):
`storage.get(key)` (nil when missing), `storage.set(key, value)` (any `net`-sendable value, nil
removes), `storage.keys()`. Keys are 1..64 bytes, values the `netval` form (64 KiB each), the
whole store 1 MiB per addon; a write past the cap or without the capability is an error in the
caller. The store is written to `<data_dir>/<realm>/<addon>.gms` (`GMS1`, count, key/value
records; temp file + rename) when dirty every 67 ticks (~1 s), on `flush_storage` and when the
runtime drops; `gm-dedicated --data` (default `data`) sets the directory, without one the store is
memory only. A missing file starts empty, a corrupt one too (logged).

Measured (`cargo test -p gm-script storage`): an actor bumping `count`, writing a nested table,
setting and removing a key and then 20 writes of 60 000 bytes gets 17 of them before the cap
errors; an actor without the capability gets `no storage capability`; after `flush_storage` a new
runtime reads count 1 and bumps to 2, the nested table and the removed key as nil; the drop flush
leaves count 2 on disk.

## File watching

With `Runtime::watch` (`gm-dedicated --watch`) the runtime fingerprints every loaded addon
(lengths and modification times of all files under its directory, or of the `.gmr`) every 33
ticks (~0.5 s; on the client at every 33rd snapshot) and hot reloads the ones that changed, the
same reload as the admin `Reload` command: actors keep their ids and `state`, new actors are
added, removed ones dropped. A reload that fails (syntax error, bad manifest) is logged as
`reload <id> failed: ...`, the actors keep running on their old sources and the file is not
retried until it changes again. The server then fires `addon.reloaded` (`addon`, `version`,
`error` = "" on success) as a serial event. No inotify dependency: polling a small tree is
cheaper than a watcher thread and works the same on every platform and over network mounts.

Measured (`cargo test -p gm-script file_watch`): an unchanged tree reloads nothing; appending
`state.marker = 42` to the example addon's source reloads it once and the actor sees the marker;
a broken edit reports an error, leaves marker 42 running and is not retried.

## Timers

Measured on srcds (`script_timers`: the Think and Tick hooks and every timer fire logged with
the tick, 40 ticks, golden `script_timers.json`): the game runs its timers after the `Think`
hook and before the `Tick` hook of the same tick; a timer created in `Think` with delay 0 fires in
that same tick, a repeating one with delay 0 fires once every tick; due times accumulate (next due
= last due + delay, not now + delay), so a 0.022 s repeater at 66.67 Hz fires on ticks 2, 3, 5
and a 0.052 s one on 4, 7, 11, 14, 18; `Adjust(name, 0.07)` right after `Create` fires at tick 5
(the new delay from the creation); `Pause` keeps the remaining time (paused at 0.02, unpaused in
tick 10's `Tick`, fires at 12); `Remove` in `Tick` stops the next fire; a callback that errors
prints `Timer Failed! [name]` and the timer is gone (`timer.Exists` false, no further fires).
Within a tick the one shot (`Simple`) timers fire first, in creation order, then the named ones
(observed in name order: `bad`, `rep0`, `rep20`; `adj`, `rep50`). Delays that are exact
multiples of the tick are a coin toss in the game: the due time is a double sum of delays while
`CurTime` is a float32 product of the tick count, so 0.015, 0.03 and 0.06 fired on time in one
run while 0.045 and 3 x 0.05 fired a tick late; our rule fires them on time and the harness
avoids such delays.

Ours (prelude `timer`, `cargo test -p gm-script timers`): due times in fractional ticks
(`delay / game.interval()`), a per-timer tick counter that starts counting the tick after the
one the timer was created in (so a 0 delay fires the same tick and 0.015 the next), the run after
the `tick.pre` handlers and before the server tick, Simple-first-then-name order, `Create` of an
existing name replaces it, `reps` 0 forever, `Exists`, `Remove` (`Destroy`), `Pause`, `UnPause`,
`Toggle`, `Start`, `Adjust`, `TimeLeft`, `RepsLeft`; a callback error prints through the actor's
queue and destroys only that timer. `script_timers` matches the game's 99 events exactly.

## Hook order around a spawn

Measured on srcds (`script_hooks`: a bot's `gm_spawn`, `gmod_undo`, `gm_spawn`, `gmod_cleanup`
run from inside `Think` on ticks 5, 15, 25, 30, every hook logged with its tick): the whole spawn
chain is synchronous inside that `Think`: `PlayerSpawnProp` (cancelable), `OnEntityCreated`,
`PlayerSpawnedProp`, then the tick's `Tick`; `EntityRemoved` for the undone and the cleaned up
prop also fires inside the same `Think` that ran the command (the removal is not deferred to the
next frame for `Remove()` from Lua), and the table is empty at the end. Ours: a command executed
in the Think phase runs `prop.spawn_request`, `entity.created`, `entity.spawned` serially in
place, `entity.removed` (new, `ServerHooks::entity_removed`: the net layer reports every
`Event::Removed` of an undo or cleanup) in the same phase; the 90 events match the game's.


## Cvar change callbacks

Measured on srcds (`script_cvar_callbacks`, `cvars.AddChangeCallback` logged with the tick):
a variable set by code (`ConVar:SetInt` in `Think`) fires its callback right there, before that
tick's `Tick`; a console command (`RunConsoleCommand`, which is also the path of a client's
`Command::Cvar`) is executed at the end of the frame, after `Tick`, and the callback fires then,
still under the same tick count; setting the same value again fires nothing. Ours: `cvar.changed`
is dispatched before and after `tick.post` (code and script sets) and once more from
`ServerHooks::after_commands`, which the net layer calls after it applied the tick's player
commands; the 66 events match the game's.

## Sandbox rules in Luau

`addons/examples/sandbox_rules` expresses the prop limit with the API alone: on
`prop.spawn_request` it counts the player's live props (`world.find()` filtered by `owner`) and
refuses once `count > sbox_maxprops - 1` (`cvar.get_number`, negative = unlimited), the game's
`Player:CheckLimit("props")`. Measured (`cargo test -p gm-dedicated --test addon_hooks
sandbox_rules`): with `sbox_maxprops` 3 and five spawn attempts through the real server the
addon refuses the 4th and 5th (2 hits, last on tick 5), three props exist, and the Rust sandbox
refuses the same 4th spawn when asked directly. This is the first piece of the sandbox gamemode
moving into Luau; undo and cleanup stay in Rust until `entity.removed` consumers need them.

Undo from an actor: `world.queue("undo.push", player, { name = "Contraption", text = ?,
entities = { ... } })` is the game's `undo.Create` / `AddEntity` / `Finish` for that player: the
entities become one undo entry (text defaults to `#tool.<name>.name`), are added to the player's
`props` cleanup list and count against the player's limits; rejected when the player is unknown,
the list is empty or an entity is gone. `gmod_undo` then removes them and fires `entity.removed`
for each (test `actor_owned_undo_entry_is_undone_by_the_player`: a weld plus one entry over both
crates, two bad pushes rejected, the undo empties props and constraints).

Player-owned constraints from an actor: `weld` and `nocollide` take `owner = <player>`; the
constraint then goes through that player's `sbox_maxconstraints` (`LimitHit constraints` event
and rejection when full), gets its own undo entry (`#tool.weld.name` / `#tool.nocollide.name`)
and joins the player's `constraints` cleanup list, exactly what the tool does; `owner` 0 or
absent keeps the server-owned behaviour. Unknown owner: rejected. Test
`actor_constraints_with_an_owner_follow_the_player_limits_and_undo`.

Tool op `remove` (`{ op = "remove", ent = ? }`, 0 or absent = the hit entity): the remover
stool in Luau (`addons/examples/remover_lua`): the prop leaves the table, `Event::Removed` is
recorded so `entity.removed` reaches the actors, no undo entry (the game's remover makes none);
the world or a gone entity is a no-op. Test
`remover_tool_in_luau_removes_the_hit_prop_and_fires_entity_removed`.

Distribution to clients: a package whose manifest lists the `client` realm is published by the
dedicated server and downloaded by clients by sha256 (see net.md, "Addon distribution"); the
client loads it from its cache with `Runtime::client().load_addon(<sha>.gmr)`, so only the
client-realm (and `shared`) actors run there; a hot reload on a `--watch` server republishes the
package and the client reloads it with `Runtime::reload_addon` (actor `state` survives).

## Dependencies

`depends = ["base", "wire >= 1.2, < 2", "props ^0.3"]` in the manifest: an id and comma-separated
constraints on its `major.minor.patch` version (`=`, `>`, `>=`, `<`, `<=`, `^` same major and at
least, `~` same major.minor and at least; a bare id takes any version; missing parts are 0).
`Runtime::load_addons(paths)` (what `gm-dedicated --addon ...` uses for directories and `.gmr`)
reads every manifest first, orders the set so a dependency loads before what needs it (the given
order otherwise, a cycle is an error) and loads nothing when a dependency is missing or has the
wrong version; actors of equal priority run in load order, so a dependency's actors run first.
`load_addon` of a single addon checks against what is loaded; a hot reload is refused (old actors
keep running) when the new manifest's own dependencies fail or a loaded dependent no longer
accepts the new version. `gmr inspect` prints the dependencies.

## Signing

A `signature` entry (file in the directory, entry in the `.gmr`) holds lines
`gmr-sig1 <ed25519 public key hex> <signature hex>`, each a signature of the content hash: SHA-256
over every other entry as `len(name) name len(data) data` in name order, so the signature made on
the directory holds for the package packed from it and the package sha256 (its download identity)
changes with every signer while the content hash does not. `gmr keygen <key>` writes a secret key,
`gmr sign <dir|.gmr> <key>` appends a signature (several signers are fine), `gmr verify` lists the
signers. A runtime with `trusted` keys (`gm-dedicated --trust <public key hex>`, repeatable) loads
only addons carrying a valid signature by one of them (`load_addons` refuses the whole set
otherwise); without a trust list anything loads, but a signature that does not match the content
is refused always. Clients trust the server they joined: what it publishes, they run.

## SDK: `gmr`

`gmr new addon <dir>` writes a manifest with a server and a client actor (an `entity.spawned`
counter answering a `net.message` from the client's first snapshot), `gmr new tool <dir>` a
scripted stool (weld two props, right click clears); the directory name is the id. Then `gmr
pack / inspect / keygen / sign / verify / publish / resolve` as below, and `gm-dedicated --addon
<dir> --watch` is the dev loop (`gmr_new_pack_sign_publish_resolve` runs the whole chain).

## Registry

A registry is a directory any static host can serve: `index.toml` with one `[[packages]]` entry
per published version (id, version, sha256 of the `.gmr`, size, depends, realms, signers) and the
packages in `blobs/<sha256>.gmr`. `gmr publish <addon> <registry>` takes signed packages only and
a version is immutable (the same id and version again must be the same bytes). `gmr resolve
<registry> wire@^1.2 hud` and `gm-dedicated --registry <dir> --addon wire@^1.2` pick, for every
id wanted directly or through `depends`, the highest version satisfying every requirement placed
on it (two requirements with no common version are an error), return the set in dependency order
and install the blobs into `<data>/registry/<sha>.gmr` after checking the sha256 and the
signatures; the server then loads them like any package (`--trust` still applies). A client with
`gm-bot --registry <dir>` takes a package the server announces from the registry by sha256 before
asking the server for the bytes. `--registry http://host/path` is the same layout fetched over
plain HTTP/1.1 with the standard library (`Content-Length` or chunked, 64 MiB cap, no redirects):
integrity comes from the sha256 and the signatures, so TLS adds privacy, not correctness, and
waits for a real client; a published package is capped at 50 MiB.

## Not yet

client HUD/draw API, client-side world for traces, the rest of the API ( tool ghosts and panels, spawn menu), WASM components,
the sandbox gamemode in Luau on top of this API.

## Players: identity and leaving

`world.player(id).identity` is the hex of the authenticated ed25519 public key or `nil` for an
anonymous connection (the stable id for bans, ownership and storage keys). `player.disconnected`
(`{ player }`) fires after the player left the server table through a timeout, a `Disconnect`, a
kick or a ban; the player's props and constraints stay and keep their owner id.
