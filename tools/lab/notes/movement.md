# Garry's Mod player movement - behaviour note

Scope: `CGameMovement` / `CHL2GameMovement` as shipped in GMod (x86 Linux srcds `server_srv.so`, Windows `client.dll`), the GMod-specific player-move wrapper around it, and the Lua that sits on top (base + sandbox gamemodes). Everything below is written so it can be implemented without the binaries. Units are Source units (u), seconds (s) or milliseconds (ms) as stated; angles in degrees.

Source tags used on every fact:

- **[bin-S]** read in the server pseudo-C (symbols present), **[bin-C]** read in the client pseudo-C (no symbols, functions identified by vtable order and by matching the server body), **[SDK]** Source SDK 2013 (`gamemovement.cpp`, `hl_gamemovement.cpp`, `baseplayer_shared.cpp`, `shareddefs.h`), **[Lua]** `garrysmod` repo (`gamemodes/base`, `gamemodes/sandbox`, `lua/includes/modules/player_manager.lua`), **[repo]** measured in gmod-rust's own oracle recordings (`docs/spec/movement.md`), **[inf]** inference.
- Confidence: **H** (logic read directly, numbers confirmed in at least two places), **M** (logic read, number taken from the SDK because the binary stores it as an anonymous float constant that the dump does not resolve), **L** (guess).

Important caveat on numbers: the pseudo-C dump references every floating constant as an anonymous data symbol whose value is not in the dump. Where the surrounding logic is identical to the SDK the SDK constant is given and tagged **M**; where GMod's logic differs the constant is marked *unresolved* with the best estimate. Literal immediates (e.g. 1000.0, 100, 80, 256, 400, 0.15) *were* visible and are **H**.

Vtable facts used to identify unnamed client functions: the client `CGameMovement` vtable has 57 slots, the SDK class has 55; GMod inserted two virtuals right after `GetPlayerViewOffset` (one pure in the base class and implemented only by the HL2 subclass: it is the "left the ladder entity / clean up ladder state" hook; the other is a second view-offset getter) and dropped the two parameterless `GetPlayerMins/Maxs` overloads. Everything else is in SDK order. **[bin-C] H**

---

## 0. Where movement runs, tick order, Lua hooks

Per usercmd on the server (GMod's `CPlayerMove` subclass; the client's prediction runs the same shared code): **[bin-S] H**

1. `StartCommand` -> Lua hook **`StartCommand(ply, cmd)`**.
2. `SetupMove`: copies the usercmd into the move data (buttons, move vector, view angles, impulse), lets a vehicle the player drives rewrite it, then Lua hook **`SetupMove(ply, mv, cmd)`** (no return value is honoured).
3. `ProcessMovement` (the movement object proper):
   - **frametime is multiplied by the player's lagged-movement scalar** (`Player:SetLaggedMovementValue`, default 1) for the whole move and restored afterwards; **[bin-S] H**
   - max speed = the player's virtual `GetPlayerMaxSpeed()` (run/walk/slow-walk/crouch rules live there and in Lua, see 2); if the player currently stands on a ground entity and his flag bit 1 is set, max speed is also multiplied by a scalar the *ground entity* reports (a GMod hook for moving-ground speed) **[bin-S] M** (the scalar's meaning is inferred; it defaults to 1);
   - Lua hook (id 0x72) with `(ply, mv)` - this is the engine-side pre-move hook, result ignored; **[bin-S] H**
   - **impulse 154** = noclip toggle: Lua hook **`PlayerNoClip(ply, desiredState)`** may veto (a `false` return cancels). Turning noclip *on* sets movetype NOCLIP, clears the ground entity and sets the engine FL_NOCLIP-style flag bit; turning it *off* sets movetype WALK and clears the flag. Base gamemode allows noclip only in single player; sandbox allows it unless `sbox_noclip 0`, the player is in a vehicle or dead. **[bin-S][Lua] H**
   - `PlayerMove()` (step 4), then `oldbuttons = buttons`.
4. `PlayerMove`: if the player is not frozen, Lua hook **`Move(ply, mv)`**: a `true` return **skips the whole engine move** (this is how `drive.*` and player classes override movement). Otherwise: `CheckParameters` (1.1), zero the "out" wish/jump velocities, `ReduceTimers` (1.3), compute view forward/right/up from the move angles; if movetype is WALK and the frozen bit is clear and `sv_optimizedmovement` != 0: `CategorizePosition` only when vertical velocity > 0 (the SDK optimised path re-categorises only when moving vertically; the comparison constant is unresolved, 0 assumed) - otherwise `CategorizePosition` always. Then: store the water level entering the tick, re-read the surface data under the player (surface friction / step sounds), `UpdateDuckJumpEyeOffset`, `Duck`, `LadderMove` unless (in a vehicle and not an observer-type vehicle); a player who was on a ladder but `LadderMove` returned false is put back to WALK (and the `IsNavIgnored`-style ladder-animation byte is cleared). Movetype dispatch: NONE -> nothing; ISOMETRIC / WALK -> `FullWalkMove`; FLY / FLYGRAVITY -> `FullTossMove`; NOCLIP -> `FullNoClipMove` (section 7); LADDER -> `FullLadderMove`; OBSERVER -> `FullObserverMove`; anything else prints "Bogus pmove player movetype". **[bin-S] H**
5. `FinishMove`: writes origin / velocity / angles back to the player, then Lua hook **`FinishMove(ply, mv)`** (a `true` return makes the engine skip its own write-back, so Lua can override the result; sandbox uses this for the jump boost, section 4.3). **[bin-S][Lua] H**
6. `RunCommand` also fires **`PlayerButtonDown(ply, btn)`** / **`PlayerButtonUp(ply, btn)`** for every changed button bit (5 x 32 bit words are scanned), and **`VehicleMove(ply, veh, mv)`** instead of the player move when driving. `sv_maxusrcmdprocessticks_warning`, `sv_maxusrcmdprocessticks_holdaim`, `sv_noclipduringpause` are consulted there. **[bin-S] H**
7. Moving ground (`CheckMovingGround`, SDK logic unchanged): if the ground entity is a conveyor-type (FL_CONVEYOR), its velocity becomes the player's base velocity (added to an existing base velocity when FL_BASEVELOCITY is set); otherwise base velocity is applied as an impulse of `basevel * (1 + frametime * 0.5)` (SDK formula) and cleared. **[bin-S] M**

