# Addon runtime (`gm-script`)

Luau (mlua, vendored Luau, `send`), one VM per actor: own globals, own GC, own memory limit
(256 MB default via `set_memory_limit`), own CPU budget. Nothing reaches the process: `os`, `io`,
`require`, `loadstring`, `dofile`, `loadfile`, `package`, `debug`, `getfenv`, `setfenv` are gone;
Luau's own sandbox mode is not used because it freezes every global table (`state` included).

## Tick

- Parallel phase: every actor on its own thread with the same immutable `Snapshot` (entities:
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

## Timers

`timer` lives in the actor's VM (prelude), so a timer runs in the realm and budget of the actor
that made it. Delays are seconds, counted in ticks of `game.interval()` (0.015 s): `ceil(delay /
interval)`, never less than one tick, so `timer.Simple(0, f)` runs on the next `tick.pre`. All due
timers run at the start of the actor's `tick.pre` dispatch, before its handlers, in creation order
within a tick. `timer.Create(name, delay, reps, fn)` replaces a timer of the same name; `reps` 0
repeats forever; `timer.Simple(delay, fn)` is a one shot with a hidden name. `Exists`, `Remove`
(`Destroy`), `Pause`, `UnPause`, `Toggle`, `Start` (restart from a full delay), `Adjust(name, delay,
reps?, fn?)` (keeps the elapsed part), `TimeLeft` and `RepsLeft` follow the sandbox names. A callback
that errors destroys its timer and prints the error through the actor's `print` queue; the actor
is not restarted and its other timers keep running.

Measured (`cargo test -p gm-script`, `timers_count_ticks_and_a_failing_one_is_destroyed`): a 0.015 s
timer with 3 repetitions fires on ticks 1, 2, 3 and is gone; a 0.03 s one shot fires on tick 2; a
0.045 s forever timer fires on 3 and 6; an erroring timer fires once and is removed; a paused one
never fires and still exists.

## Not yet

client HUD/draw API, client-side world for traces, the rest of the API (net, tool ghosts and panels, spawn menu, storage, cvars), WASM components,
file watching for reloads, the sandbox gamemode in Luau on top of this API.
