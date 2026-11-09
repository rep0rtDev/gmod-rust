# Garry's Mod `weapon_physgun` – behaviour note

Scope: the Physics Gun as shipped in Garry's Mod (server `server_srv.so`, client `client.dll`), cross-checked against Source SDK 2013 and GMod's Lua. Written so the feature can be implemented without the binaries. No binary identifiers are used; asset / convar / hook names are the real ones.

**Tag legend** (every fact carries one): `[S]` server pseudo-C, `[C]` client pseudo-C, `[L]` GMod Lua (Facepunch/garrysmod), `[SDK]` Source SDK 2013, `[M]` measured on a real server by the gmod-rust team (docs/spec/physgun.md, convars-defaults.md), `[I]` inference. Confidence `H/M/L`.

Units: Source/Hammer units (u), degrees, seconds. Vectors are world space unless stated. "aim" = the player's eye forward vector; "shoot position" = the player's weapon shoot position (eye position for a standing player).

---

## 0. What differs from Source SDK 2013 (summary)

| Topic | SDK 2013 `physgun.cpp` (the old HL2 beta gun) | Garry's Mod |
|---|---|---|
| Controller | custom point controller with velocity/torque limits (`phys_gunvel 400`, `phys_gunforce 5e5`, `phys_guntorque 100`, damping 2) | pure vphysics *shadow controller* (`ComputeShadowControl`) fed by `physgun_*` convars `[S,H]` |
| Range | 40..1024 u hold distance, trace 4096 u | `physgun_minrange 40`, `physgun_maxrange 4096`, pickup trace `2 × maxrange` `[S,H/M]` |
| Wheel | `distance *= 1 ± 0.1` per wheel step | `distance += wheel × physgun_wheelspeed(10)` `[S,H]` |
| Forward/back keys | 100 u/s | an unresolved constant × frametime (see §5) `[S,M]` |
| Rotate | no rotate mode | `E` + mouse rotates, `E`+`Shift` snaps to `gm_snapangles`, left/right keys spin yaw at `phys_spinspeed 200 °/s` `[S,H]` |
| Freeze | none (glue "pellets") | right click freezes via Lua hook `OnPhysgunFreeze`, reload unfreezes via `OnPhysgunReload` `[S,L,H]` |
| Mass while held | unchanged | set to **45678 kg** while held (restored on drop) `[S,H]` |
| Sounds | 4 looping sounds + on/off | **no sound at all is emitted by the weapon code** `[S,C,M]` |
| Beam | one straight `sprites/physbeam.vmt` beam, pellets glued | three curved (quadratic Bézier) beams, 2 materials, random widths, glow sprites at the end, player weapon colour `[C,H]` |
| Hooks | none | Lua gamemode hooks `PhysgunPickup`, `OnPhysgunPickup`, `PhysgunDrop`, `OnPhysgunFreeze`, `OnPhysgunReload`, client `DrawPhysgunBeam` `[S,C,L,H]` |
| NPCs | – | NPCs can never pick the weapon up; the weapon always reports "has ammo" `[S,H]` |

---

## 1. ConVars

### Server (registered by the weapon / controller code)
| ConVar | Default | Range / flags | Meaning |
|---|---|---|---|
| `physgun_minrange` | 40 | min 32, max 256 | nearest hold distance (u) `[S,H]` |
| `physgun_maxrange` | 4096 | min 128, max 32768, replicated+notify | farthest hold distance (u); pickup trace is twice this `[S,H]` |
| `phys_spinspeed` | 200 | – | yaw spin speed (°/s) with the move-left/right keys in rotate mode `[S,H]` |
| `physgun_timeToArrive` | 0.05 | – | shadow controller seconds-to-arrive for normal entities `[S,H name][L,M default][M,H]` |
| `physgun_timeToArriveRagdoll` | 0.1 | – | same for ragdolls `[S,H name][L,M default]` |
| `physgun_maxSpeed` | 5000 | – | shadow max linear speed (u/s) `[S name][M,H value]` |
| `physgun_maxangular` | 5000 | – | shadow max angular speed (°/s) `[S name][M,H value]` |
| `physgun_maxDampSpeed` | 10000 | – | shadow max damping speed `[S name][M,M value – read back by gmod-rust under a different spelling]` |
| `physgun_maxDampAngular` | 10000 | – | shadow max damping angular speed `[S name][M,M value]` |
| `physgun_dampFactor` | 0.8 | – | shadow damp factor `[S name][M,H value 0.8]` |
| `physgun_teleportDistance` | 0 | – | shadow teleport distance (0 = never) `[S name][L,M][M,H]` |

