# Toolgun / sandbox C++ side — gap list vs gmod-rust (clone of `main`, 2026-11-09)

Reference facts are the F-numbers in `toolgun.md`. Rust paths are relative to the repo root. Priority: P0 = obvious to any player, P1 = noticeable, P2 = subtle. "Matches" = behaviour equal within what the game does.

## A. Fire rate / input gating

| # | Behaviour (game) | Rust today | Match? | Gap | Prio |
|---|---|---|---|---|---|
| G1 | Non-automatic stools fire once per press; automatic stools (`LeftClickAutomatic`, e.g. inflator, paint while held… per stool) fire **every tick** while held (F3, F9-F10) | `crates/gm-client/src/main.rs:7727-7735`: one `Command::ToolClick` on the mouse-press edge only; no per-tool automatic flag, nothing while held | partial | Automatic stools never repeat; holding the button with the inflator/paint does one step. Add `LeftClickAutomatic/RightClickAutomatic` to the stool table and re-send (or server-side repeat) every tick while held, gated by `NextPrimaryFire <= CurTime()` | P1 |
| G2 | Clicks are predicted on the client (shared `PrimaryAttack`), sound/anim locally on first prediction (F13, F15) | Client sends a command, waits for the server's `VmAct::Fire` (`crates/gm-server/src/weapons.rs:616-618`) to play sound + view-model anim | no | One round-trip of latency on the click sound/anim and effects; the game is instant | P1 |
| G3 | 0.2 s deploy block before the first `Think` / click (F12) | Scripted weapons: `deploy_left` modelled (`docs/spec/script.md` "Scripted weapon think"); the toolgun click path (`gm-net/src/server.rs:1210`) has no deploy gate | partial | A click in the first 14 ticks after selecting the toolgun is accepted; the game ignores it | P2 |
| G4 | Reload requires `KeyPressed(IN_RELOAD)` (edge) (F11) | `main.rs:3858`: `+reload` console verb sends a reload click per press | yes | — | — |

## B. Click order and hooks

| # | Behaviour (game) | Rust today | Match? | Gap | Prio |
|---|---|---|---|---|---|
| G5 | Order: trace → `CheckObjects` → `Allowed` (toolmode_allow_<mode>) → `CanTool(ply, tr, mode, toolObj, 1/2/3)` → `LeftClick/RightClick/Reload` → `DoShootEffect` (F14) | `gm-net/src/server.rs:1218-1262`: trace → `tool_before` (`CanTool`, `glua/server.luau:935-944`) → stool → `tool_fired` → `tool_after`. No `Allowed` check, no `toolmode_allow_*` convars; `CheckObjects` is not a separate step (invalid stored entities are handled per stool) | partial | `toolmode_allow_<mode>` missing entirely (admin disables tools); `CanTool` gets `nil` as tool object (game passes the tool table, stools read `tool:GetStage()` in hooks); `CanTool` is **not** asked on reload (game: button 3) | P2 |
| G6 | `CanTool` receives the weapon's own trace (F14) | `server.luau:936`: `ply:GetEyeTrace()` — a second, different trace (no vehicle filter, default mask) | partial | Hooks can disagree with the stool about the hit (vehicles, debris) | P2 |
| G7 | `Toolgun.Single` only when the stool returned true; refused/missed clicks silent (F28) | `server.rs:1257-1259` fires only on `done`; `gm-client/src/main.rs:917-919` maps act 6 → `Toolgun.Single` | yes | — | — |
| G8 | `SendWeaponAnim(ACT_VM_PRIMARYATTACK)` + `PLAYER_ATTACK1` gesture (F15) | View-model `fire` anim on `VmAct::Fire` (status.md Client row); third-person attack gesture: not found for the toolgun | partial | Other players do not see the attack gesture | P2 |
| G9 | Muzzle-flash events 21/5003 suppressed (F4) | `main.rs:887-893` `fires_bullets` excludes `gmod_tool` | yes | — | — |
| G10 | Tool `Think` / `Deploy` / `Holster` every tick with the mode switch rules (F17-F18) | `glua/hooks.luau:270-278` runs them from the player think when `ply:GetTool()` resolves; spec says `Think/Deploy/Holster stay stubs: the server does not pass the held mode on` (`docs/spec/script.md:867`) | partial | Scripted stools' `Think` (ghost entities, inflator hold) does not run in practice; `Holster(true)/Deploy(true)` switch flag absent | P1 |
| G11 | Objects are kept per tool mode (each `TOOL` instance has its own `Objects`) (F19) | `gm-sandbox/src/lib.rs:544-554`: one object list, dropped when another stool starts selecting (`objects_tool`) | partial | Switching weld→rope→weld loses the weld's first object (game keeps it) | P2 |

