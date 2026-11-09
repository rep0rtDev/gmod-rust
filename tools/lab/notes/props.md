# Garry's Mod — `props` subsystem behaviour note (server side)

Scope: prop_physics (spawn, mass, health, breaking, prop data, gibs), prop_ragdoll, constraint entities, +use pickup, vehicle basics (jeep/airboat), doors/buttons/triggers, entity I/O, entity factory / class list.

**Sources and tags.** Each fact carries `[src conf]`:
- `bin` = read from the Ghidra pseudo‑C of the Linux `server_srv.so` (symbolised). Only the behaviour is described; no identifiers or code are reproduced.
- `sdk` = Source SDK 2013 (`src/game/server`, `src/game/shared`); used where the binary has the same structure and the SDK carries the readable constant, or for things not in the dump.
- `lua` = Facepunch/garrysmod Lua (`lua/includes/modules/constraint.lua`, `gamemodes/sandbox`, `gamemodes/base`, `lua/autorun/base_vehicles.lua`).
- `inf` = inference.
- Confidence: `high` (read directly / constant visible), `med` (structure matches SDK and SDK constant assumed), `low` (partially read or guessed).

Where the binary differs from SDK 2013 it is marked **GMOD‑DIFF**.

**What the dump contains / does not contain.** The tar contains only `server/` (2 853 functions; no `vphysics/`, `luashared/`, `client/`, `engine/` directories were present despite the prompt). Not in the dump: the +use pickup controller (`player_pickup`), the core entity I/O machinery (input dispatch / output firing / event queue), the entity factory dictionary itself (only the per‑class factories), impact‑damage computation, and vphysics constraint internals — those parts come from the SDK and are tagged accordingly. `[bin high]`

Units: Hammer units (u, 1 u = 0.0254 m), seconds, degrees, kg (vphysics mass), damage points.

---

## 1. Entity factory and class list

1. Entity factories present in the dump (one factory per classname; the user-visible classnames are): `prop_physics`, `prop_physics_multiplayer`, `prop_physics_respawnable`, `prop_dynamic`, `prop_ragdoll`, `prop_ragdoll_attached`, `phys_constraint`, `phys_hinge`, `phys_slideconstraint`, `phys_pulleyconstraint`, `phys_ballsocket`, `phys_lengthconstraint`, `phys_ragdollconstraint`, `phys_constraintsystem`, `phys_motor`, `phys_torque`, `phys_thruster`, `phys_magnet`, `phys_keepupright`, `phys_spring`, `physics_cannister`, `env_physexplosion`, `func_physbox`, `func_physbox_multiplayer`, `func_breakable`, `func_breakable_surf`, `func_door`, `func_door_rotating`, `prop_door_rotating` (+ breakable variant), `func_button`, `func_rot_button`, `momentary_rot_button`, `func_movelinear`, `func_water_analog`, `momentary_door`, `func_rotating`, `func_brush`, `trigger_*` (base + `trigger_multiple`/`trigger_once`), `logic_auto`, `logic_autosave`, `logic_active_autosave`, `logic_branch`, `logic_branch_listener`, `logic_case`, `logic_collision_pair`, `logic_compare`, `logic_coop_manager`, `logic_eventlistener`, `logic_eventlistener_itemequip`, `logic_lineto`, `logic_measure_movement`, `logic_navigation`, `logic_playerproxy`, `logic_random_outputs`, `logic_register_activator`, `logic_relay`, `math_colorblend`, `math_counter`, `math_remap`, `filter_activator_class/name/model/context/mass_greater/enemy`, `filter_multi`, `filter_combineball`, `path_track`, `point_template`, `point_entity_finder`, `env_entity_maker`, `env_ragdoll_boogie`, `game_ragdoll_manager`, `ragdoll_manager`, `phys_ragdollmagnet`, `gib`, `env_gibshooter`, items (`item_battery`, `item_suit`, `item_longjump`, `item_sodacan`), vehicles (`prop_vehicle`, `prop_vehicle_driveable`, `prop_vehicle_jeep` (episodic jalopy class), `prop_vehicle_airboat`, `prop_vehicle_prisoner_pod`, `prop_vehicle_choreo_generic`, `prop_vehicle_viewcontroller`). `[bin high]`
2. Legacy aliases handled in the prop spawn path: an entity spawned as `physics_prop` is renamed to `prop_physics`; `prop_physics_override` is also renamed to `prop_physics` after spawn (the override class only changes how prop data is applied, see §2.3). `[bin high]`
3. **GMOD‑DIFF**: the binary is the merged multi‑game branch — it carries Portal 2 / L4D2 entity logic: `prop_weighted_cube` with no model gets `models/props/metal_box.mdl`; `prop_exploding_barrel` with no model gets `models/props/coop_cementplant/exloding_barrel/exploding_barrel.mdl`; paint‑splat decals `PaintSplatBlue/Green/Pink`; `left4dead2` strings. `[bin high]`
4. `prop_physics_multiplayer` exists with the SDK auto physics‑mode logic: volume of OBB < `sv_pushaway_clientside_size`(15)³ → client‑side; mass < 8 kg → non‑solid (collision group DEBRIS); else solid. `physicsmode` keyvalue 0 = autodetect, 1 solid, 2 non‑solid, 3 client‑side; spawnflag 0x2000 forces server‑side non‑solid; with `sv_turbophysics 1` awake props use the PUSHAWAY collision group. `[bin med, sdk high]`
5. Vehicle list exposed by GMod's spawn menu (Lua `base_vehicles.lua`): Jeep = `prop_vehicle_jeep_old` model `models/buggy.mdl` script `scripts/vehicles/jeep_test.txt`; Airboat = `prop_vehicle_airboat`, `models/airboat.mdl`, `scripts/vehicles/airboat.txt`; Pod = `prop_vehicle_prisoner_pod`, `models/vehicles/prisoner_pod_inner.mdl`, `scripts/vehicles/prisoner_pod.txt`, `limitview 0`; Jalopy = `prop_vehicle_jeep`, `models/vehicle.mdl`, `scripts/vehicles/jalopy.txt`; APC = `prop_vehicle_apc`, `models/combine_apc.mdl`, Health 0 (indestructible). `[lua high]`

---

## 2. prop_physics

