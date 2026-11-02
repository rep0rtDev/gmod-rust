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
`ent2` 0 welds to the world), `nocollide` (`{ent2}`) and `set_overlay_text` (a string, `nil` / `""` clears;
scripted entities only: the world tip clients draw when aiming at the entity within 256 u, like
`ENT:SetOverlayText`; outside singleplayer the owner's name is appended on a second line). Script constraints are owned by the server
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

NPC ops on `world.queue`, `ent` being the NPC id (`npc.*` events carry it): `npc_goto`
(`{x, y, z, run?}`, the server's own navigation on its Recast mesh with the measured gaits, see
`docs/spec/ai.md`; rejected without a path, for a dead NPC or off the mesh), `npc_chase`
(`{player, run?}`: follow that player, re-pathing without halting, resting 64 u short; rejected
for an unknown player), `npc_stop`, and `npc_target` (`{player}`: engage that player under the
server's own sight and loss rules; no value: drop the target and keep the NPC passive until the
next `npc_target`; rejected for a dead NPC or unknown player). No game schedule or relationship
is involved.

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

## GLua surface

The documented Garry's Mod Lua API, reproduced function by function inside every actor VM on top
of the own `world` / `events` API (`crates/gm-script/src/glua/`). The catalogue is the wiki
(`https://wiki.facepunch.com/gmod/~pagelist`, every function page's `<function>` markup: parent,
realm, args, returns) in `tools/glua/manifest.json`: 5817 entries (5642 functions and hooks, 101
enums, 72 structures), each with a status. Statuses are not hand-written: `cargo run -p gm-script
--example glua_list` prints what a booted VM really provides (`lib.fn` from the global tables,
`Class:method` from the metatables registered in `__glua_classes`, `Global.fn`),
`tools/glua/build_manifest.py --present ... --stubs ...` writes them, and
`tests/glua_coverage.rs` fails when the manifest claims an address the VM lacks. `glua::stub`
registers a documented function that cannot work yet (wrong realm, no frontend): it exists,
raises "not available in this realm yet" when called, and is counted as `stub`, never
`implemented`. `docs/glua/coverage.md` is the generated summary per realm and per library /
class. Everything is written from the documented behaviour; no Garry's Mod Lua is copied.

Order of work: shared and server realms first (backend first), client / menu realm functions
(render, surface, vgui, the panels) are catalogued and arrive with the frontend.

Done in `glua/core.luau` (Luau, loaded at boot after the host API):

- `Vector` / `Angle` / `Color` as tables with metatables (`x y z`, `p y r`, `r g b a`; the same
  shape the host API's `{x, y, z}` tables use), the arithmetic metamethods, `tostring` in the
  game's `%f %f %f` / `r g b a` form, and every documented method: Vector 33 (`Angle` is
  Source's VectorAngles: +x is 0 0 0, straight up is pitch 270; `AngleEx`, `Rotate`,
  `WithinAABox`, `IsEqualTol`, ...; `ToScreen` is client), Angle 18 (`Forward / Right / Up`
  from the Source basis, `RotateAroundAxis` through Rodrigues on the basis vectors,
  `SnapTo`, `Normalize` to -180..180), Color 28 (`ToHSV / ToHSL / ToHWB / ToHex / Lerp`, the
  HSV/HSL/HWB getters and setters, `HSVToColor`, `HSLToColor`, `ColorToHSV`, `ColorToHSL`,
  `ColorAlpha`, `ColorRand`, `IsColor`).
- `math` (`Clamp`, `Round`, `Approach`, `ApproachAngle`, `AngleDifference`, `NormalizeAngle`,
  `Remap`, `EaseInOut`, the Bezier / Catmull-Rom / B-spline curves, `Dist`, `Rand`, `SnapTo`,
  `Truncate`, `IntToBin` / `BinToInt`, `tau`, `pow`, `mod`, the hyperbolic functions and
  `log10` Luau lacks), `Lerp`, `LerpVector`, `LerpAngle`.
- `string` (`Explode` / `Split` / `Implode`, `Replace` (plain), `Trim*`, `Left / Right / GetChar /
  SetChar`, `StartsWith / EndsWith`, `PatternSafe`, `Comma`, `NiceSize`, `NiceTime`,
  `FormattedTime` / `ToMinutesSeconds*`, `NiceName`, `CardinalToOrdinal`, the filename helpers,
  `ToColor / FromColor`, `JavascriptSafe`, `Interpolate`, `ToTable`, `gfind`), the string
  metatable indexes it so `s:Trim()` works.
- `table` (`Copy` deep with cycles, `Merge`, `Inherit`, `Count`, `HasValue`, `KeyFromValue(s)`,
  `GetKeys`, `Random` (value, key), `RemoveByValue`, `Reverse`, `Shuffle`, `IsSequential`,
  `ClearKeys`, `CollapseKeyValue`, `Flip`, `ForceInsert`, `LowerKeyNames`,
  `MemberValuesFromKey`, `GetWinningKey`, `FindNext / FindPrev`, `SortByKey / SortByMember /
  SortDesc`, `Sanitise / DeSanitise`, `ToString`, `Pack`, `getn / maxn / foreach(i)`).
- `bit` (LuaJIT semantics on Luau's `bit32`: results wrapped to signed 32-bit, shifts masked to
  5 bits, `tohex` with the negative-length upper-case rule), `utf8.GetChar / sub / force`.
- `hook`: `Add / Remove / Call / Run / GetTable` with handlers run in insertion order (a re-added
  id keeps its place), the first non-nil return stops the chain, table ids are passed as the
  first argument and dropped when `IsValid()` fails, then the gamemode's own method. The own
  event bus feeds `Think` and `Tick` (`tick.pre`) and `OnEntityCreated` (`entity.spawned`);
  the rest of the GM hooks arrive with their systems.
- `timer` is the host prelude's measured library (see Timers); the GLua surface adds
  `IsPaused` and `Check`, and the time globals `CurTime` / `RealTime` (tick x interval),
  `SysTime`, `FrameTime`.
- `FindMetaTable` over the registered classes.

Done in `glua/entity.luau` (over the snapshot and the write queue; the snapshot now carries
every entity's `class` (`prop_physics`, `prop_ragdoll`, the vehicle / world-weapon / NPC class),
`health`, `max_health` and `velocity`, and lists NPCs beside props):

- `Entity(id)` wrappers (one table per id, `==` by id, `NULL`, `IsValid` = present in the tick's
  snapshot): the reads (`GetPos / GetAngles / GetVelocity / GetClass / GetModel / GetOwner /
  GetSkin / GetBodygroup / Health / GetMaxHealth / IsNPC / IsVehicle / IsWeapon / IsRagdoll`,
  `GetForward / Right / Up`, `LocalToWorld / WorldToLocal`), the writes as queued ops
  (`SetPos`, `SetAngles`, `SetVelocity`, `SetHealth`, `Remove`, `SetSkin`, `SetBodygroup`,
  `Ignite / Extinguish`, `SetCollisionGroup / SetSolid` -> `set_collide`, `NextThink`), NW /
  NW2 vars and `GetTable` kept in the actor; `GetPhysicsObject` (`EnableMotion` -> `freeze`,
  `IsMotionEnabled`, `EnableGravity`, `EnableCollisions`, velocity / pose through the entity).
  `ents.Create(class)` returns a pending entity whose `SetModel / SetPos / SetAngles` are
  recorded and `Spawn()` queues the `spawn` op with a tag; the wrapper binds to its id when
  `entity.spawned` reports that tag (invalid until then - the game's `ents.Create` is
  synchronous, ours is a tick late by design of the serial phase).
- `NPC` (`SetEnemy / SetTarget` -> `npc_target`, `NavSetGoalPos / SetLastPosition` ->
  `npc_goto`, `NavSetGoalTarget` -> `npc_chase`, `ClearGoal / StopMoving` -> `npc_stop`,
  `SetMovementActivity(ACT_RUN)` picks the run gait), `Vehicle`, `Weapon` (a carried weapon has
  no entity id: a wrapper by class and owner, valid while the player holds it).
- `Player(id)` over the player snapshot (`Nick / Name / SteamID` from the identity, `GetPos`,
  `GetVelocity`, `EyeAngles`, `GetAimVector`, `EyePos`, `Alive / Health / Armor / GetMaxHealth /
  Frags / Deaths`, `GetWeapons / HasWeapon / GetWeapon / GetActiveWeapon`, `GetEyeTrace`,
  `IsBot` = no identity, user groups, PData in the actor) with the writes `SetPos` ->
  `player.teleport`, `SetHealth`, `SetArmor`, `Kill`, `Give`, `SelectWeapon`, `TakeDamage(Info)`.
- `ents` (`GetAll` = props, NPCs and players, `FindByClass` with `*`, `FindByModel`, `FindByName`,
  `FindInSphere / Box / Cone`, `FindAlongRay`, `GetByIndex`, `GetCount`, `Iterator`, `Create`),
  `player` (`GetAll / GetCount / GetByID / GetBySteamID / GetHumans / GetBots / Iterator`),
  `game` (`GetMap`, `MaxPlayers` from the cvars, the rest of the reads with their server-realm
  values), `engine` (`TickInterval / TickCount / ActiveGamemode`, ...).
- `util`: `TraceLine / TraceHull / QuickTrace` through `phys.trace` into the documented
  `TraceResult` shape, the geometry helpers (`DistanceToLine`, `IntersectRayWithOBB / Plane`,
  `IsPointInCone`, sphere / box tests), `TableToJSON / JSONToTable` (arrays for sequential
  tables, Vector as `"[x y z]"`, Angle as `"{p y r}"`), `KeyValuesToTable`, `Base64Encode /
  Decode`, `CRC`, `Stack`, `StringToType`, `GetPlayerTrace`; precache / effect / decal / network
  string calls accepted and inert.
- Globals: `IsValid`, `IsEntity`, the `is*` type predicates, `tobool`, `Either`, `Format`,
  `Msg / MsgN / MsgC / ErrorNoHalt` to the actor's print queue, `PrintTable`, `ProtectedCall`,
  `SortedPairs / ByValue / ByMemberValue`, `RandomPairs`, `AccessorFunc`, `TypeID`, the
  `Get/SetGlobal*` (actor-local), `SERVER / CLIENT`, `include / AddCSLuaFile` (inert: packages
  carry their files), and the enums these use (`TYPE_*`, `DMG_*`, `SOLID_*`,
  `COLLISION_GROUP_*`, `CONTENTS_*`, `ACT_*` movement, `HITGROUP_*`, `MOVETYPE_*`, `FORCE_*`).
- `ENT:PhysicsInit(solid)` / `PhysicsInitBox(mins, maxs)` / `PhysicsInitSphere(r)` /
  `PhysicsInitStatic` / `PhysicsInitShadow` / `PhysicsDestroy` choose a scripted entity's body: a
  box or icosphere hull of its own (the model's mass), the model's hulls frozen in place, or no
  body (`SOLID_NONE`, `PhysicsDestroy`). Before `Spawn` the shape rides with the spawn; on a live
  entity the body is dropped and rebuilt on the next tick. `PhysWake` wakes the body.
  `PhysicsInitConvex / MultiConvex` (hulls from points) and `PhysicsFromMesh` stay stubs.
- The server animator (`Server.anims`): every NPC and every prop a script gave a sequence has a
  sequence, a cycle advanced each tick by `rate * dt / duration` (duration `(frames - 1) / fps`
  of the sequence's first animation, include models' sequences merged in), a finished flag set
  once the end is reached (loops wrap, one-shots clamp at 1), and at most one gesture layer
  (id 0) with its own sequence, cycle, rate, weight, looping/autokill flags and priority. An
  NPC's AI sequence changes restart the cycle. Snapshots carry cycle, rate, finished and the
  layer; the client plays the server's cycle (extrapolated at the rate since the snapshot)
  and the gesture on top. GLua: `Entity:SetSequence(id|name)` / `ResetSequence` (the model
  must have it) / `ResetSequenceInfo` / `GetCycle` / `SetCycle` / `GetPlaybackRate` /
  `SetPlaybackRate` / `IsSequenceFinished`; `AddGestureSequence(seq, autokill)` /
  `AddLayeredSequence(seq, priority)` (always layer 0, a second call replaces it) /
  `FindGestureSequenceLayer` / `RemoveAllGestures` / `RemoveLayer` / `GetLayerCycle` /
  `GetLayerDuration` / `GetLayerPlaybackRate` / `GetLayerSequence` / `GetLayerWeight` /
  `SetLayerAutokill` / `SetLayerCycle` / `SetLayerDuration` (sets the rate) / `SetLayerLooping` /
  `SetLayerPlaybackRate` / `SetLayerPriority` / `SetLayerSequence` / `SetLayerWeight`. Weight
  and priority are state the client does not blend by yet (the gesture plays at full weight).
  Activity-based `AddGesture` / `FindGestureLayer` / `IsPlayingGesture` / `RemoveGesture` /
  `RestartGesture` (we have no activity table) and `SetLayerBlendIn / BlendOut` stay stubs.
- Pose parameters: the model data lists them (`name, start, end, loop range`, include models
  merged); `Entity:GetNumPoseParameters` / `GetPoseParameterName` / `LookupPoseParameter` /
  `GetPoseParameterRange` read that table, `SetPoseParameter(name|id, value)` stores a value on
  the server (wrapped into the range of a looping parameter, clamped otherwise; a parameter the
  model lacks is refused), `GetPoseParameter` reads it back, `ClearPoseParameters` drops them
  all. Snapshots carry the set values by name and the client drives the model's blend grids with
  them (NPCs and scripted entities; players keep their movement-derived parameters).
- Activities: the full `ACT_*` enum (wiki `Enums/ACT`, 1596 ids) is defined, and the model data
  carries the activity name and weight each sequence is tagged with in the MDL (include models
  merged). `Entity:GetSequenceActivity` / `GetSequenceActivityName` read it,
  `SelectWeightedSequence(act)` picks among the sequences tagged with the activity by weight
  (`SelectWeightedSequenceSeeded` deterministically, -1 when none), `AddGesture(act)` /
  `FindGestureLayer` / `IsPlayingGesture` / `RemoveGesture` / `RestartGesture` go through that
  selection onto the gesture layer; `util.GetActivityIDByName` / `GetActivityNameByID` map the
  enum. NPC / Player / Weapon activity state (`SetActivity`, `GetIdealActivity`, ...) stays
  stubbed: our AI has no activity machine.
- Collision bounds: snapshots carry each entity's box (an NPC's hull, a prop's MDL hull box
  widened to its PHY points); `Entity:GetCollisionBounds` / `OBBMins` / `OBBMaxs` / `OBBCenter` /
  `WorldSpaceAABB` / `GetSurroundingBounds` read it (previously constant zeros / the view box).
  `SetCollisionBounds` (`WS` subtracts the position) changes an NPC's hull, which its hit tests
  use; on a prop it is readback only, the body stays the PHY. `SetSurroundingBounds(Type)` stay
  stubs.
- Constant fakes replaced: `Player:GetCount(kind)` reads the sandbox ledger (every `sbox_max*`
  kind, carried per player in the snapshot) and `CheckLimit(kind)` compares it with the
  `sbox_max<kind>` cvar (true when there is no such cvar or it is negative; the server's own
  refusal still rules the spawn); `Player:IsSprinting` is `IN_SPEED` held while the command
  moves; `game.StartSpot` is the map's first spawn point; `util.GetSurfacePropName` /
  `GetSurfaceIndex` / `GetSurfaceData` read the server's `surfaceproperties` table (our index
  is the position in the sorted material list, data = friction / elasticity / density /
  dampening + the sound script names up the `base` chain; the engine's jump / speed factors and
  material letter are not in our table).
