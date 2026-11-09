# VPhysics gap list: game (vphysics.md) vs gmod-rust (`main`, clone of 2026-11-09)

Rust files referenced (paths relative to the repo root):

* `crates/gm-physics/src/lib.rs` (Jolt wrapper, 2102 lines)
* `crates/gm-physics/src/shim.cpp` (Jolt C++ shim: constraints, impact listener, 1720 lines)
* `crates/gm-physics/src/physgun.rs` (shadow controller for the physgun)
* `crates/gm-physics/src/surface.rs` (surfaceproperties)
* `crates/gm-sandbox/src/lib.rs` (sandbox constraints, tool convars)
* `crates/gm-server/src/lib.rs` (game glue)
* `crates/gm-collision/src/lib.rs` (traces/hulls; no physics rules)

Ranking: P0 obvious in game, P1 noticeable, P2 subtle. "Verdict" is match / partial / mismatch /
missing.

## A. Contact, friction, restitution

| # | behaviour (game) | Rust today | verdict | gap | rank |
|---|---|---|---|---|---|
| A1 | pair friction = f1*f2 clamped to [0,1] | `shim.cpp:601-603` sets Jolt combined friction to `min(1, max(0, f1*f2))` | match | none | - |
| A2 | pair elasticity = e1*e2 fed to an IVP impact with speed-dependent atan adjustment; measured ratio about sqrt(e1 e2) | `shim.cpp:602` computes `e1*e2`, `shim.cpp:1005-1007` separates at `sqrt(e1 e2)*vn + floor`, Jolt restitution zeroed (`:604`); but body creation still sets Jolt `Restitution = 0.5*(e + world_e)` (`lib.rs:858`, `:1644`, `world_elasticity` 0.2 at `:549`) for contacts the listener does not take over | partial | second-contact and sleeping-pair bounces use an average restitution the game never uses; the low speed floor `0.417 + 0.467 f` (`shim.cpp:1000`) is a fit, not the game's atan rule; `docs/spec/physics.md` reports 1 of 5 drop/roll harnesses within tolerance | P1 |
| A3 | contact damping = d1 + d2 (surfaceprop `dampening`) and adhesion a1 + a2 | `surface.rs:12` parses `dampening`; nothing consumes it | missing | no contact damping path; stock surfaces have 0 so only custom materials notice | P2 |
| A4 | friction solver clamps body masses to [10, 2500] kg (performance settings) | no equivalent (Jolt uses true masses) | missing | stacks of very light on very heavy props (1 kg on 5000 kg) behave differently; heavy props sliding on light ones | P2 |
| A5 | contact keeper distance 0.01 m (rest gap 0.3937 u) | `REST_GAP = 0.01 m` (`lib.rs:209`), `SPECULATIVE_CONTACT_M 0.15` (`:178`), `PENETRATION_SLOP_M 0.02` (`:196`) | match (gap), partial (slop) | penetration slop 0.02 m lets bodies sink 0.79 u before correction; the game holds the keeper distance | P2 |
| A6 | sequential per-vertex impact processing (lateral/angular asymmetry on flat landings) | single point push loop in `shim.cpp:448-1010` | partial | documented divergences for tilted and rolling cases in `docs/spec/physics.md` | P1 |
| A7 | friction and shadow material swap for held objects (reserved material) | not present | missing | carried (`+use`) props slide with the real material | P2 |

## B. Damping, drag, velocity limits

| # | behaviour | Rust today | verdict | gap | rank |
|---|---|---|---|---|---|
| B1 | linear damping v *= exp(-d dt), angular per axis exp(-D_i dt), inside the gravity step before gravity is added | `lib.rs:1229` forwards to Jolt `LinearDamping/AngularDamping` (`shim.cpp:364`); Jolt applies `v *= max(0, 1 - c dt)` after integration | partial | for d*dt small both agree to 1e-4; order vs gravity differs by one gravity increment per tick | P2 |
| B2 | extra settle damping added in the slow state | none | missing | props settle visibly slower/faster than the game in the last 0.5 s before sleep | P2 |
| B3 | quadratic orientation-dependent drag, bases from AABB and PHY axis areas, angular drag integral | `lib.rs:757-806` (`drag_factors`, `angular_drag_factors`), applied in `apply_drag` `lib.rs:1736-1770` against the gravity-predicted velocity | match | measured within 0.1 % per `docs/spec/physics.md` | - |
| B4 | drag disabled while shadow-controlled, re-enabled on release; `SetDragCoefficient` enables drag if non-zero | `lib.rs:1736-1745` skips held/frozen bodies; `set_drag_coefficient` `lib.rs:1028-1043` only stores | partial | setting a non-zero coefficient after `EnableDrag(false)` does not re-enable drag as in the game | P2 |
| B5 | air density 2.0 multiplier | folded into the measured bases (fit) | match (empirical) | `physenv.SetAirDensity` has no effect | P2 |
| B6 | max velocity 2000 u/s hard cap, angular 3600 deg/s cap, re-applied after every setter/controller | no linear cap; `MaxAngularVelocity = 200 rad/s` (`lib.rs:861`, 11459 deg/s); `clamp_angular_speed` exists (`lib.rs:1265`) but is only used by callers | mismatch | flung/explosion-launched props exceed 2000 u/s (game: never); spinning props exceed 3600 deg/s | P1 |
| B7 | `physenv.SetPerformanceSettings` / `GetPerformanceSettings` | not present | missing | addons that raise `MaxVelocity` do nothing | P2 |

