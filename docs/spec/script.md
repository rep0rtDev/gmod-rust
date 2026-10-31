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
follow. `ctx.settings` carries the clicking client's values for the tool (strings; the client's
console `tool <name> <key> <value>` stores them per tool and sends them with every
`Command::ToolClick` of that tool, at most 255 pairs), the stool applies its own defaults when a key
is missing; measured (`addon_hooks tool_settings`): a weld-to-world stool reading `forcelimit`
makes the constraint with 64 from the click's setting and with its default 10 without.
`addons/examples/weld_lua` holds `weld_lua`, `rope_lua` and `axis_lua` written this way; the
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
are dropped. `phys.trace` on the client traces the client's own collision world (brushes,
displacements) and the PHY hulls of the props in the snapshot (`Runtime::set_client_world(world,
hull_loader)`; the native client passes its map and `model_info`, `gm-bot` has no world so its
traces miss), the same `trace_line` as the server's. Tests `client_realm_sees_the_assembled_world`,
`a_client_actor_traces_the_clients_world_and_the_props_it_knows` (the spawn floor at -143.969
with `ent` 0, a crate's hull by its id, a trace ending in the air misses).

Input: the native client hands its key and mouse button edges to the client actors as
`input.key {key, down}` (`Runtime::client_input`, parallel and read-only like `tick.client`),
`key` the physical key name (`KeyE`, `Digit1`, `Space`, `ShiftLeft`, `MouseLeft` / `MouseRight`
/ `MouseMiddle`); key repeats, synthetic focus replays and keys typed into the console or a
panel field are not events. This is the client's own naming, not the game's `KEY_*` enum.

HUD: every frame the client dispatches `render.hud` with `{w, h}` (the screen in pixels);
`hud.register(name, fn, priority?)` is `events.on("render.hud", ...)`. Handlers draw with
`draw.rect(x, y, w, h, color?)`, `draw.text(x, y, text, color?, font?)` and
`draw.texture(x, y, w, h, material, color?)` (screen pixels from the top left; `color` is
`{r, g, b, a?}` in 0..255 like the game's `Color`, opaque white when omitted); the calls are
collected after the handlers returned (`Runtime::client_render`), actor order then call order,
and drawn by the client over its own HUD.

Fonts: `draw.font(name, { family, size })` is `surface.CreateFont`: the client rasterises the
TrueType `family` of the mounted content (`resource/fonts/Roboto-*.ttf` as `Roboto`, `Roboto
Bold`, `Roboto Condensed`...; the HL2 icon fonts `resource/HALFLIFE2.ttf` as `HalfLife2`; spaces,
hyphens and case do not matter) at `size` pixels into its own glyph atlas (Latin-1 and Cyrillic,
sizes 4..256) under `name`; `draw.text` with that name draws with it, an unknown name or none is
the client's default (Roboto 16). The client reports every rasterised font's metrics to the
runtime, so `draw.text_size(text, font?) -> w, h` (`surface.GetTextSize`) answers inside the
handler from the real advances; a headless runtime without a client answers 0, 0. Declare fonts
at load, not per frame: the command reaches the client with the first frame and the atlas is
built once (a name declared again is rebuilt). Not measured: the game's `size` is vgui's "tall"
and ours is the TrueType pixel size; the two are close but no recording compares them yet.

Textures: `draw.texture` takes a material (`vgui/hud/icon_locked`, its `$basetexture`) or a
texture path, decodes the VTF once on the client (at most 1024 px a side) and stretches it over
the rectangle tinted by `color`; a missing one draws nothing. Measured (`addon_hooks
render_hud`): a font declared at load plus two handlers on an 800x600 screen give, in order, the
font command, a red half-transparent bar at y 580, `tick 42` over it, `w 40 h 24` in the font
(from stand-in metrics of 10 px a glyph), the 64x64 icon, a 3D2D block with `on the wall` and a
white 3x4 rect; the next frame has the draws again and no font command.

3D2D: `draw.start3d2d(pos, angles, scale)` ... `draw.end3d2d()` is `cam.Start3D2D`: the rect,
text and texture calls between them are pixels on a plane in the world, x along the angles'
forward, y along their right, `scale` world units per pixel, the plane's normal the angles' up
(`gm_render::ui::plane_transform`, Source's `AngleMatrix` axes). The client draws the blocks
after the world and before the HUD, depth-tested against the scene without writing depth, in
call order, alpha-blended. Not measured: the game's exact axis mapping and whether its 3D2D
writes depth; the ordering with the view model is ours (3D2D under it).

The native client (`gm-client`) hosts both realms: `--addon <dir|file.gmr>` (repeatable) loads
addons into its in-process server, whose client-realm packages travel over the loopback
exactly as from a dedicated server and are verified, cached (`GMOD_RUST_CACHE`, default
`cache/`) and loaded into the client realm (`Runtime::client()`), with `net`, `snapshot.received`,
`tick.client`, `cvar.changed` and `render.hud`. `gm-bot --addon` stays the headless way.

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

Prop state ops on `world.queue`, the same paths as the context-menu properties: `set_skin`
(number; refused past the model's skin families), `set_body` (`{part, model}`, one `SetBodyGroups`
digit; refused when the model has no such part or model), `set_gravity` / `set_collide` /
`keep_upright` (boolean), `ignite` (seconds, default the property's 360) / `extinguish` (refused
when not burning), `set_health` (number, the prop's `prop_data` health).

`world.queue("spawn", 0, {model, pos, angles?, frozen?, owner?, tag?})` is `ents.Create
("prop_physics")` + `SetModel` + `Spawn` at a pose: `owner` 0 (default) is the server's prop (no
limits, undo or cleanup, like script constraints); a player id goes through `sbox_maxprops` and
that player's cleanup list like a duplicator paste, with no undo entry (the script pushes one
with `undo.push` if it wants); an unknown model, player or a hit limit is rejected. The prop
exists after the write phase; the actors hear `entity.spawned {player = owner, entity, tag}`
with the game events of that tick (not `entity.created`, which follows a player's spawn).

Measured (`cargo test -p gm-dedicated --test addon_hooks script_spawns`): a server spawn
(frozen, yaw 90) and a player-owned one land where asked and count for the player; a spawn for
player 7 and of an unknown model, a skin past the crate's two families, a body part the crate
does not have and an extinguish of a missing entity are rejected; skin, gravity, collisions,
keep-upright, fire and health show on the prop before the next tick; both spawned events carry
their tags.

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

`addons/examples/prop_protection` is what the game does not have (in the vanilla sandbox anyone
physguns, tools or edits anyone's prop; `GM:PlayerSpawnedProp` only records the owner for undo,
cleanup and the limits): on `physgun.pickup_request`, `tool.before` (`e.hit.ent`),
`player.property_request` and `player.pickup_request` it looks the entity up with
`world.get(id)` and returns `false` when its `owner` is another player (0, the world or the
server, is left alone), counting the blocks per player and sending them on `player.spawned`.
Measured (`cargo test -p gm-dedicated --test addon_hooks prop_protection`): two players through
the real server, the second one's physgun (held 5 ticks: 5 asks, the beam retries every tick
while it holds nothing), +use (1: the pickup is asked on the press only) and `remove` (1) on the
first one's crate are refused, 7 blocks, nothing held, the crate stays; the owner's own physgun
takes it.

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

Scripted entities' touches with players and while both bodies sleep; the weapon selection
menu (attack to confirm, `hud_fastswitch`, `invnext`) and the game's `KEY_*` names / `input.IsKeyDown`
polling (ours: `input.key` events with the client's key names); the game's pitch-0 miss of +use
on players (see "+use"); net message data for WASM guests. Done since this list was written: the
sandbox gamemode as one package (`addons/gamemodes/sandbox`), prop properties, player models,
the +use pickup, the spawn menu lists, prop spawn and state writes, scripted weapons, key
events, +use on players, WASM components, scripted entity classes, weapon slot digits, the
weapon `think`.

## Server events

`gm_server::GameEvent`s of a tick reach the actors after the tick and before `tick.post`, in the
order the server raised them: `player.key_press {player, key}` / `player.key_release {player, key}` (`KeyPress` / `KeyRelease`:
a usercmd button bit changed, `key` its `IN_*` name such as `IN_USE`, `IN_JUMP`, `IN_SPEED`;
raised first in the tick, before what the press does: measured `lua_use_hold`, `KeyPress`
precedes the tick's `PlayerUse`; a held key is one press and one release), `player.spawned {player}` (a respawn; the first spawn happens
before any addon sees the player), `player.damaged {player, amount, type}` (health damage
actually applied, after the armour share, fall damage included; `type` is the damage type bits),
`player.death {player, type}` (`type` 0 for a scripted `player.kill`), `physgun.pickup`,
`physgun.drop`, `physgun.freeze` (`{player, entity}`; a freeze is not followed by a drop, the
prop simply leaves the hand frozen). Writes queued in these handlers are applied in the same
serial phase.

Cancelable requests are asked synchronously inside the tick through the server's
`request_hook` (`gm_script::attach` installs the runtime as both the network hooks and that
hook): `player.damage {player, amount, type}` before the armour share, a handler returning
`false` cancels the damage, a number is dealt instead (the last number in priority order wins;
`player.damaged` then reports what was applied); `physgun.pickup_request {player, entity}`
when the physgun's trace finds an unfrozen prop with attack held, `false` keeps it out of the
hand (asked again every tick the button stays down). A request raised by a write of a running
handler (a `player.damage` queued from inside an event) is not asked again: handlers are never
re-entered, the write applies as queued.

`entity.collide {entity, other, pos, speed}` fires for every prop impact the physics shim
records (the capture behind impact sounds: the first approaching vertex or face contact of a pair
in a step, no speed cut), once per prop involved with `other` the second prop or 0 for the
world, `pos` the contact point and `speed` the approach speed in u/s. A prop set down from the
physgun onto the floor it already rests on records nothing; the measured series
(`phys_sound_drop_*`) says which drops the game voices. Measured (`addon_hooks game_events`): a
crate teleported 100 u up collides with the world at the floor height with a positive speed.

`tool.before {player, tool, right, hit}` runs for a tool gun click whose trace hit something
(`hit` as in scripted stools: `ent` or 0, `pos`, `normal`), before the stool: `false` cancels the
click (no stage change, no constraint, no shot event); `tool.after {player, tool, right, done}`
follows a click that ran, `done` the stool's return value (a Rust or a scripted stool). A click
that hits nothing raises neither. `player.noclip {player, on}` fires when `Command::Noclip`
(the client's V key or `noclip` verb) changed the move type; the server refuses it (`Noclip
denied` event, no change) when `sbox_noclip` is 0 or the player is dead. Measured
(`addon_hooks tool_before`): a weld right click on a crate (weld to the world) vetoed leaves no
constraint and no `tool.after`, the left click goes through (`after weld 0 1`, stage 1), noclip
on logs `noclip 1 1`, the same under `sbox_noclip 0` is denied.

`player.chat {player, text}` runs when a client's `Command::Chat` arrives (the line is cut at 127
characters as the game's `say` is, blank lines are dropped before asking): `false` drops the
line, a string replaces it (later actors see the replacement in `text`), anything else passes it
on; every welcomed client then gets `<name>: <text>` as an event and the server log
`chat <name>: <text>`. Measured (`addon_hooks player_chat`): an actor upper-casing player 1 and
refusing "bad" gives both clients `alice: HELLO THERE` and `bob:   hi  ` only.

`world.queue("sound.play", 0, {name, pos = {x, y, z}, volume?})` plays a sound script at a
world position for every client (the same channel as prop impacts and footsteps; `volume`
multiplies the script's, default 1). Sounds ride with the snapshots: on a tick without one they
stay pending, so none is lost.

Measured (`cargo test -p gm-dedicated --test addon_hooks game_events`): the first pick-up is
refused by `physgun.pickup_request`, the next tick's goes through; pick-up, drop, pick-up and
freeze of a crate log four events in that order; 30 damage dealt by the server arrives at the
`player.damage` handler, which halves it, and `player.damaged` reports 15; the `player.kill`
queued from `player.damaged` shows as `player.death` type 0, the respawn 135 ticks later as
`player.spawned`; the freeze handler's `sound.play` reaches the client as a `Sounds` message
with the position and volume given.

## Scripted weapons

`weapon.register(class, def)` is `weapons.Register` of a SWEP: `def` carries `print_name`,
`slot`, `slot_pos`, `hold_type`, `view_model`, `world_model`, `clip_size` (-1 none),
`default_clip`, `ammo`, `automatic`, `primary_delay`, `secondary_delay`, `reload_time` (defaults
pistol-like: slot 1, `c_pistol`/`w_pistol`, no clip, `Pistol`, 0.2 / 0.5 / 1.4333 s) and the
callbacks `primary`, `secondary`, `reload`, `deploy`, `holster`, `think`, each given the event payload
`{player, weapon, clip, reserve, eye, forward, hit, other}` (`hit` the eye trace on the attacks,
`other` the class switched to on a holster). The registration is a write (`world-write`), sent
to every client whole as `ServerMsg::WeaponDefs` whenever the set changes; the wire indexes
weapons as `LOADOUT` then the scripted classes in registration order (`PlayerSnap.weapons` is a
64-bit mask), so `world.player(id).weapons` names them like the stock ones. `player.give_weapon`
of a scripted class fills the clip with `default_clip` and adds no reserve (measured). From
`primary`, `world.queue("fire_bullet", player, {damage, pellets?, spread?})` fires hitscan
bullets from the player's eyes through the stock hit path (hit groups, prop damage, impulse);
sounds and effects are the script's (`sound.play`).

Measured on `weapon_base` with a bot (`lua_swep`, golden 261031) and implemented the same:
`Give` deploys at once (`Deploy` before any other call), a deploy blocks the primary for 0.2 s;
a semi-automatic primary fires on a press edge only and a press while blocked is lost for the
whole hold (a 60-tick hold right after the deploy fired nothing); taps 10 ticks apart with a
0.2 s delay fire on the 1st and 3rd; an automatic primary fires on the press and then every 14
ticks (0.2 s from the firing tick, 13.33 ticks rounded up); the secondary fires once per press;
`IN_RELOAD` starts a reload that blocks the primary for `reload_time` (`DefaultReload` on
`c_pistol`: 1.4333 s) and moves rounds from the reserve into the clip only at its end
(switching away before leaves the clip as it was); an empty clip plus an attack press starts the
reload instead of firing. Differences: the game also calls `PrimaryAttack` (with
`CanPrimaryAttack` false) and `Reload` twice in that empty-clip tick, we call `reload` once. The
callbacks run with the tick's game events, so a `fire_bullet` lands one tick after the trigger
pull.

`think` (`WEAPON:Think`), measured (`lua_swep_think`, golden 261031, every call logged with the
engine tick): once per tick while the weapon is active, in the tick itself, unchanged while the
attack is held; not during the deploy block: `Give` + select at tick 0, the first `Think` at tick
14 (the 0.2 s deploy delay), then every tick. The 147 calls in 200 ticks of the older recording
were the deploy gaps of its weapon switches, not a cadence. Ours: the same (`deploy_left` on the
weapon state; test `scripted_weapon_think_runs_every_tick_after_the_deploy`).

Measured (`cargo test -p gm-dedicated --test addon_hooks scripted_weapons`): the `lua_swep`
button script against a semi and an automatic weapon gives the recorded call sequence (deploy;
three primaries at 80/100/120; one secondary; a reload; holster + deploy on the switch; five
automatic primaries 14 ticks apart; a reload on the empty press) with the recorded clips.

## Scripted entity classes

`entity.register(class, { model, think_interval?, use_type?, init, think, start_touch, touch,
end_touch, use, use_off, remove, damage })` is `scripted_ents.Register` of a `base_anim` with physics: an instance is a sandbox
prop with the class's model (`world.queue("spawn", owner, { class = ..., pos, angles?, frozen?,
tag? })`, same limits and `entity.spawned` as a prop spawn) whose callbacks reach the script by
class, each with `{entity, class, player, other, amount}`:

- `init` in the spawn tick (the game's `Initialize` runs inside `Spawn`; `Activate` adds nothing);
- `think` first 7 ticks after the spawn, then every `round(think_interval / tick)` ticks
  (`think_interval = 0.1` is every 7, `0.5` every 33) or every 13 ticks (about 0.2 s) when it is
  not given, the engine's
  default cadence when the think does not reschedule itself. A `think` that returns a number
  of seconds is `self:NextThink(CurTime() + seconds) return true`: the next think moves to that
  time (rounded to ticks, at least the next tick), as does `world.queue("next_think", entity,
  seconds)` from anywhere; the time is counted from the think's own tick when the write comes
  from its callback (the game's `CurTime()` there), from the current tick otherwise;
- `use` with `player` and `amount = 1` (`USE_ON`): `use_type = "continuous"` (default,
  `CONTINUOUS_USE`) every tick E is held on it, exactly one per tick and no lag tick; `"simple"`
  (`SIMPLE_USE`) once per press; `"onoff"` (`ONOFF_USE`) on the press and `use_off` (`USE_OFF`,
  `amount = 0`) on the release. `player.use` / `player.used` still run first (`PlayerUse`), a
  scripted entity is never picked up by E;
- `start_touch` / `touch` / `end_touch` with `other` (a prop id, 0 the world) from the physics
  step's contact pairs: a new pair is a `start_touch` and a `touch`, a pair still in contact a
  `touch` every tick, a separated pair an `end_touch`. Differences from the game: its three
  `Touch` in the impact tick are one here, and Jolt reports a resting pair only while one of the
  bodies is awake (a crate asleep on a frozen scripted entity stops touching it; the game keeps
  calling `Touch`); players do not touch yet (only props and the world have bodies here);
- `damage` with `amount` on every damage the prop takes (`OnTakeDamage`), before health is applied;
- `remove` in the tick the prop goes (`OnRemove`), whatever removes it (property, undo, cleanup,
  break, script).

Measured (`lua_sent`, golden 261031): four `parity_sent` entities on srcds; `pre-Spawn,
Initialize, post-Spawn, post-Activate` in one tick, Think ticks `-23, -10, 3, 16` (every 13)
for the default, every 7 with `NextThink(CurTime() + 0.1)`, every 33 with `+ 0.5`; `Use` 5 times
for 5 held ticks (continuous), once (simple), `USE_ON` then `USE_OFF` on the release (onoff);
`OnRemove` in the `Remove` tick, `OnTakeDamage(5)` at once; `StartTouch` on the first contact,
`Touch` three times in the impact tick then every touching tick, `EndTouch` when the pair
separates (two ticks later on the bounce), `PhysicsCollide` twice per impact with two speeds
(ours is `entity.collide`, once).

Test (`addon_hooks scripted_entities_follow_the_measured_sent_cadences`): two classes spawned
in the first tick, `init` in it, thinks 7 ticks later then every 13 (default), every 7
(`think_interval = 0.1`) and every 20 (a think returning 0.3), five
`use` for five held ticks, `damage` 5 and `remove` in their ticks; a crate dropped on the
second starts with `start_touch`, touches every tick and never ends a touch it did not start.

## Weapon slots

Measured (`lua_slots`, golden 261031): the sandbox loadout's `GetSlot()` / `GetSlotPos()` are
slot 0 crowbar 0, physcannon 2, physgun 5, tool gun 100; slot 1 pistol 0, .357 1; slot 2 SMG 0,
AR2 1; slot 3 shotgun 0, crossbow 1; slot 4 grenade 0, RPG 1; slot 5 camera 100; the spawn's
active weapon is the crowbar. `slot1`..`slot6`, `invnext`, `invprev` and `lastinv` as a bot's
console commands change nothing on the server: the selection is the client's menu and the
server only sees the final switch, so the cycling itself is not measurable there.

Ours (`gm_server::SLOTS`, `slot_order`): the digit keys 1..6 are the slot categories; a press
takes the first owned weapon of the slot in `slot_pos` order, or the one after the current
weapon when it is in that slot (wrapping); scripted weapons join by their `slot` / `slot_pos`
(ties in class list order). Not modelled: the game's selection menu with its attack-to-confirm
and `hud_fastswitch` (ours applies at once), the mouse wheel / `invnext`. Every loadout weapon
has a client viewmodel (`c_physcannon` with `hold_idle` / `altfire`, `c_grenade` with `throw` /
`lob`); the game's camera has no weapon model, so nothing is drawn for it, as in the game.

## WASM components

An actor can be a WASM module instead of Luau handlers: `kind = "wasm"` in its manifest entry,
`source` a `.wasm` (or `.wat`) under the addon's `wasm/` (packed by `gmr pack`). The guest runs
on wasmtime with a flat ABI, no component model: it exports `memory`, `gm_alloc(len) -> ptr` and
`gm_event(name_ptr, name_len, json_ptr, json_len) -> i32` (0 pass, 1 cancel, the same answer a
Luau handler gives with `return false`), and imports `gm.queue(op_ptr, op_len, ent, json_ptr,
json_len)` and `gm.log(ptr, len)`. Every event the Luau actors get (`tick.pre`, requests, game
events, net messages) reaches `gm_event` with the payload as a JSON object (numbers, booleans,
strings, vectors as `{x, y, z}`, `hit` as `{ent, pos, normal}`, `settings` as an object, a net
message's `name`; its binary data is not passed yet). `gm.queue` is `world.queue(op, ent, arg)`
with `arg` as JSON: the writes go through the actor's own Lua `world.queue`, so the parsing, the
`world-write` capability and the per-run cap are the same, and they land in the serial phase
with everyone else's. `gm.log` is `print`.

Limits like a Luau actor's: the store's memory limit is the actor's (256 MiB default), the CPU
budget becomes wasmtime fuel per dispatch (`cpu_ms * 2,000,000`); running out traps, counts an
overrun and throttles the same way; any trap restarts the actor (a fresh instance; there is no
`state` carried over for a guest). The Lua VM of a WASM actor still exists (`state`, `eval` in
tests, the write parser) but gets no events.

Measured (`cargo test -p gm-script --test actors a_wasm_component`): a hand-written `.wat`
guest counts three `tick.pre`, queues `freeze` of entity 7 with `true` on the third, logs a
line and cancels `prop.spawn_request`; the queue holds exactly `Freeze { ent: 7, frozen: true }`
and the request dispatch answers `Some(false)`.

## Sandbox gamemode package

`addons/gamemodes/sandbox` is the sandbox gamemode's Luau side as one package (`gm-dedicated
--addon addons/gamemodes/sandbox`, `gm-client --addon ...` for the embedded game): `limits`
(the `sbox_maxprops` rule of `examples/sandbox_rules`) and `rules` (`sbox_godmode` 1 cancels
every `player.damage`, the game's `PlayerShouldTakeDamage`; `sbox_weapons` 0 answers
`player.loadout` with `false` and gives `gmod_camera`, `gmod_tool`, `weapon_physgun`, physgun
out, the game's `PlayerLoadout`). The server itself keeps `sbox_noclip`,
`sbox_playershurtplayers`, `mp_falldamage` and the `sbox_max*` counts, so the package changes
nothing with the default cvars. Measured (`addon_hooks sandbox_gamemode_package`): godmode on
takes 30 damage to 0 and off deals it; `sbox_maxprops 1` refuses the second spawn; after a kill
with `sbox_weapons 0` the respawn holds exactly the three weapons with the physgun active.

## Sandbox gamemode hooks

Four more of the sandbox gamemode's decisions are asked synchronously in the tick (`return
false` cancels):

- `player.loadout {player}` (`PlayerLoadout`): on join and on every (re)spawn the inventory is
  emptied first, then the hook runs; `false` skips the default 13 weapons, and
  `world.queue("player.give_weapon", player, class)` from the handler gives the hook's own (any
  class the server knows is added to the player and selected; unknown classes are refused).
- `player.undo_request {player, name}` (`CanUndo`): before `gmod_undo` pops the player's last
  entry; `name` is the entry's (`prop_physics`, `#tool.weld.name`...). An empty stack asks nothing.
- `player.unfreeze_request {player, entity}` (`CanPlayerUnfreeze`): the physgun's reload
  (`IN_RELOAD` press, physgun out) now unfreezes every frozen prop the player owns, asking per
  prop; `player.unfroze {player, count}` follows with how many went. Note: the game unfreezes
  what the player froze; ours uses the owner, the freezer is not tracked.
- `player.fall_damage {player, speed, amount}` (`GetFallDamage`): the landing speed and the
  damage the server computed; a number replaces the amount, `false` cancels.

Measured (`addon_hooks sandbox_gamemode_hooks`): the first spawn gets 13 weapons; the vetoed
undo keeps the prop and the next removes it with `name == "prop_physics"`; the vetoed R press
keeps the crate frozen, the next unfreezes it (`count` 1); a 700 u drop lands above 526 u/s,
the first landing cancelled costs nothing, the second doubled costs `2 * amount`; after a
kill the respawn's loadout is refused and the player holds only the hook's `weapon_pistol`.

## Tool setting schemas

`tool.register(name, { settings = { { name, type, min, max, default }, ... }, left = ... })`
declares the stool's settings: `type` is `number` (default), `bool` or `text`; `min`/`max`
bound numbers (unbounded when absent); `default` is sent as a string (`false` → `0`, `10` →
`10`). The schema is a write at registration (`Server::tool_schemas`, `tool_schemas_version`
bumped when a tool's schema changes), sent whole and reliably as `ServerMsg::ToolSchemas` to
every client whose version differs, kept in `ClientWorld::tool_schemas`. The native client
builds the click's `ctx.settings` with `gm_net::tool_click_settings`: every schema key is
present (the player's value or the default), numbers are clamped to `min..=max` and fall back
to the default when unparsable, bools normalise `1/true/on` and `0/false/off`, keys the schema
does not know pass through, and `tool <name>` in the console lists the schema with the current
values. Measured (`addon_hooks setting_schema`): a three-setting schema reaches the client in
one version step; `forcelimit=5000` becomes `1000`, `nocollide=true` becomes `1`, `lots`
becomes the default `10`. The native client draws the panel from the schema: with the tool gun
out, the right third of the held-Q menu lists the selected stool's settings (`Tool: <name>`), a
number as a slider over `min..=max` (a press sets the value at the cursor and the knob follows
the cursor until the button is released, even outside the panel; whole numbers when the range
is wider than 20, else two decimals), a bool as a checkbox that flips on click, a text setting
as a field: a click gives it the keyboard, typed characters edit it (255 at most), Enter stores
it, Escape or a click elsewhere drops the edit, closing the menu drops both the edit and a
drag; values go into the same per-tool map the clicks send, so the next click carries them.
The console's `tool <name> <key> <value>` still works for all of them. `addons/examples/weld_lua` declares `forcelimit` (0..10000) and
`nocollide` and reads them in its weld op.

## Tool ghosts

A stool's client part declares `ghost = function(ctx)` in its `tool.register`. Every frame the
native client, with the tool gun out, traces from the eyes along the view over the client's
world and prop hulls (`Runtime::client_ghost`) and dispatches `render.ghost` with
`ctx.hit {ent, pos, normal}`, `ctx.tool` and `ctx.settings` (the same shaped values the click
would carry); the prelude calls the selected stool's `ghost(ctx)` and turns its
`{ model, pos?, angles?, alpha? }` (pos defaults to the hit, angles to zero, alpha 0..255 to
150) into `draw.model`, which any handler may also call directly. The client draws
`DrawCmd::Model` as a translucent model (alpha-blended, no depth write) after the opaque
models; no hit, no ghost. Measured (`addon_hooks ghost`): a stool returning a crate 12 u above
the floor hit under the spawn draws one model at z -131.969, yaw 90, alpha 0.4 (102/255); a
trace into the sky and a tool without `ghost` draw nothing.

## +use

Every command with `IN_USE` down (player alive) traces from the eyes along the view with the
tool gun's trace. The reach is the public
SDK's `FindUseEntity` test: `hit - eye` with the vertical part replaced by how far the hit is
above or below the player's hull, shorter than `PLAYER_USE_RADIUS` (80 u). Measured on srcds
(`use_crate_*`): a crate whose face is 79.6 u ahead is used, one at 89.6 u is not, whatever the
view pitch. A prop in reach asks `player.use {player, entity}` synchronously in the tick
(`return false` cancels) and, when allowed, raises `player.used {player, entity}` after the
tick, every tick the button stays down over an entity in reach. Measured on srcds
(`lua_use_hold`): a 40-tick hold on a crate gives 41 `PlayerUse` calls (one per tick plus the
game's one-command lag after the release), a 5-tick hold 6, and the hook keeps firing while the
player already carries the crate; ours fires once per held tick with no lag. The world and far
props are not asked.

A used prop that is light and small enough (mass at most 35, every hull extent at most 128,
`CanPickupObject(35, 128)`) then asks `player.pickup_request {player, entity}`
(`GM:AllowPlayerPickup`) on the press only (the edge: down this command, up in the last one),
not on the held ticks after it; allowed, the player carries it (`player.pickup {player, entity}`
after the tick) until the next +use press (measured: the press at tick 210 drops, the crate is
loose at 211), death, the prop's removal or a physgun taking it (`player.drop {player, entity}`).
A frozen crate is asked every tick and never picked up (measured, `EnableMotion(false)`). The carry itself is in `docs/spec/sandbox.md`, "+use pickup".
The native client sends `IN_USE` on E.
Use on players, recorded (`lua_use_player`, `lua_use_player2`, golden 261031): a second bot 60 u
ahead; looking level (pitch 0, eye 64 u up, the hull top at 72 u) the engine's use found nothing
(and `GetEyeTrace` passed through the player while an explicit `util.TraceLine` to its chest hit
it), locked or not, at 60 and 100 u; looking down 20 degrees onto its chest, E held 20 ticks
gave `PlayerUse(ply, player)` 21 times (every tick plus the command lag, like a prop), nothing
is picked up or moved, and `FindUseEntity`'s default argument was NULL in every one of its 42
calls. Ours: when the use trace finds no prop, the use ray (80 u from the eyes) is tested
against the other live players' hulls (16 x 16 x 72, 36 ducked) and the nearest hit raises
`player.use_player {player, target}` every tick E is held (not cancelable: nothing happens to
the target). Known difference: a hull test also hits at pitch 0, where the game found nothing;
why the game misses there is not understood (its own eye trace passed through the hull too).
Measured (`addon_hooks use_on_another_player`): five ticks of E at 20 degrees down onto a
player 60 u ahead give five events and no `player.use`.

## Player models

`player.set_model {player, model}` is `PlayerSetModel`, asked on every (re)spawn with the path
the player's choice translates to (`gm_server::player_models`, kleiner for an unknown name);
`return "<path>"` makes the player wear that instead.

## Props breaking

`entity.break {entity}` follows a prop whose `prop_data` health reached zero (fire, so far); the
prop is gone when it arrives.

## Properties

`player.property_request {player, entity, property}` is `CanProperty`, asked synchronously
when a player sends a context-menu property for the prop under the crosshair (`return false`
refuses); `player.property {player, entity, property}` follows after the tick when it was
applied. The properties and what they do are in `docs/spec/sandbox.md`, "Properties".

## Spawn menu

`spawnmenu.add(category, models)` (server realm, a write) registers models under a spawn menu
category for every client: a new category goes to the end, a repeated one gets the new models
appended with duplicates dropped, an empty model name is ignored. `Server::spawn_lists` holds
them with `spawn_lists_version`, which every change bumps; the net layer sends the whole set
reliably (`ServerMsg::SpawnLists`) to each client whose version differs, on join and after a
change, and the client keeps it in `ClientWorld::spawn_lists`. The native client's Q menu lists
the addons' categories first (`-- category --` header rows, not spawnable, then the models) and
the mounted content's props after them. Measured (`addon_hooks spawn_menu`): two categories
registered at load reach the client in order, a later `add` to the first appends one new model
and skips the repeated one, one version step per change.

## Players: identity and leaving

`world.player(id).identity` is the hex of the authenticated ed25519 public key or `nil` for an
anonymous connection (the stable id for bans, ownership and storage keys). `player.disconnected`
(`{ player }`) fires after the player left the server table through a timeout, a `Disconnect`, a
kick or a ban; the player's props and constraints stay and keep their owner id.