## C. Trace

| # | Behaviour (game) | Rust today | Match? | Gap | Prio |
|---|---|---|---|---|---|
| G12 | Line trace 56756 u, mask SOLID|MOVEABLE|MONSTER|WINDOW|DEBRIS|GRATE|AUX, filter owner + vehicle (F20) | `gm-sandbox/src/lib.rs:1255-1258` `tool_trace` → `trace_line(world, props, …)`: world + PHY props only | partial | Players, NPCs, ragdoll bones, brush entities, map `func_*` are not hittable; no vehicle filter | P1 |
| G13 | Zero-size hull trace fallback when the line hits nothing with an entity, `trace.HullTrace = true` (F21) | absent (`docs/spec/toolgun.md:99` lists it as not measured) | no | Edge cases: start-solid inside a prop, grates | P2 |
| G14 | `PhysicsBone` of the hit passed as effect attachment and object bone (F22) | `ToolHit` carries the prop id; ragdoll bones handled for inflator/finger | partial | Fine for props; effect parenting (G17) not done anyway | P2 |

## D. Screen texture animation

| # | Behaviour (game) | Rust today | Match? | Gap | Prio |
|---|---|---|---|---|---|
| G15 | Text scrolls at 250 px/s on `RealTime()` (continuous wall clock), period width+64, y 104, 3 px shadow, Helvetica 60/900, bg `screen_bg`, RT 256² (F25-F27) | `gm-client/src/main.rs:805-885` paints the same layout every frame while `weapon == 1` (`:5810-5817`); time base = `self.fps_at.elapsed()` | partial | `fps_at` is the FPS counter and is **reset every second** (`main.rs:6095-6108`), so the text jumps back to phase 0 once a second instead of scrolling continuously. Use a monotonic start time | P1 |
| G16 | `TOOL:DrawToolScreen(256,256)` override per stool (F26) | not supported (text only) | no | Stools with custom screens (e.g. addons) show the default text | P2 |
| G16b | Font Helvetica 60 px weight 900 | Helvetica → Arial → Roboto-Bold fallback (`main.rs:826-829`); content-mounted fonts only | ~ | Falls back when the game's font is not mounted; acceptable | P2 |

## E. Beam / tracer and hit effects

| # | Behaviour (game) | Rust today | Match? | Gap | Prio |
|---|---|---|---|---|---|
| G17 | `ToolTracer`: 3 + 1 `DrawBeam` width 8, material `effects/tool_tracer`, 0.25 s, start slides from the muzzle, alpha fades (F30) | **No tool tracer at all**: `grep ToolTracer|tool_tracer` = 0 hits in the repo; the client's `fx_tracers` (`main.rs:5507-5577`) is the bullet streak (128 u at 5000 u/s) and is only pushed from `EffectEvent::Tracer` which the tool path never emits | no | The most visible toolgun feature is missing | P0 |
| G18 | `selection_indicator` dot (4 px, 1 s) + six `selection_ring`s (random speed, random blue colour, 0.13-0.4 s) parented to the hit entity/bone (F31-F32) | absent (0 hits for `selection_indicator` / `selection_ring` / `select_dot`) | no | No hit marker on click | P0 |
| G19 | `gmod_drawtooleffects` convar gates effects (F15) | absent | no | — | P2 |
| G20 | Client effect lifecycle: `Init` returning false removes the effect with `Error: Couldn't initialize effect!`; callbacks cached at init; `Think` false → removed; `Render` culled by render bounds (F33) | `glua/client.luau:2111-2160`: `Init` return ignored; `Think() == false` removes before `Render` the same frame; `Render` every frame regardless of bounds; functions looked up live | partial | Scripts relying on cached callbacks / init failure differ; bounds culling missing (perf only) | P2 |