Notes: the controller convars are read **every physics simulate step** (not cached) `[S,H]`. Spelling in the binary is exactly as above (camel-case `maxSpeed`, `maxDampSpeed`, `dampFactor`, lower-case `maxangular`) `[S,H]`; gmod-rust's convars-defaults.md lists them as `physgun_maxSpeedDamping`, `physgun_maxAngularDamping`, `physgun_DampingFactor` – these spellings are not in the binary (convar lookup is case-insensitive, so only the `Damping`/`Damp` word order matters).

### Client (userinfo / client-side, read by the server through the client's convar value)
| ConVar | Default | Meaning |
|---|---|---|
| `physgun_rotation_sensitivity` | 0.05 | degrees of prop rotation per mouse count in rotate mode; server clamps the received value to an internal [lo, hi] (constants not resolvable from the dump; the menu slider is 0.01..1) `[S,H][L,M]` |
| `physgun_wheelspeed` | 10 | u of hold-distance change per wheel notch; server caps |value| at an internal constant (menu slider 0..50) `[S,H][L,M]` |
| `gm_snapangles` | 45 | rotation snap increment (°) with E+Shift `[S,M][L,H]` |
| `gm_snapgrid` | 0 | grid snap for the held target position (0 = off) `[S,M][L,H]` |
| `physgun_drawbeams` | 1 | client: draw the beam at all `[C,M][L,H]` |
| `physgun_halo` | 1 | sandbox Lua: halo around the held entity `[L,H]` |
| `effects_freeze` / `effects_unfreeze` | 1 / 1 | sandbox Lua: freeze/unfreeze effects `[L,H]` |
| `cl_weaponcolor` | "0.30 1.80 2.10" | Vector, userinfo; the player's weapon colour → beam/glow tint (values are linear colour multipliers, may exceed 1) `[L,H]` |
| `sbox_persist`, `physgun_limited` (sv, 0) | – | sandbox pickup restrictions, §9 `[L,H]` |

---

## 2. Weapon basics

- Class `weapon_physgun`; hold type `"physgun"`; `HasAnyAmmo()` always true; cannot be picked up by NPCs `[S,H]`.
- Networked weapon state: held entity handle, beam entity handle, the grab point in the held entity's local space (vector), plus the base HL2MP weapon table `[S,H]`. A separate server entity ("phys beam") is created for the weapon and carries the beam enabled flag / target data to clients `[S,H][I,M on contents]`.
- Lifecycle `[S,H]`:
  - Deploy: drop whatever is held, (re)create the beam entity.
  - Holster, weapon dropped, removed, restored from save: drop held entity, remove (restore: recreate) the beam entity.
- Viewmodel animation: on the **initial** attack press (not every frame) and on freeze, the weapon sends activity index 181 of the shared activity table = `ACT_VM_SECONDARYATTACK` if GMod's table matches the SDK ordering `[S,H index][SDK,M mapping]`. No other activity is sent by the weapon code; idle/draw come from the base weapon class. The model is `models/weapons/c_superphyscannon.mdl` (client expects the muzzle to be **attachment #1**) `[C,H attachment][L/I,M model]`.
- Sounds: **none** are emitted by the physgun code paths (no sound strings referenced server or client side) `[S,C,H]`. GMod's sound scripts still define `Weapon_Physgun.On/Off/Special1/LockedOn/Scanning/LightObject/HeavyObject` (wave files `weapons/physgun_on.wav`, `physgun_off.wav`, `physgun_loop1..4.wav`) but nothing plays them `[L,H][I,M]`.