## C. Gravity and sleep

| # | behaviour | Rust today | verdict | gap | rank |
|---|---|---|---|---|---|
| C1 | sv_gravity 600 u/s^2, v += g dt then integrate | `lib.rs:555` gravity factor 600*0.0254/9.81, `set_world_gravity` `lib.rs:1657`; `sv_gravity` convar `gm-server/src/lib.rs:8708`; measured ticks 1..3 match | match | - | - |
| C2 | per-object `EnableGravity`, held bodies weightless | `lib.rs:1676`, `:938-962` (hold sets factor 0), `:964` release | match | - | - |
| C3 | sleep state machine: position/orientation checkpoints, freeze check time, long term timer; crate sleeps about 1.8 s after rest; whole contact group sleeps together | Jolt defaults (velocity below 0.03 m/s for 0.5 s, per island); `deactivate/activate` `lib.rs:1625-1636` | mismatch | props fall asleep roughly 1.3 s earlier than the game; props sleeping mid-slide on slopes differ; `PhysObj:IsAsleep` timing visible to Lua (ragdoll fade, sandbox "freeze all" logic) | P1 |
| C4 | sleep callbacks wake/sleep to the game after each simulate | not surfaced (no wake/sleep events) | missing | `ENT:PhysicsUpdate` cadence and `RagdollIsAsleep` not reproducible | P2 |

## D. Shadow controllers (physgun, carry)

| # | behaviour | Rust today | verdict | gap | rank |
|---|---|---|---|---|---|
| D1 | a = delta*scale - v*damp, clamp each component of a to [-maxSpeed, maxSpeed], v += a | `physgun.rs:92-108`: `want = v(1-damp) + delta/T`, then the *resulting* speed magnitude is clamped to `max_speed`, plus an extra `max_dv` cap | partial | the game clamps the per-axis velocity change, not the result; with maxSpeed 5000 both are rarely hit for props, but for `physgun_maxSpeed` changed by addons or for the carry (lower limits) the shapes differ; `max_dv` (`physgun.rs:21`, `gm-server/src/lib.rs:23602` RAGDOLL_HOLD_DV 80) is a fit for what the game gets from mass scaling in the constraint group, not a game parameter | P1 |
| D2 | angular controller: axis*angle delta, same formula, per axis clamp | `physgun.rs:110-131` same formula with magnitude clamp | partial | same clamp shape difference | P2 |
| D3 | teleport distance (beam when |delta| > d), default 0 for physgun | not implemented | missing | `physgun_teleportDistance` > 0 has no effect | P2 |
| D4 | `Update` ignores tiny target changes (no wake) | hold always activates (`lib.rs:958`) | partial | held sleeping props never stay asleep (minor) | P2 |
| D5 | ground correction for shadow objects allowed to move physically | none | missing | carried props pushed against slopes sink slightly | P2 |
| D6 | held object keeps velocity on release | `lib.rs:964-973` | match | - | - |
| D7 | one tick lag: game velocity 7 u/s faster in the first two ticks | documented open in `docs/spec/physgun.md` | partial | ordering of controller vs integration | P2 |

## E. Player controller

| # | behaviour | Rust today | verdict | gap | rank |
|---|---|---|---|---|---|
| E1 | player shell driven by a shadow controller (dampFactor 1.0, push limits 50000/10000, teleport when off target, ground velocity ride-along, push impulse limiting, 100 angular damping) | no physics shell for players; props are pushed by game logic (`gm-server/src/lib.rs:2268-2273` NPC shove logic, no player-prop impulse) | missing | players cannot push props by walking into them the way the game does (momentum-based, mass-limited); standing on props does not press them down; moving platforms do not carry the shell | P0 |
| E2 | `IsInContact`, `GetShadowVelocity`, `StepUp` | none | missing | depends on E1 | P1 |

## F. Motion controllers and springs