## F. gmod_camera weapon

| # | Behaviour (game) | Rust today | Match? | Gap | Prio |
|---|---|---|---|---|---|
| G21 | Zoom/Roll network vars; RMB + mouse: zoom `+= mouseY*FrameTime*6.6` clamped 0.1..175, roll `+= mouseX*FrameTime*1.65`; `TranslateFOV` = zoom; `CalcView` roll; sensitivity zoom/80 (F38, F40-F41) | Only a view-model entry (`gm-client/src/main.rs:609`), a `WeaponDef` (`gm-server/src/weapons.rs:308-321`) and the sound script name (`gm-audio/src/lib.rs:370`); no zoom/roll/fov/sensitivity | no | The camera weapon does nothing in hand | P0 |
| G22 | LMB: `NPC_CScanner.TakePhoto`, vm attack anim, 0.4 s rate limit, `jpeg` screenshot, `camera_flash` dlight 512 u for 0.02 s to other players in MP (F35, F39) | none (`fire: None`) | no | — | P1 |
| G23 | `FreezeMovement` while RMB held/released; HUD hidden except weapon selection + chat; `Reload` resets zoom to `fov_desired` and roll to 0; `Equip` sets zoom from `fov_desired` (F38, F41) | `FreezeMovement` only polled for scripted SWEP/TOOL tables (`glua/hooks.luau:330-334`); nothing for the camera | no | — | P1 |

## G. SENT bridge (server)