---

## 3. Per-tick input state machine (server, weapon post-frame)

Runs every server tick while the weapon is active and the owner is a living player `[S,H]`:

1. If `IN_RELOAD` was *pressed* this tick → Reload (§8 unfreeze).
2. If `IN_ATTACK2` was *pressed* this tick → Secondary attack (§8 freeze).
3. If `IN_ATTACK` is **not** held, or `IN_ATTACK2` **is** held → Drop (§7) and stop. Otherwise → Primary attack (§4) (it early-outs if already holding).
4. If nothing is held → stop.
5. `IN_USE` held → set the player's `DIROVERRIDE` physics flag (rotate mode, §6); cleared otherwise.
6. Range / spin keys (only inside rotate mode, see §6): forward/back keys move the hold distance; move-left/right keys spin yaw.
7. Mouse-wheel (§5), then recompute target = shoot position + aim × distance.
8. Update rotation target (§6), update position target (§5).

Timing: all of this is tick-rate bound; "pressed" means button edge in this usercmd. The hold is established the tick after the press is processed (pickup shows as "held" the next tick) `[M,H]`.

---

## 4. Pickup (primary attack)

Preconditions `[S,H]`: `nextPrimaryAttack` elapsed, owner is a player, nothing held.

1. Aim direction: the player's cached aim vector (vehicle aware), otherwise `AngleVectors(EyeAngles)` `[S,H]`.
2. Trace: a line from the shoot position along the aim for **`2 × physgun_maxrange`** (8192 u default) `[S,M – the range term appears doubled in the decompile; the client's beam trace multiplies maxrange by a constant too]`, filter = simple filter ignoring the owner (collision group none). The mask is not visible server-side; the client's equivalent beam trace uses `MASK_SHOT_HULL` (0x600400B: solid|moveable|monster|window|debris|grate) `[C,H]`. `r_visualizetraces` draws it red `[S,H]`.
3. Reject if fraction ≥ 1 and not start-solid `[S,H]`. Reject if the hit is the world or no entity `[S,H]`.
4. Lua hook `GM:PhysgunPickup(ply, ent)` must return true `[S,H]`.
5. Store: held entity, `lastYaw = EyeAngles.y`, target rotation = entity's current absolute angles, hold `distance = |hitPos − shootPos|`, grab point = hit position in the **entity's local space** (origin-relative if angles are zero, else inverse-transformed by the entity matrix) `[S,H]`.
6. Only if the entity's movetype is VPHYSICS `[S,H]`:
   - choose the physics object: `physicsObjectList[trace.physicsBone]` when the index is valid, else the entity's main physics object;
   - `Wake()` it; attach the controller (§5.1) with the hit position;
   - if the entity is **not a ragdoll**: `EnableMotion(true)` on **every** physics object of the entity (a frozen multi-part prop is fully unfrozen). A ragdoll only gets the grabbed bone enabled (inside the controller attach).
   - Entities that are not VPHYSICS (players, NPCs, brush entities without physics) are still "held" (beam, drop hooks) and are moved by the non-physics path (§5.3).
7. Lua hook `GM:OnPhysgunPickup(ply, ent)` (notification) `[S,H]`.
8. Enable + update the beam entity (owner, local grab point, held entity, physics bone) `[S,H]`.
9. If `IN_ATTACK` was pressed *this tick* → viewmodel activity 181 (§2) `[S,H]`.

Game event `physgun_pickup { entindex }` is fired by the generic pickup helper (used by physcannon-style pickups; the physgun weapon itself does not call it) `[S,M]`.

Multiple players may hold the same entity at once; each has its own controller, the "player held" flag is only cleared when the last one lets go `[S,H]`.

---

## 5. Holding: position