| # | behaviour | Rust today | verdict | gap | rank |
|---|---|---|---|---|---|
| F1 | `PhysicsSimulate` motion controller with 4 result modes, velocity limit after each body | thruster/balloon forces as per-tick impulses in `gm-sandbox/src/lib.rs:604,648,894,937` | partial | no generic motion controller for Lua SENTs; no velocity limit after the push | P1 |
| F2 | spring actuator: F = k(len - rest) - c(v_rel . dir), extra relative damping term, stretch-only, break length | `lib.rs:1132-1178` (`spring`), `apply_springs` `lib.rs:1402-1448`: `F = -(k*stretch + c*rate)` applied at anchors; stretch-only handled in sandbox | partial | no relative-velocity isotropic damping term, no break length | P2 |
| F3 | hydraulic/winch/muscle constants `CalcElasticConsts` | `gm-sandbox/src/lib.rs:480` | match (lua) | - | - |

## G. Constraints

| # | behaviour | Rust today | verdict | gap | rank |
|---|---|---|---|---|---|
| G1 | weld: position-level fixed constraint solved in a local system, one-tick lag, residual deformation 1.8 u | `shim.cpp:83-155` Jolt fixed constraint tuned to 60 Hz critically damped (rot 15 Hz, 0.5) with env overrides | partial | tuned to approximate; the game's residual deformation and re-push at landing remain open (documented in the shim comment) | P1 |
| G2 | breakable wrapper: forceLimit*0.0254 and torqueLimit*pi/180 tested against the constraint impulse per PSI, break -> `OnBreak` / constraint removed | `gm-sandbox/src/lib.rs:199-300` stores `forcelimit`/`torquelimit` ("stored, not simulated", line 213); convars clamped at `gm-server/src/lib.rs:1386-1549` | missing | welds/ropes/axes never break under load; `constraint.Weld` with forcelimit is a common contraption element | P0 |
| G3 | rope: distance constraint min/max, acts the tick before the length is passed, never pushes when slack | `shim.cpp:155-210` Jolt distance constraint with 60 Hz spring on the limits plus `gmp_rope_predict`; `lib.rs:1056-1085` | match (measured) | rigid rope uses min = max (`lib.rs:1067`), fine | - |
| G4 | hinge with limits, motor (`SetAngularMotor`) and hinge friction | `shim.cpp:250-268`, `lib.rs:1450` | partial | motor and friction units not matched to the game's torque conversion (not verified) | P2 |
| G5 | ball socket | `shim.cpp:269`, `lib.rs:1087` | match | - | - |
| G6 | slider with limits, friction, linear motor | `shim.cpp:295-312`, `lib.rs:1103` ("slides without friction") | partial | no slider friction, no linear motor, no limits | P2 |
| G7 | pulley: |p1-w1| + ratio*|p2-w2| <= L, rigid option | `lib.rs:1298`, `apply_pulleys` `lib.rs:1357-1400` explicit impulse solver | partial | solver is a per-tick impulse, not solved jointly with contacts (stretch under load) | P2 |
| G8 | constraint group error state (minErrorTicks 2, errorTolerance 3 u) and `RagdollSolveSeparation` | none | missing | exploded ragdolls do not snap back; stretched ragdolls stay stretched | P1 |
| G9 | ragdoll joints: limited ball socket, per axis limits, dry friction, `animatedfriction` ramp | `shim.cpp:213-248` swing/twist with per-axis `mMaxFriction`, `gm-server/src/lib.rs:23596-23612` ramp with `RAGDOLL_FRICTION_SCALE 0.05` | partial | friction scale 0.05 is a fit; the game's joint friction is the .phy torque value applied through the solver's mass scaling | P1 |
| G10 | local constraint system solved per PSI with strength/tau from blueprint | Jolt solver settings `shim.cpp:435-443` (`gmp_set_solver`) | partial | different solver; welded towers and long ropes chains behave softer/stiffer | P1 |

## H. Buoyancy and fluids

| # | behaviour | Rust today | verdict | gap | rank |
|---|---|---|---|---|---|
| H1 | fluid controller per water volume: buoyancy by submerged volume * ratio, viscous damping, current, no buoyancy for held bodies | nothing in the crates (no `buoyan`/fluid code; only player water level in `gm-server/src/lib.rs:941-943, 22670-22750`) | missing | props dropped in water sink like in air; wooden crates do not float; `PhysObj:SetBuoyancyRatio` no-op | P0 |
| H2 | `ObjectEnterTrigger`/`ObjectLeaveTrigger` (splash) | player splash only (`gm-server/src/lib.rs:652`) | missing | no prop splash sounds/effects | P1 |

## I. Collision rules and groups