| # | Behaviour (game) | Rust today | Match? | Gap | Prio |
|---|---|---|---|---|---|
| G24 | First think 0.1 s after spawn, then every 0.2 s unless `Think` returns true after `NextThink` (F44-F45) | `docs/spec/script.md:1511-1516`: 7 ticks then every 13, `NextThink` honoured; **test passes** | yes | Note: Rust also treats "a number returned" as `NextThink(CurTime()+n) return true` — in the game only the boolean result matters (nothing/false → default reschedule, true → none; a bare number's effect was not read from the dump, low confidence) while Rust reschedules at +n; harmless for real scripts | P2 |
| G25 | AI SENT thinks every 0.1 s by default (F44) | NPC think cadence per `SetMoveInterval` / measured (`script.md:840`) | ~ | Not verified for `ai` SENTs specifically | P2 |
| G26 | `Initialize` inside `Spawn` after `SetupDataTables`; precache allowed during it (F42) | `glua/scripted.luau:126` (SetupDataTables then Initialize in the spawn tick) | yes | — | — |
| G27 | `OnTakeDamage` return value used; base health **not** applied by C++ (F50) | `scripted.luau:134-141` returns `OnTakeDamage(d)`; the server's `damage` event is "before health is applied" (`script.md`) — health applied by the host anyway | partial | A SENT that does not call `SetHealth` loses health in Rust but not in the game; damage info lacks position/type/force | P2 |
| G28 | `PhysicsCollide` table with 16 keys incl. surface props, angular velocities, real normal; called twice per impact in the game; gated by `SOLID_VPHYSICS`; base impact sounds only when `PhysicsSounds` true (F54-F55) | `scripted.luau:208-222`: once per impact, normal = hit→centre, old==new velocities, no surface props / angular velocities; no `PhysicsSounds` gate (sounds are the prop's) | partial | Scripts reading `HitNormal`/`OurSurfaceProps` get approximations | P2 |
| G29 | `PhysicsUpdate` only while awake (F56) | `scripted.luau:604`: every tick for every scripted entity | partial | Scripts counting updates or applying forces in `PhysicsUpdate` keep running on sleeping bodies | P2 |
| G30 | `PhysicsSimulate` via a motion controller started by `StartMotionController`; return order `(angular, linear, mode)`; mode < 1 ignores the vectors (F57) | `scripted.luau:608-622`: runs every tick for any entity defining it (no `StartMotionController` needed) and destructures `lin, ang, mode` — **linear and angular swapped** relative to the Lua convention | no | A scripted thruster written for GMod (`return angular, linear, SIM_…`) spins instead of pushing | P1 |
| G31 | `UpdateTransmitState` Lua return 0/1/other → ALWAYS/PVS/NEVER, default ALWAYS (anim) / PVS (point) (F46) | `scripted.luau:602` comment only; `SetPreventTransmit`/`TRANSMIT_*` constants exist (`script.md:705`) but the hook is not dispatched | no | Entities hidden by `UpdateTransmitState` are still sent | P2 |
| G32 | `KeyValue` → Lua, then base always (F47); `AcceptInput` true consumes (F48) | `KeyValue` stub stores a `kv` table (`scripted.luau:686,700,768`); `AcceptInput` via `ServerHooks::accept_input` (`gm-server/src/lib.rs:1746-1757`, true handles) | partial | `KeyValue` hook not called for map-placed scripted entities' keys | P2 |
| G33 | `TestCollision` table return overrides the trace; `ImpactTrace`/`DoImpactEffect`/`HandleAnimEvent` true suppresses the engine (F51-F52) | stubs (`glua/entity.luau:2862, 2868`); `HandleAnimEvent`/`DoImpactEffect` dispatched but return ignored (`hooks.luau:388, 407`) | partial | Custom-collision SENTs and effect suppression unsupported | P2 |
| G34 | `PhysicsSolidMask`, `AutomaticFrameAdvance`, `PhysicsSounds` table fields (F44, F53-F54) | 0 hits for `AutomaticFrameAdvance` / `PhysicsSolidMask` in gm-script | no | Animated SENTs do not advance their frame automatically | P2 |
| G35 | Client-side binding on first data update with the four error strings; `Initialize` on the client after `SetupDataTables` (F60-F61) | client realm: `scripted_ents` instances exist (status.md); binding time not verified | ? | Not compared | P2 |

## H. Effect script loading

| # | Behaviour (game) | Rust today | Match? | Gap | Prio |
|---|---|---|---|---|---|
| G36 | `effects/base.lua` first, then `effects/<name>/init.lua` or `effects/<name>.lua`, lower-cased, auto-`AddCSLuaFile` (F34) | `effects.Register / Create` through the addon manifest (`script.md:986-987`), no `effects/` directory loader | partial | Stock sandbox effects (tooltracer, selection_*) would need porting as addon files | P2 |

## I. Things Rust does that the game does not

- `tool_settings` is one table for the whole server (`gm-net/src/server.rs:1215-1217` comment "not per player yet"): a player's click overwrites every other player's stool convars. The game reads `GetClientNumber` per owner. (P1 in multiplayer)
- The toolgun click is a dedicated network command (`Command::ToolClick`) with the settings attached, instead of `IN_ATTACK` in the usercmd processed by the weapon on both realms; there is no client-side prediction of the click.
- The screen time base restarts every second (G15) — an artefact, not a feature.
- `PhysicsSimulate` applies the returned acceleration directly as a velocity change on every scripted entity defining it (`scripted.luau:608-622`), without the `StartMotionController` opt-in and before any constraint solve; the game lets vphysics integrate it inside the step.
- The ghost entity for stools (F23-F24) is drawn by the client from the device model (`docs/spec/toolgun.md` emitter "ghost model on the client") rather than created by the stool's `MakeGhostEntity`; there is no 150-alpha trans-colour prop in single player on the server.
- `Think` returning a number schedules `NextThink` (G24); the game ignores numbers.

## Counts

- Behaviours compared: 36 rows (G1-G36, plus G16b) + 6 extra behaviours.
- Match: 6 (G4, G7, G9, G24, G26 and G3-partial deploy for SWEPs). Partial: 18. Missing: 12.
- P0: 3 (G17, G18, G21). P1: 8 (G1, G2, G10, G12, G15, G22, G23, G30 + the per-player settings extra). P2: the rest.