### 5.1 Controller attach (VPHYSICS entities)
`[S,H]` unless tagged:
- `secondsToArrive = ent:IsRagdoll() ? physgun_timeToArriveRagdoll : physgun_timeToArrive`.
- **Mass override**: unless the object's mass is already above a threshold constant, or the entity is a ragdoll, `prop_vehicle_jeep`, `prop_vehicle_apc` or `prop_vehicle_jeep_old`: remember mass and buoyancy ratio, then `SetMass(45678)` `[S,H value]`; the threshold constant could not be read from the dump – it is most likely 45678 itself (`[I,M]`). On detach the saved mass and buoyancy are restored if a mass was saved `[S,H]`.
- `EnableMotion(true)`; local grab point = `WorldToLocal(hitPos)` of the physics object; create a motion controller, `AttachObject`, set game flag `FVPHYSICS_PLAYER_HELD`, priority HIGH; target rotation = object's current angles; target position = current grab point (so grabbing a resting prop does not move it `[M,H]`).

### 5.2 Target position
Each tick: `desired = shootPos + aim × distance` `[S,H]`.
Controller target: `targetOrigin = desired − (LocalToWorld(grabLocal) − objectOrigin)` i.e. the body origin such that the grab point lands on `desired` **using the body's current orientation** `[S,H]` (this is why the body swings around the grab point while its yaw catches up `[M,H]`). Then `SnapToGrid(player, target)` (`gm_snapgrid`, default off) `[S,H][L,M]`.

### 5.3 Shadow controller law (what moves the body)
Every physics step the controller `[S,H]`: `EnableMotion(true)`, `Wake()`, fills a standard HL shadow-control parameter block {targetPosition, targetRotation, maxAngular = `physgun_maxangular`, maxDampAngular = `physgun_maxDampAngular`, maxSpeed = `physgun_maxSpeed`, maxDampSpeed = `physgun_maxDampSpeed`, dampFactor = `physgun_dampFactor`, teleportDistance = `physgun_teleportDistance`} and calls the vphysics object's `ComputeShadowControl(params, secondsToArrive, dt)`; it returns zero linear/angular acceleration itself (the shadow call already set the velocities). For `prop_vehicle_jeep` only: `maxAngular *= 1 − (|fx·gx| + |fy·gy| + |fz·gz|)` where f = forward of the target angles, g = forward of the current angles `[S,H]`.
Measured response (crate, 66.67 Hz) `[M,H]`: `v' = v × (1 − 0.8) + Δ / 0.05` per tick, clamped to 5000 u/s – i.e. critically damped arrival in ~3 ticks, no overshoot; gravity has no effect while held; first two ticks after a target step are ~7 u/s faster than that fit.

### 5.4 Hold distance
- Initial: distance from shoot position to the hit point `[S,H]`.
- Mouse wheel: `distance += wheelDelta × clamp(|physgun_wheelspeed|, ≤ cap)`; the result is clamped to `[physgun_minrange, physgun_maxrange]` **only when the wheel moved** `[S,H]` (so an object grabbed beyond maxrange stays there until the wheel is touched `[I,M]`).
- Rotate mode keys (E held): `IN_FORWARD` → `distance = Approach(maxrange, distance, K × frametime)`, `IN_BACK` → `Approach(minrange, …)`; K is a float constant not resolvable from the dump (SDK used 100 u/s) `[S,H structure][SDK,L value]`.

### 5.5 Non-VPHYSICS held entities (players, NPCs, …) `[S,H]`
No controller. Each tick: `grabWorld = entityMatrix × grabLocal`; `newOrigin = desired − (grabWorld − origin)`; `SnapToGrid`. If the entity is stuck where it is but free at `newOrigin` → teleport there. Otherwise sweep the entity hull from origin to `newOrigin` with `MASK_SOLID_BRUSHONLY`; if blocked, try the three axes separately (x, then y, then z) and set the origin to the furthest unblocked point. Rotation (§6) for one category of entity (most likely players/NPCs) keeps yaw only (pitch = roll = 0) `[S,M on which category]`. Sandbox Lua refuses players anyway (§9).

