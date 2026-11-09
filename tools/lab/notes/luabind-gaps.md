# `luabind` gap list: Garry's Mod binary behaviour vs the Rust rewrite

Reference: `luabind.md` (facts F1..F94). Rust paths are relative to the `gmod-rust` clone at
branch `main` (commit checked out on 2026-11-09). Line numbers refer to that checkout.

Ranking: P0 = obvious to a player (hooks not firing, wrong order visible in gameplay/addons),
P1 = noticeable (addon compatibility, timing, limits), P2 = subtle (edge cases, messages).

Where the Rust runtime is a deliberate different design (Luau actors, own protocol), the gap says
so; the ranking still measures the player-visible difference for a GLua addon run on it.

---

## A. Hook dispatch and gamemode lifecycle

- G1 (P0) Gamemode start-up hook order. Game (F23, F24): `CreateTeams`, `PreGamemodeLoaded`,
  `OnGamemodeLoaded`, `PostGamemodeLoaded`, `Initialize`, and `InitPostEntity` later when map
  entities exist. Rust: `crates/gm-script/src/glua/server.luau:636-645` runs
  `PreGamemodeLoaded`, `OnGamemodeLoaded`, `CreateTeams`, `Initialize`, `InitPostEntity`,
  `PostGamemodeLoaded` on the first `tick.pre`. Mismatch: `CreateTeams` must be first,
  `Initialize` must follow `PostGamemodeLoaded`, `InitPostEntity` must not be in this chain.
  Gamemodes that create teams in `CreateTeams` and read them in `OnGamemodeLoaded`, or that
  expect `Initialize` after `PostGamemodeLoaded`, break.
- G2 (P1) Think / timers / Tick order. Game (F27, F68): `Think`, then timer pass, then `Tick`.
  Rust: `crates/gm-script/src/glua/core.luau:775` fires `Think` and `Tick` inside one `tick.pre`
  handler and the prelude timers run after all `tick.pre` handlers
  (`crates/gm-script/src/api.rs:1531`), so for GLua code timers run after `Tick`. The host-level
  prelude ordering (`docs/spec/script.md:311-333`) is right; the GLua bridge is not.
- G3 (P1) Engine hook argument contracts on connect. Game (F18): `CheckPassword(steamid64, ip,
  sv_password, client password, name)` with two returns deciding accept/reject and reason, fired
  before the client is a player; `NetworkIDValidated(name, networkid, steamid64)` (3 args);
  `PlayerConnect(name, ip)`; `ClientSignOnStateChanged(userid, old, new)`. Rust:
  `crates/gm-script/src/glua/server.luau:652-663` fires `CheckPassword` with an empty client
  password and ignores the result, `PlayerConnect(name, "")`, `NetworkIDValidated(name, steamid)`
  with 2 args, all at the first tick the player is visible; `ClientSignOnStateChanged` is never
  raised. Mismatch: a Lua password/whitelist gate cannot refuse a connection, and args differ.
- G4 (P1) `CheckPassword` default. Game fallback (F18) is a prefix compare of
  `sv_password` against the client password (`strncmp` with `strlen(sv_password)`), and a
  string return becomes the kick reason. Rust `crates/gm-script/src/glua/server.luau:1004` and
  `crates/gm-script/src/glua/hooks.luau:228` use exact equality and no reason path.