- More constants replaced: `game.GetAmmoTypes` / `GetAmmoID` / `GetAmmoName` / `GetAmmoData` /
  `AddAmmoType` over the game's default ammo table (wiki `Default_Ammo_Types`, 37 ids) plus
  what scripts add (`GetAmmoData` carries the added table; `GetAmmoMax` is a stub: we have no
  carry caps); `Weapon:GetSlot` / `GetSlotPos` / `GetMaxClip1/2` / `GetHoldType` / `GetPrintName`
  from the game weapon defs or the SWEP table;  `engine.GetAddons` lists the runtime's loaded addons, `GetGames` / `GetMountedContent` the
  mount providers. `Player:GetInfo` / `GetInfoNum` became stubs (no client cvars here).
  `util.DateStamp` is the host's UTC clock (`YYYY-MM-DD HH-MM-SS`), `util.TimerCycle` the
  milliseconds since its previous call, `file.Time` the unix time the addon store entry was
  written (0 for none).
- Map data for scripts: `game.GetMapVersion` is the BSP header revision, `game.GetSunInfo` the
  direction towards the sun from `light_environment` (`obstruction` 0; nil without one),
  `Entity:TestPVS(pos|ent)` asks the collision world's PVS (cluster visibility, true when there
  is no world), `Entity:GetLocalAngularVelocity` is the body's angular velocity (previously
  constants). `load_world` carries revision and sun to both servers like the spawn points.
- Sequence movement: the model data carries each sequence's root movement (first animation) and
  each bone's surface property / contents, so `Entity:GetSequenceMoveDist` / `GetSequenceMovement`
  / `GetSequenceVelocity` / `GetSequenceGroundSpeed` / `GetSequenceMoveYaw` / `GetGroundSpeedVelocity`
  and `GetBoneContents` / `GetBoneSurfaceProp` read the MDL; `Entity:FrameAdvance(dt)` steps the
  server animator now (`Server::frame_advance`), `GetAnimTimeInterval` is the tick interval,
  `SetAnimation(act)` picks the weighted sequence and resets to it.