| # | behaviour | Rust today | verdict | gap | rank |
|---|---|---|---|---|---|
| I1 | pair filter (nocollide, ragdoll self pairs, held object vs holder) | `lib.rs:1606` `set_pair_collision`, `lib.rs:1612` `set_world_only`; sandbox nocollide `gm-sandbox/src/lib.rs:2636`, collide property `:2432`, `gm-server/src/lib.rs:23223` | match (subset) | - | - |
| I2 | collision groups table (debris, interactive debris, weapon, player movement, NPC ...) and the Lua `ShouldCollide` hook | only `COLLISION_GROUP_WORLD` semantics via `set_world_only`; NPC hull blocking note at `gm-server/src/lib.rs:16054`; no group matrix in `gm-collision` | missing | debris gibs collide with players; weapons on the floor block props; `hook.Add("ShouldCollide")` has no effect | P1 |
| I3 | `CollisionRulesChanged` re-filtering, callback re-entrancy warning | none | missing | depends on I2 | P2 |
| I4 | ragdoll `collisionrules` from .phy (default: all self pairs off, `collisionpair` re-enables) | ragdoll bodies created in `gm-server`; pair handling via `set_pair_collision` (not verified for the .phy block) | partial | not verified | P2 |

## J. Callbacks

| # | behaviour | Rust today | verdict | gap | rank |
|---|---|---|---|---|---|
| J1 | collision callback table (`DeltaTime`, `Speed`, both surface props, old/new linear and angular velocities, `HitNormal`, `HitPos`, `HitSpeed`, entities) | impact records `lib.rs:401-444` (`RawImpact`: ids, point, speed; `Contact` kinds) | partial | no pre/post velocities, no surface props, no normal in the record; Lua `PhysicsCollide` payload incomplete | P1 |
| J2 | friction callback 4-slot table, scrape sounds by `scrapeRoughThreshold`, player material suppressed | `gm-server/src/lib.rs:1707-1716` slip thresholds 66/40 u/s and 0.15 fade | partial (measured) | no 4 slot limit, no rough/smooth selection by the other surface, no dust/spark effects | P2 |
| J3 | wake/sleep callbacks | none | missing | see C4 | P2 |

## K. Materials

| # | behaviour | Rust today | verdict | gap | rank |
|---|---|---|---|---|---|
| K1 | surfaceproperties defaults 0.8 / 0.25 / 2000 / 0 and `base` inheritance | `surface.rs:19-22, 41-126` | match | - | - |
| K2 | all keys incl. `thickness`, `jumpfactor`, `maxspeedfactor`, `climbable`, audio keys | only friction/elasticity/density/dampening numeric (`surface.rs:116-126`) | partial | movement-related keys live elsewhere (`gm-move`); physics has no consumer | P2 |

## Things Rust does that the game does not

1. Jolt restitution average `0.5*(e_body + e_world)` at body creation (`lib.rs:858`, `:1644`): the game has no averaging anywhere; only the product rule exists.
2. Speculative contacts 0.15 m and linear-cast motion quality (`lib.rs:178-185`, `:869`): the game has no CCD; fast props tunnel in the game (and are capped at 2000 u/s instead).
3. `max_dv` velocity-change cap in the physgun controller (`physgun.rs:19-21`) and `RAGDOLL_HOLD_DV 80` (`gm-server/src/lib.rs:23602`): a fit standing in for the constraint-group mass scaling.
4. Face-landing speed floor `0.417 + 0.467 f` (`shim.cpp:1000`): a fit for the IVP impact rule described in vphysics.md 5.2.
5. Weld as a 60 Hz critically damped spring with env-var tuning (`shim.cpp:79-98`): the game's weld has no frequency; it is a position-level solve in a local system with strength 1.0.
6. Rope limit spring 60 Hz (`shim.cpp:155`): the game's rope is a stiff (rigid at the limit) constraint.
7. Angular velocity cap 200 rad/s (`lib.rs:861`): the game caps at 3600 deg/s = 62.8 rad/s.
8. Ragdoll joint friction scale 0.05 (`gm-server/src/lib.rs:23596`): not a game constant.
9. `gm_phys_paused` convar (`gm-server/src/lib.rs:8705-8710`): ours (documented as such).

## Ranked summary

P0: E1 player physics shell / prop pushing; G2 breakable constraints (forcelimit/torquelimit);
H1 buoyancy.
P1: B6 velocity/angular caps; C3 sleep timing; A2/A6 impact model; D1 controller clamp shape;
G1/G10 weld and solver stiffness; G8 ragdoll error recovery; G9 ragdoll friction; I2 collision
groups and `ShouldCollide`; J1 collision callback payload; F1 generic motion controller; H2 prop
splashes.
P2: everything else above.