---

## 6. Holding: rotation

Every tick `[S,H]`:
1. **Yaw follow**: `targetAngles.yaw += EyeAngles.yaw − lastYaw; lastYaw = EyeAngles.yaw` (the prop keeps its orientation relative to the view; pitch changes do not tilt it `[M,H]`).
2. **Rotate mode** (`IN_USE` held):
   - the player's `DIROVERRIDE` physics flag is set, which is what stops the mouse from turning the view while rotating `[S,H flag][I,M effect]`;
   - `IN_MOVELEFT`: `yaw −= phys_spinspeed × frametime`; `IN_MOVERIGHT`: `yaw += …` `[S,H]`;
   - mouse: `sens = clamp(client physgun_rotation_sensitivity, lo, hi)` (default 0.05 °/count); `dPitch = −mouseDY × sens`, `dYaw = mouseDX × sens`; the target angles are moved into the space of the player's **yaw-only** eye matrix, pre-multiplied by `AngleMatrix(dPitch, dYaw, 0)` (i.e. the rotation is applied about the view's right axis for pitch and world up for yaw), and transformed back `[S,H]`. Measured: 20 counts/tick → 1 °/tick `[M,H]`.
3. **Angle snap**: `snapped = round(a / gm_snapangles) × gm_snapangles` per component `[S,H][L,M name]`. When `IN_USE` **and** `IN_WALK` (shift) are both held the snapped angles are used as the target; on the press edge of that combination the stored target itself is replaced by the snapped value `[S,H]`.
4. Apply: controller `SetTargetRotation(angles)`; non-physics entities get `SetAbsAngles` (yaw-only for the category in §5.5) `[S,H]`.

Angular response `[M,H]`: the same damped law as position (`w' = w × 0.2 + Δangle / 0.05`, clamp 5000 °/s). Rotating a prop grabbed off-centre swings its origin around the grab point because the position target uses the body's current orientation (§5.2).

---

## 7. Drop (release)

Triggered by releasing `IN_ATTACK`, pressing `IN_ATTACK2` (after the freeze hook), holster, weapon removal, owner death/disconnect, or any code calling the force-drop helper `[S,H]`:
1. Lua hook `GM:PhysgunDrop(ply, ent)` `[S,H]`.
2. Beam entity disabled `[S,H]`.
3. Controller detach: restore saved mass + buoyancy (if saved), clear `FVPHYSICS_PLAYER_HELD` unless another player still holds the entity, destroy the motion controller `[S,H]`.
4. `ent:SetPhysicsAttacker(ply, curtime)` (damage dealt by the thrown prop is credited to the player) `[S,H][I,M on meaning]`.
5. Held entity / grab point cleared. The object keeps the velocity the shadow controller gave it and gravity resumes next tick (`IsPlayerHolding` false two ticks after the button goes up) `[M,H]`. Nothing is re-frozen on drop `[S,H]`.

No "launch"/punt exists in the physgun; the `physcannon_*` launch helpers in the same binary belong to the gravity gun `[S,H]`.

---

## 8. Freeze / unfreeze