- G5 (P2) `hook.Call` iteration order. Game (F17): `pairs` over the hook table (hash order, not
  stable). Rust `crates/gm-script/src/glua/core.luau:745-772`: deterministic insertion order
  via an `order` list. Not a visible bug (the game's order is unspecified), but addons that
  accidentally rely on the game's hash order will behave differently; the first-non-nil
  short-circuit and object-identifier validity handling match.
- G6 (P1) Engine-called hooks are a fixed name table (F33: 309 names). Rust raises its
  hooks from its own event bus; five GM-level names never appear anywhere in the GLua surface
  (`CaptureVideo`, `SpawniconGenerated`, `ViewPunch`, `OnCloseCaptionEmit`,
  `OnChangeActivity`), and many entity/weapon/NextBot callbacks of F33 (e.g. `PhysicsCollide`,
  `PhysicsUpdate`, `PhysicsSimulate`, `OnTakeDamage`, `UpdateTransmitState`, `TestCollision`,
  `ImpactTrace`, `TranslateFOV`, `GetNPC*` weapon callbacks) are not raised by
  `crates/gm-script/src/glua/scripted.luau` / `server.luau` (grep for the quoted names finds no
  dispatch). Each missing callback is a silent no-op for an addon.
- G7 (P1) `ShutDown` timing. Game (F18): first action of level shutdown, before entities are
  removed and the state is killed. Rust `crates/gm-script/src/glua/server.luau:688` raises it on
  `addon.reloaded` (hot reload) only; there is no map change path. `OnReloaded` (line 687) is
  Lua-only in the game too, fine.
- G8 (P2) `hook.Call` capture semantics. Game (F12, F13): engine hooks go through the
  `hook.Call` captured when the gamemode was set; replacing `hook.Call` later has no effect on
  engine hooks. Rust: `hook.Run` is looked up live
  (`crates/gm-script/src/glua/core.luau:772`), so a replaced `hook.Call` is honoured. Addons
  that wrap `hook.Call` (common for profilers) will see engine hooks in Rust but not in the game.
- G9 (P2) Hook return "finish bool" rule (F16): a non-boolean return leaves the engine default.
  Rust returns whatever `hook.Run` yields to its bridges (e.g.
  `crates/gm-script/src/glua/server.luau:628` tests `== true`, `:759` tests `== true`), which is
  the same outcome for the boolean cases; non-boolean truthy returns (a string from
  `EntityTakeDamage`) are treated as "not handled" in both. Matches in practice; noted for
  completeness.

## B. Metatables and entity tables

- G10 (P2) Entity `__index` order. Game (F38): the entity's own Lua table first, then the class
  metatable chain, so `ent.SetPos = f` shadows the method. Rust
  `crates/gm-script/src/glua/entity.luau:28-34`: scripted class table, then the shared `ENT`
  methods, then the per-entity `__vars`. A field assigned on the entity never shadows a method.
- G11 (P2) `MetaBaseClass` / typed userdata. Game (F35..F37): every class metatable exposes
  `MetaBaseClass`, `FindMetaTable("Entity")` etc. return the real metatable with `__index`
  pointing at itself for base classes. Rust `crates/gm-script/src/glua/entity.luau:471` (`PLY`)
  and `:28` use closures; `MetaBaseClass` is absent (no grep hit). Addons that walk
  `MetaBaseClass` or compare `getmetatable(ent) == FindMetaTable("Entity")` differ.

## C. Networked variables

- G12 (P0) Legacy NWVar transport. Game (F41..F48): reliable user message `NetworkedVar`, value
  typed, rate-limited per entity (random 0.5..1.0 x K1 seconds, active weapon 0.5 s), byte budget
  `lua_networkvar_bytespertick`, catch-up for joining players at <= 512 bytes per cycle, only
  changed values sent. Rust `crates/gm-script/src/glua/entity.luau:173-190, 1563-1590`: all NW
  families (`NW`, `NW2`, `Networked`, `DT`) alias one table `__vars.__nw` that lives in the actor
  and is replicated by the actor's own net values (`crates/gm-script/src/netval.rs`). Mismatch:
  no 200-byte string truncation (F43), no per-type coercion (game sends NWInt as float32, so
  integers above 2^24 lose precision; Rust keeps exact numbers), no cadence/budget (late joiners
  in the game see NWVars arrive over several 100 ms cycles; Rust is immediate), no
  `networkvars` string table limit of 4096 names (F42).
- G13 (P1) NW2/DT variables are entity snapshot properties in the game (F52, F55): 32 slots per
  type, delta compressed, PVS filtered, `SetDTInt(slot)` slot 0..31. Rust aliases
  `SetDT<T>(i, v)` to `SetNWVar("dt_<T><i>")` (`entity.luau:1567-1568`) with no slot bound and
  no PVS semantics. Mismatch for addons that depend on `NetworkVar` slots being limited to 32 or
  on DT values being present on the client before `Initialize`.
- G14 (P1) `EntityNetworkedVarChanged` and proxies. Game (F43, F51): a legacy `SetNW*` calls the
  per-var proxy `(ent, name, old, new)` when the value changed; the
  `EntityNetworkedVarChanged` hook is the NW2 client-side change notification, not fired by the
  server set path. Rust `entity.luau:1579-1585` fires the proxy and the hook on every changed
  set server-side. Extra hook calls that the game does not make on the server.
- G15 (P2) `Entity:GetNWVarTable()` layout: game returns `{[0] = globals, [idx] = {...}}` (F49);
  Rust `compat.luau:344` returns `out[e:EntIndex()] = nw` with no `[0]` global slot (no global
  `SetGlobalVar` store is networked through the same table).

## D. Net library, user messages, envelope

- G16 (P0) `net` framing and limits. Game (F57, F58): bit stream; 8-bit envelope type, 16-bit
  string-table id header, `net.ReadHeader` returns it, `len - 16` is passed to the receiver,
  `net.WriteEntity` writes 13 bits, `WriteUInt(n, bits)` writes exactly `bits` bits,
  `WriteTable` is typed key/value pairs with an 8-bit type id each. Rust
  `crates/gm-script/src/glua/server.luau:10-120`: a message is a list of `{type, value}` items
  serialised by the host (`crates/gm-net/src/msg.rs:141-146, 423-429, 1275-1281, 1767`,
  `ServerMsg::Addon`/`ClientMsg::Addon`), bit widths are ignored (`WriteInt(n, bits)` stores the
  number), `WriteTable` is JSON (`server.luau:48`), `net.ReadHeader()` returns 0
  (`server.luau:79`), `BytesWritten` is the JSON length (`:80`), `BytesLeft` is `items*8` (`:81`).
  Player-visible when addons depend on bit-exact framing: `net.ReadUInt(16)` after `WriteUInt(x,
  8)` returns the full number in Rust and garbage in the game; reading more values than written
  returns nil in Rust and zeros in the game; the 64 KB message cap does not exist.
- G17 (P1) Network string ids. Game (F60): `util.AddNetworkString` server-side only, ids are
  16-bit string-table indices, `util.NetworkIDToString(id)` resolves them. Rust
  `server.luau:118-120`: ids are CRC32 of the name; `net.Receive` is case-sensitive
  (`server.luau:103`) while the game lowercases both sides (F58). An addon registering
  `net.Receive("MyMsg")` and sending `net.Start("mymsg")` works in the game, not in Rust.
- G18 (P1) `net.Incoming(len, ply)` contract: game passes the bit length minus 8, then Lua
  subtracts 16 (F57, F58); Rust `server.luau:104-108` passes `#items*8` and never reads a header.
  Addons that override `net.Incoming` (some anti-cheat/logging addons) break.
- G19 (P1) Client -> server envelope types (F57): type 2 (client Lua error relay ->
  `OnClientLuaError`), type 4 (Lua file requests) and the invalid-packet drop message do not
  exist in Rust (`msg.rs` has no such tags; `OnClientLuaError` is never raised, grep hit only as a
  stub). Server owners lose client error visibility.
- G20 (P2) User messages. Game (F56): a registry of name -> size, `umsg` is a separate bit
  stream, duplicate registration is fatal, hooking on the server is fatal. Rust
  `crates/gm-script/src/glua/libs.luau:91-104` maps `umsg.*` onto `net.*` and
  `crates/gm-script/src/glua/compat.luau:279-290` dispatches `usermessage.IncomingMessage`
  by name. Semantics differ (umsg has a 255-byte payload limit in the game), but `umsg` is
  deprecated and rarely load-bearing.

## E. Timers

- G21 (P2) Removal moment of a finishing repeater. Game (F65): `reps` is decremented before
  the call and the record is erased after it, so `timer.Exists(name)` is true inside the last
  callback and `timer.Adjust` inside it works. Rust `crates/gm-script/src/api.rs:1499-1503`
  deletes the record before `pcall`, so `timer.Exists` is false and `Adjust`/`UnPause` fail
  inside the last fire.
- G22 (P2) Failure message. Game (F66): the Lua error is printed by the error handler, then
  `Timer Failed! [Simple][<name>]` / `Timer Failed! [<kind>][<name>]`. Rust `api.rs:1507`
  prints `timer <name>: <err>`. Also the game prints the error through the error callback (so
  `OnLuaError` fires); Rust uses `print`, so `OnLuaError` does not fire for timer errors.
- G23 (P2) Pending queue for simple timers created inside a timer callback (F62): deferred to the
  next cycle in the game. Rust collects the due list before firing (`api.rs:1483-1489`), so a
  simple timer created inside a callback also waits; matches. A `timer.Simple(0)` created in
  `Think` fires in the same pass in both (game: the pass follows `Think`). Matches.
- G24 (P2) Simple timers live in the named map under `__simple<n>` names in Rust
  (`api.rs:1413-1416`), so `timer.Exists("__simple1")`, `timer.Remove("__simple1")` and
  `timer.Pause` work on them; in the game simple timers are anonymous (F61). Negligible.
- G25 (P2) Time base. Game (F63, F67): due = `CurTime + delay` in seconds (double) compared
  with the float `CurTime`; Rust converts to fractional ticks (`api.rs:1403-1407`) and counts
  ticks. Equivalent at a fixed tick rate; differs if `host_timescale` or a changed tickrate is
  in play (game timers follow `CurTime`, Rust timers follow tick count).

## F. Console variables and commands

- G26 (P1) `FCVAR_ARCHIVE` emulation. Game (F70, F71): the archive bit is stripped before the
  engine sees the convar and the value is persisted in `cfg/server.vdf` (`FCVAR_LUA_SERVER`) or
  `cfg/client.vdf`, re-applied on creation, written at Lua shutdown. Rust
  `crates/gm-script/src/glua/server.luau:186-194` passes `archive` to the host cvar store
  (`crates/gm-script/src/api.rs:3380-3400`); persistence path and format differ (no `.vdf`,
  no `MOD` path). Server operators editing `cfg/server.vdf` see no effect.
- G27 (P2) Flag mask. Game strips 0x80, 0x100000, 0x200000, 0x800000 (F70) and manages
  `FCVAR_LUA_*` lifetime (unregistered at Lua shutdown). Rust keeps a flat `flagsof[name]` table
  (`server.luau:190`) and never unregisters; `-withjit`-style per-realm lifetime does not apply.
- G28 (P2) Pre-set cache for replicated Lua convars (F72): a value set in `server.cfg` before
  the Lua convar exists is applied at creation. Rust `server.luau:191-193` creates the cvar only
  when the host does not already have it (`hostcvar.get(name) == nil`), which keeps an earlier
  value; equivalent for the common case. Non-replicated Lua convars in the game do not pick up
  the cached value; Rust does. Minor.
- G29 (P2) Name limits: Rust rejects cvar names longer than 64 bytes (`api.rs:3378`); the game
  has no such limit.

## G. Files, include, require

- G30 (P1) `include` / `AddCSLuaFile` / `require` semantics. Game (F83, F86): `include` resolves
  relative to the running file then the Lua roots, pushes the path for nested includes, errors
  `Couldn't include file ...`; `require` loads `includes/modules/<name>.lua` or a binary module
  and records `_MODULES[name]`. Rust: `crates/gm-script/src/glua/entity.luau:1329-1331`
  (`include`, `AddCSLuaFile`, `IncludeCS` are empty stubs), `crates/gm-script/src/glua/client.luau:1464-1472`
  (`require` returns an already-registered module table or errors `Module not found`), the host
  sandbox removes `require`, `loadstring`, `dofile`, `loadfile`, `package`, `debug`
  (`crates/gm-script/src/api.rs:1629-1645`). Multi-file gamemodes/addons cannot load.
- G31 (P1) `file` library sandbox. Game (F88, public docs): `DATA` writes under `data/` with an
  extension whitelist, reads from `GAME`/`LUA`/`MOD`/`DATA` roots, `file.Find` over mounted
  content, `file.Time` is the mtime, `file.AsyncRead` callbacks `(fileName, gamePath, status,
  data)` (F69). Rust `crates/gm-script/src/glua/server.luau:261-290`: a key/value store keyed by
  lower-cased path, `gamePath` ignored, no extension rule, `file.Find` pattern over store keys
  only, no `AsyncRead`. Addons reading shipped content files (`settings/users.txt` is handled by
  a special path in `compat.luau:177-181`) or writing `.txt`/`.json` under `data/` behave
  differently.
- G32 (P2) Byte-code refusal and chunk naming (F79): Rust `api.rs:3475-3490` compiles Luau from
  source; Luau cannot load Lua 5.1 bytecode anyway, so the refusal is implicit. Chunk name
  default `RunString` vs the game's `@<path><name>`; error texts differ (`Couldn't include
  file ...` never appears).
- G33 (P2) Lua download cache / `send.txt` (F84, F85): no equivalent; Rust ships addons as
  `.gmr` packages by SHA-256 (`docs/spec/script.md:404-430`). Different design, same intent.

## H. Error handling

- G34 (P1) `OnLuaError` timing and arguments. Game (F18, F77): called synchronously from the
  error callback at the moment of the error, with `(err, realm, stackTable, addonName,
  addonId)`; every error prints a red block with a numbered stack. Rust
  `crates/gm-script/src/lib.rs:751-757` queues actor errors and dispatches `lua.error` at the
  start of the next tick; `crates/gm-script/src/glua/server.luau:1409` passes `(err, "server",
  "", name, 0)` (empty stack). Timer errors use `print` (G22) and bridge errors are reported once
  per distinct text (`crates/gm-script/src/glua/hooks.luau:9-15`), while the game prints every
  occurrence and only dedups for the remote error reporter.
- G35 (P2) `ErrorNoHalt` / `ErrorNoHaltWithStack` (F78): game prints through the error callback
  (red, with realm, with stack for the WithStack variant, feeds `OnLuaError`). Rust
  `crates/gm-script/src/glua/entity.luau:1284-1285` prints `[ERROR] ...` with no stack and no
  hook.
- G36 (P2) `lua_log` file (`lua_errors_server.txt`, F74, F77) and the stack-leak abort (F75)
  have no counterpart; the leak check is not applicable to Luau actors.
- G37 (P2) `debug` library: game removes only `setlocal`, `setupvalue`, `upvalueid`,
  `upvaluejoin` (F3); Rust removes `debug` entirely and re-exposes `info`/`traceback` under
  `__debug_*` (`api.rs:1640-1645`). Addons using `debug.getinfo`/`debug.traceback` directly
  fail in Rust.

## I. Global environment

- G38 (P2) `GAMEMODE_NAME` is hard-coded `"sandbox"` and `VERSION = 240000`
  (`crates/gm-script/src/glua/entity.luau:1335-1336`); game sets `GAMEMODE_NAME` from the
  loaded gamemode (F23) and `VERSION`/`VERSIONSTR`/`BRANCH` from the build (F8). `VERSIONSTR`
  and `BRANCH` are absent in Rust.
- G39 (P2) `SERVER`/`CLIENT` globals, `includes/init.lua` module list (F11): Rust provides its
  own prelude (`crates/gm-script/src/glua/mod.rs:19-35`); libraries exist as tables but the
  `_MODULES` table and `module()` registration differ (`client.luau:1472-1478`).

---

## Things the Rust runtime does that the game does not

- Deterministic hook order (insertion order with an `order` list, `core.luau:745-772`) and
  event priorities (`docs/spec/script.md:18-23`); the game's `hook.Call` uses `pairs`.
- Fires `PlayerSetModel` and the whole connect chain on the first tick a player is seen
  (`server.luau:652-663`), and `OnReloaded` on hot reload (`server.luau:687`).
- Fires `EntityNetworkedVarChanged` server-side on every legacy `SetNW*` change
  (`entity.luau:1584`).
- `net` messages as typed item lists with JSON tables, unlimited size, no bit widths, CRC ids
  (`server.luau:10-120`); `net.receive(name, fn, priority)` host API (`api.rs:1541-1545`); at most
  256 messages per actor run (`docs/spec/script.md:234`).
- Cvar names limited to 64 bytes, `min`/`max`/`help` options on creation (`api.rs:3378-3395`),
  `cvar.changed` dispatched after `post_tick` and after commands (`lib.rs:765-775`).
- `file` as a per-addon key/value store with `#t` timestamp keys (`server.luau:267`).
- Signed `.gmr` packages with dependency resolution (`crates/gm-script/src/depends.rs`,
  `registry.rs`, `package.rs`), hot reload, actor isolation with world-write capabilities.
- Simple timers addressable by name (`__simple<n>`), timers counted in ticks not seconds.
- `safe()` wrappers that report a failing bridge once and keep the frame running
  (`hooks.luau:9-15`); the game lets a failing hook print every time.
- `debug.info`/`debug.traceback` re-exposed as `__debug_info`/`__debug_traceback` only.