### 2.1 Spawn
6. Spawn order: breakable‑prop spawn (prop data, health, fade) → collision group → create VPhysics → nav‑obstacle registration → gib precache. `[bin high]`
7. Collision group: if spawnflag 4 (DEBRIS) **or** the model has the physgun interaction `onbreak explode_fire`‑style flag index 11 (create‑flare), the group is `COLLISION_GROUP_DEBRIS_TRIGGER` (2) when spawnflag 0x1000 (force touch triggers) is set, else `COLLISION_GROUP_DEBRIS` (1). `[bin high]`
8. Spawnflag 0x200 (prevent pickup) adds the "no physcannon interaction" entity flag → neither +use nor physgun/gravity gun can grab it. `[bin high]`
9. **GMOD‑DIFF / quirk**: if the model is exactly `models/props_c17/door01_left.mdl` the prop's fade distance is forced to (-1, 0) (never fades) and its fade scale cleared. `[bin high]`
10. Spawnflag table (prop_physics): 1 start asleep, 2 don't take physics damage, 4 debris, 8 motion disabled, 0x20 pressure‑break, 0x40 enable motion on physcannon, 0x80 no rotorwash push, 0x100 enable pickup output, 0x200 prevent pickup, 0x400 prevent player‑touch enable, 0x800 has attached ragdolls, 0x1000 force touch triggers, 0x2000 force server side, 0x4000 radius pickup, 0x100000 physcannon always pick up, 0x200000 no collisions on spawn, 0x400000 is gib (counted against `prop_active_gib_limit`, default 999999; `prop_active_gib_max_fade_time` 999999). `[sdk high, bin med]`
11. Keyvalues: `massscale`, `inertiascale`, `Damagetype` (1 = ignore… see 2.2), `overridescript`, `damagetoenablemotion`, `forcetoenablemotion`, `ExplodeDamage`, `ExplodeRadius`, `minhealthdmg`, `PerformanceMode`, `BreakModelMessage`, `puntsound`, `PressureDelay`, `health`, `physdamagescale`, `fademindist`/`fademaxdist`/`fadescale`. `[sdk high]`

### 2.2 VPhysics object creation / mass
12. Solid parsed from the model's physics data; if `massscale` > 0 the parsed mass is multiplied by it; if `inertiascale` > 0 the inertia is multiplied (with a lower clamp); mass‑centre override from model data is applied; `overridescript` (surface‑prop/physics override text) is applied; then a normal VPhysics object is created as SOLID_VPHYSICS, start asleep = spawnflag 1. `[bin high]`
13. If no physics object could be created: solid NONE, movetype NONE, warning "can't create physics object". `[bin high]`
14. Motion is disabled right after creation when any of: spawnflag 8, `damagetoenablemotion` > 0, `forcetoenablemotion` > 0, or the entity's "no‑motion" flag bit (set e.g. by `DisableMotion` input). `[bin high]`
15. Mass data for reference: `models/props_junk/wood_crate001a.mdl` = 30 kg (already measured in the Rust repo). The game's mass comes solely from the model's `$collisionmodel` `$mass` plus `massscale`; there is no per‑class default mass. `[bin high, inf]`
16. Lua spawn placement (sandbox): angles = eye angles with yaw+180, pitch 0, roll 0 (ragdoll: pitch -90); position = trace hit, then the prop is pushed out along the hit normal so its OBB touches the surface: `offset = pos − NearestPoint(hit − normal·512)`; then three axis fix‑ups (`fixupProp`): for each local axis a line trace from the entity origin to its OBB min and to its OBB max; if only one hits, the entity is moved by (hitpos − endpos) of that trace; if both hit it is left alone. For ragdolls every physics object is shifted by the same offset. `[lua high]`
17. Lua `FixInvalidPhysicsObject`: if the physics AABB diagonal ≤ 5 u and differs from the model OBB diagonal by ≥ 10 u, the prop is re‑initialised as a box of the model OBB, collision group DEBRIS, mass 100 kg, woken. `[lua high]`

### 2.3 Prop data (health, damage modifiers)
18. Prop data files: `scripts/propdata.txt` is loaded, **GMOD‑DIFF**: additionally every `scripts/propdata/*.txt` is loaded and merged as extra base classes; the `BreakableModels` block provides the random chunk lists. `[bin high]`
19. Keys parsed per prop (model `prop_data` block or base class): `base` (chain, warns if the base is missing from propdata.txt), `blockLOS` (-1 = unset), `AIWalkable` (-1), `damage_table` (name of an impact damage table, e.g. `glass`), `physicsmode`, `multiplayer_break` (`server`=1, `client`=2, `both`=3), `dmg.bullets`, `dmg.club`, `dmg.explosive`, `explosive_damage`, `explosive_radius`, `breakable_model`, `breakable_skin`, `breakable_count`, `allowstatic`, `health`. For `prop_physics_override` / `prop_dynamic_override` the health and explosive values from prop data are **not** applied (mapper values win). `[bin high]`
20. Breakable base spawn: fade scales set to 1; if health is 0 the prop does not take damage (unbreakable), else it does; max health = max(health, 1); impact energy scale (`physdamagescale`) defaults to 0.1 when 0; preferred carry angles default (-5, 0, 0); animate‑on‑pickup state chosen from sequences for activities 0x194/0x193 (physcannon animate/detach); touch handler for pressure/touch breaking installed; gibs precached. `[bin high]`
21. Common HL2 values from propdata.txt (reference, already in repo docs): wooden crate 30 hp, `Wooden.Medium` modifiers bullets 0.75 / club 2.0 / explosive 1.5; metal drum health 0 = unbreakable. `[sdk high]`