- NPC state for scripts: the snapshot carries stuck counter, chase reach, move speed, next path
  point and a `CAP_*` mask derived from the class (ground move / flight / weapon use + ranged
  attack + aiming / innate melee), so `NPC:CapabilitiesGet` / `CapabilitiesHas`, `HasObstacles`,
  `GetArrivalDistance`, `GetIdealMoveSpeed`, `GetMoveInterval`, `GetIdealYaw` / `IsFacingIdealYaw`
  (towards the enemy, else the next path point, 15 deg tolerance), `GetTimeEnemyLastReacquired`
  read it and `MarkTookDamageFromEnemy` stamps the NPC's last enemy damage (`npc_mark_damaged`).
  `CapabilitiesAdd/Remove/Clear` stay stubs (the mask is what the AI does, not a setting).
  `Entity:GetFlexBounds` / `GetFlexType` / `GetMaterials` read the MDL flex controllers / textures.
- `Player:LimitHit(kind)` queues the same `LimitHit <kind>` client notice the sandbox sends on a
  refused spawn (`Server::notices`, drained by the network layer each tick); `Player:Ban` drops
  the player with the identity banned for the server's lifetime (`Server::pending_bans` ->
  `NetServer::kick(.., ban)`; minutes are not kept across restarts); `Player:RemoveAllItems` strips
  weapons and ammo.
- Player gestures: the server runs one gesture layer per player (`Server::player_anims`; the
  base sequence stays the client's pose), networked in `PlayerSnap.gesture` (protocol 12) and
  played by the client over its pose, ahead of the weapon one-shot. `Player:AnimRestartGesture(slot,
  act, autokill)` picks a sequence of the player's model tagged with the activity by weight,
  `AnimSetGestureSequence` / `AnimSetGestureWeight` edit the layer (a fresh one for a sequence
  without a layer), `AnimResetGestureSlot` removes it; `DoAnimationEvent` maps attack / reload
  events to the `ACT_HL2MP_GESTURE_RANGE_ATTACK_<hold>` / `RELOAD_<hold>` gestures of the active
  weapon's hold type, custom gesture / sequence events to the layer, cancel to a reset (jump,
  swim, die, flinch, spawn events are main-sequence business of the client pose: no-op).
  `DoAttackEvent` / `DoSecondaryAttack` / `DoReloadEvent` / `DoCustomAnimEvent` wrap it. Slot ids
  are accepted and not kept (one layer). `AnimRestartMainSequence` stays a stub.
- `ENTITY:PhysicsCollide(data, phys)` fires from the physics impacts the server already reports
  (`entity.collide`: contact point, approach speed, other entity or the world). `CollisionData`
  carries HitPos / Speed / HitSpeed / HitEntity / HitObject / PhysObject / DeltaTime; HitNormal is
  approximated from the hit point towards the entity centre and the Our/Their velocities are the
  snapshot's (old == new): per-contact normals are not reported yet. Stale stub names dropped from
  the GM / SANDBOX lists for hooks that are fired (SetupMove / Move / FinishMove / ScaleNPCDamage,
  PlayerSpawn* / PlayerSpawned* / CanTool).
- `WEAPON:OnDrop` fires when a player drops a scripted weapon (`weapon.drop`: player, class and
  the world entity it became) and `GM:OnNPCDropItem(npc, item)` when a dying NPC drops its weapon
  (`npc.drop`), both from the server's drop paths.
- `GM:FindUseEntity(ply, default)` is asked before +use acts (`Request::FindUse`, answered with an
  entity id; one that is not a live prop / NPC / vehicle means nothing); `WEAPON:OnRemove` fires
  when a weapon leaves the inventory (`weapon.remove` from `strip_weapon`; the instance stays for a
  re-equip); `OnCleanup(kind)` fires per cleaned-up kind (`sandbox.cleanup` from the sandbox's
  cleanup events).