- **Freeze** (`IN_ATTACK2` pressed while holding) `[S,H]`: set a cooldown on both attacks (constant not resolvable; sub-second), validate the physics object, call `GM:OnPhysgunFreeze(weapon, physobj, ent, ply)`, send viewmodel activity 181, then Drop (§7). The actual freezing is Lua `[L,H]`: base gamemode – if `physobj` valid, `IsMoveable()` and `!ent:GetUnFreezable()` → `physobj:EnableMotion(false)` and `ply:AddFrozenPhysicsObject(ent, physobj)` (fires `PlayerFrozeObject`). Only the **grabbed physics object** stops (one ragdoll bone, one part of a multi-part prop) `[L,H][M,H]`. Sandbox adds hints and the `effects_freeze` effect. Measured: the body stops dead the next tick, no settling `[M,H]`.
- **Unfreeze** (`IN_RELOAD` pressed) `[S,H]`: if the reload cooldown elapsed, set it (constant not resolvable), call `GM:OnPhysgunReload(weapon, ply)`. Lua `[L,H]`: `ply:PhysgunUnfreeze()` – if the previous unfreeze was < 0.25 s ago → unfreeze **all** of the player's frozen objects; else eye-trace, and for `constraint.GetAllConstrainedEntities(tr.Entity)` every physics object that is not moveable, not `UnFreezable`, and allowed by `CanPlayerUnfreeze(ply, ent, physobj)` → `EnableMotion(true)`, `Wake()`, `PlayerUnfrozeObject`. Returns the count; sandbox shows the unfreeze effect when > 0.
- Picking up a frozen prop unfreezes it (every part for non-ragdolls, the grabbed bone for ragdolls) and it stays unfrozen after drop `[S,H]`.

---

## 9. Pickup permissions / prop protection (Lua, sandbox)

`GM:PhysgunPickup(ply, ent)` `[L,H]`: false for persistent entities when `sbox_persist` is set; delegates to `ent:PhysgunPickup(ply)` if the entity defines it; false if `ent.PhysgunDisabled` (set by map keyvalue `gmod_allowphysgun 0`); false for class `player`; with `physgun_limited 1` also false for `prop_dynamic*`, `prop_door*`, `func_physbox` with motion disabled, `prop_*` with MOTIONDISABLED / PREVENT_PICKUP spawnflags, and any other `func_*`; otherwise true and sends hints `PhysgunFreeze` (2 s) / `PhysgunUse` (8 s). Base gamemode: only `!ent:IsPlayer()`. Prop-protection addons hook `PhysgunPickup`, `PhysgunDrop`, `CanPlayerUnfreeze`, `OnPhysgunFreeze`, `OnPhysgunReload` `[L,H]`.
Engine-side the generic pickup interface (`OnAttemptPhysGunPickup` etc.) exists for entities implementing it, defaulting to allow `[S,H]`; ragdoll model keyvalues `physgun_interactions` are parsed by the ragdoll class (SDK-identical) `[S,H]`.

---

## 10. Beam rendering (client, every frame, for every player's physgun)