### 2.4 Taking damage
22. Damage pipeline (breakable base): (a) if damage < `minhealthdmg` → ignored; (b) damage filter checked; (c) attacker recorded as physics attacker (the attacker or its owner if a combat character); (d) damage is passed through the breakable damage‑modifier routine: DMG_BULLET ×`dmg.bullets` (×2 additionally for DMG_BUCKSHOT), DMG_CLUB ×`dmg.club`, DMG_BLAST ×`dmg.explosive` — modifiers multiply when several type bits are set; (e) again ignored if < `minhealthdmg`. `[bin high, sdk high]`
23. Damage received during the prop's creation tick is applied even if takedamage was off (takedamage temporarily forced on). `[bin med]`
24. Fire: DMG_BURN without DMG_DIRECT is ignored when the prop is marked fire‑immune (`fire_interactions` none / not flammable); otherwise DMG_BURN ignites the prop for random(10, 15) s. DMG_BULLET on a prop with `fire_interactions ignite_halfhealth` ignites it (random(10,15) s) once health−damage < maxhealth/2. `[bin high]`
25. Lethal DMG_BLAST on a prop with `fire_interactions explosive_resist` and an attacker: a survival chance is computed from the attacker distance (near: random(0.1,0.2) extra; far: linear fall‑off) + random(0, 0.5); the prop may survive with reduced health instead of breaking. `[bin med]`
26. After base damage: `OnHealthChanged` output fires with health/maxhealth clamped 0..1, `OnTakeDamage` fires; health ≤ 0 → Break with the attacker. The health‑update path (used by the `SetHealth/AddHealth/RemoveHealth` inputs) fires `OnHealthChanged` and breaks at ≤ 0. `[bin high]`
27. prop_physics specific: with `forcetoenablemotion` set, a damage force whose magnitude ≥ that value enables motion; with `damagetoenablemotion` > 0, health below it enables motion; `EnableMotion` wakes the object, clears the stored "enable position" and fires `OnMotionEnabled`. `[bin high]`
28. Physics impact damage to the prop itself: on every VPhysics collision the prop computes impact damage via the default impact routine with its own energy scale (`physdamagescale`, default 0.1) and the prop‑data `damage_table` (default = NPC table), damage type DMG_CRUSH; if the prop was thrown by the player and this is not the first collision after launch, the damage is scaled by a constant; a "clear flags" think scheduled right after the impact resets the thrown/launched flags. `[bin high]` Table details in §3.
29. Pressure / touch break (spawnflags 0x10 / 0x20 of the breakable base): touch by an entity moving at speed v applies damage `v × 0.01` (DMG_CRUSH) if that is ≥ health, then a second slash damage with a melee push; with the pressure flag, if the toucher stands on the prop a break is scheduled after `PressureDelay` s. `[bin med, sdk high]`
30. Flare interaction: if the prop carries a created flare and an NPC that can be ignited touches it, the NPC is ignited for 25 s, the flare is killed after 5 s and the game event `flare_ignite_npc` fires. `[bin high]`

