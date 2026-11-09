# Gravity gun (`weapon_physcannon`) — behaviour note for the Rust rewrite

Scope: GMod's `weapon_physcannon` as it really runs (Linux `server_srv.so`, Windows `client.dll`), cross-checked
against Source SDK 2013 (`src/game/shared/hl2mp/weapon_physcannon.cpp` — GMod is derived from the HL2MP
*shared/predicted* variant, not the single-player CSprite one) and the Rust repo's own measured convar table.

Tag legend (every fact): `[src: …; conf: high|med|low]`
- `bin-srv` = read in the server pseudo-C, `bin-cl` = read in the client pseudo-C, `bin-str` = string/convar table of the binary,
  `sdk` = Source SDK 2013 HL2MP shared file, `sdk-sp` = SDK single-player file, `script` = `scripts/weapon_physcannon.txt`,
  `measured` = `docs/spec/convars-defaults.md` / `docs/spec/server.md` recordings in gmod-rust, `inf` = inference.
- Unnamed float constants in the binary (Ghidra `DAT_*`) are tagged `conf: med` when the surrounding code and the SDK agree on
  the value, `conf: low` when only the SDK value is available.

Units: Source units (u, 1 u = 0.75 in), seconds, kg, degrees. "Force" through `ApplyForceCenter` is an impulse in
kg·u/s (vphysics integrates it as Δv = F / m in one step). Tick = 1/66.67 s on srcds.

---

## 0. Summary of GMod ≠ SDK (binary wins)

| Topic | SDK 2013 (HL2MP) | GMod binary | tag |
|---|---|---|---|
| Server precache | `sprites/orangelight1.vmt`, `glow04_noz`, `orangeflare1`, `orangecore1`, `orangecore2` (+ mega set) | only `sprites/orangelight1_gmod.vmt`, `sprites/orangelight1_noz.vmt`, `sprites/lgtning.vmt`, `sprites/lgtning_noz.vmt`, sound `Weapon_PhysCannon.HoldSound` | `[bin-srv Precache; high]` |
| Client sprite materials | `sprites/orangecore1`, `orangecore2`, `glow04_noz`, `orangeflare1`, mega `blueflare1_noz`, `effects/fluttercore` | same names with `_gmod` suffix: `sprites/orangecore1_gmod`, `sprites/orangecore2_gmod`, `sprites/glow04_noz_gmod`, `sprites/orangeflare1_gmod`, `sprites/blueflare1_noz_gmod`, `effects/fluttercore_gmod` | `[bin-cl strings; high]` |
| Grab controller stiffness | `maxSpeed 1000`, `maxDampSpeed 2000` | `maxSpeed 2500`, `maxDampSpeed 5000` (hex `0x451c4000`, `0x459c4000` in the controller ctor) | `[bin-srv ctor; high]` |
| Holding core-sprite scale | 16 (viewmodel) / 14 (world) | **14 (viewmodel) / 13 (world)** | `[bin-cl; high]` |
| Idle glow flicker | scale `rand(0.05,0.075)·128`, alpha `randint(24,32)` | scale `rand(0.05,0.075)·128`, alpha **`randint(16,24)`** (dimmer dots) | `[bin-cl; high]` |
| Mega physcannon | compile-time / upgrade sequence | runtime flag: `physcannon_mega_enabled 1` **or** global state entity `super_phys_gun == 1`, re-evaluated every frame; off in sandbox | `[bin-srv Spawn/ItemPostFrame; high]` |
| Pickup slows the player | `EnableSprint(false)` / `SetMaxSpeed` on attach, restored on drop | **absent** (no speed change while carrying) | `[bin-srv AttachObject/DetachObject; high]` |
| Punt of a held object | emits a chain of `SOUND_DANGER` AI sounds ahead | **absent** | `[bin-srv LaunchObject; high]` |
| Punting players | `PuntNonVPhysics` damages any `m_takedamage` entity | hit entity that `IsPlayer()` → dry fire, nothing happens to players | `[bin-srv PrimaryAttack; high]` + `[measured hit_physcannon*; high]` |
| Lua hooks | none | `GravGunPunt`, `GravGunPickupAllowed`, `GravGunOnPickedUp`, `GravGunOnDropped` (return `false` blocks) | `[bin-srv; high]` |
| Multi-holder flag | `FVPHYSICS_PLAYER_HELD` cleared on detach | cleared only if no other player still holds the object | `[bin-srv DetachEntity; med]` |
| Barnacle-grabbed object | — | position error ×3 while entity flag "barnacle" set (faster forced drop) | `[bin-srv ComputeError; med]` |
| Vehicles | heavy-object punt max mass 250 | punt mass cap **625** for vehicles (250 otherwise); force zeroed for vehicles whose first phys object is a static constraint | `[bin-srv PuntVPhysics; med]` |
| `physcannon_instant` convar | — | GMod-only: on `Deploy` sets next primary/secondary **and** the player's `m_flNextAttack` to now (no draw delay) | `[bin-srv Deploy; high]` |
| Client pickup trace | line → 4 u hull | same **plus** a brush-only re-trace from the muzzle to the hull hit; if a brush is hit first (or start/all solid) the candidate is discarded (client prediction only; server not seen doing it) | `[bin-cl FindObjectTrace; med]` |