- `Player:SetFOV(fov, time)` is server state (`Player.fov`), networked in the player snapshot
  (`fov`, `fov_time`); the client blends its own camera to it over `time` seconds and returns to
  `fov_desired` when the server clears it (`fov <= 0`); `GetFOV` reads the snapshot (75, the
  game's default `fov_desired`, when nothing is set). `Entity:SetModelScale(scale)` is a render
  scale per prop (`Server::model_scales`, `EntityState.model_scale`): the client scales the
  model about its origin, physics is untouched (as in the game); the `deltaTime` blend is not
  kept. Both were constant fakes before (90 and 1).
- `Entity:SetPreventTransmit(ply, stop)` is a server set of (entity, player) pairs the network
  layer skips for that client (`Server::prevent_transmit`, checked in the per-client interest
  test next to the radius and PVS); `GetPreventTransmit(ply)` reads it from the server-realm
  snapshot. `ENT:SetTrigger(on)` gates the touch callbacks of a scripted entity (on by default
  here, where the game needs the call; off drops start_touch / touch / end_touch),
  `UseTriggerBounds(on, padding)` grows its player-touch boxes by the padding (the prop-contact
  path is unpadded). `SetSubMaterial` stays a stub: the client does not apply the networked
  entity material yet.
- `Player:SendHint(name, delay)` and `Player:ScreenFade(flags, color, time, hold)` ride the
  per-player notice queue (`Hint <name> <delay>`, `ScreenFade r g b a time hold flags`): the
  client shows the `#Hint_<name>` token from the mounted `*_english.txt` after the delay, and
  draws a full-screen fade (SCREENFADE_IN: the colour held `hold` then fading out over `time`;
  OUT: fading in over `time`, held `hold`; STAYOUT keeps it; MODULATE / PURGE are ignored). The
  dead player's ragdoll is a server record without an entity id, so `GetRagdollEntity` /
  `CreateRagdoll` stay stubs; `LagCompensation` (shots already rewind by `seen_tick`, script
  traces do not) and `SetAvoidPlayers` (players do not collide) too.
- Scripted NPC schedules (`Type = "ai"`), as the game's base_ai runs them in Lua: `RunAI` on every
  think picks `SelectSchedule()` when idle and steps the current `ai_schedule` task by task
  (`StartTask` on the first think, `RunTask` after; `TaskComplete()` ends a task; `NextTask`,
  `ScheduleFinished`, `TaskTime`, `SetTask`, `DoSchedule`, `StartSchedule`); `Schedule:AddTaskEx`
  / `Init`, `Task:Init` / `IsEngineType` / `IsFNameType` / `Start` / `Run` (+ `_FName`) and
  `TASKSTATUS_*` complete the classes. Engine tasks (`EngTask`) have no backend here and
  complete at once; `StartEngineSchedule` / `StartEngineTask` / `RunEngineTask` and the
  `NPC:SetSchedule` family on engine NPCs stay stubs (our AI is not schedule-based).
- `ENTITY:UpdateTransmitState` is asked once after `Initialize`; `TRANSMIT_ALWAYS` / `TRANSMIT_NEVER`
  become a per-entity override (`Server::transmit_mode`) the network interest test honours
  (always sent / never sent, `SetPreventTransmit` still wins), `TRANSMIT_PVS` (default) keeps the
  radius + PVS test. `ENTITY:PhysicsUpdate(phys)` runs every server tick on the scripted entities
  that define it (not per physics sub-step). `NextBot:MoveToPos` calls `OnMovementComplete` on
  arrival and `OnMovementFailed` when the path is invalid or the bot is stuck.
- Sandbox spawn hooks for the remaining spawn paths: `gm_spawnsent` asks `PlayerSpawnSENT(ply,
  class)` and fires `PlayerSpawnedSENT(ply, ent)` (kind `sent` on the spawn request / spawned
  events), `gm_giveswep` asks `PlayerGiveSWEP(ply, class, swep)` (kind `giveswep`), a ragdoll
  model from the prop spawn path asks `PlayerSpawnRagdoll(ply, model)` instead of
  `PlayerSpawnProp` (the server flags the request from the model's .phy) and fires
  `PlayerSpawnedRagdoll` for a `prop_ragdoll`.
- `WEAPON:Tick()` runs every server tick on every carried scripted weapon; `WEAPON:EquipAmmo(ply)`
  when the player walks into a weapon of a class he already carries (the entity is gone, the
  inventory unchanged, as measured); `WEAPON:OnReloaded()` right after `DefaultReload` refilled
  the clip (our reload is instant). `WEAPON:AcceptInput` / `KeyValue` stay stubs: carried
  weapons have no entity id to fire inputs at.
- `GM:AcceptInput(ent, input, activator, caller, value)` runs before any `Entity:Fire` /
  `Entity:Input` reaches a scripted entity's `ENTITY:AcceptInput` (true swallows the input).
- `GM:GetGameDescription()` is asked every 64 ticks (`gamemode.description`) and the string
  travels in the master heartbeat (`ServerInfo.game`); the server browser shows it as the
  gamemode column (was the constant "sandbox").
- `GM:PlayerStepSoundTime(ply, kind, walking)` is asked after every footstep the server plays
  (`Request::StepTime`; kind `STEPSOUNDTIME_NORMAL` / `WATER_KNEE` / `WATER_FOOT` from the
  water level, ladder steps do not pass here): a number replaces the milliseconds to the next
  step, nil keeps the measured cadence (400 / 350 / 250 ms by max speed, +50 crouched).
  `GM:PlayerDeathSound` stays a stub (the server emits no death sound to suppress), `GM:ShouldCollide`
  too (the physics has a layer-pair filter only, no per-entity contact filter).
- NextBots: a scripted entity with `Base = "base_nextbot"` (or `Type = "nextbot"`) gets a server
  ground mover (`Server::nextbots`, `NextBot`): each tick the xy velocity ramps toward the
  `Approach` goal at the desired speed (accel / decel), gravity pulls an airborne bot, the hull
  steps over the world like an NPC's (`hull_step`: step-height lift, box sweep, wall slide, floor
  snap), the body turns toward the `FaceTowards` target (else the goal) at the max yaw rate, and a
  bot that moves < 10 % of its intent for 3 ticks is stuck. The entity's prop stays frozen and is
  teleported by the mover. `CLuaLocomotion` (`self.loco`) maps onto it (Approach / FaceTowards /
  speeds / step & jump heights / yaw rate / gravity / Jump / JumpAcrossGap / velocity / IsOnGround
  / IsStuck / ClearStuck; avoid / climb / gaps flags are kept but not used by the mover),
  `ENT:RunBehaviour` runs as a coroutine resumed on every think (`coroutine.yield()` waits a
  tick; `BehaveStart` / `BehaveUpdate` around it), `NextBot:MoveToPos(pos, {tolerance, lookahead,
  maxage, repath})` drives a `PathFollower` to the goal, `PathFollower:Update(bot)` approaches the
  current goal and advances the cursor on arrival, `Chase(bot, ent)` recomputes when the target
  moved > 30 u. `NextBot:StartActivity` / `GetActivity` go through the entity animator and the
  model's activity tags, `PlaySequenceAndWait` yields the behaviour until the sequence finished
  (or its duration at the playback rate passed), `BodyMoveXY` writes `move_x` / `move_y` pose
  parameters from the locomotion velocity in the body frame over the desired speed, `SetFOV` /
  `SetMaxVisionRange` (90 deg / 2000 u) feed `IsAbleToSee(ent, useFOV)` (range, cone, clear
  `util.TraceLine` from the eyes), `GetLastKnownArea` is the nav area under the bot refreshed on
  every think (`ClearLastKnownArea` forgets it). Stubs stay for hiding spots (`FindSpot(s)`),
  `BecomeRagdoll` (no ragdoll write) and `SetSolidMask` (the mover sweeps with
  `MASK_PLAYERSOLID`). `IsAreaTraversable` is true, the ground normal up.

Done in `glua/server.luau`:

- `net`: `Start`, every documented `Write*` / `Read*` (string, int / uint / uint64, float /
  double, bool / bit, vector / normal, angle, color, entity, player, data, table as JSON,
  matrix, `WriteType / ReadType`), `Send(player | table | CRecipientFilter)`, `Broadcast`,
  `SendOmit`, `SendPVS / SendPAS` (= broadcast), `SendToServer`, `Receive`, `BytesWritten /
  BytesLeft`, `Abort`; the message travels as a typed item list on the own transport
  (`net.send(name, items, player)`, `net.message` on the other side), `util.AddNetworkString /
  NetworkStringToID / NetworkIDToString`, `RecipientFilter()` (`CRecipientFilter`).
- `ConVar` over the snapshot's cvars and the `cvar.create / set` ops (`GetString / Int / Float /
  Bool / Default / HelpText / Min / Max / Flags / IsFlagSet`, the setters, `Revert`),
  `CreateConVar(name, default, flags, help, min, max)` (replicated / archive from the
  `FCVAR_*` bits), `CreateClientConVar`, `GetConVar` (nil when unknown), `GetConVarNumber /
  String`, `ConVarExists`, `cvars` (`Number / String / Bool`, change callbacks fed from
  `cvar.changed`).
- `concommand` (`Add / Remove / GetTable / Run / AutoComplete`, an actor-local registry),
  `RunConsoleCommand` (a registered command, else a cvar set), `game.ConsoleCommand`.
- `file` over the addon's storage as its DATA folder (`Write / Append / Read / Exists / Delete /
  Find` with `*` / `CreateDir / IsDir / Size / Rename / Async*`, `Open` -> `File` with
  `Read / ReadLine / Write / Seek / Tell / Size / EndOfFile / Close` and the typed
  read / write pairs); without the `storage` capability every call fails closed.
- `CTakeDamageInfo` (`DamageInfo()`, damage / base / max / bonus / custom, attacker,
  inflictor, type with `IsDamageType / IsBulletDamage / IsExplosionDamage / IsFallDamage`,
  position, force, reported position, ammo type).
- `sound` (`Add / Get / GetTable / GetProperties`, `Play` -> the `sound.play` op from a
  registered script's file or the raw name), `EmitSound`, `Entity:EmitSound`, `SNDLVL_* /
  CHAN_*`.
- `undo` (`Create / AddEntity / AddFunction / SetPlayer / SetCustomUndoText / ReplaceEntity /
  Finish` -> the `undo.push` op, `GetTable`, `Do_Undo`), `cleanup` (`Register / GetTable /
  Add / GetList / ReplaceEntity / CC_Cleanup / CC_AdminCleanup`, actor-local lists),
  `constraint` (`Weld` and `NoCollide` as the own ops, `GetTable / HasConstraints / Find /
  FindConstraint(s) / FindConstraintEntity / GetAllConstrainedEntities / RemoveAll /
  RemoveConstraints` over what the actor made; `Axis / Rope / Slider / Elastic / ...` are
  stubs until the constraint ops exist outside stools).
- The GM hook bridge: `GM` / `GAMEMODE`, `Initialize / InitPostEntity / PostGamemodeLoaded` on
  the first tick, `OnReloaded`, and every own server event raised as its documented hook with
  the documented arguments - `OnEntityCreated`, `PlayerSpawnedProp`, `EntityRemoved`,
  `PropBreak`, `OnNPCKilled`, `PlayerSpawn`, `PlayerDisconnected`, `PlayerHurt`,
  `DoPlayerDeath / PlayerDeath / PostPlayerDeath`, `PlayerNoClip`, `PlayerUsedEntity`,
  `PlayerUse`, `GravGunOnPickedUp / OnDropped`, `WeaponEquip`, `PlayerEnteredVehicle /
  PlayerLeaveVehicle`, `KeyPress / KeyRelease` (`IN_*`), `PropertyUsed`, `OnPhysgunReload`,
  `PhysgunDrop`, `OnPhysgunFreeze`; the requests map the hook's cancel value onto the own
  request's `false`: `EntityTakeDamage` (true blocks, a changed `CTakeDamageInfo` damage is
  dealt instead), `GetFallDamage`, `PlayerUse`, `GravGunPickupAllowed / AllowPlayerPickup`,
  `PhysgunPickup`, `CanPlayerUnfreeze`, `CanUndo`, `CanProperty`, `PlayerCanPickupWeapon`,
  `PlayerLoadout` (true skips the default), `PlayerSpawnProp`, `CanTool`, `PlayerSay` (`""`
  drops the line, a string replaces it).

Done in `glua/classes.luau` and `glua/scripted.luau`:

- `math.ease` (all 30 documented easings), `VMatrix` (`Matrix(table?)`, row-major 4x4:
  translation / scale / angles / forward-right-up getters and setters, `Translate / Scale /
  Rotate`, `Transpose`, `InvertTR / GetInverseTR` for rigid transforms, `Invert / GetInverse`
  by Gauss-Jordan, `*` with a matrix or a point, `+ / -`, `Unpack / SetUnpacked / ToTable`),
  `CEffectData` (`EffectData()` with every documented field pair) with `util.Effect` and
  `effects.Register / Create` as inert data until the client realm draws them.
- `scripted_ents.Register(ENT, class)` -> the host `entity.register` (Model / ThinkInterval /
  UseType / PrintName / Category) with the documented `ENTITY` callbacks dispatched on a
  per-entity instance table whose lookups fall through `ENT` (with `Base` inheritance and
  `BaseClass`) to the `Entity` wrapper: `Initialize`, `Think` (a number returned via
  `NextThink` keeps the own next-think op), `StartTouch / Touch / EndTouch(other)`,
  `Use(activator, caller, USE_ON | USE_OFF, value)`, `OnRemove`, `OnTakeDamage(CTakeDamageInfo)`;
  `Get / GetStored / GetList / GetSpawnable / GetType / GetMember / Alias / IsBasedOn /
  OnLoaded`.
- `weapons.Register(SWEP, class)` -> the host `weapon.register` (`Primary.ClipSize /
  DefaultClip / Ammo / Automatic`, Slot / SlotPos / HoldType / ViewModel / WorldModel /
  PrintName) with `PrimaryAttack / SecondaryAttack / Reload / Deploy / Holster / Think` on an
  instance bound to the carrying player (`self.Owner`); `Get / GetStored / GetList /
  IsBasedOn / OnLoaded`; `baseclass.Get / Set`.

The rest of `Entity` and `NPC` (end of `glua/entity.luau`):

- Real over the snapshot and the own ops: local / abs / network position and velocity aliases,
  `IsOnGround / GetGroundEntity` (a 2 u trace down), `GetMoveType` (frozen = `MOVETYPE_NONE`) and
  `SetMoveType` through the freeze op, `GetSolid / IsSolid / SetNotSolid`, `SetCollisionGroup`
  (the collide op), `SetGravity` (the gravity op), `TakeDamage / TakeDamageInfo /
  DispatchTraceAttack` (players through the damage request, entities through the health op),
  `Visible / VisibleVec / IsLineOfSightClear`, `DropToFloor`, `PointAtEntity`, `Fire / Input`
  (`Kill` only), `CallOnRemove / DeleteOnRemove` from `entity.removed`, `GetFlags / IsFlagSet`
  (`FL_ONGROUND / FL_FROZEN / FL_CLIENT / FL_NPC`), spawn flags, key values, `AddCallback`,
  `FireBullets` (players, the bullet op), `GetConstrainedEntities` from the `constraint` table.
- `NW2 / Networked / Networked2 / DT` variants alias the actor-local NW storage; `NetworkVar /
  NetworkVarElement / NetworkVarNotify / InstallDataTable / GetNetworkVars /
  RestoreNetworkVars / SetNetworkKeyValue` build the scripted-entity data table on it, with
  `Set*VarProxy` change callbacks.
- `NPC`: `GetEnemy / GetTarget / GetGoalTarget` (the snapshot's `npc_target / npc_chase`),
  `GetGoalPos / IsGoalActive / GetPathDistanceToGoal / GetPathTimeToGoal / GetCurGoalType`
  (the end of the own path, `npc_goal`), `GetShootPos` (+40, the crouch-fire eye height),
  `GetAimVector`, `IsInViewCone`, enemy-memory reads over the single target, `Classify /
  Disposition` from the class table, `MoveOrder / TargetOrder / NavSetGoal / MoveStop`.
- Honest stubs (listed as `stub` in the manifest) for everything bound to model data (bones,
  sequences, gestures, layers, flexes, hitboxes, attachments, body groups, render colour /
  material / mode), parenting, map IO beyond `Kill`, physics (re)initialisation, and the whole
  NPC schedule / condition / relationship / capability / behaviour / sentence surface - the AI
  has none of those by design.

The rest of `Player`, `PhysObj`, `Weapon` and `Vehicle` (end of `glua/entity.luau`):

- `PhysObj` is one shared metatable over the owning entity (`__ent`): `GetInvMass / GetEnergy /
  GetVelocityAtPoint / GetPositionMatrix / LocalToWorldVector / WorldToLocalVector`,
  `ApplyForceOffset` (centre force, the torque is dropped), `IsGravityEnabled /
  IsCollisionEnabled` remember the last `Enable*` call. Angular velocity, damping, drag, AABB,
  mesh, shadow and game flags are stubs: the snapshot carries no angular state.
- `Player`: the engine's movement constants (`GetHull` 32x32x72, `GetHullDuck` 36, view offsets
  64 / 28, step 18, `GetMaxSpeed` from the walk / run speeds), `SetActiveWeapon /
  SwitchToDefaultWeapon` through the weapon-select op, `PickupWeapon` (give + remove),
  `PhysgunUnfreeze / UnfreezePhysicsObjects / AddFrozenPhysicsObject` through the freeze op on
  the player's frozen props, `UniqueIDTable`, `OwnerSteamID64`. Ammo, vehicles, flashlight,
  spectating, view punch, animation events, kick / ban and every setter without a server op are
  stubs.
- `Weapon`: reads of the registered SWEP table (`GetWeaponViewModel / GetWeaponWorldModel /
  GetWeight / GetHoldType / AllowsAutoSwitch* / GetPrimaryAmmoType / GetDeploySpeed`),
  actor-local `Get/SetNextPrimaryFire / NextSecondaryFire / LastShootTime`, `HasAmmo`.
  `SetClip1 / SetClip2 / SendWeaponAnim / DefaultReload / CallOnClient` are stubs.
- `Vehicle`: `GetSpeed / GetHLSpeed` (velocity), `GetPassenger` (driver), `GetWheelCount /
  HasBoost` from the class table. Driving is the server's: `SetThrottle / SetSteering /
  SetHandbrake / ReleaseHandbrake` queue the controls onto the server's vehicle (kept until
  changed, as the game keeps a driverless vehicle's), `StartEngine / EnableEngine` the engine
  switch (on with a driver, off when he leaves; off, the wheels park the car); `GetThrottle /
  GetSteering / IsEngineStarted / IsEngineEnabled` read them back from the snapshot, `GetRPM /
  GetSteeringDegrees / GetMaxSpeed / GetOperatingParams` (`speed`, `RPM`, `gear`,
  `steeringAngle`, `wheelsInContact`, `isTorqueBoosting` false) the drive law's last tick, and
  `GetVehicleParams` carries `engine` / `steering` tables from it. The law is measured on srcds
  (`server_vehicle_drive`: a driverless jeep on gm_flatgrass driven through this API, since a
  bot driver's command resets the controls every tick): full throttle gains 5.85 u/s per tick to
  35 mph then creeps, coasting loses 1 u/s per tick, the handbrake 11.8, reverse caps at 14 mph,
  the steering cone is 50 deg up to 14 mph and 18 from 20 mph, and 18 deg at 620 u/s turns 66.7
  deg/s. The airboat's fan is measured the same way (`server_airboat_drive`): `a = 260 - 1.1 v` forward
  (terminal 235 u/s), `187 - 1.87 |v|` in reverse, a 60 deg cone at every speed, sliding tyres
  that stop a coasting boat at 3.7 u/s per tick and let it slide through a turn; RPM reads 0.
  The jalopy (`server_jalopy_drive`) is the jeep's law pulling harder (+6.6 u/s per tick) and
  turning a little slower (60 deg/s at 18 deg); the APC cannot be spawned in the game's spawnmenu
  (`bad_vehicle`) and keeps the jeep's law.
  `SetBoost / IsBoosting` is the turbo (`server_vehicle_boost`: the jeep goes 637 -> 880 u/s, 50
  mph, in 28 ticks while boosted, creeps above, and decays back once off; the game holds 677 in its
  higher gear afterwards, ours returns to the plateau; no 3 s limit through the API, as in the
  game). `SetMaxThrottle / SetMaxReverseThrottle` stay stubs: measured, they do not touch the API
  throttle (full acceleration with the cap at 0.5). `GetWheelContactPoint(i)` is the wheel's ground trace of the last tick (point,
  touching). `GetWheel / SetSpringLength / SetWheelFriction / SetSteeringDegrees / SetVehicleParams`
  stay stubs (no wheel bodies, no such inputs on our wheels).

GM hooks and the coverage manifest: a hook is not a Lua value, so `glua::list` counts a `GM`
hook as present when a bridge in the GLua sources raises it (`hook.Run("Name"`), and a scripted
`ENTITY` / `WEAPON` callback when its dispatcher carries an `-- @hook CLASS:Name` annotation.
Bridged from the own events beyond the earlier set: the gamemode load chain
(`PreGamemodeLoaded / OnGamemodeLoaded / CreateTeams / Initialize / InitPostEntity /
PostGamemodeLoaded` on the first tick), per player the first-sight chain `PlayerConnect /
PlayerAuthed / NetworkIDValidated / PlayerInitialSpawn / PlayerSetModel` (the snapshot carries no
connect event, so "connected" means "first seen in a snapshot"), `PlayerTick / PlayerPostThink`
every tick, `PlayerShouldTakeDamage / ScalePlayerDamage / PostEntityTakeDamage` around the
damage request, `OnPhysgunPickup / OnPlayerPhysicsPickup / OnPlayerPhysicsDrop /
PlayerFrozeObject / PlayerUnfrozeObject`, `PreUndo / OnUndo / PostUndo`, `ShutDown` before a
reload, `EntityKeyValue` from `SetKeyValue`, `EntityNetworkedVarChanged` from every NW change,
`PreRegisterSENT / PreRegisterSWEP` (returning `false` refuses the registration). The sandbox
gamemode's pure-Lua defaults live on `GM` (`PlayerCanSeePlayersChat`, `PlayerShouldTakeDamage`,
`CanPlayerSuicide`, `PlayerCheckLimit`, `PlayerRequestTeam -> PlayerJoinTeam -> *ChangedTeam`,
`CheckPassword`, `GetGameDescription`, `VariableEdited -> CanEditVariable`, ...) so `hook.Run`
falls through to them when no addon answers. Hooks whose trigger does not exist in the own
server yet (animation events, footsteps, voice, water level, ammo changes, `ShouldCollide`,
`SetupMove / Move / FinishMove`, `StartCommand`) have only the default and are therefore
listed as `stub`, never as implemented.

`navmesh` / `CNavArea` over the own Recast mesh: the snapshot carries a `NavRef` to the server's
`gm_nav::NavMesh` (same lifetime rule as `WorldRef`) and the host exposes `nav.loaded /
poly_count / vertices / nearest / path`. A "nav area" is a point snapped onto the mesh whose ID
is the index of the nearest mesh vertex; real: `navmesh.IsLoaded / GetNavAreaCount /
GetNearestNavArea / GetGroundHeight / Find / FindInBox / GetAllNavAreas / GetNavAreaByID`,
`CNavArea:GetCenter / GetID / IsValid / GetClosestPointOnArea / GetZ / Contains / IsVisible /
ComputeGroundHeightChange`. The mesh has polygons, not the game's rectangular areas, so sizes,
corners, adjacency, attributes, hiding spots, ladders and the editing API are stubs.

Libraries without an engine object behind them (`glua/libs.luau`): `team`, `list`, `numpad`
(registry + Activate/Deactivate/Toggle; `FromButton` needs raw key input and is a stub),
`player_manager` (model/hands/class registries, `RunClass` walks `Base`), `system`, `umsg` and
`SendUserMessage` over `net`, `physenv` reads (`sv_gravity`; setters are stubs), the `game`
ammo reads, geometry helpers in `util` (ray/sphere, ray/triangle, box/box, `SharedRandom`),
remaining globals (`LocalToWorld` / `WorldToLocal` via VMatrix, `OrderVectors`, `HexToColor`,
`SafeRemoveEntity[Delayed]`, `TimedSin/Cos`, `STNDRD`, `SQLStr`, `IsEnemy/FriendEntityName`,
`IsUselessModel`), the `Tool` state object (objects, stage, operation, mode convars; ghost
entities are client-side stubs) and `duplicator` (class / modifier / constraint registries,
`Copy` / `CopyEnts` / `Paste` / `CreateEntityFromTable` over `ents.Create`). `CUserCmd`,
`CMoveData`, `CNavLadder`, `PathFollower`, `CLuaLocomotion` and `drive` exist as all-stub
classes until the server has the objects. The documented `KEY_` enum is defined. The presence
scan also lists one nested table level (`math.ease.*`).

More scripted callbacks: `ENTITY:SetupDataTables` runs once before `Initialize`, `KeyValue` /
`AcceptInput` are reached through `Entity:SetKeyValue` / `Entity:Input` (an `AcceptInput`
returning true handles the input), `PreEntityCopy` / `OnEntityCopyTableFinish` /
`PostEntityCopy` / `OnDuplicated` / `PostEntityPaste` from the duplicator. Every SWEP inherits
a `weapon_base` layer (`weapons.__base`): `CanPrimaryAttack` / `CanSecondaryAttack` gate the
attacks and track a local clip through `TakePrimaryAmmo`, `ShootBullet` fires through the owner's
`FireBullets`, `Equip` / `OwnerChanged` run on `player.weapon_pickup`, `Initialize` and
`SetupDataTables` on first deploy. `PLAYER` class callbacks (`Init / Spawn / Loadout / Death /
SetModel / ClassChanged`) run through `player_manager` on the gamemode hooks. Raised `GM:` hooks
are listed under `SANDBOX:` as well. Callbacks the server never calls (AI schedules / tasks,
movement, animation, client effects) are acknowledged as stubs in the manifest, not claimed.

Shared / server tail: `util.Stack` (own Stack object), `CreateSound` / `CSoundPatch` as a
one-shot over `sound.Play` with local pitch / volume / level state (`Stop` / `FadeOut` need a
sound handle and are stubs), `cookie` persisted through the addon's file storage
(`cookies.json`), `properties` and `gamemode` registries, `ai_schedule` / `ai_task` as data
objects, `CTakeDamageInfo:GetWeapon`, `CRecipientFilter:AddPlayers`, `net.WriteVars /
ReadVars / Receivers`. `sql`, `saverestore`, `gmsave`, `IMaterial` / `ITexture`, `debug`, `jit`,
`debugoverlay`, `ai` graph queries, `NextBot`, `http`, `resource`, `steamworks` are acknowledged
stubs (no sqlite, save system, material system or debug backend on the own server). `os` and
`package` stay absent: the sandbox test requires it.

Player ammo and clips over the server inventory: `PlayerSnap.ammo` (reserve per ammo type) and
`PlayerSnap.clips` (`class, clip1, clip2`) come from `weapons::Inventory`; `Player:GetAmmoCount /
GetAmmo / SetAmmo / GiveAmmo / RemoveAmmo / RemoveAllAmmo / StripAmmo / StripWeapon / StripWeapons`
and `Weapon:Clip1 / Clip2 / SetClip1 / SetClip2` write through `player.set_ammo / give_ammo /
set_clip / strip_weapon` (`Server::set_ammo`, `set_clip`, `strip_weapon`; stripping the active
weapon selects the first remaining one; counts clamp at zero). Reads see the state of the
snapshot they run on, so a write shows up one tick later like every other queued write.

PhysObj over the body: `EntitySnap.ang_velocity` (deg/s), `mass`, `damping` and `asleep` come
from the Jolt body (`Server::entity_phys`); `PhysObj:GetMass / SetMass / GetAngleVelocity /
SetAngleVelocity[Instantaneous] / AddAngleVelocity / GetDamping / GetSpeedDamping / GetRotDamping /
SetDamping / IsAsleep / Wake / Sleep` write through `set_mass / set_velocity / set_damping /
set_asleep` (`Physics::set_damping`, `set_asleep`; damping in Jolt's 1/s units, not Source's
tuned table). `CalculateForceOffset`, `RotateAroundAxis` and `AlignAngles` are pure math over the
body pose. `ApplyTorqueCenter`, `CalculateVelocityOffset`, inertia, buoyancy, drag coefficients,
AABB / volume / mesh queries still have no body-side reader and stay stubs.

Entity parenting: `Server::set_parent` stores the child prop's pose in the parent's space
(`ParentLink`, parent = prop or player with origin + view yaw), freezes the child and re-poses it
right after every physics step (`follow_parents`, `parent_world`); unparenting leaves it where it
is, still frozen; a vanished parent drops the link. `EntitySnap.parent` feeds
`Entity:GetParent / GetChildren / GetParentWorldTransformMatrix`, `SetParent / SetMoveParent`
queue `set_parent`, and `Get/SetLocalPos`, `Get/SetLocalAngles` are parent-relative when
parented (world otherwise). Bone / attachment parenting (`SetParentPhysNum`, `FollowBone`,
`GetParentAttachment`) has no model-side anchor yet and stays a stub.

Render state: `EntitySnap.color / material / render_mode / render_fx` are the prop's own fields
(the ones the colour and material stools set and the client receives); `Entity:GetColor /
SetColor / Get/SetColor4Part / Get/SetMaterial / Get/SetRenderMode / Get/SetRenderFX` write
through one `set_render` write per part (`Server::set_prop_render` keeps the untouched parts).
Sub-materials (`SetSubMaterial`, `GetMaterials`) need per-mesh material lists and stay stubs.

Model data: `ModelScriptData` (body groups with sub-model counts, skin families, bone names and
parents, sequence names, hitbox / hitbox-set counts, flex controller names, hull bounds,
attachments with bone / local pos / forward) is filled from the MDL when the server registers a
model and the snapshot points at the map (`ModelsRef`); the host `model.info(name)` serves it to
`Entity:GetBodyGroups / GetNumBodyGroups / GetBodygroupCount / GetBodygroupName /
FindBodygroupByName / SkinCount / GetBoneCount / GetBoneName / LookupBone / GetBoneParent /
GetSequenceCount / GetSequenceName / LookupSequence / GetSequenceList / GetHitBoxCount /
GetHitboxSetCount / GetFlexNum / GetFlexName / GetFlexIDByName / GetModelBounds /
GetModelRenderBounds / GetModelRadius / GetAttachments / LookupAttachment / GetAttachment`
(attachment pose = local pos / forward through the entity's pose; bone-relative animation is not
applied). Bone matrices / positions, sequence durations, flex weights and bodygroup/skin writes
on anything but a prop keep needing the animation layer and stay stubs.

Player mover state: `PlayerSnap.buttons / buttons_last` (the command processed this tick and the
one before), `on_ground`, `ducked`, `vehicle` (seat), `driving` (driven prop), `use_target`,
`flashlight`; `Player:KeyDown / KeyDownLast / KeyPressed / KeyReleased / IsWalking / IsOnGround /
Crouching / InVehicle / GetVehicle / GetDrivingEntity / GetUseEntity / FlashlightIsOn` read them,
`Flashlight / SetFrags / SetDeaths / AddFrags / AddDeaths / Kick / EnterVehicle / ExitVehicle`
write through `player.set_flashlight / set_frags / set_deaths / kick / vehicle` (`remove_player`,
`enter_vehicle`, `exit_vehicle`). The flashlight is a server flag only (no client impulse or
light yet). Movement tunables are one `MoveConfig` for the whole server, so per-player speed
setters (`SetWalkSpeed`, `SetRunSpeed`, `SetJumpPower`, ...) are stubs rather than silent no-ops.

Animation data: `EntitySnap.sequence` is the sequence the server plays (NPCs; props do not
animate server-side), `ModelScriptData.sequence_anim` / `anims` carry fps / frames / flags from
the MDL animation descriptors; `Entity:GetSequence / SequenceDuration ((frames - 1) / fps) /
GetSequenceInfo / GetAnimCount / GetAnimInfo` read them. Bounds math is pure over the model hull:
`OBBMins / OBBMaxs / OBBCenter / BoundingRadius / GetRotatedAABB / WorldSpaceAABB / NearestPoint /
IsPointInBounds / GetWorldTransformMatrix`; `GetNoDraw / SetNoDraw` map onto render mode 10
(`RENDERMODE_NONE`). Cycle, playback rate, activities, pose parameters, gestures / layers and
sequence writes need a server-side animation clock and stay stubs.

Poser state: `EntitySnap.bone_scales / bone_angles` (non-identity entries), `flex` (weights +
scale) and `eye_target` are the tables the inflator, bone poser, face poser and eye poser write;
`Entity:ManipulateBoneScale / ManipulateBoneAngles / GetManipulateBoneScale /
GetManipulateBoneAngles / HasBoneManipulations / SetFlexWeight / GetFlexWeight / SetFlexScale /
GetFlexScale / HasFlexManipulatior / SetEyeTarget / GetEyeTarget` read them and write through
`set_bone_manip / set_flex / set_eye_target` (`Server::set_bone_manip`, `set_flex`,
`set_eye_target`, bounded by the model's bone / flex counts). `SetSkin` now also applies to NPCs.
Bone position offsets and jiggle have no storage and stay stubs; `Player:Say` waits for a
server-side chat injection (chat lives in the net layer).

`Player:Say` queues the line on `Server::pending_chat`; the net layer drains it with the typed
chat of the same tick, so it goes through the `player.chat` hook and the broadcast exactly like
a typed line. `EntitySnap.weapon` is the NPC's weapon class; `NPC:GetActiveWeapon` reads it and
`NPC:Give / SelectWeapon / DropWeapon` write `npc_weapon` (`Server::set_npc_weapon`, only classes
in the NPC weapon table).

Vehicle record: the snapshot points at the server's `vehicles` map (`VehiclesRef`), the host
`vehicle.info(id)` serves driver / mass / add_gravity / health / wheels (hub, radius, spring,
damping); `Vehicle:GetDriver / GetVehicleParams / GetWheelCount / GetWheelBaseHeight /
GetWheelTotalHeight / GetSpringLength / IsEngineEnabled` read it. Throttle / steering /
handbrake come from the driver's command every tick and have no script override, so the vehicle
`Set*` family stays stubs.

`EntitySnap.npc_target_seen` is the tick an NPC last saw its target (`NPC:GetEnemyLastTimeSeen`
= tick x tick interval); `ModelScriptData.friction / elasticity` are the PHY surface of the
registered model (`Entity:GetFriction / GetElasticity`).

Player script state: `no_target` (NPC target selection skips the player), `player_color` /
`weapon_color` (0..1 RGB, data only until the client paints them), `max_armor` (armour is
clamped to it). `Player:DropWeapon / DropNamedWeapon` strip the weapon and place it as a world
weapon 32 u in front of the origin (`Server::drop_weapon`, same undo / cleanup as a spawned
one); `DropObject` releases the held prop.

`Player:Lock / UnLock / IsFrozen`: a locked player's commands are run with no buttons and no
move (view angles still follow). `Entity:SetFriction / SetElasticity` set the Jolt body's
surface at once and are remembered per prop (`Server::surface_overrides`); the snapshot's
`friction / elasticity` is the surface in use (override, else the model's PHY values).

Player snapshot also carries `joined_tick` (`TimeConnected` = ticks since x interval) and
`last_hitgroup` (last recorded hit on the player); entity snapshot carries `on_fire`
(`IsOnFire`). `Entity:ForcePlayerDrop` makes every holder let go (`Server::force_player_drop`).

The server remembers the weapon a player had active before the current one (updated as the
player's command is processed) for `Player:GetPreviousWeapon`. `Entity:AddEffects / GetEffects /
RemoveEffects / IsEffectActive` and the `EFlags` family keep a per-entity bitmask in the script
layer; `EF_NODRAW` is the one flag with a server meaning and maps onto `SetNoDraw`.

Player damage carries the hit behind it (attacker, hit group, position): `PlayerTraceAttack` may
swallow a bullet hit, `ScalePlayerDamage` sees the real hit group, `OnDamagedByExplosion` follows
blast damage. Every sandbox spawn under its limit asks `PlayerCheckLimit` (a `false` denies; a reached
limit denies without asking, there is no `true` override) and every undo entry asks `CanCreateUndo`. Every footstep asks `PlayerFootstep` (position, foot, sound, volume; `true`
silences it). Every entity sound leaving the server asks `EntityEmitSound` with the game's
sound table (`false` drops it; `true` applies a changed `SoundName` or `Volume`; pitch / level / DSP /
channel are placeholders, our sounds carry none). `OnEntityWaterLevelChanged` follows a player's water level
from the mover (0 dry, 1 feet, 2 waist, 3 eyes); props carry no water level, so only players report. `PlayerSelectSpawn` may return any entity: the player spawns at its
position and angles instead of the server's pick (we have no spawn-point entities, so
`IsSpawnpointSuitable` is asked only about team spawn entities): the default
`PlayerSelectSpawn` hands a player whose team has spawn classes (`team.SetSpawnPoint`) to
`PlayerSelectTeamSpawn`, which picks a random spawn entity `IsSpawnpointSuitable` accepts. `Player:ViewPunch` / `SetViewPunchAngles` / `SetViewPunchVelocity` /
`ViewPunchReset` write the mover's view punch spring (angle in Source degrees, the spring in
gm-move decays it), `GetViewPunchAngles` / `GetPunchAngle` / `GetViewPunchVelocity` read it from
the snapshot; `PickupObject` carries an entity as +use would (no reach check, the pickup rules and
`PlayerPickup` still apply), `PlayStepSound` plays one footstep through the footstep rule (right
foot), `ExitLadder` lets go of the mover's ladder. `SetEyeAngles` sets the view angles and runs the next usercmd the
server processes with them (the client's own angles return with the command after; nothing is
pushed to the client), `EquipSuit` / `RemoveSuit` / `IsSuitEquipped` keep the HEV suit flag
(cleared on spawn, the loadout event equips it before `PlayerLoadout` runs; no item pickups depend on it yet),
`SetLastHitGroup` overrides `LastHitGroup` until the next hit lands on anyone, `ShouldDropWeapon`
drops the active weapon as a world weapon on death through the `DropWeapon` path. `Entity:GetBloodColor` reads the NPC's blood colour (the class table's value in the
game enum; players red, everything else `DONT_BLEED`), `SetBloodColor` changes it on NPCs
only. `NPC:GetEnemyFirstTimeSeen` is the tick the current target was first seen,
`GetLastTimeTookDamageFromEnemy` the last damage taken from it (both reset when the target
changes); `NavSetWanderGoal(min, max)` walks to a random navmesh vertex in that distance band,
`NavSetRandomGoal(minDist, dir)` the same with `minDist..minDist+512` (the direction is ignored).
`NPC:GetMaxLookDistance` / `SetMaxLookDistance` is the NPC's own target search range
(default 2048); `PickupWeapon(ent)` takes a world weapon of a class the NPC can carry and
removes the world entity. `NPC:IgnoreEnemyUntil(ply, time)` drops that player as the target and
keeps every target search from picking it before `time`; `GetNPCState` is derived from our
state: dead `NPC_STATE_DEAD`, a target `COMBAT`, a goal `ALERT`, else `IDLE`.
`Player:SetWalkSpeed/SetRunSpeed/SetJumpPower/SetStepSize/SetCrouchedWalkSpeed/SetDuckSpeed/
SetUnDuckSpeed/SetMaxSpeed` are per-player overrides of the server's movement tunables,
swapped into the mover for that player's moves only (the snapshot carries the effective
values the getters read; `SetMaxSpeed` sets the velocity cap, `SetSlowWalkSpeed` /
`SetLadderClimbSpeed` stay stubs: our mover has neither). `SetViewOffset` / `SetViewOffsetDucked` (the z
component) and `SetHull` / `SetHullDuck` (half width from `maxs.x`, height from `maxs.z`;
`ResetHull` clears) are overrides of the same kind: the player's own hull is used for its
movement, hit tests and eye position.
`impulse 100` travels in the command (protocol 8, `NetCmd.impulse`; the `f` bind of the
game's `kb_def.lst`) and asks `PlayerSwitchFlashlight(ply, on)` before toggling the server
flag; `Player:Flashlight` does not ask, as in the game. The flag is server state only until
the client light exists.
`PhysObj:GetAABB/GetVolume/GetSurfaceArea/GetMassCenter/GetInertia/GetInvInertia/GetMesh/
GetMeshConvexes` read the PHY of the model: vertex AABB, PHY volume, convex faces as
triangle fans (`MeshVertex` with the face normal, `u`/`v` 0), mass centre, rotation inertia
times the body mass. `SetInertia`, buoyancy/drag coefficients, game flags, shadow control
stay stubs (no such inputs on our bodies).
`Player:GetCurrentCommand` is a `CUserCmd` copy of the command the server processed for
the player this tick (buttons, forward/side/up move, view angles, mouse deltas, impulse,
server tick as `TickCount` / `CommandNumber`); the command has already run, so the setters
change the copy only. Mouse wheel is not in our command (stub).
`SetupMove(ply, mv, cmd)`, `Move(ply, mv)` and `FinishMove(ply, mv)` bracket the mover's
step of every player command once an addon listens to any of them: `mv` is a `CMoveData`
of origin, velocity, angles, buttons (and the previous command's), forward/side/up move and
the command's ground speed cap; what the hooks change in it is written back (`SetMaxSpeed`
caps walk and run for that step), `Move` returning `true` skips the mover (the hook moved
the player). Constraint / final-velocity / abs-move-angle fields have no mover input: stubs.
`CompileString(code, name, handleError)` / `RunString` / `RunStringEx` compile a chunk from
a string in the actor's own sandbox (same globals, no file or process access; a failed
compile returns the error text when `handleError` is false, else raises / `ErrorNoHalt`s).
`CompileFile` and `include` stay stubs: actors do not see files.
A `CNavArea` is one polygon of our Recast mesh (ID = index; the mesh keeps each polygon's
corners and the polygon across every edge): centre, corners, extent, `Contains` / `GetZ` /
`GetClosestPointOnArea` from the polygon, `GetRandomPoint` uniform over its fan, neighbours
and sides (NORTH -y, EAST +x, SOUTH +y, WEST -x by the dominant axis between centres),
incoming connections, overlap and corner visibility; `navmesh.GetNavArea` finds the polygon
under a point. Attributes, places, hiding spots, ladders, search lists and editing stay stubs.
`Path(kind)` is a `PathFollower` over our navmesh path query: `Compute(bot, goal)` stores
the game's segment table (pos, length, distanceFromStart, forward, area) along the straight
path (false when there is none or it is partial); length, start/end, first/last/next/prior
segment, cursor moves, cursor data, position on path and closest position are polyline math.
`Update` / `Chase` need a NextBot locomotion and `Draw` the client: stubs.
`Entity:IsPlayerHolding` is true while any player holds the entity with the physgun or
carries it with +use; `Entity:WaterLevel` is the mover's water level for players (0 dry,
1 feet, 2 waist, 3 eyes) and 0 for everything else (props have no water state).
A Player object answers every Entity method: its own methods first, then the Entity
method (those read the entity snapshot, which a player has none of: parent NULL, no children,
model scale 1 and so on), then its NW vars; before this only a whitelist of Entity methods
existed on players.
`Player:Ping` (smoothed ack round trip of our channel, ms), `PacketLoss` (incoming sequence
gaps in percent), `GetTimeoutSeconds` / `IsTimingOut` (seconds since the last packet; timing
out past 1 s) and `IPAddress` (the transport's peer name, `loopback` when it has none) come
from the network layer's per-tick connection record.
`IsMounted(name)` matches the labels of the mounted content providers the host passed to
the server; `SoundDuration(name)` reads the `fmt`/`data` chunks of `sound/<name>` from the
mounted content (WAV only; 0 for anything else or when the host offers no lookup). `GetPreferredCarryAngles` (the hook, else the scripted
entity's method) is asked at every +use pickup; an Angle holds the prop at those angles in the
player's yaw frame instead of as it lies.

Every bullet shot asks `EntityFireBullets` (a `false` fires nothing) and reports
`PostEntityFireBullets` after it resolved; a hit on an NPC runs `ScaleNPCDamage` / `EntityTakeDamage`
over a CTakeDamageInfo the way player damage does (the changed amount is what the NPC loses).

A dead player's due respawn asks `PlayerDeathThink` (a `true` keeps them dead, asked again next
tick), the `kill` command asks `CanPlayerSuicide`, and the player's own weapon selection asks
`PlayerSwitchWeapon` (scripted `Player:SelectWeapon` / `Kill` are not asked).

Spawning an NPC, vehicle or weapon for a player asks the addon runtime first (`PlayerSpawnNPC /
PlayerSpawnVehicle / PlayerSpawnSWEP`) and reports the entity afterwards (`PlayerSpawned*`); props
already did through the network layer's spawn request. Hooks the server raises count as
implemented in the manifest even when their gamemode default is a stub.

+use on a vehicle and +use while seated ask the addon runtime first (`CanPlayerEnterVehicle`,
`CanExitVehicle`); a `false` leaves the player where they are. Scripted `Player:EnterVehicle /
ExitVehicle` are not asked, like the game.

The model table carries each bone's bind pose relative to its parent; `Entity:GetBonePosition /
GetBoneMatrix` compose it up the parent chain and through the entity's pose. The server has no
animation layer, so these are the reference-pose bones, not the animated ones the game returns.

The model table also carries the default hit box set (bone, hit group, mins, maxs per box) for
`Entity:GetHitBoxBone / GetHitBoxHitGroup / GetHitBoxBounds` (set argument ignored: one set);
`GetChildBones` walks the bone parents. `util.SHA256` hashes on the host, `util.Timer` is a
`CurTime`-based timer object. `util.SHA1` stays a stub (no SHA-1 in the workspace).

`Weapon:GetMaxClip1 / 2` read `Primary / Secondary.ClipSize` of the registered SWEP table
(-1 without one); `Weapon:DefaultReload` moves reserve ammo into the clip through the same
queued clip / ammo writes (no reload animation or timing: the game plays the reload sequence
and refills at its end). `DropEntityIfHeld` is `Entity:ForcePlayerDrop`.

Per-player delta hooks (only when something is subscribed, like `PlayerTick`): the bridge
keeps the previous tick's `on_ground` / velocity / ammo per player and raises
`OnPlayerHitGround` (speed = the fall speed), `OnPlayerJump` (`IN_JUMP` pressed while on the
ground) and `PlayerAmmoChanged`. `PlayerDroppedWeapon` is raised from `Player:DropWeapon`.
`PlayerButtonDown / Up` stay stubs: they take raw key codes and we only carry `IN_` flags.

Tests: `glua_core` (vectors / angles / colours / math / string / table / bit / utf8 / hooks /
timers against documented values), `glua_entity` (reads from a snapshot, setters as queued
writes, `ents` / `player` queries, `ents.Create` binding, traces and JSON), `glua_server`
(a net round trip through the host transport, ConVar reads, concommand, damage info, the hook
bridge for events and requests), `glua_classes` (easings, a rigid matrix round trip, ENT / SWEP
tables registering and dispatching) and `glua_coverage`.

## Not yet

Scripted entities' touches while both bodies sleep; the weapon selection
menu (attack to confirm, `hud_fastswitch`, `invnext`) and the game's `KEY_*` names / `input.IsKeyDown`
polling (ours: `input.key` events with the client's key names); the game's pitch-0 miss of +use
on players (see "+use"). Done since this list was written: the
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
  calling `Touch`). Players have no physics body, so their touches are an overlap test after
  the move (the player hull against the entity's convex AABBs, every tick), with `player` set and
  `other` 0; the test walks a player into a crate entity and back: one `start_touch`, a `touch`
  per tick in contact, one `end_touch`;
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
message's `name` and its decoded `data` as JSON). `gm.queue` is `world.queue(op, ent, arg)`
with `arg` as JSON: the writes go through the actor's own Lua `world.queue`, so the parsing, the
`world-write` capability and the per-run cap are the same, and they land in the serial phase
with everyone else's. `gm.net_send(name, player, json)` is `net.send(name, value, player)` (0 =
everyone), the JSON becoming the net value through the same Lua `net.send` and its limits.
`gm.log` is `print`.

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