---

## 1. Common per-tick pieces

### 1.1 CheckParameters (input shaping) **[bin-S][bin-C] H**

Unless movetype is ISOMETRIC, NOCLIP or OBSERVER:

- `maxspeed = min(mv.maxspeed, mv.maxclientspeed)`;
- `factor = min(constraint speed factor, speed scale of a player-attached constraint entity if present else 1)`; the constraint factor is the SDK `point_playermoveconstraint` rule: 1 inside `radius - width`, falling linearly to `speedfactor` across `width` when moving outward; `maxspeed *= factor`;
- if `|move| > maxspeed` the (forward, side, up) move is scaled to `maxspeed` (the squared form when movement optimisations are on, same result).

FL_FROZEN or FL_ATCONTROLS -> forward/side/up move = 0.

`DecayPunchAngle` (section 8). Then view angles for the move: `mv.angles = cmd.angles + punchangle` when alive (or observer); dead players keep their stored angles. Roll: 0 for ISOMETRIC/NOCLIP, else `CalcRoll(angles, velocity, sv_rollangle, sv_rollspeed)` (SDK formula: `side = dot(velocity, right); sign; side = |side|; value = rollangle; if side < rollspeed: side = side * value / rollspeed else side = value; return side * sign`). GMod ships `sv_rollangle 0`, `sv_rollspeed 200` **[repo]**, so there is no strafe roll in practice.

Dead and not observer: view offset = gamerules dead view height (SDK `VEC_DEAD_VIEWHEIGHT` = (0,0,14)) **times the player's model scale**. **[bin-S][bin-C] H** (scale multiply is GMod)

Yaw wrap: `if yaw > 180: yaw -= 360`.

### 1.2 Speed cropping while ducked **[bin-S] H / const M**

`HandleDuckingSpeedCrop`: once per move, if FL_DUCKING **and on the ground**, forward/side/up move *= 0.33333 (SDK constant; unresolved in the dump). In GMod the crouch speed a player actually reaches is `walk speed * CrouchedWalkSpeed` (0.3 by default, set per player class, `Player:SetCrouchedWalkSpeed`) **[Lua][repo] H**; this is applied in the player's max-speed getter, not in the movement class (not in the dump) **[inf] M**.

### 1.3 Timers **[bin-S] H**

`ReduceTimers` subtracts `1000 * frametime` ms from: duck time, duck-jump time, jump time, and one more GMod per-player ms timer (unused by the shared code read here), clamping each at 0. The water-jump timer is handled separately (section 6).

### 1.4 Velocity sanity **[bin-S] H**

`CheckVelocity`: NaN components are zeroed with a "PM Got a NaN velocity/origin" message; each axis clamped to +-`sv_maxvelocity` (GMod default 3500 **[repo]**).

### 1.5 Hull sizes and view offsets (GMod: per player, scaled) **[bin-S][bin-C] H**

- Standing / ducked hull mins and maxs are **per-player fields** (`Player:SetHull(mins,maxs)`, `Player:SetHullDuck`), multiplied component-wise by the **model scale** (`Player:SetModelScale`). Defaults are the HL2 view vectors: stand (-16,-16,0)..(16,16,72), duck (-16,-16,0)..(16,16,36). Observers use the gamerules observer hull x scale.
- Standing / ducked view offsets are per-player fields (`Player:SetViewOffset`, `Player:SetViewOffsetDucked`) x model scale; defaults (0,0,64) and (0,0,28). In a vehicle the vehicle's view offset x scale is used instead.
- Hence the ducked eye-offset curve (section 5.3) uses these per-player values, and a model scale of 2 doubles hull, eye height and dead-view height. **[bin-S][bin-C] H**

---

## 2. Speeds (where the numbers come from) **[Lua] H**

| Setting | base gamemode default | sandbox default | API |
|---|---|---|---|
| WalkSpeed | 400 | **200** | `Player:SetWalkSpeed` |
| RunSpeed (sprint, IN_SPEED) | 600 | **400** | `Player:SetRunSpeed` |
| SlowWalkSpeed (IN_WALK) | 200 | **100** | `Player:SetSlowWalkSpeed` |
| CrouchedWalkSpeed (multiplier) | 0.3 | 0.3 | `Player:SetCrouchedWalkSpeed` |
| DuckSpeed (s, time to duck) | 0.3 | **0.1** | `Player:SetDuckSpeed` |
| UnDuckSpeed (s) | 0.3 | **0.1** | `Player:SetUnDuckSpeed` |
| JumpPower (u/s) | 200 | 200 | `Player:SetJumpPower` |
| Ladder climb speed (u/s) | 200 (engine default) | 200 | `Player:SetLadderClimbSpeed` |
| AvoidPlayers | true | true | swerve rule, not in this dump |

Set on every spawn by `player_manager.OnPlayerSpawn`. `sv_maxspeed` is 10000 in GMod, so the per-player speed is the only cap. Convars: `sv_gravity 600`, `sv_friction 8`, `sv_stopspeed 10`, `sv_accelerate 10`, `sv_airaccelerate 10`, `sv_maxvelocity 3500`, `sv_noclipspeed 5`, `sv_noclipaccelerate 5`, `sv_sticktoground 1`, `sv_rollangle 0`, `sv_rollspeed 200`, `mp_falldamage 0`, `sv_standable_normal` (GMod-only, default 0.7 **[inf] M**), `sv_optimizedmovement 1`, `sv_ladder_useonly 0`, `sv_autoladderdismount 1`, `sv_ladderautomountdot 0.4`, `sv_specspeed 3`, `sv_specaccelerate 5`, `sv_specnoclip 1`. **[repo][SDK] H/M**. Step size is the per-player `m_flStepSize` (18, `Player:SetStepSize`), not a convar. **[bin-S] H**