Everything not listed here matched the SDK where both could be read.

---

## 1. Convars and defaults

| ConVar | Default | Used for | tag |
|---|---|---|---|
| `physcannon_tracelength` | 250 u | punt reach, pickup attach reach (×1 for the punt, ×4 for the pickup trace) | `[bin-str + measured; high]` |
| `physcannon_maxmass` | 250 kg | heaviest object the normal gun may carry; heavy-object speed law; load percentage | `[bin-str + measured; high]` |
| `physcannon_maxforce` | 1500 | launch speed for ≤100 kg | `[bin-str + measured; high]` |
| `physcannon_minforce` | 700 | launch speed at ≥600 kg | `[bin-str + measured; high]` |
| `physcannon_pullforce` | 4000 | pull impulse per pull tick | `[bin-str + measured; high]` |
| `physcannon_cone` | 0.97 (cos ≈ 14.1°) | cone search when the trace misses | `[bin-str + measured; high]` |
| `physcannon_ball_cone` | 0.997 | combine-ball cone (unused in sandbox) | `[measured; high]` |
| `physcannon_punt_cone` | 0.997 | SDK-only helper, not referenced by the GMod punt path read | `[sdk; low]` |
| `physcannon_chargetime` | 2 s | upgrade sequence only (mega) | `[measured; high]` |
| `physcannon_mega_enabled` | 0 | runtime mega switch | `[bin-str + measured; high]` |
| `physcannon_instant` | (GMod) | instant deploy, see §9 | `[bin-srv Deploy; high; default not read]` |
| `player_throwforce` | 1000 | +USE throw, not the gravgun | `[sdk-sp; high]` |
| `hl2_normspeed` | 190 u/s | ×1.5 = 285 u/s drop-velocity clamp | `[sdk; med]` |

Global state entity `super_phys_gun` (value 1) also turns the mega mode on. `[bin-srv Spawn; high]`

---

## 2. Weapon data (`scripts/weapon_physcannon.txt`)