Draw function runs per weapon instance `[C,H]`:
- Owner must be a valid HL2MP player whose active weapon class is `weapon_physgun`. First person (local player, not third-person view): needs the viewmodel; third person / other players: needs the world model and the player to be visible; skipped for one observer mode `[C,H]`.
- Lua hook `GM:DrawPhysgunBeam(ply, weapon, enabled, target, physBone, hitPos)`; returning false suppresses the beam `[C,H][L,H]`. Sandbox uses it to add a **halo** around the held entity: colour = weapon colour + `VectorRand()×0.3`, blur size random 1..2, 1 pass, additive, not ignoring Z (`physgun_halo 1`) `[L,H]`.
- `physgun_drawbeams 0` → nothing drawn while holding `[C,M]`.
- **Beam start** = attachment **1** of the viewmodel (first person) or of the world model (third person) `[C,H]`. In the third-person path the render context's depth range is set to (0, 0.01) around the draw and reset to (0, 1) after `[C,M that it is DepthRange]`.
- **Not holding** (beam disabled): only the muzzle effect is drawn (see muzzle below) `[C,H]`.
- **Holding**:
  - **End point** = grab point: `entityMatrix × grabLocal`, or for VPHYSICS entities with a bone index the matrix of that physics bone `[C,H]`. When no entity is held but the beam is enabled, the end is the trace from the shoot position along the aim for `physgun_maxrange × K` with `MASK_SHOT_HULL` (K is a constant, 2 by analogy with the server `[I,M]`) `[C,H]`.
  - **Control point** = `shootPos + aim × |start − end| × K2` (K2 a float constant, not resolvable from the dump; the beam leaves along the aim and bends into the target) `[C,H structure][C,L value]`.
  - **Colour** = `player:GetWeaponColor()` (from `cl_weaponcolor`, default (0.30, 1.80, 2.10)), run through a normalising/clamping helper before use `[C,H][L,H]`.
  - **Material**: `sprites/physbeama.vmt` when holding, `sprites/physbeam.vmt` otherwise (both `FindMaterial`'d at weapon creation; `sprites/physg_glow1` and `physg_glow2` are precached there too) `[C,H]`. Each material has its own pair of scroll constants `[C,H structure]`.
  - **Three Bézier beams** through (start, control, end) `[C,H]`:
    1. start → end, width **2.0 u**, texture scroll = constant × frametime;
    2. end → start, width **random 2.0..5.0 u**, scroll × another constant;
    3. end → start, width **random 2.0..6.0 u**, scroll × a third constant.
    Widths are re-rolled every frame (flicker). The curve-drawing helper itself is not in the dump; it is called with (p0, p1, p2, width, colour, scroll), i.e. a quadratic Bézier `[C,H args][I,M curve type]`.
  - **End glow**: bind `sprites/physbeam.vmt` (the beam texture used as a sprite), colour = weaponColour × 255 as bytes, alpha = random 100..255, and draw **7 sprites** (2 when not holding) at the end point, each **random 1..16 u** square `[C,H]`.
- **Muzzle effect** (both states): a helper called with (start position, holding flag, start angles) draws the muzzle glow (`physg_glow1`/`physg_glow2`) and is the only candidate for the dynamic light; **that function is not in the supplied dump** – see Blockers. The game visibly has a dlight at the muzzle/beam end; its radius/colour/decay could not be read `[I,L]`.
- Ragdoll bone: the client gets the physics-bone index and transforms the local grab point by that bone's matrix `[C,H]`.

---

## 11. Special entity cases

| Case | Behaviour |
|---|---|
| Ragdoll | grabbed physics object = `trace.physicsBone`; only that bone is motion-enabled / frozen; `timeToArriveRagdoll` (0.1 s); mass **not** overridden `[S,H]`. `Entity:IsPlayerHolding()` stays false for ragdolls `[M,H]`. Hit test is against the physics hulls (not hitboxes) `[M,H]`. |
| Multi-part prop (non-ragdoll) | grabbed object = hit bone's physics object; **all** objects get `EnableMotion(true)` on pickup `[S,H]`. |
| Frozen prop | pickup unfreezes (above); drop leaves it unfrozen `[S,H]`. |
| Constrained groups | the controller only drives the grabbed object; constraints (welds, ropes…) pull the rest via vphysics. Unfreeze (reload) walks `constraint.GetAllConstrainedEntities` `[L,H]`. Nothing physgun-specific for constraints server-side `[S,H]`. |
| Heavy objects | mass above the threshold constant keeps its mass `[S,H]`; everything else is 45678 kg while held (so held props push everything) `[S,H]`. |
| Vehicles | `prop_vehicle_jeep` / `apc` / `jeep_old`: mass untouched; jeep: angular limit scaled by the forward-vector mismatch (§5.3) `[S,H]`. |
| Players | can be "held" by the engine via the non-physics path (§5.5) but sandbox refuses class `player` `[S,H][L,H]`. |
| Non-physics entities (brushes, NPCs) | swept-move path §5.5 `[S,H]`. |
| Same entity, two players | both hold; flags cleared only when the last drops `[S,H]`. |
| Owner dies / weapon holstered | drop + beam removed `[S,H]`. |

---

## 12. Constants that could not be resolved from the dump
- forward/back distance speed K (§5.4), freeze cooldown, reload cooldown, rotation-sensitivity clamp bounds, wheel-speed cap, the 45678 mass threshold (very likely 45678), client control-point factor K2, client scroll constants, client range factor K (very likely 2), and everything inside the muzzle-glow/dlight helper. They are plain float globals whose data section was not included.
