# Addon runtime (`gm-script`)

Luau (mlua, vendored Luau, `send`), one VM per actor: own globals, own GC, own memory limit
(256 MB default via `set_memory_limit`), own CPU budget. Nothing reaches the process: `os`, `io`,
`require`, `loadstring`, `dofile`, `loadfile`, `package`, `debug`, `getfenv`, `setfenv` are gone;
Luau's own sandbox mode is not used because it freezes every global table (`state` included).

## Tick

- Parallel phase: every actor on its own thread with the same immutable `Snapshot` (entities:
  id, model, pos, angles, frozen, owner; players: origin, velocity, view, held prop; counts).
  `world.get(id)`, `world.find(model?)`, `world.players()`, `world.count()`, `game.tick()` read
  it; `world.queue(op, ent, arg)` queues a write (`set_pos`, `set_angles`, `freeze`, `remove`);
  actors without the `world-write` capability get an error from `queue`.
- Serial phase: queued writes are applied in addon priority, then actor id, then enqueue order;
  a write to an entity that is gone is rejected and counted.
- Cancelable events (`prop.spawn_request`) run serially in priority order; the first handler
  returning `false` cancels, later actors never see the request.
- Events in v1: `tick.pre`, `tick.post`, `prop.spawn_request` (player, model), `entity.spawned`
  (player, entity). `events.on(name, fn, priority?)` orders handlers inside an actor the same way.
- The server hooks (`gm_server::ServerHooks`) are called by `ServerNet` around the tick and the
  spawn command; `gm-dedicated --addon file.luau` loads actors (file name = actor name).

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

## Not yet

Manifests and `.gmr` packages, client realm, the rest of the API (entity write views, players,
phys traces and constraints, net, timers, tools, spawn menu, storage, cvars), WASM components,
hot reload command, the sandbox gamemode in Luau on top of this API.