- Viewmodel `models/weapons/c_physcannon.mdl` (GMod `c_` model with bone-merged hands; mega: `models/weapons/c_superphyscannon.mdl`, skin index 2 set on the weapon when mega is on), world model `models/weapons/w_Physics.mdl`, anim prefix `gauss`, bucket 0 position 2, no ammo/clip, crosshair font glyph `Q`. `[script + bin-srv SetViewModel/Spawn; high (mega model) / med (skin index)]`
- Sound slots → script entries: `single_shot`=`Weapon_PhysCannon.Launch`, `empty`=`.DryFire`, `special1`=`.Pickup`, `special2`=`.OpenClaws`, `melee_hit`=`.CloseClaws`, `melee_miss`=`.Drop`, `special3`=`.TooHeavy`, `reload`=`.Charge` (unused in sandbox). Hold loop: `Weapon_PhysCannon.HoldSound` (mega: `Weapon_MegaPhysCannon.HoldSound`, `.Launch`, `.DryFire`, `.Drop`, `.Pickup` override the first four when mega). `[script + bin-srv GetShootSound/GetMotorSound; high]`
- Viewmodel activities: `ACT_VM_PRIMARYATTACK` (pickup, drop, dry fire — and the client's predicted drop), `ACT_VM_SECONDARYATTACK` (punt / launch), `ACT_VM_RELOAD` = holding idle, `ACT_VM_IDLE` = idle and on open/close, `ACT_VM_DRAW` on deploy. In `c_physcannon.mdl` these are the `fire`, `altfire`, `hold_idle`, `idle`, `draw` sequences. `[bin-srv WeaponIdle/Open/Close/Punt*; high for acts; med for sequence names (HL2 model)]`
- Claw pose parameter `active` (0 closed … 1 open) on the viewmodel (first person) and on the weapon model (third person): on open the client interpolates it from the current value to 1 over **0.2 s**, on close to 0 over **0.5 s**, both with spline easing. `[bin-cl element-pose fn + sdk; high]`

---

## 3. State machines

### 3.1 Effect state (networked `m_EffectState`)
`NONE(0) → CLOSED(1) → READY(2) → HOLDING(3) → LAUNCH(4)`; `Deploy` sets READY after closing the claws; `ItemPostFrame` sets READY whenever +attack2 is not held and nothing is held; attach sets HOLDING; punt sets LAUNCH (client-side it only triggers the blast and goes back through READY); `StopEffects` sets NONE. `[bin-srv; high]`

### 3.2 Element (claw) state
Three values: `-1 no change pending`, `0 open requested`, `1 close requested` (`m_nChangeState`) plus `m_bOpen`. Processed in `ItemPostFrame` when `now > m_flElementDebounce`. `[bin-srv ItemPostFrame; high]`

Timers set by the game (seconds from now):

| Event | next primary | next secondary | change state | element debounce | check suppress | tag |
|---|---|---|---|---|---|---|
| Punt (any kind) | +0.5 | +0.5 (vphysics punt) / unchanged (non-vphysics) | close | +0.5 | +0.25 | `[bin-srv Punt*; high]` |
| Dry fire (no target) | +0.5 | — | — | — | — | `[bin-srv PrimaryAttack; high]` |
| Launch held object | +0.5 | +0.5 | close | +0.1 | +0.25 | `[bin-srv LaunchObject; high]` |
| Pickup (attach) | — | +0.4 (attach) then +0.5 (SecondaryAttack) | open now | — | — | `[bin-srv AttachObject/SecondaryAttack; high]` |
| Secondary, nothing found | — | +0.1 | close now | — | — | `[bin-srv SecondaryAttack; high]` |
| Secondary, object refused by `Pickup_OnAttemptPhysGunPickup` | — | +0.01 | — | — | — | `[bin-srv; high]` |
| Drop (secondary while holding) | +0.5 | +0.5 | — | — | — | `[bin-srv; high]` |
| Target lost in `CheckForTarget` | — | — | close | +0.5 | — | `[bin-srv CheckForTarget; high]` |
| Re-punt guard | the same entity cannot be punted again for 0.5 s | | | | | `[bin-srv; high]` |

Measured confirmation: holding +attack with nothing in reach punts every 34 ticks (0.5 s re-armed from "now"), and a fresh `Give` leaves the secondary timer at now. `[measured fire_physcannon_hold; high]`

### 3.3 Target feeler (`CheckForTarget`, every frame while not holding)
Skipped while `now < check-suppress` or the weapon has `EF_NODRAW`. Runs the pickup trace (§4.1); if nothing is hit, or the hit point is farther than `physcannon_tracelength` from the muzzle, or the object cannot be picked up and does not force the claws open (`Pickup_ForcePhysGunOpen`), then — if no change is pending and the debounce expired — request close with debounce +0.5 s; otherwise clear the pending change and open the claws immediately. `[bin-srv CheckForTarget; high]`

---

## 4. Finding an object

### 4.1 Pickup trace (`FindObjectTrace`)
Start = `Weapon_ShootPosition` (eye), direction = eye forward, length = **4 × `physcannon_tracelength` = 1000 u**. Line trace with the physcannon filter (ignores the owner, mask `MASK_SHOT`); if it did not hit a non-world entity, a **4 u half-size hull** (−4..4) is swept along the same ray. The hit entity is replaced by its root move-parent. `[bin-srv FindObjectTrace + bin-cl; high]`
Client-side addition: see §0 (brush re-trace). `[bin-cl; med]`

### 4.2 Attach vs pull (`FindObject`)
- Direct hit with `fraction ≤ 0.25` (≤ 250 u) → attach; `fraction > 0.25` → pull. `[bin-srv FindObject; high]`
- No direct hit → cone search (§4.3); a cone candidate is attached if `|center − eye|² ≤ (1000 u)²`, else pulled. `[bin-srv; high]`
- If `CanPickupObject` fails, `Pickup_OnFailedPhysGunPickup(entity, muzzle)` may substitute another entity; if that also fails, play `Weapon_PhysCannon.TooHeavy` once (latched until the next close/open cycle) and return "not found". `[bin-srv; high]`
- `Pickup_OnAttemptPhysGunPickup(entity, player, PICKED_UP_BY_CANNON)` returning false → "being detached" (secondary re-tries after 0.01 s). `[bin-srv; high]`
- Attach → `AttachObject` (§5); success plays `Weapon_PhysCannon.Pickup` + `ACT_VM_PRIMARYATTACK`. `[bin-srv SecondaryAttack; high]`

### 4.3 Cone search (`FindObjectInCone`)
Box of half-size `tracelength + 1` around the muzzle, up to 256 entities that own a physics object; keep the nearest whose center is closer than the current best (initial best = `tracelength + 1`), whose normalized `(center − muzzle)·forward > physcannon_cone (0.97)`, and that the physcannon line trace from the muzzle to its center actually reaches. `[bin-srv FindObjectInCone; high (structure) / med (radius +1)]`

### 4.4 Pull
`dir = normalize(muzzle − center)`, `F = physcannon_pullforce (4000)`, mega ×2; if `mass < 50 kg`: `F *= (mass + 0.5) / 50`; `ApplyForceCenter(dir · F)` once per secondary tick (every 0.1 s because "not found" re-arms the secondary at +0.1 s). So a 20 kg can receives Δv = 4000·0.41/20 = 82 u/s per pull, a 100 kg crate 40 u/s. `[bin-srv FindObject + sdk; high (law) / med (0.5, 50 constants)]`

### 4.5 `CanPickupObject` (normal mode)
In order: null → no; dissolving → no; `func_physbox` with `SF_PHYSBOX_ALWAYS_PICK_UP (0x100000)` or `NEVER_PICK_UP (0x200000)` → result is `!NEVER`; `prop_physics` with `SF_PHYSPROP_ALWAYS_PICK_UP (0x100000)` → yes; entity flag `EFL_NO_PHYSCANNON_INTERACTION` → no; the object is the player's ground entity → no; its physics object already has `FVPHYSICS_PLAYER_HELD` → no; Lua `GM:GravGunPickupAllowed(ply, ent)` returned false → no; combine ball → player rule with no limits; flesh (`VPhysicsIsFlesh`, i.e. ragdolls/NPC corpses) → no (mega: allowed); else `CBasePlayer::CanPickupObject(ent, physcannon_maxmass = 250 kg, size 0 = unlimited)`. `[bin-srv CanPickupObject; high]`
`EntityAllowsPunts`: `func_physbox` with `SF_PHYSBOX_NEVER_PUNT (0x400000)` and weapons with `SF_WEAPON_NO_PHYSCANNON_PUNT (4)` refuse punts. `[bin-srv; high]`

---

## 5. Carrying (grab controller)

### 5.1 Attach
- `AttachObject` refuses if already holding or `CanPickupObject` fails; needs a physics object. Calls `Physgun_OnPhysGunPickup` (→ `Pickup_OnPhysGunPickup(ent, ply, PICKED_UP_BY_CANNON)`), attaches, stores the networked object-space grab offset and player-space angles, sets the player as owner if the entity had none (restored on drop), secondary +0.4 s, effect HOLDING, opens the claws if closed, starts the motor sound at volume 0 / pitch 50 and ramps pitch→100 and volume→0.8 over **0.5 s**, then Lua `GM:GravGunOnPickedUp(ply, ent)`. `[bin-srv AttachObject; high]`
- Controller attach: plays the impact sound of the object's material against the player's material (`PhysicsImpactSound`, volume 1.0, speed 64); saves `BlocksLOS` and sets it false; creates a motion controller, attaches the object, wakes it, sets `FVPHYSICS_PLAYER_HELD`; for every physics part of the entity saves mass, rotational damping and material index, sets mass to `1 kg / max(1, parts/7.5)` and rotational damping to **10**; the grabbed part itself gets mass 1 kg and drag disabled; `m_flLoadWeight` = sum of the original masses; `errorTime = −1.0 s` (mega −1.5), `error = 0`, `contactAmount = 0`. `[bin-srv AttachEntity + sdk; high (structure) / med (1 kg, 10)]`
- Carry angles: player-space angles of the object at grab time; if the prop data defines preferred carry angles they are used instead (transformed by the player's yaw matrix). The gravgun does **not** snap angles to 90° (angle alignment = 0; only the +USE carry uses cos 30° = 0.866 snapping). Grab point: object-space `WorldSpaceCenter` (ragdolls: the grabbed bone, offset 0). `[bin-srv AttachEntity/ UpdateObject; high]`

### 5.2 Controller constants
`dampFactor 1.0`, `teleportDistance 0`, `maxSpeed 2500 u/s`, `maxDampSpeed 5000 u/s`, `maxAngular 3600 °/s` (`DEFAULT_MAX_ANGULAR`, binary constant unnamed), `maxDampAngular = maxAngular`; `timeToArrive = gpGlobals->frametime` (one tick, set every `SetTargetPosition`). `ComputeMaxSpeed` (heavy-object remap 1000 u/s → 300 u/s between `maxmass` and 500 kg) exists but is **not called** on attach, so it is inert. `[bin-srv ctor/SetTargetPosition/AttachEntity; high (2500/5000) / med (3600)]`

### 5.3 Per-tick target (`UpdateObject`, once per tick, called from `ItemPreFrame`; drop if it returns false)
1. Fail (→ `DetachObject`) when: no entity, `ComputeError() > 12 u` (mega 18), the player stands on the object, no physics object or it is not moveable. `[bin-srv UpdateObject (both); high]`
2. `pitch = clamp(AngleDistance(eyePitch, 0), −75°, +75°)`; forward from (pitch, eye yaw). (No "allow overhead" −90° variant in GMod's gravgun path.) `[bin-srv + sdk; med]`
3. `radial = CollideGetExtent(object, −forward)`, `playerRadius = Length2D(player OBB maxs)` (= 22.63 u for the 16 u hull), `radius = playerRadius + |forward · radial|`, `distance = 24 + 2·radius`. `[bin-srv + sdk; high]`
4. Trace a ray from the eye `distance` along forward with `MASK_SOLID_BRUSHONLY`, ignoring the player and the object: `fraction < 0.5` → `end = eye + forward · radius/2`, else `end = eye + forward · (distance − radius)`. `[bin-srv + sdk; high]`
5. Keep-out: nearest point on the player's vertical center line (z clamped to the hull); if `|end − nearest| < radius`, push `end` out to `radius`. `[bin-srv + sdk; high]`
6. Angles = stored player-space angles re-expressed with the current player matrix (or preferred carry angles); target origin = `end − rotate(grabOffset, angles)`; `SetTargetPosition` (wakes the object; if the object vanished → detach). `[bin-srv; high]`

### 5.4 Physics step (`Simulate`, motion controller)
- Contact scan: for every friction contact of the held object, if the other body is not moveable **or** heavier than `m_flLoadWeight`: `contactAmount = Approach(0.1, contactAmount, 2·dt)` and stop scanning; if no such contact: `Approach(1.0, contactAmount, 2·dt)`. `maxAngular_eff = maxAngular · contactAmount³` (the object stops torquing when pressed against walls/heavy things). `[bin-srv Simulate + sdk; high]`
- `timeToArrive = ComputeShadowControl(params, timeToArrive, dt)`; then the resulting velocity is passed through `PhysComputeSlideDirection` (removes the component pushing into a static contact) and written back; `errorTime += dt`; linear/angular outputs 0 (`SIM_LOCAL_ACCELERATION`). `[bin-srv Simulate + sdk; high]`
- `ComputeError`: `error = |shadow target − object position|`; if `errorTime > 0`: smoothed `m_error = (errorTime > 1 ? error : lerp(m_error, error, errorTime))` and if the speed needed (`error/errorTime`) exceeds `maxSpeed` the error is halved first (benefit of the doubt right after attach); `errorTime = 0` after use; ×3 for a barnacle-held entity (GMod). Entity without physics → detach and return 9999. `[bin-srv ComputeError + sdk; med]`

### 5.5 Detach / drop
- `DetachObject(launched, playSound)`: controller detach, `Pickup_OnPhysGunDrop(ent, ply, launched ? LAUNCHED_BY_CANNON : DROPPED_BY_CANNON)`, Lua `GM:GravGunOnDropped(ply, ent)`, motor sound volume→0 and pitch→50 over **1.0 s**, owner restored, `m_bActive=false`; `playSound` → `Weapon_PhysCannon.Drop`. `[bin-srv DetachObject; high]`
- Controller detach restores `BlocksLOS`, and for every part: material index, mass, rotational damping, wake; clears `FVPHYSICS_PLAYER_HELD` unless another player still holds it (GMod). If not launching: **clamp** linear speed to `hl2_normspeed × 1.5 = 285 u/s` and angular to `720 °/s`; if launching the velocity is cleared entirely before the launch impulse. `[bin-srv DetachEntity + sdk; high (structure) / med (285/720)]`
- Drop on secondary press while holding: primary and secondary +0.5, effect READY, `ACT_VM_PRIMARYATTACK`, drop sound. `[bin-srv SecondaryAttack; high]`
- Holster is refused while holding (`CanHolster` false); `Holster`, `Drop`, `UpdateOnRemove` and a mega-mode flip call `ForceDrop` = close claws + detach + `StopEffects(true)` (effect NONE, motor sound fade-out 0.1 s). `[bin-srv; high]`
- No player slowdown while carrying (GMod removed `EnableSprint(false)`/`SetMaxSpeed`). `[bin-srv; high]`

### 5.6 Load percentage
`clamp(m_flLoadWeight / physcannon_maxmass, 0, 1)` — networked for the HUD/viewmodel strain. `[bin-srv GetLoadPercentage; high]`

---

## 6. Punt (primary attack)

### 6.1 Target selection (`PrimaryAttack`)
- Holding: Lua `GM:GravGunPunt(ply, held)` false → nothing. If `|held.center − player.center| > tracelength` → next primary +0.5 and dry fire; else `LaunchObject(eye forward)` (§6.4), `PrimaryFireEffect`, `ACT_VM_SECONDARYATTACK`. `[bin-srv PrimaryAttack; high]`
- Not holding: next primary +0.5; **hull trace** (half-size 8 u, `MASK_SHOT`, ignore owner) from the eye `tracelength` forward; if it hit nothing/world/an entity flagged `EFL_NO_PHYSCANNON_INTERACTION`, retry with a **line** trace (physcannon filter). Still nothing → dry fire. `[bin-srv + sdk + bin-cl; high]`
- Hit entity with `MOVETYPE_VPHYSICS`: refuses if `EntityAllowsPunts` false or (normal mode) it is flesh → dry fire; mega + ragdoll → `PuntRagdoll`; else `PuntVPhysics`. `[bin-srv; high]`
- Other entities: `m_takedamage == DAMAGE_NO` → dry fire; **a player → dry fire (GMod)**; (mega) NPC → dissolve; else `PuntNonVPhysics`. `[bin-srv; high]`
- Dry fire = `Weapon_PhysCannon.DryFire` + `ACT_VM_PRIMARYATTACK`. `[bin-srv DryFire; high]`

### 6.2 `PuntVPhysics(entity, forward, trace)`
1. Re-punt guard: same entity within 0.5 s → return. Lua `GravGunPunt` false → return. Record entity, guard = now+0.5. `[bin-srv; high]`
2. Damage: `DispatchTraceAttack` 0 damage, type `DMG_PHYSGUN (0x800000)`, then `ApplyMultiDamage` (wakes damage-based logic, breaks nothing by itself). `[bin-srv; high]`
3. `Pickup_OnAttemptPhysGunPickup(ent, ply, PUNTED_BY_CANNON)`; if allowed:
   - get all physics parts (≤1024); no parts → `Physgun_OnPhysGunPickup(...PUNTED)` + dry fire.
   - if `forward.z < 0`: `forward.z *= −0.65` (aiming down flips the vertical component up, scaled). `[bin-srv + sdk; high]`
   - `Physgun_OnPhysGunPickup(ent, ply, PUNTED_BY_CANNON)` (unfreezes a frozen prop through the pickup rules; GMod's `Physgun_OnPhysGunPickup` also runs the "heavy object" AI interaction when mass > 625 kg under episodic rules — inert in sandbox). `[bin-srv; med]`
   - vehicle whose first part carries `FVPHYSICS_CONSTRAINT_STATIC` → `forward = 0`.
   - **Mass law (normal mode, unless `Pickup_ShouldPuntUseLaunchForces` says otherwise):**
     `total = Σ mass_i`; `maxMass = 250 kg` (625 for vehicles); `mass = min(total, maxMass)`;
     for each part: `ratio = mass_i / total`; grabbed part (the one `VPhysicsGetObject()` returns): `ratio = min(ratio + 0.5, 1)`, others `ratio *= 0.5`;
     `ApplyForceCenter(forward · 15000 · ratio)` and `ApplyForceOffset(forward · mass · 600 · ratio, trace.endpos)`.
     For a single-part prop: impulse `15000 + 600·min(m,250)` kg·u/s → Δv = `15000/m + 600·min(m,250)/m` u/s (e.g. 10 kg: 2100 u/s; 100 kg: 750 u/s; 250 kg: 660 u/s; 500 kg: 330 u/s), plus the torque from the off-center application point. `[bin-srv PuntVPhysics (constants 15000/600/0.5/250 read as named floats) + sdk; high]`
   - else (launch-forces objects, mega): `ApplyVelocityBasedForce` (§6.4).
4. Always afterwards (even if step 3 was refused): view punch `QAngle(rand(1,2), rand(−1,1), 0)`, `DoEffect(LAUNCH, trace.endpos)`, `PrimaryFireEffect`, `ACT_VM_SECONDARYATTACK`, close claws (debounce +0.5, suppress +0.25), next primary & secondary +0.5. `[bin-srv; high]`

### 6.3 `PuntNonVPhysics` (breakables, doors, NPCs without vphysics…)
Re-punt guard + Lua hook as above; `DispatchTraceAttack` **1.0 damage**, type `DMG_CRUSH|DMG_PHYSGUN (0x800001)`, force direction = forward, position = hit point; `ApplyMultiDamage`; `DoEffect(LAUNCH, hit)`, `PrimaryFireEffect`, `ACT_VM_SECONDARYATTACK`, close claws (+0.5 / suppress +0.25). No extra view punch, no velocity. `[bin-srv PuntNonVPhysics; high]`

### 6.4 Launch of a held / launch-forces object (`LaunchObject` → `ApplyVelocityBasedForce`)
- `LaunchObject`: re-punt guard, detach (velocity cleared), guard = now+0.5, `ApplyVelocityBasedForce(obj, forward, LAUNCHED_BY_CANNON)`, next primary & secondary +0.5, `DoEffect(LAUNCH, obj.center)`, clear held state, motor sound volume→0 / pitch→50 over 1.0 s, close claws (debounce +0.1, suppress +0.25). `[bin-srv LaunchObject; high]`
- `ApplyVelocityBasedForce`: `v = Pickup_PhysGunLaunchVelocity(obj, forward, reason)` → default: `speed = physcannon_maxforce (1500)`; if `mass > 100 kg`: `mass = min(mass, 1000)`, `speed = SimpleSplineRemapValClamped(mass, 100, 600, maxforce 1500, minforce 700)` (smoothstep in mass; 250 kg → ≈1356 u/s, 350 kg → 1100, ≥600 kg → 700); `ω = Pickup_PhysGunLaunchAngularImpulse(obj, reason)` → default `RandomAngularImpulse(−600, 600)` per axis (°/s); applied with `AddVelocity(v, ω)` to the object, or to every ragdoll bone. (No `HL2_EPISODIC` jeep branch in GMod.) `[bin-srv + sdk-sp player_pickup.cpp; high]`
- A launched held object therefore leaves at exactly 1500 u/s up to 100 kg regardless of mass, while a punt from the floor uses the impulse law of §6.2. `[inf from the two paths; high]`

### 6.5 `PuntRagdoll` (mega only reachable)
`Pickup_OnPhysGunDrop(ent, ply, LAUNCHED_BY_CANNON)`, 0 damage `DMG_PHYSGUN`, `OnAttemptPhysGunPickup`, `forward.z<0 → ·−0.65`, every bone gets `AddVelocity(forward · k)` (k = unnamed constant; SDK 1500 ×… not read), then the same recoil/effects/timers as §6.2 step 4. `[bin-srv PuntRagdoll; med]`

### 6.6 Shot feedback (`PrimaryFireEffect`)
`ViewPunch(QAngle(−6, SharedRandomInt("physcannonfire", −2, 2), 0))`, screen fade colour (245,245,255) alpha 32, 0.1 s, hold 0, `FFADE_IN`; `Weapon_PhysCannon.Launch`. Predicted on the client with the shared seed. `[bin-srv + bin-cl + sdk; high]`

---

## 7. Effects

### 7.1 Server → client
`DoEffect(state, pos)` writes the networked effect state; `LAUNCH` additionally dispatches the temp-entity effect **`PhyscannonImpact`** (origin = hit point or, if none, the held object's center or a 1024 u forward trace end; normal = `normalize(pos − muzzle)`; entity = the weapon; scale 1; PVS filter at the hit point; prediction rules when predicted). `[bin-srv DoEffectLaunch; high]`
Client handler (not in the dump; SDK HL2MP, GMod expected equal apart from the `_gmod` material): `FX_GaussExplosion` at the impact point (local player) and a beam from the weapon's attachment 1 (the viewmodel's attachment for the local player, `FormatViewModelAttachment`) to the impact: `sprites/orangelight1` (GMod precaches `sprites/orangelight1_gmod.vmt` for this), life **0.1 s**, width **12 → 4**, brightness 255, white, 16 segments, no noise amplitude, `FBEAM_ONLYNOISEONCE`. Other players only see the `FX_GaussExplosion` at the gun. `[sdk; med]`

### 7.2 Client sprite set (`StartEffects`, 11 parameters, drawn by `DrawEffects` during `ViewModelDrawn` in first person / `DrawModel(STUDIO_TRANSPARENCY)` in third person; each is a camera-facing `DrawSprite(pos, scale, scale, rgba)` at an attachment, viewmodel attachments formatted for first person)
| # | Attachment(s) | Material (normal / mega) | Initial scale | Initial alpha | Colour | tag |
|---|---|---|---|---|---|---|
| CORE | 1 (`muzzle`) | `sprites/orangecore1_gmod` / `effects/fluttercore_gmod` | 0 → 1 over 0.1 s | 255 | white | `[bin-cl; high]` |
| BLAST | 1 | `sprites/orangecore2_gmod` / `effects/fluttercore_gmod` | 0 → 1 over 0.1 s, hidden | 255 | white | `[bin-cl; high]` |
| GLOW1-6 | viewmodel: `fork1m fork1t fork2m fork2t fork3m fork3t`; world model: `fork1b fork1m fork1t fork2b fork2m fork2t` | `sprites/glow04_noz_gmod` / `sprites/blueflare1_noz_gmod` | 6.4 (=0.05·128) | 64 | (255,128,0) / blue | `[bin-cl; high (names, 6.4, 64) / med (128 green)]` |
| ENDCAP1-3 | `fork1t fork2t fork3t` | `sprites/orangeflare1_gmod` / `sprites/blueflare1_noz_gmod` | 6.4, hidden | 255 | white | `[bin-cl; high]` |
`SPRITE_SCALE = 128`. `[bin-cl; high]`

### 7.3 Per-state targets (interpolations are linear over the given time unless noted)
| State | CORE | GLOW1-6 | ENDCAPS | BEAMS | BLAST | tag |
|---|---|---|---|---|---|---|
| NONE | hidden | hidden | hidden | brightness 0 | hidden | `[bin-cl; high]` |
| CLOSED | unchanged | unchanged | hidden | — | — | `[bin-cl; high]` |
| READY | scale→**14** (vm) / **8** (world) over 0.2 s, alpha→128 (vm) / 0 (world) over 0.2 s, visible | scale→51.2 over 0.2 s, alpha→64 over 0.2 s, visible | hidden | — | — | `[bin-cl + sdk; high]` |
| HOLDING | scale→**14** (vm) / **13** (world) over 0.2 s, alpha→255 over 0.1 s | scale→64 over 0.2 s, alpha→64 over 0.2 s | 3 visible (vm) / 2 visible (world) | 3 beams `fork1t/2t/3t` → held object (vm), 2 beams `fork1t/2t` (world), brightness 255 | hidden | `[bin-cl; high]` |
| LAUNCH | — | — | — | — | scale 8→64 over 0.1 s, alpha 255→0 over 0.2 s, visible | `[sdk; med — GMod fn not in dump]` |
| Idle flicker (every frame, `UpdateEffects`) | (mega only: alpha rand 32..64, scale rand(0.10,0.18)·128 holding / rand(0.01,0.12)·128) | scale = `rand(0.05,0.075)·128` = 6.4..9.6, alpha `randint(16,24)` | scale `rand(3,5)·SpriteScaleFactor`, alpha `randint(200,255)` | brightness 0 unless HOLDING | — | `[bin-cl; high]` |

### 7.4 Hold beams (`CPhysCannonEffectBeam`)
Created on the first HOLDING: `TE_BEAMPOINTS` from the fork tip attachment to the held entity (attachment 1 / entity origin), model `sprites/orangelight1_noz.vmt` in first person, `sprites/orangelight1_gmod.vmt` in third person (SDK: `orangelight1_noz` / `orangelight1`; GMod precaches exactly these two names), halo 0, life 0 (`FBEAM_FOREVER`), first person width **0 → 4**, third person **0.5 → 2**, fade 0, **amplitude 16**, speed 150, framerate 30, white, 8 segments; visibility is brightness 255/0. `[sdk .h + bin-srv Precache; med]`

### 7.5 Sound envelope
Hold loop `Weapon_PhysCannon.HoldSound` on `CHAN_STATIC` from the weapon entity, attenuation 0.8 (`CPASAttenuationFilter` + owner): start vol 0 / pitch 50, ramp to vol 0.8 / pitch 100 over 0.5 s on pickup; on drop/launch/close vol→0 and pitch→50 over 1.0 s; `StopEffects` fades 0.1 s and `StopLoopingSounds` destroys it. `[bin-srv GetMotorSound/AttachObject/DetachObject; high]`
`OpenClaws` plays on every open, `CloseClaws` on every close (`CloseElements` also `ACT_VM_IDLE`); mega mode never closes. `[bin-srv Open/CloseElements; high]`

---

## 8. Per-frame glue (`ItemPostFrame`)
No owner → debounce 0, return. Re-evaluate mega (convar or `super_phys_gun`); on change: stop effects, force drop, play `Weapon_Physgun.Off` when switching off, swap the viewmodel. Dead owner → return. Not holding → `CheckForTarget`. Then the pending element change when `now > debounce`. If +attack2 is not pressed (after the per-press debounce `m_nAttack2Debounce`): clear the debounce, effect READY if not holding, `WeaponIdle`. Base `ItemPostFrame` dispatches Primary/Secondary by the timers. `[bin-srv ItemPostFrame; high]`
`ItemPreFrame`: base, then `UpdateObject` when holding. `WeaponIdle`: `ACT_VM_RELOAD` (holding) / `ACT_VM_IDLE`. `[bin-srv; high]`

## 9. Deploy / holster
`Deploy`: close elements, effect READY, base deploy (draw animation and the base `m_flNextPrimaryAttack` from the sequence); GMod `physcannon_instant` ≠ 0 → next primary/secondary and the player's `m_flNextAttack` = now. Measured select-time of a fresh `Give`: primary 0.605 s, secondary unchanged. `[bin-srv Deploy; high]` `[measured; high]`
`Holster`: false while holding; otherwise force drop + stop effects. `[bin-srv; high]`

## 10. Mega physcannon in GMod
Compiled in and reachable only via `physcannon_mega_enabled 1` or a `super_phys_gun` global; off in sandbox, so: the blue sprite set, `c_superphyscannon.mdl`, 2× pull, ragdoll/NPC pickup, 18 u error tolerance, −1.5 s error grace, mega sounds and the dissolve are **never** seen in normal play. `[bin-srv Spawn/ItemPostFrame + measured default 0; high]`

## 11. +USE carry (for contrast; same controller, different weapon-independent rules)
`CPlayerPickupController::Init` uses `CBasePlayer::CanPickupObject(35 kg, 128 u)` rules upstream, holsters nothing, moves `COLLISION_GROUP_DEBRIS` objects to `INTERACTIVE_DEBRIS`, sets angle alignment cos 30° and "allow overhead"; `Use`: dropped when +use ends, +attack2, `ComputeError() > 12` or not moveable; +attack throws: `massFactor = remap(clamp(m, 0.5, 15), 0.5..15 → 0.5..4)`, `ApplyForceCenter(forward · player_throwforce · massFactor)`, torque `RandomAngularImpulse(−10, 10) · massFactor`. `[bin-srv Init/Use + sdk-sp; high]`