---

## 3. Ground movement

### 3.1 FullWalkMove order **[bin-S][bin-C] H**

Identical structure to the SDK:

```
if waterjump timer == 0:
    if not (CheckWater() reports water): StartGravity()
    if water level < 2 (not waist-deep):
        if IN_JUMP: CheckJumpButton() else clear "jump held" bit in oldbuttons
        if on ground: velocity.z = 0; Friction()
        CheckVelocity()
        if on ground: WalkMove() else AirMove()
        CategorizePosition(); CheckVelocity()
        if not in water: FinishGravity()
        if on ground: velocity.z = 0
        CheckFalling()
    else (waist or deeper):
        if water level == 2: CheckWaterJump()
        if velocity.z < 0 and waterjump timer != 0: waterjump timer = 0
        jump button as above
        WaterMove(); CategorizePosition(); if on ground: velocity.z = 0
    if (water level entering tick == 0) != (water level now == 0): play "Player.Swim" at the origin and PlayerSetAnimation(walk)
else:
    WaterJump(); TryPlayerMove(); CheckWater()
```

Gravity halves: `StartGravity`: `velocity.z -= (ent_gravity or 1) * sv_gravity * 0.5 * frametime` plus `basevelocity.z * frametime`, then base velocity z is zeroed; `FinishGravity`: another half (skipped while the water-jump timer runs). **[bin-S] H**, factor 0.5 **M**. `AddGravity` (toss move) applies the full step. 

### 3.2 Wish velocity **[bin-S] H**

Forward and right vectors from the move angles are flattened (z = 0) and normalised; `wishvel = forward * fmove + right * smove`, z = 0; `wishspeed = |wishvel|` clamped to `maxspeed`. **If the player's "movement blocked" byte is set (set while a vehicle/drive mode or Lua freezes input) fmove/smove are treated as 0** in both walk and air moves and acceleration is disabled. **[bin-S][bin-C] H**

### 3.3 Friction (GMod differs) **[bin-S] H**

Runs only when the water-jump timer is 0 and `speed >= 0.1` (SDK constant, M):

```
friction = sv_friction * surfaceFriction * entityFriction     <- entityFriction is GMod's Entity:SetFriction on the player (default 1)
control  = speed < sv_stopspeed ? sv_stopspeed : speed
drop     = control * friction * frametime        (only when on ground; 0 in the air)
newspeed = max(0, speed - drop); velocity *= newspeed/speed
outWishVel -= (1 - newspeed/speed) * velocity
```

`surfaceFriction` is reset to 1 at every `CategorizePosition`, set to 0.25 when rising past a non-standable surface (3.6), and otherwise comes from the surface property under the feet (SDK rule: `surfaceFriction = min(1, surfaceprop friction * 1.25)`, measured by the repo).

### 3.4 Accelerate / AirAccelerate **[bin-S][bin-C] H**

`Accelerate`: if `CanAccelerate()` (input not blocked and water-jump timer 0; the HL2 subclass always returns true after calling the base, so the base check is effectively bypassed **[bin-S][SDK] H**): `add = wishspeed - dot(velocity, wishdir); if add > 0: accelspeed = min(add, accel * frametime * wishspeed * surfaceFriction); velocity += accelspeed * wishdir`.

`AirAccelerate` (server body lost to the decompiler; client body matches the SDK): `wishspd = min(wishspeed, GetAirSpeedCap())` with the cap a constant (SDK 30, **M**; the repo measured 30); `add = wishspd - dot(v, wishdir)`; `accelspeed = min(add, accel * wishspeed * frametime * surfaceFriction)` (note: rate uses the uncapped wishspeed). **[bin-C] H / cap M**

### 3.5 WalkMove **[bin-S] H**

wish (3.2) -> `Accelerate(wishdir, wishspeed, sv_accelerate)` -> `velocity.z = 0` -> add base velocity -> if `|velocity| < 1.0`: velocity = 0, subtract base velocity, return (**1.0 is M**) -> `dest = origin + velocity.xy * frametime` -> hull trace to dest; if it arrives (fraction 1) take it, else `StepMove` (3.7) -> remove base velocity -> `outWishVel += wishdir * wishspeed` -> `StayOnGround` (3.8). (The direct-arrival shortcut and the StayOnGround call are the SDK sequence; the dump lost the middle of the function to the decompiler, so the exact shortcut is **M**.)

### 3.6 CategorizePosition (ground detection; GMod differs) **[bin-S] H**