### 2.5 Breaking
31. **GMOD‑DIFF**: game event `break_prop` (userid = attacking player, or the driver when the attacker is a driveable vehicle; entindex) fires **only if the model name does not contain "crate"** (case‑insensitive). SDK fires it only in TF2 builds. `[bin high]`
32. Break sequence: takedamage off → `OnBreak` output → made non‑solid → position/angles/velocity/angular velocity read from the physics object → break sound (surface‑prop break sound via the physics break sound helper) → explosion (if any) → gibs → removal (unless `prop_physics_respawnable`). `[bin high]`
33. Explosion: when `explosive_damage` or `explosive_radius` > 0 (prop data or keyvalues) an explosion is created at the world‑space centre with **damage × model scale and radius × model scale** (**GMOD‑DIFF**: scaled by the entity's model scale), flags 0x2428 (no decal‑less / no fireball sprite style set); if the model has `onbreak explode_fire` the explosion is created without sound and `PropaneTank.Burst` is emitted instead. Attacker = stored physics attacker if within the physics‑influence window, else the damage attacker. `[bin high]`
34. `onbreak explode_fire` without explosive values: a 1‑damage explosion with `explosive_radius` radius, and every combat character inside that radius passing the damage filter is ignited for 30 s. `[bin high]`
35. Gib parameters: impact energy scale from the prop, default burst scale 100 u/s, collision group = prop's group or INTERACTIVE_DEBRIS (4) if 0. `[bin high]`
36. **GMOD‑DIFF**: if `BreakModelMessage` is set, a user message of that name is sent (PVS) with model index, origin and angles. `[bin high]`
37. Multiplayer gib mode (maxclients ≥ 2 and `breakable_multiplayer` ≠ 0, default 1): `multiplayer_break` server (1) → server gibs; client (2) → the `PhysicsProp` temp‑entity (client gibs, carries skin, effects, model scale); both (3) → both; default (0) → client. Singleplayer: server gibs; gib limit ignored when `breakable_disable_gib_limit` (default 0) is set or performance mode says so. `[bin high, sdk high]`
38. Gib creation (shared routine): max pieces = `props_break_max_pieces` (default -1 = unlimited; `props_break_max_pieces_perframe` -1); refuses if edicts would exceed MAX_EDICTS; pieces come from the model's break list, placed at (offset − `placementOrigin` attachment) transformed by the parent, velocity = parent velocity at that point (or the passed velocity) + burst impulse along (piece − break origin) normalised × burstScale (100); angular velocity = parent's. If the model has no break list but prop data gives `breakable_model`: `breakable_count` random chunk models from the `BreakableModels` list, fade time random(5, 10) s, spawned at random points on the plane of the two largest axes, velocity = direction from origin × 100. `[bin high, sdk high]`
39. Gib entity (`gib`): friction 0.55, SOLID_BBOX, lifetime 25 s then die‑think; initial velocity = attack dir + random(±0.25) per component, speed random(min,max); angular velocity random(100..200) and (100..300) deg/s; waits for landing re‑checking every 0.5 s; on bounce the velocity is damped and 1‑in‑3 impacts play the material bounce sound with volume |v_z|/450 capped at 1; on death fades, flame lifetime 1 s. Human gibs `models/gibs/hgibs.mdl` (head gib), alien `models/gibs/agibs.mdl`, sticky `models/stickygib.mdl`, German builds `models/germangibs.mdl`. `[bin high]`
40. Removal: `prop_physics_respawnable` is not removed; it hides (non‑solid, no‑draw) and materialises after `RespawnTime` (default 60 s) when its bounds are clear of players. `[bin high]`
41. Convar defaults (SDK, used by the same code paths in the binary): `props_break_max_pieces -1`, `props_break_max_pieces_perframe -1`, `breakable_disable_gib_limit 0`, `breakable_multiplayer 1`, `func_break_max_pieces 15`, `func_break_reduction_factor 0.5`, `func_breakdmg_bullet 0.5`, `func_breakdmg_club 1.5`, `func_breakdmg_explosive 1.25`. `[sdk high]`

### 2.6 Physgun / gravity‑gun interactions on props (model `physgun_interactions`)
42. Interaction indices: 0 world stick (`onworldimpact stick`), 1 first‑impact break, 2 first‑impact paint, 3 first‑impact impale, 4 launch spin none, 5 launch spin z, 6 on‑break explode_fire, 7 damage none, 8 fire flammable, 9 fire explosive_resist, 10 fire ignite_halfhealth, 11 on‑pickup create_flare, 12 allow overhead, 13 world bloodsplat, 14 notify children. `[sdk high, bin high]`
43. First collision after launch: `break` → Break with damage = health at the impact position; `paintsplat` → trace from the object along the contact velocity (length constant, SDK 64 u) and decal `PaintSplatBlue/Green/Pink` chosen by entindex % 3 (ragdolls: random 1..3) when the hit surface is not a player/NPC‑like entity; `impale` uses the `impale_forward` prop‑data angles; notify‑children forwards the interaction to parented props. `[bin high]`
44. World stick: only if contact speed > a threshold, the collision normal faces the velocity, and the other surface's material is not metal ('M'), glass ('G') or grate ('X'); then a post‑simulation "stick at position" is queued that moves the prop slightly forward along its velocity and freezes it. `[bin med]`
45. Pickup outputs: `OnPhysGunPickup` always; reason "picked up by cannon" also fires `OnPhysGunOnlyPickup` and (with spawnflag 0x100) `OnPlayerPickup`; "punted" fires `OnPhysGunPunt`; drop fires `OnPhysGunDrop`; +use fires `OnPlayerUse`. Picking up a static/motion‑disabled prop with spawnflag 0x40 enables its motion. `[bin high]`
46. Physcannon pickup check: refused when the no‑motion flag bit is set, or when the object is static and neither `damagetoenablemotion` nor `forcetoenablemotion` is set. `[bin high]`
47. Awake/asleep tracking fires `OnAwakened` / `OnAsleep`‑style outputs on state change during the VPhysics update. `[bin med]`

---

## 3. Impact damage tables (used by props, players, NPCs)
48. Default NPC table (props use it unless `damage_table` names another): linear energy→damage [(150 u/s)²→5, 250²→10, 350²→50, 500²→100, 1000²→500]; angular [(100 °/s)²→10, 150²→25, 200²→50, 250²→500]; minimum linear speed 24 u/s; minimum angular 360 °/s; other object must weigh ≥ 2 kg; "small" < 5 kg: damage capped at 5 and must exceed 36 u/s; large mass 500 kg: energy ×4, falling scale ×5 (blended by downward velocity component); heavy‑object flag → mass treated as 500 kg and energy scale ≥ 2; damaged entity floating in water (other not world) → energy scale ≥ 3. `[sdk high]`
49. Glass table: linear [25²→10, 50²→20, 100²→50, 200²→75, 500²→100, 250²→500]; angular [50²→25, 100²→50, 200²→100, 250²→500]; min speed 8; min mass 2; small < 1 kg capped 10 above 8 u/s; large 50 kg ×4. `[sdk high]`
50. Player table: linear [150²→5, 250²→10, 450²→20, 550²→50, 700²→100, 1000²→500]; angular [100²→10, 150²→20, 200²→50, 300²→500]. Vehicle‑driver table: linear [450²→5, 600²→10, 700²→25, 1000²→50, 1500²→100, 2000²→500]. `[sdk high]`
51. Formula: Δv = |post − pre velocity| of the damaged body (ignored if 0 < Δv < table min velocity); eliminatedEnergy = Δv²·m_self + Δv²·m_other (×largeMassScale / falling scale when applicable); then × (1/m_self) × energyScale; damage = table lookup (piecewise, interpolated by the SDK reader) + angular‑table lookup when spinning above threshold (adds DMG_SLASH); if other mass < small‑mass max the result is clamped to the cap. Damage type DMG_CRUSH. Held objects: a player cannot be damaged by the object it holds; ragdoll parts never damage players; dissolving objects deal 1000. `[sdk high]`

---

## 4. prop_ragdoll
52. Spawnflags: 4 debris, 0x1000 LRU retirement (counts against `g_ragdoll_maxcount`, default 8; `g_ragdoll_important_maxcount` 2), 0x2000 allow dissolve, 0x4000 motion disabled, 0x8000 allow stretch, 0x10000 start asleep. Keyvalues: `angleOverride`, `StartDisabled`. `[sdk high, bin med]`
53. Initialisation: SOLID_VPHYSICS; the ragdoll is built from the model's ragdoll data with the animation pose applied as initial velocities; **maximum 32 elements** (more → "Invalid ragdoll, too many elements!" and no ragdoll); a pose sanity check logs "Bad ragdoll pose … model changed?"; size recalculated after init. Death pose uses the weighted sequence of activity ACT_DIERAGDOLL (0x18) when present. `[bin high]`
54. Bone follow / networking: once per server tick the ragdoll copies each element's position and orientation (up to 32) into the networked ragdoll pose; clients place each bone from its element. `[bin high]`
55. Teleport: a global teleport stack makes attached/child ragdolls teleport together with the root. `[bin high]`
56. Fade: `FadeOut(delay, time)` with time −1 uses the default 0.5 s; each think the render alpha = (1 − elapsed/fadeTime) × 255 with render mode trans‑alpha; when elapsed ≥ fadeTime the ragdoll takes 10 000 damage (removed). `[bin high, sdk high]`
57. **GMOD‑DIFF**: before emitting physics impact sounds, the ragdoll consults a Lua function stored on the entity (SENT hook) that can veto the sound. `[bin high]`
58. **GMOD‑DIFF (quirk)**: models whose path contains `models/player/spy.mdl` get a special case in spawn (sets an extra body/skin‑type field when the model has more than 3 of something — not fully extracted). `[bin low]`
59. Physgun interactions (`physgun_interactions` in the model): `onfirstimpact break` → Break with full health as damage (vector sanity checked), `onfirstimpact paintsplat` → random(1..3) paint decal along contact velocity, `onfirstimpact bloodsplat` / `alienbloodsplat` → blood decal; `onpickup boogie` → when picked up by the cannon an `env_ragdoll_boogie` is attached with magnitude 150 for 2.0 s (random impulses every 0.1–0.2 s per bone) and removed on drop/launch; `onlaunch spin_zaxis` → on launch an angular impulse about z is added, split 1/N across the N elements. `[bin high]`
60. `game_ragdoll_manager`: `MaxRagdollCount` (−1 unlimited) and `MaxRagdollCountDX8` (used when `mat_dxlevel` < 90); feeds the ragdoll LRU. `[bin high]`
61. Death ragdolls in GMod are the server `prop_ragdoll`/`hl2mp_ragdoll` with the player's model and velocity; Lua spawn uses pitch −90 (face up) and shifts every element by the flush offset (§2.1 #16). `[lua high]`

---

## 5. Constraint entities
Common (all `phys_*` constraints):
62. Keyvalues: `attach1`, `attach2` (entity names; empty = world), `constraintsystem`, `forcelimit` (lbs), `torquelimit` (lbs), `breaksound`, `teleportfollowdistance`, `minSoundThreshold`, `maxSoundThreshold`, `slidesoundfwd`, `slidesoundback`, `reversalsoundthresholdSmall/Medium/Large`, `reversalsoundSmall/Medium/Large`. Spawnflag 1 = disable collisions between the two objects (not on `phys_lengthconstraint`/`phys_pulleyconstraint` where 2 = rigid). `[sdk high]`
63. Break parameters: `forceLimit = forcelimit × 0.4536` and `torqueLimit = torquelimit × 0.4536` (lbs→kg conversion), strength 1.0, 0 = unbreakable. `[bin high]`
64. Activation refuses to run inside a physics callback (warning, ignored); warns "attached to two static objects" and "attached to two shadow objects" (constraint still created except for static‑static); logs the mass ratio; both objects must exist — a missing object (no physics) aborts. `[bin high]`
65. **GMOD‑DIFF**: physics objects may be set directly (Lua `Entity:SetPhysConstraintObjects`); when set (and valid) they are used instead of the `attach1/attach2` name lookup for the constraint, for hinge/slide axis set‑up, teleport handling, and for `logic_collision_pair`. `[bin high]`
66. Break: `OnBreak` output; if `breaksound` is set it is emitted from the midpoint of the two objects with PAS attenuation 1.25; the constraint entity is then removed. Inputs `Break`, `TurnOn`, `TurnOff`. `[bin high]`
67. `phys_constraintsystem`: `additionaliterations` keyvalue; constraints in one system are solved together. `[sdk high]`

Per type:
68. `phys_constraint` (fixed/weld): fixed constraint between the two objects' current relative transform; works with the world as one side. `[bin high]`
69. `phys_hinge`: keyvalues `hingeaxis` (world point; axis = origin→hingeaxis), `hingefriction`, `SystemLoadScale`; inputs `SetHingeFriction` (prints "Setting hinge friction to %f"), `SetVelocity` (drives a motor to the given angular velocity, scaled by inertia / load), `SetAngularVelocity`. Defaults of the hinge parameters are 1.0 (strength / limits off). Angles in degrees. `[bin high]`
70. `phys_slideconstraint`: `slideaxis` (world point), `slidefriction`, sounds as above; **GMOD‑DIFF**: if |slideaxis − origin| < 1 u the axis is forced to (0,0,1) and "GMOD: Forcing sane values for slider constraint!" is printed; then normalised. Input `SetVelocity`. `[bin high]`
71. `phys_pulleyconstraint`: `position2` (second pulley point), `addlength`, `gearratio`; all positions and the ratio are validated against finite ranges; spawnflag 2 = rigid; total rope = |p1−obj1| + |p2−obj2| + addlength; force limits converted lbs→kg. `[bin high]`
72. `phys_lengthconstraint`: `attachpoint` (world point on object 2), `addlength`, `minlength`; total length = distance + addlength; spawnflag 2 = rigid (both directions); 0 ≤ minlength. `[bin high]`
73. `phys_ballsocket`: socket at the entity origin; positions validated; strength 1. `[bin high]`
74. `phys_ragdollconstraint` (Lua AdvBallsocket): `xmin..zmax` angle limits, `xfriction/yfriction/zfriction`, spawnflag 1 nocollide, 2 only‑rotation (free translation). `[sdk high, lua high]`
75. `phys_motor`: keyvalues `speed` (deg/s), `spinup` (s), `inertiafactor`, `addangaccel`, `axis` (helper point), `attach1`; spawnflags 1 start on, 2 no‑collide with world, 4 also acts as hinge, 8 local; spawn requires an axis helper with |axis−origin| ≥ 1 u else it self‑destructs with a warning; acceleration = |speed/spinup| (or |speed| if spinup 0); each think the current speed approaches the target by acceleration × frame time; the motor applies torque through a motion controller; "No physics object found. Deleting." if the target has no physics. `[bin high]`
76. `phys_thruster`: `force`, `forcetime`, `attach1`; spawnflags 1 start active, 2 apply force, 4 apply torque, 8 local orientation, 16 mass independent, 32 ignore position; when not attached the velocity offset is computed from the thruster's local position on the object. `[bin high]`
77. `phys_torque`: `force` (angular acceleration), `axis`, `forcetime`; zero/NaN axis speed is rejected. Lua Motor = `phys_hinge` (Axis with torquelimit 0) + `phys_torque` with spawnflags 4 and both objects = object 1; `Scale` input sets the fraction of force. `[bin high, lua high]`
78. `phys_magnet`: keyvalues `massScale`, `overridescript`, `maxobjects`, `forcelimit`, `torquelimit`; spawnflags 1 asleep, 2 motion disabled, 4 suck, 8 allow rotation, 16 coast hack. Sticks to objects on collision when: the object is a `prop_vehicle_jeep`, **or** (unless flag 16) the surface material letter is not player ('P') / metal‑ish ('M'‑class) and its surface prop name begins with `metal`; a fixed constraint (force/torque limits lbs→kg) is created up to `maxobjects`; "suck" pulls entities within a sphere toward the magnet each think. Vehicle attachments get a special velocity. `[bin med]`
79. `phys_spring` (Lua Elastic/Hydraulic/Winch/Muscle): `constant`, `length`, `damping`, `relativedamping`, `springaxis`, spawnflag 1 = stretch only. `[sdk high, lua high]`
80. `phys_keepupright`: `angularlimit`, `attach1`; both constraint objects = the same object (Lua). `[sdk high, lua high]`
81. `logic_collision_pair`: `startdisabled`, `disable_on_remove`; `EnableCollisions`/`DisableCollisions` inputs; **GMOD‑DIFF**: objects may be given directly (Lua NoCollide). `[bin high]`
82. Lua constraint library (what GMod's tools actually create): Weld = `phys_constraint`, `forcelimit` if given, spawnflag 1 if nocollide, objects set as (obj2, obj1); Axis = `phys_hinge` at world point 1 with `hingeaxis` = world point 2, `forcelimit`/`torquelimit`/`hingefriction` only when > 0; Ballsocket = `phys_ballsocket` at the point on object 2; Slider = `phys_slideconstraint` (world side gets a `gmod_anchor`), plus `keyframe_rope` visual; Rope = `phys_lengthconstraint`, `minlength 0`, `length = length + addlength` (addlength clamped ±56756), spawnflag 2 if rigid; Elastic = `phys_spring`; Pulley = `phys_pulleyconstraint` at WPos2, `position2` = WPos3, `ObjOffset1/2`, `addlength` = |WPos3−WPos4|, spawnflag 2 rigid; Motor as #77; Keepupright as #80; AdvBallsocket as #74; NoCollide = `logic_collision_pair` startdisabled 1. `[lua high]`

---

## 6. +use pickup (player carry)
83. Not in the dump; the SDK logic is used, gated by Lua `GM:AllowPlayerPickup(ply, ent)` (base gamemode returns true) and `GM:PhysgunPickup` (sandbox: false for persistent entities when `sbox_persist` is set). `[sdk high, lua high]`
84. Eligibility (SDK pickup check, mass limit 35 kg, size limit 128 u): movetype must be VPHYSICS; summed mass of all physics objects ≤ 35 kg; any object flagged "no player pickup" or hinged → not allowed; largest OBB side < 128 u; non‑moveable objects are allowed only if the prop permits enabling motion. `[sdk high]`
85. Carry controller (`player_pickup`): the object's collision group becomes INTERACTIVE_DEBRIS while held; the grab controller ignores pitch and aligns angles to the nearest 30° (dot 0.866); shadow controller: damp factor 1.0, teleport distance 0, max speed 1000 u/s, max angular 3600 °/s, max damp speed 2000 u/s, max damp angular 3600 °/s; held object mass is set to 1 kg (children 1/factor). For mass above `physcannon_maxmass` (250) the speeds are lerped (spline remap of mass 250..500 → 0..1) toward 300 u/s (= (1/40)·60·200) and an inertia‑based angular limit. Hold position: eyes −8 u level, down to 2 u above feet, up to 24 u above eyes, pitch range 30°, reach‑down 24 u; the target error is smoothed over ≤ 1 s. `[sdk high]`
86. Drop conditions each use‑tick: use released/USE_OFF, +attack2, position error > 12 u, object no longer moveable; `+attack` while holding throws: force = `player_throwforce` (1000) × clamp(mass, 0.5, 15) along the eye direction applied at the centre. `[sdk high]`

---

## 7. Vehicles (jeep / airboat basics)
87. Four‑wheel vehicle physics parses `scripts/vehicles/*.txt` (axleCount, wheels per axle, wheel radius, engine.gearCount validated with warnings; "does not exist" removes the vehicle), creates the body as SOLID_VPHYSICS and the vphysics vehicle controller; wheel attachments `wheel_fl/fr/rl/rr`, raycast attachments `raytrace_*`; pose parameters `vehicle_wheel_{fl,fr,rl,rr}_{height,spin}`, `vehicle_steer`, `vehicle_guage`, `vehicle_action`. `[bin high]`
88. Controls: throttle 0..1 (xbox_autothrottle/throttlebias convars consulted), brake (max 1.0, back‑then‑forward scalar 2.0, `r_vehicleBrakeRate` 1.5), steering approaches the target at the script's steering rate × frame time (slow/fast/boost angles from the script; jeep script 50°/15°), handbrake forced on while no driver; throttle rate = 1/engine.throttleTime (instant if unset); engine sound filter PAS attenuation 0.8; boost available when the script's `boostDelay` > 0, turbo sound ≥ 2.75 s; wheel dust size remaps speed 5..maxSpeed → 0..1. 400 u extents for the vehicle's wheel sticking traces. `[bin med, sdk high]`
89. Driveable vehicle: `vehicle_driver_eyes` attachment for the view, outputs for attack/attack2 presses, `TurnOn`/`TurnOff` inputs, exit allowed below 10 u/s (jeep & airboat), overturned exit wait 2 s, `g_jeepexitspeed` 100. `[bin med, sdk high]`
90. Jeep (`prop_vehicle_jeep`): gun pose params `vehicle_weapon_yaw/pitch`, `gun_spin` at 20/s, cannon pitch ±20°, yaw ±90°, `sk_jeep_gauss_damage` 15, ammo crate close delay 2 s, sounds `PropJeep.FireCannon`, `PropJeep.FireChargedCannon`, `PropJeep.AmmoOpen/Close`, `Jeep.GaussCharge`; effects `GaussTracer`, `AR2Tracer`, `ImpactJeep`, `RedGlowFade`, `waterripple`, `watersplash`; seagull perching (think 10 s, poop 45 s, hidden 15 s, max 60 s; disabled with `no_seagulls_on_jeep`); **GMOD‑DIFF**: episodic behaviour is chosen by matching the current map name (prefix `maps/`) against an "episodic" list. `[bin high, sdk high]`
91. Jeep/airboat damage: damage force is scaled down (constant), DMG_VEHICLE‑type hits halved/limited, components sanity‑checked for NaN. `[bin med]`
92. Airboat (`prop_vehicle_airboat`): convars `airboat_fatal_stress 5000` (kg of stress that kills the driver; checked every physics update via stress damage), `sk_airboat_max_ammo 100`, `sk_airboat_recharge_rate 15`, `sk_airboat_drain_rate 10`; no brake pedal; sounds `Airboat_engine_idle/fullthrottle/start/stop`, `Airboat_fan_idle/fullthrottle`, `Airboat_water_fast/stopped`, `Airboat_impact_hard/splash`, `Airboat_headlight_on/off`, gun `Airboat.FireGunHeavy/Loop/RevDown`, effects `AirboatGunTracer/HeavyTracer/Impact/MuzzleFlash`, `effects/splashwake1`, `effects/splashwake4`, `watersplash`; `propeller_spin1` sequence; pontoon/water handled by a PD controller (coefficients k = 1/(1 + dt·2ζω + dt²ω²) style); cannon pitch up 60°; `Frame_Flex_L/R`, `Steer_Shock`, `Gauge` pose params; splash ripple size 20. `[bin high, sdk high]`
93. The vehicle controller itself (suspension, tyre friction, engine torque curve) is in vphysics and is **not** in this dump; those numbers must come from the vehicle scripts. `[inf high]`

---

## 8. Doors
### 8.1 prop_door_rotating
94. Keyvalues: `hardware` (0 none, 1 lever, 2 push bar, 3 keypad → body group), `returndelay` (−1 = never auto‑close), `soundmoveoverride`, `soundopenoverride`, `soundcloseoverride`, `soundlockedoverride`, `soundunlockedoverride`, `slavename`, `dmg` (block damage), `forceclosed`, `spawnpos` (0 closed, 1 open forward, 2 open back, 3 ajar), `opendir` (0 both, 1 forward only, 2 backward only), `axis`, `distance` (degrees, default 90), `ajarangles`; speed 0 → 100 °/s. `[sdk high, bin high]`
95. Sounds from the model's `door_options` block: `skin%d` → `open`, `close`, `move`, `surfaceprop`; `hardware%d` → `locked`, `unlocked`; defaults `DoorSound.Null`; "Unlocked prop_door … has no hardware" warning (doors with hardware 0 should be locked). `[bin high]`
96. States: CLOSED 0, OPENING 1, OPEN 2, CLOSING 3, AJAR 4. `OnUse`: locked → `ACT_DOOR_LOCKED` animation + locked sound; closed/ajar → open away from the activator (direction = sign of dot(activator→door, door forward), overridden by `opendir`), open → close; `OnLockedUse`. `[bin high]`
97. Open: `OnOpen` output, AI sound inserted, open sound, angular move toward the goal angle at `speed` °/s (angular velocity = Δangle × speed/|Δangle|, done after |Δangle|/speed s); on arrival `OnFullyOpen`, NPC notification, and if `returndelay` ≠ −1 an auto‑close think at now + returndelay. Close: `OnClose`, `OnFullyClosed`. `OnBlockedOpening/Closing`, `OnUnblocked…` fire from the blocked handlers; block damage `dmg` to the blocker. `[bin high]`
98. Door clearance: before opening the swept door volume (computed at spawn for forward and back open angles) is tested; blocked → the other direction is tried. `[bin high]`
99. `AreaPortal` keyvalue links a `func_areaportal` toggled with the door. `[bin high]`
100. **GMOD‑DIFF**: door sound recipient filters are post‑processed (PAS fix) so door sounds reach the right players. `[bin med]`

### 8.2 func_door / func_door_rotating
101. Defaults/sounds: `DoorSound.DefaultMove/Arrive/Locked` (`RotDoorSound.*` for rotating), `DoorSound.Null`; legacy locked sound `buttons/latchlocked1.wav`; speed 0 → 100 u/s; `dmg` block damage; `wait` (−1 = stay open); `lip`; `locked_sentence`/`unlocked_sentence`; `noise1`/`noise2`. Spawnflags: 1 start open, 2 rotate backwards, 4 non‑solid to player, 8 passable, 16 one‑way, 64 roll, 128 pitch, 256 +use opens, 512 NPCs can't, 1024 touch opens, 2048 locked, 4096 silent, 16384 silent to NPCs, 32768 ignore use, 65536 new use rules. `[bin high, sdk high]`
102. Arrive sound at top/bottom: PAS attenuation 0.8, reliable; outputs `OnOpen`, `OnClose`, `OnFullyOpen`, `OnFullyClosed`, `OnBlockedOpening/Closing`, `OnLockedUse`. Moving sound think loops the move sound while moving unless silent. Blocked: deals `dmg` to the blocker; if `dmg` is 0 and the door is not forced closed it reverses direction (SDK logic). Use on a locked door plays lock sounds (sound wait 1 s, sentence wait 6 s). `[bin high, sdk high]`
103. "Start Open" with "Spawn Position = Open" conflict and `ignoredebris` conflicts are warned about and reverted. `[bin high]`

---

## 9. Buttons (func_button / func_rot_button / momentary_rot_button)
104. Defaults: speed 0 → 40 u/s, wait 0 → 1 s (−1 = stay pressed), lip 0 → 4 u; `sounds` index → `buttons/button%d.wav`, `locked_sound`, `unlocked_sound`, `locked_sentence`, `unlocked_sentence`, `health` (damage activates when > 0), `min_use_angle` (momentary). Spawnflags: 1 don't move, 0x20 toggle, 0x40 spark if off (spark think every 0.5 s), 0x100 touch activates, 0x200 damage activates, 0x400 use activates, 0x800 locked. `[bin high, sdk high]`
105. Press logic (use/touch/damage): if locked → lock sounds and `OnUseLocked`; state machine NOTHING / ACTIVATE / RETURN: pressing while at rest → `OnPressed`, sound (PAS 0.8), move to position 2 (origin + movedir × (|size·movedir| − lip)), `OnIn` on arrival; non‑toggle buttons return after `wait` s (`OnOut` when back home); toggle buttons return on the next press; `OnDamaged` when damaged. Player‑ally/NPC filters as SDK. `[bin high]`

---

## 10. Triggers
106. Trigger filtering spawnflags: 1 clients, 2 NPCs, 4 pushables (`func_pushable`), 8 physics objects (SOLID_VPHYSICS), 0x10 only player‑ally NPCs, 0x40 everything except debris, 0x400 also debris; plus the optional `filtername` filter entity; `showtriggers` convar draws them; `StartDisabled` keyvalue. `[bin high, sdk high]`
107. Outputs: `OnStartTouch` for every entering entity, `OnStartTouchAll` when the first enters; `OnEndTouch` when one leaves, `OnEndTouchAll` when the last leaves; `Enable`/`Disable`/`Toggle` inputs. `[bin high]`
108. `trigger_multiple`: `wait` 0 → 0.2 s; on touch, if the re‑arm time has passed: `OnTrigger` (activator = toucher), then re‑arm at now + wait (wait < 0 → the trigger removes itself after firing; wait 0 → 0.2 s). `trigger_once` = wait −1. `[bin high]`

---

## 11. Brush entities
109. `func_movelinear` / `func_water_analog` / `momentary_door`: `movedir`, `movedistance`, `speed`, `startposition` (0..1), `startsound`/`stopsound`, `blockdamage`; inputs `Open`, `Close`, `SetPosition`, `SetSpeed`; outputs `OnFullyOpen/Closed`; debug "Current Pos / Target Pos". `[bin med, sdk high]`
110. `func_rotating`: `maxspeed`, `fanfriction` (spin‑up/down rate), `message` (sound), `volume`, spawnflags 1 start on, 2 reverse, 4 x‑axis, 8 y‑axis, 16 accel/decel, 32 fan pain, 64 not solid, 128 small radius …; spin‑up/down ramp sound pitch and volume with speed; blocked deals `dmg`. `[bin med, sdk high]`
111. `func_brush`: `Solidity` (0 toggles, 1 never, 2 always), `StartDisabled`, `InvertExcludeNPCs`, `excludednpc`; `Enable/Disable/Toggle/SetExcluded/SetInvert`. `[sdk high]`
112. `func_physbox`: spawnflags as §2.1 but at 0x1000 asleep, 0x2000 ignore use, 0x4000 debris, 0x8000 motion disabled, 0x10000 use preferred carry angles, 0x20000 enable on physcannon, 0x80000 pickup output, 0x100000 always pick up, 0x200000 never pick up; keyvalues `massScale`, `Damagetype`, `overridescript`, `damagetoenablemotion`, `forcetoenablemotion`, `preferredcarryangles`, `notsolid`, `health`, `material`, `explosion`, `gibdir`, `spawnobject`, `explodemagnitude`, `propdata`; "Mass: %.2f kg / %.2f lb" debug text. `[sdk high, bin med]`
113. `func_breakable`: `material` (0 glass … 10 none), `explodemagnitude`, `health`, `spawnobject`, `gibdir`, `propdata` index; `func_break_max_pieces` 15 pieces per break with `func_break_reduction_factor` 0.5; damage modifiers from `func_breakdmg_*`. `[sdk high]`
114. `path_track`: `altpath`, `speed`, `radius`, `orientationtype`; spawnflags 1 disabled, 2 fire once, 4 branch reverse, 8 disable train, 16 teleport to this; `EnableAlternatePath`/`Disable…`/`Toggle…`, `EnablePath`/`DisablePath`/`TogglePath`, `OnPass`. `[sdk high, bin med]`

---

## 12. Entity I/O (semantics the props depend on)
115. The core dispatch (inputs by name, `OnXxx` outputs with target, input, parameter, delay, times‑to‑fire (−1 infinite), event queue sorted by fire time, `!activator`/`!caller`/`!self`/`!player` targets, `AddOutput`, `Kill`, `KillHierarchy`, `FireUser1..4`/`OnUser1..4`, `Use`) is not in the dump; it is the SDK's entity output system and GMod exposes it via `Entity:Fire` / `Entity:Input` with the same delay semantics. `[sdk high, inf]`
116. Logic entities present (per factory list): `logic_auto` (`OnMapSpawn`, `OnNewGame`, `OnLoadGame`, `OnMapTransition`, `OnBackgroundMap`, `OnMultiNewMap`, `OnMultiNewRound`, flag 1 remove on fire), `logic_relay` (`Trigger`, `Enable/Disable/Toggle`, `CancelPending`, flags 1 only once, 2 allow fast retrigger; `OnTrigger`, `OnSpawn`), `logic_timer` (`RefireTime`, `UseRandomTime`, `LowerRandomBound`, `UpperRandomBound`, flag 1 oscillator; `OnTimer`, `OnTimerHigh/Low`), `logic_branch`, `logic_branch_listener`, `logic_case` (16 cases, `PickRandom`, `PickRandomShuffle`), `logic_compare`, `logic_compare_integer`(?), `logic_collision_pair`, `logic_measure_movement`, `logic_random_outputs`, `logic_eventlistener`, `logic_lineto`, `logic_navigation`, `logic_playerproxy`, `logic_register_activator`, `logic_coop_manager` (Portal 2), `math_counter` (min/max, `Add/Subtract/Multiply/Divide/SetValue/GetValue`, `OutValue`, `OnHitMin/Max`), `math_remap`, `math_colorblend`, filters (`filter_activator_class/name/model/context/mass_greater/enemy/multi`, `filter_combineball`), `point_template`, `env_entity_maker`, `point_entity_finder`. The SDK definitions apply. `[bin high (presence), sdk high (semantics)]`

---

## 13. Numbers quick‑reference
| Item | Value | Tag |
|---|---|---|
| prop default impact energy scale | 0.1 | bin high |
| preferred carry angles | (-5, 0, 0) | bin high |
| burst speed of gibs | 100 u/s | bin+sdk high |
| chunk gib fade | random(5,10) s | sdk high |
| gib entity lifetime / friction | 25 s / 0.55 | bin high |
| ignite durations | burn 10–15 s, flare 25 s, explode_fire 30 s | bin high |
| constraint force/torque limit conversion | ×0.4536 (lbs→kg) | bin high |
| break sound attenuation | 1.25 | bin high |
| door/button arrive sound attenuation | 0.8 | bin high |
| prop_door speed / distance | 100 °/s / 90° | bin+sdk high |
| func_door speed | 100 u/s | bin+sdk high |
| func_button speed / wait / lip | 40 u/s / 1 s / 4 u | bin+sdk high |
| trigger_multiple wait | 0.2 s | bin high |
| ragdoll max elements | 32 | bin high |
| ragdoll fade default | 0.5 s | sdk high |
| boogie on pickup | magnitude 150, 2.0 s, impulses every 0.1–0.2 s | bin high |
| +use pickup limits | 35 kg, 128 u | sdk high |
| carry controller | 1000 u/s, 3600 °/s, held mass 1 kg, error drop 12 u | sdk high |
| throw force | 1000 × clamp(mass,0.5,15) | sdk high |
| respawnable prop | 60 s | bin high |
| airboat fatal stress | 5000 kg | bin high |
| g_ragdoll_maxcount | 8 | sdk high |