1. `surfaceFriction = 1`; `CheckWater()`.
2. On a ladder: ground entity = none, done. Observer flag: return.
3. **`sv_sticktoground`** (GMod convar, default 1): when it is **0** the SDK "moving up rapidly" rule applies: if `velocity.z > 140` (NON_JUMP_VELOCITY, M) and either there is no ground entity or `velocity.z - groundEntity.velocity.z > 140`, the ground is dropped without tracing. With the default **1 this rule is skipped**: the ground is always probed. (Rising fast still finds no floor because the probe below is short.) Independently, a player on a ladder moving up loses the ground.
4. Probe: sweep the hull from **origin.z + 2** down to **origin.z - 1** (the SDK probes origin -> origin - 2; GMod starts 2 u above the origin and reaches only **1 u below the feet**). The sweep is first done with GMod's **lower-half hull** (a box of the same footprint whose top is at half the hull height; this is the GMod-added virtual) and then, if nothing standable was hit, with the SDK "quadrant" fallback (`TryTouchGroundInQuadrants`: four quarter-footprint boxes, standing only if a hit normal is standable) and finally the full-hull ground trace. **[bin-S] H** for the +2/-1 geometry and the lower-half hull; the repo measured the effective reach as 1.0-1.3 u, consistent.
5. Standable means `normal.z >= sv_standable_normal` (GMod convar replacing the SDK's hard 0.7; default assumed 0.7 **M**). If the hit is not standable: no ground, and if `velocity.z > 0` and movetype != NOCLIP: `surfaceFriction = 0.25` (0x3e800000 literal, **H**).
6. Otherwise `SetGroundEntity(trace)` (SDK logic): when the ground entity changes, a player who just landed on a moving ground inherits its velocity into base velocity (`vecBaseVelocity += groundVel` when the old ground was none) and a player who just left one keeps his; the fall velocity is not touched here. The origin is **never moved** by categorisation. **[bin-S] H**
7. Every call also re-evaluates the player-surface trigger material (`CEnvPlayerSurfaceTrigger`).

### 3.7 StepMove **[bin-S] H**

```
save origin/velocity; TryPlayerMove() straight (result A = down-pos, down-vel)
restore; up = origin; if m_bAllowAutoMovement: up.z += stepsize + DIST_EPSILON(1/32, M)
trace hull origin->up; if not startsolid/allsolid: origin = trace end
TryPlayerMove()
down = origin; if allowAutoMovement: down.z -= stepsize + DIST_EPSILON; trace origin->down
if normal.z < sv_standable_normal: take A (no step)
else: if not startsolid/allsolid: origin = trace end; compare 2D distance travelled; if A went farther: take A else keep stepped result with A's velocity.z
outStepHeight += (origin.z - start.z) if > 0
```
Differences from the SDK: the standable test uses the convar, and the step height is added only while `m_bAllowAutoMovement` is set (it is normally set; the SDK ignores the flag here). **[bin-S] H**

### 3.8 StayOnGround (GMod differs) **[bin-S] H**

`start = origin + (0,0,stepsize)` (SDK: +2), `end = origin - (0,0, 2 * stepsize)` (SDK: - stepsize). Trace origin->start, then trace from that end point down to `end`. If `0 < fraction < 1`, not startsolid, `normal.z >= sv_standable_normal` and `|origin.z - trace.end.z| > 0.5` (SDK constant, M; the repo measured a 1/64-u threshold - re-check), the origin snaps to the trace end. So GMod can snap down a full two step heights (36 u) when walking off a ledge-shaped slope, the SDK only one.

### 3.9 TryPlayerMove (slide along planes) **[bin-S] H**

SDK algorithm (up to 4 bumps, 5 planes, `ClipVelocity` with overbounce 1, crease handling, "stop dead if going back against original velocity") with: the standable test from `sv_standable_normal`; ground bounce uses `sv_bounce`; GMod's `OnTryPlayerMoveCollision` hook called per impact (the HL2 subclass adds `SetGroundEntity`-related ladder bookkeeping). End of function: lateral slam volume: `stop = |v_before.xy| - |v_after.xy|`; `fvol = 1.0 if stop > 2*580 (1160), 0.85 if > 580, else 0` -> `PlayerRoughLandingEffects(fvol)` (section 8). **[bin-S] H**, 580/0.85 **M**.

---

## 4. Jumping

### 4.1 Engine jump (CheckJumpButton) **[bin-C] H** (server body unrecoverable in the dump, client is the same shared code)

Returns false (no jump) when:

- input blocked byte set (and the jump-held bit is set so a later release is needed);
- water-jump timer > 0: it is decremented by frametime (clamped at 0) and no jump happens;
- **waist-deep or deeper (water level >= 2)**: ground entity cleared; `velocity.z = 100` if the water contents are WATER, **80** in SLIME; a "Player.Swim" sound every **500 ms** (swim-sound timer); return false;
- no ground entity: set the jump-held bit, return false (so landing with jump already held does **not** jump: no auto-bhop);
- jump was already held last tick (oldbuttons has IN_JUMP);
- a duck-in-progress flag is set while FL_DUCKING, or the duck-jump timer > 0 (SDK `m_bInDuckJump`/`m_flDuckJumpTime` guards; in GMod the timer is never armed by the jump itself, see below).

Otherwise:

```
SetGroundEntity(none)
groundFactor = surfaceprop jumpFactor (1.0 when no surface data)
PlayStepSound(origin, surface, volume 1.0, force=true)          <- the jump "step"
PlayerSetAnimation(JUMP)
jumpSpeed = groundFactor * JumpPower            <- per-player Player:SetJumpPower, default 200 (SDK: sqrt(2*800*21)=268.3 fixed)
if ducked or ducking: velocity.z = jumpSpeed else velocity.z += jumpSpeed
FinishGravity()                                  <- half a tick of gravity immediately
outJumpVel.z += velocity.z - old velocity.z
outStepHeight += 0.15
OnJump(outJumpVel.z)  (empty in the base class)
set jump-held bit; return true
```

**Not present in GMod's binary** (SDK single-player only blocks): the HL2 forward jump boost and the `m_flJumpTime = 510 ms` / `m_bInDuckJump = true` arming. The boost exists in GMod anyway - in Lua (4.3). **[bin-C] H**

Jump height with the defaults: `v0 = 200`, gravity 600, tick 0.015: the jump tick ends at `200 - 1.5 * 9 = 186.5` (the jump's own half plus the air move's two halves), apex `200^2 / (2 * 600) = 33.3 u` minus integration offsets (the repo measured this exactly). **[repo] H**

### 4.2 Landing **[bin-S] H** - see section 8 (fall damage / punch / sounds). `OnLand` is empty.

### 4.3 Sandbox jump boost (Lua, `player_sandbox.lua`) **[Lua] H**

`StartMove`: if IN_JUMP pressed this tick (not in oldbuttons) and `Player:OnGround()` -> remember "jumping". `FinishMove` of the same command (after the engine move, so the player is already airborne with `velocity.z` set):

```
forward = move angles with pitch 0, as a unit vector
share = Crouching() and 0.1 or 0.5              (sprint does NOT reduce it: comment "this never applied to GMod")
addition = |ForwardSpeed * share|                (ForwardSpeed = the move's forward input after the max-speed clamp of 1.1)
cap = MaxSpeed * (1 + share)
if addition + |velocity.xy| > cap: addition -= (addition + |velocity.xy| - cap)
if dot(velocity, forward) < 0: addition = -addition   (note: sign from the *velocity*, not from the input as in the SDK)
velocity += forward * addition
```
Only the sandbox player class does this; the base class (and any gamemode not deriving from sandbox's class) has no boost. Walking 200 -> 300, sprinting 400 -> 600 (repo measurements agree).

---

## 5. Ducking (crouch)

All times are kept in a ms clock `ducktime` that starts at **1000** and counts down `1000 * frametime` per tick; `elapsed = max(0, 1000 - ducktime) / 1000` s. GMod replaces the SDK's fixed `TIME_TO_DUCK` / `TIME_TO_UNDUCK` with the per-player **DuckSpeed** / **UnDuckSpeed** seconds (base 0.3 / 0.3, sandbox 0.1 / 0.1); if a value is <= 0 an engine fallback constant is used (unresolved; SDK would be 0.2/0.4 and 0.2). **[bin-S] H**

### 5.1 State machine **[bin-S] H** (SDK structure, GMod constants)

Flags: FL_DUCKING (hull is small), FL_ANIMDUCKING (set when a duck starts, cleared when an unduck starts - drives the crouch animation), `ducked`, `ducking` (transition running), `inDuckJump`, `duckJumpTime`, `jumpTime`.

Each tick (`Duck()`):

1. *Duck-jump release*: if IN_DUCK was just released while FL_DUCKING and (`inDuckJump` or `CanUnduck()` / `CanUnDuckJump()`): clear `ducked`, FL_DUCKING, `ducking`, `inDuckJump`.
2. Record IN_DUCK into oldbuttons' duck bit. Dead and not observer: return.
3. If IN_DUCK held, or `ducking`, or FL_DUCKING, or `duckJumpTime > 0`:
   - **Duck pressed / holding** (IN_DUCK or duckJumpTime>0):
     - On the *press* tick (IN_DUCK newly down, not FL_DUCKING, no duck-jump): `ducktime = 1000`, `ducking = true`.
     - If `ducking` and not duck-jumping: set FL_ANIMDUCKING; `elapsed` as above; **if `elapsed > DuckSpeed` or FL_DUCKING or not on ground -> `FinishDuck()`** (instant crouch in the air); else `SetDuckedEyeOffset(SimpleSpline(elapsed / DuckSpeed))`.
     - Duck-jump handling: if `duckJumpTime > 0` and not FL_DUCKING -> `StartUnDuckJump()`; if FL_DUCKING and IN_DUCK not held and `CanUnDuckJump(trace)` -> `FinishUnDuckJump(trace)`, `duckJumpTime = (1 - trace.fraction) * UnDuckSpeed*1000 + (1000 - UnDuckSpeed*1000)`.
   - **Duck released** (not IN_DUCK):
     - duck-jump variant as above when `inDuckJump`.
     - If allowed to move (m_bAllowAutoMovement) or in the air or `ducking`: on the *release* tick: if FL_DUCKING: `ducktime = 1000` (start the unduck clock); else if `ducking` and not `ducked` (released before the duck finished): **remap the clock**: `ducktime = (1000 - UnDuckSpeed*1000) + ((1000 - ducktime) / (DuckSpeed*1000)) * UnDuckSpeed*1000` so the eye comes back up from where it was over the proportional time.
     - If `CanUnduck()`: if `ducking` or `ducked`: clear FL_ANIMDUCKING; **if `elapsed > UnDuckSpeed` or not on ground -> `FinishUnDuck()`** (instant in the air); else `SetDuckedEyeOffset(SimpleSpline(1 - elapsed / UnDuckSpeed))`, `ducking = true`.
     - If cannot unduck: if `ducktime != 1000`: `SetDuckedEyeOffset(1)`, `ducktime = 1000`, `ducked = true`, `ducking = false`, add FL_DUCKING (stay crouched, retry next tick).
4. Else (not ducking, standing, no buttons): if alive, not in a vehicle (`IsInAVehicle`-type virtual) and `duckJumpTime == 0`: if `|current view offset.z - standing view offset.z| > epsilon` (unresolved small constant) -> `SetDuckedEyeOffset(0)` (repair the eye height).

`FinishDuck`: `ducked = true`, `ducking = false`, add FL_DUCKING; **in the air the origin is raised by (standing hull maxs - ducked hull maxs) = 36 u** so the hull shrinks from the top; on the ground it shrinks from the top anyway (origin unchanged). Then `SetDuckedEyeOffset(1)`, `FixPlayerCrouchStuck(upward)` and `CategorizePosition`. **[bin-S] H** (36 u and the rest are SDK logic; the dump shows the model-scale factor in the hull getters, so the shift is 36 x model scale).

`FinishUnDuck`: in the air the origin is lowered by 36 x scale, FL_DUCKING cleared, `ducked = ducking = inDuckJump = false`, `ducktime = 0`, view offset = standing, `CategorizePosition`.

`CanUnduck`: test the standing hull at the origin (in the air: origin - 36 x scale) with a hull trace against the player solid mask; must not be start/all solid. `CanUnDuckJump` is the duck-jump version (trace down to find where the standing hull would rest). **[bin-S] H**

### 5.2 The duck-jump eye correction **[bin-S] H**

`UpdateDuckJumpEyeOffset` runs every tick before `Duck()`: if `duckJumpTime != 0`: `elapsed = max(0, 1000 - duckJumpTime)/1000`; `speed = UnDuckSpeed > 0 ? UnDuckSpeed : fallback (0.1?)`; if `elapsed > speed`: `duckJumpTime = 0`, eye offset fraction 0; else fraction `SimpleSpline(1 - elapsed/speed)`.

### 5.3 Eye-offset curve **[bin-S] H**

`SimpleSpline(v) = 3 v^2 - 2 v^3` (the 3.0 is an unresolved constant but the shape `k*v^2 - 2*v^3` is explicit; k = 3 is the only sensible value, **M**).

`SetDuckedEyeOffset(f)`:
```
more   = duckHullMins.z - standHullMins.z          (0 with default hulls)
eye.z  = (duckView.z - more) * f + standView.z * (1 - f)
```
with the per-player (scaled) view offsets, i.e. 64 -> 28 over DuckSpeed seconds (sandbox: 0.1 s = 7 ticks), 28 -> 64 over UnDuckSpeed seconds. x/y of the view offset are untouched.

Effective sandbox timings (tick 0.015): the duck finishes on the tick where `elapsed > 0.1`, i.e. the 7th tick after the press (the press tick itself counts 0); crouching in the air is instant. The repo measured 27 ticks for 0.4 s: that is the SDK/base-class constant, **not** sandbox's 0.1 - see gap list.

---

## 6. Water **[bin-S] H** (SDK logic; constants M unless literal)

- `CheckWater`: water level 0/1/2/3 from point contents at the feet (origin + hull mins + 1 u), the waist (origin + half the hull height) and the eyes (origin + view offset), with the GMod point-contents cache (a probe is re-queried only after moving > 1 u, as the repo measured). Water type stored on the player.
- `WaterMove`: wish = forward*fmove + right*smove + up*umove with the view vectors unflattened; if IN_JUMP: `wish.z += maxspeed`; if no input at all: `wish.z -= 60` (sink); wishspeed clamped to maxspeed then **x 0.8**; friction on the full 3D velocity: `newspeed = speed - frametime * speed * sv_friction * surfaceFriction` (only if speed > 0.1), acceleration = ground rule with `sv_accelerate`; then `TryPlayerMove`; when the move hit something, a second attempt "stepping up" 1/... (the SDK step-up retry from origin + (0,0,stepsize); GMod's version ends with the lower-half hull trace for the floor check). Swim sound every **1000 ms** when jump is held under water (`Player.Swim` timer); the engine-level "Player.Swim" also fires when the water level crosses 0.
- `CheckWaterJump` (waist-deep, moving forward): flat velocity must point forward; probe **24 u** ahead at mid-hull, then from `origin.z + viewOffset.z + 8` (WATERJUMP_HEIGHT); if the upper probe is clear: `waterJumpVel = -50 * wall normal` (stored), `velocity.z = 256`, add FL_WATERJUMP, `waterJumpTime = 2000 ms`. **[bin-S] H** (256 and 2000 are literals).
- `WaterJump`: `waterJumpTime` capped at **10000**, decremented `1000*frametime`; ends (flag cleared, timer 0) when it reaches 0 or the player leaves the water; while active the horizontal velocity is set to `waterJumpVel`.
- Drowning (player think, not the move): SDK rule - 12 s of air, then drown damage every second (2, 4, 6... up to 10 per hit) with "Player.DrownStart"; GMod keeps the code, but the repo measured **no drowning damage at all in sandbox for 60 s** (gamemode or convar gating; not resolved here). **[bin-S][repo] M**

---

## 7. Noclip (GMod rewrite) **[bin-S] H**, speed constant **M**

`FullNoClipMove` ignores the acceleration/friction parameters the caller passes (the SDK's accelerate-then-friction glide is gone):

```
dir = forward * fmove + right * smove                  (view vectors, not flattened)
if IN_JUMP and the "allowed" condition: dir += up * 1  (the condition is: the blocked byte clear, or the drive-mode virtual reports mode 6)
dir = normalize(dir)                                   (component-wise normalisation of the three terms, then of the sum)
speed = sv_noclipspeed * K                             (K unresolved; the repo measured 400 u/s at the default 5 -> K = 80)
if IN_SPEED: speed *= 3   (unresolved constant, reads as 3.0: the repo measured 1000 -> 2.5; one of the two is off, see gaps)
if IN_DUCK:  speed *= small constant (reads like 0.1; unmeasured)
velocity = dir * speed
origin += velocity * frametime          (no collision, no ground, no clamp)
```
Also when the noclip impulse toggles, Lua `PlayerNoClip` decides (section 0). `sv_noclipaccelerate` is read but has no effect. **[bin-S] H**

Observer move (`FullObserverMove`) keeps the SDK accelerate/friction glide with `sv_specspeed`, `sv_specaccelerate`, `sv_friction`, `sv_specnoclip`. **[bin-S] H**

---

## 8. Falling, landing, view punch

### 8.1 Fall velocity tracking **[bin-S] H**

`m_flFallVelocity` is `-velocity.z` tracked while airborne (SDK `CheckFalling` input). `CheckFalling` runs at the end of every ground tick:

1. **GMod hook first**: if on ground, alive, `fallVelocity >= small threshold` (unresolved; reads as the same constant as the 8-deg punch clamp, so 8 u/s) -> Lua **`OnPlayerHitGround(ply, inWater, onFloater, fallSpeed)`**. A `true` return: fall velocity reset to 0, **no sound, no punch, no damage**.
2. Otherwise, if on ground and `fallVelocity > 0`:
   - if alive and `fallVelocity >= 350` (PLAYER_FALL_PUNCH_THRESHOLD):
     - in water (water level != 0): `fvol = 0.5`;
     - else: if the ground entity floats (`IsFloating`): `fallVelocity -= 200`; if the ground entity moves down (`ground.velocity.z < 0`): `fallVelocity = max(0.1, fallVelocity + ground.velocity.z)`; then `fallVelocity > 580` -> **fall damage** via the move helper (-> `GetFallDamage`, 8.2), `fvol = 1.0`; `> 290` -> `fvol = 0.85`; `< 200` -> `fvol = 0`; else `fvol = 0.5`.
     - `PlayerRoughLandingEffects(fvol)`; if still alive `PlayerSetAnimation(walk)`.
   - `OnLand(fallVelocity)` (empty); `fallVelocity = 0`.

Constants 350/200/580/290/0.85/0.5 are the SDK's (the comparison chain matches exactly) **M**; the repo measured "a 342 u/s landing punches, 191.5 does not", consistent with a threshold between - note that with 350 a 342 landing should *not* punch: see gap list (the threshold may be the hook's or `PlayerRoughLandingEffects` is reached via another path; flagged).

### 8.2 Fall damage **[Lua][SDK] H**

Engine -> `GM:GetFallDamage(ply, speed)`: `mp_falldamage 1` -> `(speed - 526.5) * 100/396`; else flat **10**. Damage type DMG_FALL (armour does not absorb it). `Player.FallDamage` sound script on the hurt landing (repo measured volume 0.75, pitch 90-110). Death by fall plays `Player.FallGib`. The 580 threshold above is what triggers the call (the repo measured hurt at >= 526.5 - SDK HL1-branch value; see gaps).

### 8.3 PlayerRoughLandingEffects(fvol) **[bin-S] H**

If `fvol > 0`: `stepSoundTime = 400 ms` (literal), `PlayStepSound(origin, surface, fvol, force)`; **punch angle roll = fallVelocity * 0.013** (set, not added; M for 0.013, repo-confirmed), **punch pitch clamped to <= 8** (literal 8.0); rumble (long if fvol > 0.85). The lateral wall slam (3.9) reuses this with its own fvol.

### 8.4 DecayPunchAngle **[bin-S] H** (SDK formula, constants M, repo-confirmed)

If `|punch|^2 > 0.001` or `|punchVel|^2 > 0.001`: `punch += punchVel * dt`; `punchVel *= max(0, 1 - 9*dt)`; `punchVel -= punch * min(2, 65*dt)`; clamp punch to pitch +-89, yaw +-179, roll +-89. Else both are zeroed. Punch is added to the view angles (1.1) and networked; `Player:ViewPunch` adds to it. GMod runs this inside the move (per command), as the SDK does.

---

## 9. Ladders

Two systems coexist, both in the binary:

### 9.1 `func_useableladder` (HL2 path) **[bin-S] H**

- Every tick with no ladder: `Findladder(64 u)` collects the nearest ladder whose segment comes within 64 u of the player (SDK rule). `sv_ladder_useonly 0`: automount when **looking along the ladder axis** (`LookingAtLadder`: `dot(ladderAxis, forward) > sv_ladderautomountdot (0.4)` and within reach) and pressing forward -> `StartForcedMove` pulls the player to the ladder line at the per-player **ladder speed** (a GMod field; the repo measured 200 u/s) with velocity zeroed and movetype NONE until on the line, then movetype LADDER; `CheckLadderAutoMountCone(ladder, 15 deg, 32 u)` and `CheckLadderAutoMountEndPoint` when on the ground and moving forward. IN_USE pressed near a ladder mounts it too (`SwallowUseKey`).
- On the ladder (`LadderMove`, HL2 override): IN_USE pressed -> dismount (movetype WALK). The move: `forward/back/left/right buttons` add +-(per-player ladder speed) to a forward and a right scalar (the analog move vector is ignored); IN_JUMP -> dismount with `velocity = normalize(view forward, negated when fmove < 0) * climbSpeed` and `+50 z` when `forward.z >= 0` (constant unresolved, SDK 50; repo-confirmed) ; no buttons -> velocity 0, stay. With input: wish = forward*f + right*r normalised; `ladderDir` from the ladder (bottom->top); climb up if `dot(wish, ladderDir) >= -threshold` where the threshold is **-0.3 / 0.3** depending on whether the ladder is near-vertical (height difference < 64 u => "horizontal" ladder uses a different factor - SDK `ishorizontal`); `velocity = climbSpeed * (+1 or -1) * ladderDir`, with **climbSpeed = the per-player `Player:GetLadderClimbSpeed()`** (GMod virtual replacing MAX_CLIMB_SPEED 200 everywhere in both ladder paths). Distance to the ladder line must stay within a tolerance (constant); otherwise fall off (movetype WALK). `sv_autoladderdismount 1`: reaching the top/bottom within `climbSpeed * tickInterval` u of the end dismounts onto the dismount node (`info_ladder_dismount`, sorted by distance, `ExitLadderViaDismountNode`: forced move to the node at 100 u/s?? (literal 100.0 appears there) - **M**).
- Ladder step sounds: `UpdateStepSound` ladder branch (section 10).

### 9.2 Brush / "climb contents" ladders (base path) **[bin-S] H**

The SDK base `LadderMove`: on the ladder when a hull trace along the view/velocity hits a surface with CONTENTS_LADDER (`OnLadder`), within `LadderDistance()` (SDK 2 u, const unresolved), mask `LadderMask()` (MASK_PLAYERSOLID), `ClimbSpeed()` (SDK 200; **GMod calls the per-player climb speed here too**), `LadderLateralMultiplier()` 1. Climbing: `velocity = normal cross ...` the SDK "lateral/perpendicular" decomposition; jumping off: `velocity = ladder normal * 270` (SDK) **[SDK] M**; the GMod per-player climb speed replaces the 200.

---

## 10. Footsteps (not in this dump: SDK + Lua + repo measurements)

`UpdateStepSound` (SDK, in the shared player code, run once per move with the state entering the tick) **[SDK] H**:

- `stepSoundTime -= 1000*frametime` (clamp 0); return while > 0; dead -> return; observer / frozen -> return.
- `speed = |velocity.xy|`; thresholds `velwalk/velrun` = **90 / 220** standing, **60 / 80** when FL_DUCKING (the SDK comments: "these should be based on cl_movespeedkey"); on a ladder the SDK uses its own branch (volume 0.5, 450 ms via Lua).
- Step only if `speed >= velwalk` and (on ground or on a ladder) and the player moved (origin changed).
- `walking = speed < velrun`; volume: on ladder 0.5; water level 2 (waist): 0.65 ("wade"); water level 1 / ground: `walking ? 0.2 : 0.5`, some surfaces 0.25/0.55, 0.4/0.7 (metal grate / chainlink in the SDK table); **ducked multiplies by 0.65** (SDK rule, repo-measured).
- Then `SetStepSoundTime(type, walking)` -> **GMod routes this through Lua `GM:PlayerStepSoundTime(ply, type, walking)`** **[Lua] H**: NORMAL / WATER_FOOT: `GetMaxSpeed() <= 100 -> 400 ms; <= 300 -> 350; else 250`; ON_LADDER 450; WATER_KNEE 600; +50 ms when `Crouching()`. (SDK defaults would be 400/300, 350, 600, +100 ducked.) `GetMaxSpeed` is the current cap (200 walking, 400 sprinting, 100 slow-walking; the crouch factor does not change it).
- The wave: left/right alternate (`m_Local.m_nStepside`), `PlayStepSound(origin, surface, volume, force)` -> Lua **`GM:PlayerFootstep(ply, pos, foot, soundName, volume, filter)`** (return true = silence; base gamemode silences dead players); otherwise the surface property's `stepleft`/`stepright` script is emitted (`Concrete.StepLeft` = concrete1/3, `StepRight` = 2/4; channel body, level 75). Jump: a forced step at volume 1.0 (4.1). Landing: `PlayerRoughLandingEffects` plays a forced step at `fvol` (0.85 / 1.0 / 0.5) and resets the timer to 400 ms.
- Repo measurements (`move_step_*`) agree with all of the above: intervals 24/17/27 ticks (350/250/400 ms), volumes 0.2/0.5, ducked x0.65, jump step 1.0, landing step 0.85 above 290 u/s, 1.0 above the hurt threshold. **[repo] H**

---

## 11. Client prediction and the eye **[bin-C] H / [SDK]**

- The client runs the same shared movement class inside `CPrediction::RunCommand` for every unacknowledged usercmd, with the same Lua hooks (`SetupMove`, `Move`, `FinishMove` run client-side too, which is why the sandbox jump boost predicts).
- Predicted fields include origin, velocity, view offset, punch angle and velocity, duck state, duck/unduck/jump timers, fall velocity, step side and step-sound time, water level, ladder entity. `m_vecViewOffset` is what the eye uses: the client's eye position = origin + view offset plus the SDK stair smoothing (`SmoothViewOnStairs`: when the origin jumps up by a step the eye follows at 150 u/s, offset clamped to 18 u; not in this dump) **[SDK] M**.
- Prediction error correction (SDK `cl_predict`, `cl_smoothtime 0.1`): when the server state differs from the predicted copy, the client replays the pending commands from the server state and smooths the origin error over `cl_smoothtime` (0.1 s) when it is below 64 u (snaps above); standard SDK; GMod keeps `cl_predict 1` forced (`FCVAR_REPLICATED`). **[SDK] M**
- View punch decays inside the predicted move, so the camera gets it with no extra interpolation; `CalcRoll` is also in the move (not in the client view code).

---

## 12. GMod vs Source SDK 2013 - summary of differences found in the binary

| Area | SDK 2013 | GMod binary |
|---|---|---|
| Jump impulse | `sqrt(2*800*21)` = 268.3 constant | per-player JumpPower (200) x surface jumpFactor **[bin-C] H** |
| Forward jump boost / jump time | single-player only, in engine | removed from the engine; sandbox Lua FinishMove re-adds it (0.5 / 0.1 shares, no sprint rule) **[bin-C][Lua] H** |
| Duck/unduck durations | 0.4(0.2)/0.2 s fixed | per-player DuckSpeed/UnDuckSpeed (sandbox 0.1/0.1) **[bin-S] H** |
| Hull / eye offsets | gamerules view vectors | per-player hull & view vectors x model scale **[bin-S][bin-C] H** |
| Friction | `sv_friction * surfaceFriction` | also x player's entity friction **[bin-S] H** |
| Standable slope | 0.7 hard-coded | `sv_standable_normal` convar **[bin-S] H** |
| Ground probe | origin -> -2 u | +2 u -> -1 u, lower-half hull first; `sv_sticktoground` disables the "rising > 140" early-out **[bin-S] H** |
| StayOnGround | up 2 / down stepsize | up stepsize / down 2 x stepsize **[bin-S] H** |
| Step | always `+stepsize` | only while allow-auto-movement **[bin-S] H** |
| Noclip | accelerate + friction glide | direct velocity: `sv_noclipspeed * K`, x3 sprint, slower while ducked, no acceleration **[bin-S] H** |
| Frametime | engine | x lagged-movement scalar **[bin-S] H** |
| Max speed | convars | per-player speeds + ground-entity speed scalar + constraint **[bin-S] H** |
| Ladder climb speed | 200 constant | per-player climb speed (+ a separate per-player mount/button speed) **[bin-S] H** |
| Landing | engine only | `OnPlayerHitGround` hook can suppress everything; `GetFallDamage` Lua; `PlayerStepSoundTime`/`PlayerFootstep` Lua **[bin-S][Lua] H** |
| Move pipeline | engine | `StartCommand`/`SetupMove`/`Move`/`FinishMove`/`PlayerNoClip`/`PlayerButtonDown/Up`/`VehicleMove` hooks **[bin-S] H** |
| Dead view height | 14 | 14 x model scale **[bin-S] H** |
