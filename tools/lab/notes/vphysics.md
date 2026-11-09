# VPhysics (Garry's Mod, IVP based) behaviour note

Scope: the physics middleware (`vphysics_srv.so`) plus the server-side glue that the dump
covered (`server_srv.so`: collision/friction callbacks, gravity convars, ragdoll creation).
Nothing here quotes or names binary internals; everything is described as behaviour.

Tag legend (one per fact):

* `[pc:vphysics]` read in the vphysics pseudo-C (3207 functions in the dump).
* `[pc:server]` read in the server pseudo-C (484 functions).
* `[sdk]` Source SDK 2013 game code (server/shared), used only where the binary does the same or
  where the dump does not contain the function.
* `[lua]` GMod Lua / wiki behaviour.
* `[meas]` already measured against the real game in `docs/spec/physics.md` /
  `docs/spec/physgun.md` of the Rust repo (oracle traces).
* `[inf]` inference (stated as such).
* Confidence: H (read directly, no unresolved constants), M (read, but one or more constants
  live in the data segment and could not be resolved from the pseudo-C), L (structure seen,
  value inferred from SDK-era knowledge or measurement).

A note on constants: the dump is code only. Floating constants that the compiler kept in the
data segment appear as unresolved globals; where the value matters I give the SDK value or the
measured value and tag it. Constants that were inlined as immediates (1.0, 0.5, 0.03, 10, 2500,
10000, 50000, 100 ...) are exact and tagged H.

---------------------------------------------------------------------------------------------

## 0. Units, frames and the simulation loop

* All game-facing APIs are in Source units (inches, degrees); internally the engine converts
  with 0.0254 m per inch and pi/180 for angles. Internally the vertical axis is the second
  component and the third component is the negated Source y, i.e. internal (x, y, z) =
  (sx * 0.0254, sz * 0.0254, -sy * 0.0254). Every position/velocity setter and getter does this
  conversion. `[pc:vphysics]` H
* The simulation is a fixed-step "PSI" integrator. The game sets the step to the server tick
  interval (1/66 s = 0.015151 s by default). `[sdk]` H. The per-tick entry point refuses a step
  larger than 1.0 s and smaller than a tiny epsilon; if the requested step exceeds an upper
  bound it is clamped to a data-segment constant (SDK: 0.1 s). When the requested time equals
  the PSI step it runs exactly one PSI, otherwise it integrates time with sub-stepping up to the
  next PSI boundary. `[pc:vphysics]` M
* Order inside one PSI for a moving body: controllers run in priority order. The gravity
  controller does, for every body it owns: (1) velocity damping (below), (2) commit queued
  asynchronous pushes, (3) add gravity * dt (or an alternative per-body gravity if the body
  has the "alternate gravity" flag). Then the friction/contact system, constraints, then
  position integration, then the collision pipeline. `[pc:vphysics]` H for the gravity
  controller's internal order; M for the global controller order (priorities are in data).
* After each simulate call the wrapper runs the sleep bookkeeping (objects that changed
  movement state fire wake/sleep callbacks to the game) and then the "virtual mesh" PSI for
  displacement terrain. `[pc:vphysics]` H
* The whole simulate call is guarded by a mutex (a single physics thread). `[pc:vphysics]` H
* The wrapper keeps a "dead object" list; objects destroyed during callbacks are only deleted
  at the start of the next simulate. `[pc:vphysics]` H

## 1. Environment creation defaults

* On environment creation the wrapper: creates the IVP environment, sets the global collision
  tolerance from a data-segment constant scaled to metres (SDK value 0.25 inch = 0.00635 m,
  see 1.1), installs the object/collision/constraint listeners, and pushes default
  performance settings. `[pc:vphysics]` H (structure), M (tolerance value)
* 1.1 Collision tolerance: `SetGravity` re-derives the tolerance from the gravity magnitude and
  prints "Set Gravity %.1f (%.3f tolerance)" to the console. Formula structure: tolerance =
  base + k * |g| in metres, constants in data. `[pc:vphysics]` M
* Air density is a per-environment scalar passed straight into the drag controller; the game
  sets 2.0. `[pc:vphysics]` H for the setter, `[sdk]` for the value.
* Gravity default: the server reads `sv_gravity` (default 600 u/s^2) and calls the
  environment gravity setter with (0, 0, -sv_gravity) whenever the convar changes.
  `[pc:server]` H. Measured: semi-implicit Euler, v += g*dt before position integration (ticks
  1..3 from rest: v = -9, -18, -27 u/s; z = -0.135, -0.405, -0.81 u). `[meas]` H
* Performance settings (the game pushes these at level init; GMod exposes them to Lua through
  `physenv.SetPerformanceSettings`, which the dump confirms exists as a redirected Lua binding):

  | field | default | note |
  |---|---|---|
  | max velocity | 2000 u/s (50.8 m/s) | stored in m/s; see 7 for how it is applied |
  | max angular velocity | 3600 deg/s | stored as radians per PSI = deg2rad * dt * value |
  | min friction mass | 10 kg | clamped into a data-segment range; the IVP default is also 10 |
  | max friction mass | 2500 kg | same; the IVP default is also 2500 |
  | look-ahead time object vs world | 1.0 s | passed to the collision manager |
  | look-ahead time object vs object | 0.5 s | passed to the collision manager |
  | max collisions per object per timestep | 6 | `[sdk]` |
  | max collision checks per timestep | 250 | `[sdk]` |

  Defaults `[sdk]` H; the conversion/storage rules `[pc:vphysics]` H; the IVP fallback values
  (10, 2500, pi/2 rad per PSI angular cap, effectively unlimited linear) `[pc:vphysics]` H.
* The raw IVP anomaly defaults (before the game overrides them) are: linear limit effectively
  infinite, angular limit pi/2 rad per PSI, min/max friction mass 10/2500. `[pc:vphysics]` H

## 2. Per-object defaults and setters

* An object is created from a collide, a material index, a volume, and two drag coefficients
  (linear, angular). Static objects get drag coefficients forced to 0 and never recompute drag
  bases. A volume of exactly 0 means "unknown": buoyancy ratio is then 1.0 and the volume is
  left at 0; otherwise the buoyancy ratio is computed from the volume (see 6). `[pc:vphysics]` H
* Default game flags at creation: callback flags for global collision, shadow collision,
  friction and so on are a fixed bit mask; the object starts with "collisions enabled" and a
  "needs drag recompute" style state. `[pc:vphysics]` H (bit layout not relevant to a rewrite)
* `SetMass(m)`: m is clamped to [0, max mass]; the upper bound is the same data constant used
  as the upper clamp of the friction-mass performance settings (SDK: 50000 kg). The object is
  temporarily given motion if it was motion-disabled, the IVP mass is changed, the buoyancy
  ratio is recomputed from the stored volume (or set to 1.0 if the volume is the "unknown"
  marker), and drag bases are recomputed. `[pc:vphysics]` H (structure) / M (50000)
* `SetInertia`, `SetDamping(lin, ang)`: linear damping is a scalar on the body; angular damping
  is a single scalar copied to all three body axes. Either pointer may be null to leave a value
  unchanged. `[pc:vphysics]` H. Default damping for a `prop_physics` comes from the model's .phy
  solid ("damping"/"rotdamping" keys); the SDK fallback object params are 0.04 linear and 0.01
  angular, but a spawned crate reports 0 / 0 in GMod. `[sdk]` + `[meas]` M
* `SetVelocity(v, w)`: wakes the body, writes the linear velocity (m/s) and angular velocity
  (rad/s) directly into the core AND clears the per-PSI accumulated push vectors, then applies
  the velocity limit unless the body has a shadow controller. `SetVelocityInstantaneous` is the
  same but also writes the "velocity at PSI start" copy. `[pc:vphysics]` H
* `ApplyForceCenter(F)`: this is an impulse, not a force: dv = F * 0.0254 * invMass added
  through the asynchronous push queue (applied at the next PSI), then velocity limit if not
  shadow-controlled. `ApplyForceOffset` additionally applies the torque impulse about the mass
  centre. `ApplyTorqueCenter` is an angular impulse in deg-based units converted with pi/180.
  `[pc:vphysics]` H
* `AddVelocity(v, w)`: adds to the async push queue (so it lands at the next PSI), not to the
  current velocity. `[pc:vphysics]` H
* `EnableGravity(b)`: adds/removes the environment gravity controller from the body's
  controller list. Static bodies ignore it. `[pc:vphysics]` H
* `EnableMotion(b)`: pins/unpins the body (a pinned body is treated like static mass for
  contacts), re-applies a hinge if the object was hinged, then re-evaluates the collision
  filter and wakes contacts. `IsMoveable` = not static and not pinned. `[pc:vphysics]` H
* `EnableDrag(b)`: adds/removes the environment's drag controller on the body. Static bodies
  ignore it. `SetDragCoefficient(lin, ang)` stores the coefficients and enables drag if either
  is non-zero (it never disables drag). `[pc:vphysics]` H
* `Wake()` = ensure the body is in simulation; `Sleep()` puts it out of simulation. `[pc:vphysics]` H
* `BecomeTrigger()` / `BecomeHinged(axis)` exist (trigger volumes fire enter/leave callbacks;
  hinged pins all but one rotation axis). `[pc:vphysics]` H (names only)
* `SetMaterialIndex(i)`: changes the body's material; notifies an attached shadow controller
  so it can restore it later. `[pc:vphysics]` H
* `RecheckCollisionFilter()`: re-runs the collision filter for all pairs of this object and
  wakes it; guarded against re-entrancy by a flag. `[pc:vphysics]` H

## 3. Damping and velocity limits

### 3.1 Velocity damping (per PSI, inside the gravity controller)

For each body with damping d (linear, scalar) and D = (Dx, Dy, Dz) (angular, body axes):

* factor_i = exp(-D_i * dt) when the sum of squares of D_i*dt is above a small threshold,
  otherwise the linear approximation 1 - D_i*dt; w_i *= factor_i.
* linear: v *= exp(-d * dt) (same small-value approximation). `[pc:vphysics]` H (shape),
  threshold constant M.
* Bodies in the "slow, might sleep" movement state (state 2 in 4.1) get an additional constant
  added to d and to each D_i before the factor is computed, i.e. extra damping while settling
  (constant is in data; SDK-era value 0.1 /s). `[pc:vphysics]` M
* The shadow player controller overrides the angular damping of the player body to 100 on all
  three axes while attached and restores the old value on detach. `[pc:vphysics]` H

### 3.2 Velocity limit

`ClampVelocity()` and most setters call the core velocity limit:

* if max linear velocity > 0 and |v| > max, v is scaled to max (both the current velocity and the
  "velocity at PSI start" copy). `[pc:vphysics]` H
* if the angular limit > 0, the angular speed is scaled to (angular limit per PSI) / dt, i.e.
  3600 deg/s with default settings. `[pc:vphysics]` H
* The collision pipeline uses a different check: if a body exceeds the limits, an "anomaly"
  callback is fired (max velocity exceeded / max angular velocity exceeded) and the wrapper
  clamps it; this is where the 2000 u/s cap is visible as a hard speed cap on flung props.
  `[pc:vphysics]` H (mechanism) / `[sdk]` for the wrapper behaviour (not in dump).
* Motion controllers call the velocity limit for each body after they push it (see 9).
  `[pc:vphysics]` H

## 4. Sleep / movement states

### 4.1 Movement state machine (per body, evaluated each PSI)

States: 1 = moving, 2 = slow (candidate), 3 = not moving (sleep).

* The body keeps a position/orientation checkpoint. If, since the checkpoint, the mass centre
  moved more than a distance threshold or the orientation changed by more than a threshold
  (compared through 2 * r^2 * (1 - |q1 . q2|)^2 against a constant, r = body radius), the body
  is "moving" (state 1) and the checkpoint is reset. `[pc:vphysics]` H (structure), M (thresholds)
* Otherwise, if the time since the checkpoint is <= the environment "freeze check time" the
  state is 2. `[pc:vphysics]` H / M
* Otherwise, if the angular speed squared is below (threshold / freeze time)^2 the body goes to
  state 3; else it compares the orientation to an older checkpoint and goes to state 3 if the
  long-term rotation is also tiny. A second long-term timer (another data constant) also
  forces state 3. `[pc:vphysics]` M
* Measured: a 30 kg crate dropped flat sleeps about 120 ticks (1.8 s) after it stops
  visibly moving; sleeping bodies rest with a gap of about 0.39 u above the floor (contact
  keeper distance, see 5.1). `[meas]` H
* Sleep is per "simulation unit" (friction system): bodies in contact share a unit and all go to
  sleep together; waking one wakes the others. Removing a body wakes its contact partners.
  `[pc:vphysics]` H (structure)
* The game is told through object wake/sleep callbacks after each simulate; the Lua side sees
  this as `PhysObj:IsAsleep()`, and ragdolls use it for `RagdollIsAsleep` (all parts asleep).
  `[pc:vphysics]` + `[pc:server]` H

## 5. Contact, friction and restitution

### 5.1 Contact model (IVP "mindist" / keeper model)

* Pairs are tracked by a minimal-distance object. A persistent contact point ("friction
  synapse") exists while the surfaces are within a keeper distance (data constant; the measured
  rest gap of 0.3937 u = 0.01 m is this distance). `[pc:vphysics]` H + `[meas]` H
* When two bodies approach, an impact is solved at the single contact point: the solver reads
  both materials, derives the pair elasticity and pair friction (5.2), pushes along the contact
  normal tilted by friction until the bodies stop approaching, then separates them with the
  "rescue speed": rescue = 2 * (rotational speed uncertainty + (keeper distance - current
  distance) / dt if the distance is below the keeper distance, else 0). `[pc:vphysics]` H
* Pre and post collision events are fired to the environment for each impact; the wrapper
  turns those into the game's collision callbacks (section 10). `[pc:vphysics]` H
* Contacts between resting bodies are solved every PSI by the friction system, which treats a
  connected group of touching bodies as a "friction system"; the system solves normal forces
  and then friction forces so that the tangential velocity goes to zero if the required force
  is below mu * normal force, otherwise slides with mu * N. The masses used in that solver are
  clamped to [min friction mass, max friction mass] (10 .. 2500 kg): a 5000 kg prop and a 1 kg
  prop on top of each other are solved as 2500 / 10. `[pc:vphysics]` M (clamp consumer is the
  friction solver per structure; values H)
* Measured side effects: a flat landing is processed sequentially vertex by vertex, which gives
  the small lateral and angular asymmetries recorded in `docs/spec/physics.md`. `[meas]` H
* A sleeping body that is hit wakes and takes the impact with its stored contact history
  (asleep drums take rim-vertex impacts and tumble). `[meas]` H

### 5.2 Material combination

The material manager combines two surfaces as follows:

| quantity | combination | where used |
|---|---|---|
| friction | f1 * f2 (product), then clamped to [lower, 1.0] by the Source wrapper; vehicle wheel contacts can override it with a per-wheel value | friction solver and impact tilt |
| elasticity | e1 * e2 (product) | impact solver (restitution) |
| damping | d1 + d2 (sum) | contact damping ("dampening" key) |
| adhesion | a1 + a2 (sum) | only if non-zero (unused by stock surfaces) |

`[pc:vphysics]` H for all four rules; the lower friction clamp constant is in data (0 by
inference). Note: this differs from Jolt's default (geometric mean of friction, max of
restitution) and from the SDK's documented "average" assumptions. Measured: rebound ratio of a
wood crate (e 0.1) on concrete (e 0.2) is 0.153; sqrt(0.1 * 0.2) = 0.141, so the product
elasticity enters roughly as a velocity ratio of sqrt(e1*e2) with a small additive term.
`[meas]` H, interpretation `[inf]` M.

* The impact solver adjusts the pair elasticity by the approach speed: it computes
  x = e * (1 + sqrt(s)) (s a stored speed-related term), a = atan(x), f = 1 - c1 * a^2 + c2 * a^4,
  and uses (f, f * x) as the elasticity pair; when either body is in a special state (a core
  field that marks it as not freely moving) the pair is (1.0, 1.0). Constants c1, c2 in data.
  `[pc:vphysics]` M. Practical consequence: low-speed rebounds are not pure restitution.
  Measured: a face landing leaves the floor at a "floor" speed 0.417 + 0.467 * friction
  (u/s-scale constants in the Rust shim's comments) regardless of elasticity; faster impacts use
  sqrt(e) scaling. `[meas]` M
* Material default values (`scripts/surfaceproperties.txt`, default entry): friction 0.8,
  elasticity 0.25, density 2000, dampening 0, thickness 0. Keys parsed by the binary:
  `base density elasticity friction dampening thickness gamematerial jumpfactor
  maxspeedfactor climbable audioHardMinVelocity audioHardnessFactor audioReflectivity
  audioRoughnessFactor impactHardThreshold scrapeRoughThreshold` plus the sound keys
  `stepleft stepright impacthard impactsoft scraperough scrapesmooth bulletimpact rolling break
  strain`. Any invalid material index resolves to the `default` entry. `[pc:vphysics]` H
* A shadow-controlled object can be switched to a reserved "shadow" material (index 0xF000
  family, no friction/elasticity changes from the game's point of view) and back; the Source
  wrapper uses this to make held objects slide. `[pc:vphysics]` H

### 5.3 Friction sounds and friction effects (server glue)

* Friction (scrape) callbacks from the engine are rate-limited through a 4-slot table per
  level: a slot is reused for the same entity pair, expires after a data constant (SDK: 0.1 s
  without update) and the sound envelope is stopped on expiry. `[pc:server]` H (4 slots) / M
  (time constant)
* `PhysFrictionSound`: requires a valid entity, a speed above the material's scrape
  threshold and a valid surface; if either surface is the "player" material (index 0x58 in
  that table) nothing plays; volume is proportional to speed^2 times a constant; the rough vs
  smooth scrape sound is picked by comparing the other surface's `scrapeRoughThreshold`.
  `[pc:server]` H (structure) / M (constants)
* `PhysFrictionEffect`: dust puffs (effect "Dust") for concrete and dirt game materials
  (`gamematerial` 'C' and 'D') above a speed threshold; sparks for metal above another
  threshold (data constants). `[pc:server]` M
* The Rust spec already measured: scrape loops start at about 66 u/s slip and fade below about
  40 u/s with a 0.025 per tick envelope from 0.25. `[meas]` H

## 6. Drag and buoyancy

### 6.1 Air drag (per body, drag controller; air density 2.0)

Drag bases are recomputed whenever mass, volume or the collide change (moving bodies only):

* Let e = (ex, ey, ez) be the collide's AABB extents in metres and A = per-axis "drag axis
  areas" from the .phy (defaults 1,1,1). Linear basis_i = (area of the face perpendicular to
  axis i, e_j * e_k) * A_i * invMass. `[pc:vphysics]` H (shape) with constants confirmed by the
  measured fits in `docs/spec/physics.md`.
* Angular basis_i = (1/16) / I_i * (A_k * T(e_i, e_j, e_k) + A_j * T(e_i, e_k, e_j)) with
  T(l, w, h) = w^2 l^3 / 3 + w^4 l / 2 + l w^2 h^2 (half-extent factors folded in). `[meas]` H,
  structure `[pc:vphysics]` H.
* Per PSI the drag controller computes, in body space with the velocity direction u = v/|v|:
  k = (|u_x| * basis_x + |u_y| * basis_y + |u_z| * basis_z) * linearDragCoefficient and applies
  dv = -v * k * |v| * airDensity * dt (quadratic drag, orientation dependent); angular:
  dw_i = -w_i * |w_i| * angbasis_i * angularDragCoefficient * (deg2rad factor) * dt.
  `[pc:vphysics]` H (shape), air density entry M (it is stored in the controller; the exact
  multiplication point was not followed).
* `GetDragInDirection(dir)` / `GetAngularDragInDirection(dir)` return the same k for a given
  world direction (dir transformed to body space, abs components dotted with the bases times the
  coefficient). `[pc:vphysics]` H
* Default coefficients are 1.0 / 1.0 (from the object params); a held (shadow) object has drag
  removed while held. `[sdk]` + `[meas]` M

### 6.2 Buoyancy / fluid controllers

* A fluid controller is created per water volume with the game's fluid params (surface plane,
  current velocity, contents, damping, density). It owns a buoyancy attacher that applies to
  every body touching the water volume. `[pc:vphysics]` H (structure)
* Per body the attacher asks for the body's buoyancy ratio and uses
  density_factor = buoyancyRatio * fluidDensityFactor only if the body has no shadow controller
  and its "gravity enabled"-class flag is set; otherwise the factor is 0 (held or
  shadow-driven bodies get no buoyancy). `[pc:vphysics]` H
* Buoyancy ratio = mass / (materialDensity * volume_in_m3) where volume_in_m3 =
  volume_u3 * 0.0254^3 and materialDensity is the object's surfaceprop `density`; a volume of
  0 forces ratio 1.0; volume is clamped to a minimum constant. `[pc:vphysics]` H (shape) / M
  (minimum constant). `PhysObj:SetBuoyancyRatio` just overwrites the ratio. `[pc:vphysics]` H
* Buoyancy solver: the body's hull is approximated by balls; for each ball the submerged
  volume, its centroid and projected area are computed against the plane; the resulting impulse
  is buoyancyForce = density_factor * g * submergedVolume at the submerged centroid (torque
  included), plus viscous damping proportional to the medium damping * sqrt(submerged area)
  on the linear velocity, plus angular damping, both scaled by dt; a 3-ball (sphere) special
  case subtracts a torque term proportional to friction * damping / r^2. Finally the
  controller's remembered fluid velocity is relaxed towards the body's current velocity
  with factor (1 - viscosity * dt) and a position-restoring term. Constants of the template
  (medium damping, viscosity, torque factor ...) live in data. `[pc:vphysics]` M
* Fluid enter/leave triggers the game's `ObjectEnterTrigger` / `ObjectLeaveTrigger` events
  (splash sounds/effects). `[pc:server]` H (names)

## 7. Shadow controller (physgun hold, carry, motion-controlled pickup)

A shadow controller drives one body towards a target transform every PSI.

* Creation: `CreateShadowController(obj, allowTranslation, allowRotation)`. Defaults:
  teleport distance 0, max speed 1.0 and max angular 1.0 (per axis; the game always sets
  them), flags from the two booleans, not enabled until the first Update. `[pc:vphysics]` H
* `MaxSpeed(maxSpeed, maxAngular)` stores both limits (u/s and deg/s converted on use).
  `SetTeleportDistance(d)` stores d * 0.0254. `SetPhysicallyControlled(b)` and
  `UseShadowMaterial(b)` toggle flags; the latter swaps the body's material to the reserved
  shadow material and restores it when switched off. `[pc:vphysics]` H
* `Update(targetPos, targetAngles, timeOffset)`: the arrival time is clamped to a minimum
  epsilon; the target position and quaternion are stored; the controller is enabled. If the
  position change squared is below a tolerance AND the quaternion component change sum is
  below another tolerance the body is not woken; otherwise it is woken. `[pc:vphysics]` H
  (tolerances in data, M)
* Each PSI (if enabled):
  1. fraction = min(dt / secondsToArrive, 1); scale = fraction / dt.
  2. delta = targetPos - currentPos (mass-centre corrected). If teleport distance > 0 and
     |delta| > teleportDistance the body is beamed to the target (position and orientation set
     directly, velocity kept). `[sdk-era vphysics source; structure confirmed by the wrapper in
     the dump]` M
  3. Linear: a = delta * scale - v * dampFactor; each component of a is clamped to
     [-maxSpeed, +maxSpeed]; v += a; the clamped a is remembered as "last impulse" (used by the
     game's `GetLastImpulse` for break checks). `[same]` M. With `physgun_dampingFactor` 0.8 and
     `physgun_timeToArrive` 0.05 this is exactly the fitted v' = 0.2 v + 20 * delta. `[meas]` H
  4. Angular: delta_rot = axis * angle of (target * inverse(current)) in radians; same
     controller formula with maxAngular (deg2rad converted) and the same dampFactor. `[same]` M
  5. If the body is allowed to move physically (flag) and the gravity-per-PSI the body
     accumulates is below the stored vertical impulse, the controller scans the friction
     snapshot: for a contact whose normal's up component is below a threshold (a steep
     surface) it adds a vertical correction so the held body does not sink into the ground.
     `[pc:vphysics]` M
  6. secondsToArrive -= dt (clamped at 0). `[pc:vphysics]` H
* Game-side physgun convars (GMod defaults): `physgun_maxSpeed 5000`, `physgun_maxAngular 5000`,
  `physgun_dampingFactor 0.8`, `physgun_timeToArrive 0.05`, `physgun_teleportDistance 0`.
  `[lua]` H + `[meas]` H. Gravity and drag are disabled while held; the held object uses the
  shadow material only for the carry (use-key) pickup, not for the physgun. `[sdk]` M
* The game's carry (`+use`) pickup uses the same controller with maxSpeed 300 u/s... in the SDK;
  the measured GMod carry turn rate is 3600 deg/s (the angular performance cap). `[meas]` M
* When a shadow-controlled object has a non-zero "last impulse" above the game's break
  threshold the game drops it (physcannon/physgun ragdoll pulls). `[sdk]` M

## 8. Player controller (the player's physics shadow)

The player's VPhysics shell is a shadow driven by a dedicated controller.

* Constructor defaults: dampFactor 1.0, push mass limit 50000 kg (effectively unlimited),
  push speed limit 10000 u/s (effectively unlimited), teleport distance = data constant *
  0.0254 (SDK-era 24 u). `[pc:vphysics]` H for 1.0 / 50000 / 10000, M for teleport distance.
* `AttachObject`: disables gravity on the shell, saves and overrides angular damping to 100,
  marks the body as "controlled by game" (it is excluded from the slow-state extra damping and
  from `IsInContact` partner checks). `[pc:vphysics]` H
* `Update(targetPos, targetVel, secondsToArrive, onGround, groundObj)`: positions converted to
  metres; the target is only re-stored if the position or velocity changed by more than a
  tiny tolerance; secondsToArrive is clamped to the epsilon; the shell is forced into
  simulation and the controller enabled. If |targetVel|^2 <= a small constant (SDK 0.1) the
  ground object is dropped and the "on ground" flag cleared; otherwise `MaxSpeed(targetVel)`
  is applied (see below) and the on-ground flag follows the argument. The ground object is
  remembered with a deletion listener and the target is stored in the ground object's local
  space so the shell rides moving platforms. `[pc:vphysics]` H (SDK 0.1 constant M)
* `MaxSpeed(targetVel)`: the component of the current velocity along the target direction that
  is already present is removed before the limit is formed, i.e. the per-axis limit is
  |targetVel - targetDir * max(0, v . targetDir)|; so the controller may only add what the
  shell does not already have. `[pc:vphysics]` H
* Per PSI: if a ground object is set, the target is re-derived from the ground object's current
  transform and the ground surface velocity is subtracted from the shell velocity. delta =
  target - current (mass-centre corrected). If the teleport flag is set or |delta| >
  teleportDistance: the game's `ShouldMoveTo`-style callback is asked (null or true means
  teleport) and the shell is beamed to the target keeping orientation (collisions toggled off
  and on around the beam if enabled), done for this PSI. Otherwise: if on ground, gravity*dt is
  removed from the velocity (the shell does not fall); scale = min(dt / secondsToArrive, 1) / dt;
  the same controller formula as section 7 with maxSpeed = |delta| per axis when no explicit
  MaxSpeed was set this tick, else the stored limits; the resulting impulse is remembered; the
  ground velocity is added back. `[pc:vphysics]` H
* Then the contact loop: for each contact in the shell's friction snapshot the normal's up
  component classifies ground (below one threshold) and "object we can push" (above a stricter
  threshold with a movable partner whose mass is under the push mass limit). For pushes the
  controller's impulse along the normal is limited by the push speed limit; the accumulated
  vertical impulse is corrected so that a grounded shell is not pushed downwards
  (velocity.up += |impulse| - impulse.up when the accumulated vertical impulse is positive).
  secondsToArrive -= dt. `[pc:vphysics]` M (thresholds in data)
* `IsInContact()`: true if any contact partner is non-static, movable and not game-controlled.
  `StepUp(h)`: beams the shell vertically by h * 0.0254. `GetShadowVelocity()`: shell velocity
  minus ground surface velocity, back to u/s. `Jump()`: no-op. `TryTeleportObject()`: as above.
  `[pc:vphysics]` H
* Game-side: the server moves the shell to the player's origin each tick with secondsToArrive
  = tick interval and `onGround` from the move; `phys_pushscale`-style props interaction is
  done through the impulse this controller records (SDK `CPlayer::VPhysicsShadowUpdate`).
  `[sdk]` M

## 9. Motion controllers and springs

* A motion controller holds a list of bodies and a game callback. Each PSI, for each attached
  body that is movable, the callback returns one of: 0 nothing; 1 local acceleration; 2 local
  force; 3 global acceleration; 4 global force, plus a linear and an angular vector.
  Accelerations are in u/s^2 and deg/s^2 and are multiplied by dt and added to the velocity
  (local vectors are rotated to world first); forces are multiplied by dt and applied as
  impulses divided by mass / inertia. The angular vector is always interpreted in body space.
  After each body the velocity limit is applied. `[pc:vphysics]` H. GMod Lua: `PhysObj`
  motion is driven through `ENT:PhysicsSimulate` returning `SIM_LOCAL_ACCELERATION` (1),
  `SIM_LOCAL_FORCE` (2), `SIM_GLOBAL_ACCELERATION` (3), `SIM_GLOBAL_FORCE` (4). `[lua]` H
* Controllers have a priority (the wrapper exposes `SetPriority`); the shadow and player
  controllers run after gravity. `[pc:vphysics]` H (names) / M (order)
* Springs (`phys_spring`, `constraint.Elastic`): an actuator between two anchor points. Per PSI:
  len = |p2 - p1|; if len is below a tiny epsilon nothing happens. If a break length is set and
  len exceeds it the spring fires its break listeners instead of pulling. If "stretch only"
  is set and len < restLength no force. Otherwise F_along = (len - rest) * constant -
  (relativeVelocity . dir) * damping, plus an extra isotropic term -relativeVelocity *
  relDamping; impulse = F * dt applied at both anchors with opposite signs (bodies that are
  pinned or not simulated are skipped). `[pc:vphysics]` H. `SetSpringDamping` /
  `SetSpringConstant` / `SetSpringLength` map onto the actuator values; game units: constant in
  (kg u/s^2 per u) converted with 0.0254 for the length and for the force. `[pc:vphysics]` H /
  `[sdk]` M for the conversions.

## 10. Collision callbacks, collision rules and groups

### 10.1 What the engine hands to the game

* Pre-collision event: the wrapper computes the approach speed and either suppresses the
  collision (collision solver's pair test, see 10.2) or records it. Post-collision: the game's
  `VPhysicsCollision` is called with delta time since the pair's last impact, approach speed,
  both surface props, pre and post linear/angular velocities, hit normal and position.
  `[pc:vphysics]` H (events) / `[pc:server]` H (fields)
* GMod's Lua `ENT:PhysicsCollide` / `PhysObj` collision callbacks receive a table with:
  `DeltaTime`, `Speed`, `OurSurfaceProps`, `TheirSurfaceProps`, `OurOldVelocity`,
  `TheirOldVelocity`, `OurNewVelocity`, `TheirNewVelocity`, `OurOldAngularVelocity`,
  `TheirOldAngularVelocity`, `OurNewAngularVelocity`, `TheirNewAngularVelocity`,
  `HitEntity`, `HitObject`, `PhysObject`, `HitNormal`, `HitPos`, `HitSpeed`. `[pc:server]` H
* The entity's generic `VPhysicsCollision` picks, by surface properties and collision group,
  the impact sound (`impacthard` / `impactsoft` by the surface's `impactHardThreshold`), a
  screen shake for heavy impacts and the dust effect. `[pc:server]` M (thresholds not read)
* Friction callbacks: `VPhysicsFriction(obj, energy, surfaceProps, otherSurfaceProps)` is
  forwarded to `PhysFrictionSound` (5.3). `[pc:server]` H

### 10.2 Collision rules

* The engine asks the game's collision solver `ShouldCollide(obj0, obj1, gameData0, gameData1)`
  whenever a pair is first considered or `RecheckCollisionFilter` runs; the result is cached
  per pair. The low-level filter stack is a meta filter: an exclusive-pair filter (pairs
  disabled with `DisableCollisions`, e.g. nocollide constraints, ragdoll self pairs, welded
  pairs in GMod) and the collision-set filter (bit matrix of up to 32 indices used by
  ragdolls: index i collides with index j iff bit j of row i is set). `[pc:vphysics]` H
* `CollisionRulesChanged()` on an entity warns if called from inside a physics callback
  ("Changing collision rules within a callback is likely to cause crashes!") and re-runs the
  filter for every physics object of the entity (up to 1024). `[pc:server]` H
* Game collision groups (SDK `physics.cpp`, not in the dump; GMod exposes the same table to
  Lua as `COLLISION_GROUP_*` and lets the `ShouldCollide` hook override the answer):
  debris never collides with anything but the world and other non-debris that requests it;
  `COLLISION_GROUP_WORLD` collides only with the world; `COLLISION_GROUP_INTERACTIVE_DEBRIS`
  does not collide with players/NPCs; `COLLISION_GROUP_WEAPON` ignores players/NPCs/debris;
  `COLLISION_GROUP_PLAYER_MOVEMENT` ignores other players; held (`FVPHYSICS_PLAYER_HELD`)
  objects do not collide with their holder; `FVPHYSICS_CONSTRAINT_STATIC`,
  `FVPHYSICS_NO_SELF_COLLISIONS` (ragdolls) and `FVPHYSICS_NO_PLAYER_PICKUP` are game flags on
  the physics object (`GetGameFlags`). `[sdk]` M, `[lua]` H for the hook existence.
* The physics object game flags observed set by the ragdoll code: part of ragdoll (bit 3,
  `FVPHYSICS_PART_OF_RAGDOLL`) on every part at creation, multi-object entity (bit 4,
  `FVPHYSICS_MULTIOBJECT_ENTITY`) at activation. `[pc:server]` H

## 11. Constraints

All constraints are built on a small Havok-derived local constraint solver ("local constraint
system"). A constraint either joins a game-created constraint group or gets a private system.
Every private system is created with iteration/strength blueprint values of 1.0, 1.0, a damping
constant from data, "active" true, and a penetration/epsilon of 0.03 for weld and rope.
`[pc:vphysics]` H

Common: every constraint converts anchor positions with 0.0254 and angles with pi/180; the
constraint type id is stored (3 fixed, 6 pulley, 7 length; hinge, ballsocket, sliding and
ragdoll have their own ids); the constraint registers itself as listener for object
creation/deletion/freeze/revive so that deleting or freezing a body deactivates it. `[pc:vphysics]` H

### 11.1 Breakable wrapper (weld/rope/pulley/hinge/ballsocket/slider/ragdoll)

A constraint is breakable when `forceLimit > 0` or `torqueLimit > 0` (or the "is active"
flags demand it). In that case the native constraint is wrapped:

* linear limit = forceLimit * 0.0254 if forceLimit > 0 else "infinite" (a data constant);
* torque limit = torqueLimit * pi/180 if torqueLimit > 0 else infinite;
* body mass scales = the two given values if > 0 else 1.0;
* the wrapper measures the impulse the constraint had to apply during the PSI and, if the
  linear impulse per second or the torque impulse exceeds the limits, deactivates the
  constraint and notifies the game (`ConstraintBroken` -> the entity's `OnBreak` output;
  sandbox removes the constraint entity). `[pc:vphysics]` H (conversions) / M (test details)
* Game units: the Lua `forcelimit` is in "kg * u/s^2" and the binary multiplies by 0.0254, so
  the internal test is against forcelimit * 0.0254 N (SI). `[pc:vphysics]` H

### 11.2 Weld (`phys_constraint`, `constraint.Weld`)

* Blueprint: the attached body's transform relative to the reference body (full 3x4, stored
  as a quaternion-free matrix), strength = the given "strength" param (1.0 = rigid). Solved as a
  6 DOF fixed constraint inside the local system every PSI. `[pc:vphysics]` H
* The game's weld is solved as a position-level correction one PSI after the violation: a
  measured landing on a welded pair shows the lower crate stopping first and the upper one
  being corrected on the next tick (the "one-tick lag" in `docs/spec/physics.md`), with a
  residual rest deformation of about 1.8 u on a two-crate stack. `[meas]` H
* GMod's `constraint.Weld` also disables collisions between the two entities when `nocollide`
  is set (exclusive pair filter). `[lua]` H

### 11.3 Rope / length (`phys_lengthconstraint`, `constraint.Rope`)

* Blueprint: a stiff spring (distance) constraint with two body-space anchors (converted to
  metres), min length = minLength * 0.0254, max length = totalLength * 0.0254, strength 0.5
  stored next to the given strength, and the usual breakable wrapper. If minLength == maxLength
  the rope is rigid (a rod). `[pc:vphysics]` H
* Measured: the rope only acts when the distance would exceed the length by the end of the
  step (it "catches" at 79.59 u for an 80 u rope), and it never pushes when slack. `[meas]` H

### 11.4 Hinge (`phys_hinge`, `constraint.Axis`)

* World axis -> per-body axis frames built from the two anchor positions and the axis
  direction (perpendicular basis constructed), positions in metres; optional angular limits
  (min/max in degrees -> radians) and an angular motor: `SetAngularMotor(speed deg/s, maxTorque)`
  converts speed with pi/180 and torque with the force conversion. Hinge friction
  (`hingefriction`) is applied as a motor with zero target speed and torque = friction. Breakable
  wrapper as above. `[pc:vphysics]` H (axes / conversions), M (motor torque units)

### 11.5 Ball socket (`phys_ballsocket`, `constraint.Ballsocket`)

* Two body-space anchors in metres; private local system unless grouped; breakable wrapper
  with force and torque limits. `[pc:vphysics]` H

### 11.6 Slider (`phys_slideconstraint`, `constraint.Slider`)

* The attached body's frame relative to the reference body plus a slide axis; optional
  linear limits (min/max in metres), a friction value, and an optional linear motor
  (`SetLinearMotor(speed u/s, maxForce)`); breakable wrapper. `[pc:vphysics]` H

### 11.7 Pulley (`phys_pulleyconstraint`, `constraint.Pulley`)

* Two world pulley positions and two body anchors (metres), total length (metres), gear ratio,
  "is rigid" flag, strength 1.0 default; breakable wrapper. The pulley enforces
  |p1 - w1| + gearRatio * |p2 - w2| <= totalLength (rigid: equal). `[pc:vphysics]` H (fields) /
  M (equation from field names)

### 11.8 Ragdoll joints (`phys_ragdollconstraint`, .phy `ragdollconstraint`)

* A limited ball socket: per axis min/max angles in degrees -> radians, with the axis order
  remapped to the internal frame (Source axis 2 is mapped with a sign flip and an extra
  conversion constant); per-axis friction (the "torque" value) acts as dry friction on the
  joint; `onlyAngularLimits` builds the joint without the positional part;
  `UpdateRagdollTransforms` lets the game move the joint frames. `[pc:vphysics]` H (shape) / M
  (axis constants)
* Ragdolls use a constraint group: the group wraps one local system for all joints; its error
  parameters are `minErrorTicks` (int) and `errorTolerance` * 0.0254 (SDK defaults 2 ticks and
  3 u); `IsInErrorState()` reports when the solver could not satisfy the joints for that many
  ticks and the game then teleports bones back (`RagdollSolveSeparation`); `SolvePenetration`
  pushes interpenetrating parts apart; `Activate()` starts solving. `[pc:vphysics]` H /
  `[sdk]` for the defaults.
* Server-side ragdoll creation (`RagdollCreate`): creates one physics object per .phy solid
  (max 24 in the SDK; the collision set supports 32), sums the masses (floored by a constant),
  clamps any initial force component above a huge constant to 10000, applies the initial force
  and angular velocity to the "force bone" and distributes force to the other bones by mass
  fraction (force * m_i / totalMass); sets the "part of ragdoll" game flag on every part.
  `RagdollActivate` parses the .phy `collisionrules` block (default: every pair of parts
  i < j has collisions disabled, then `collisionpair` entries re-enable, `selfcollisions 0`
  disables all), sets per-part game index, "multi-object entity" flag, enables collisions,
  wakes parts (optionally), activates the group. `[pc:server]` H (structure) / M (constants)
* `animatedfriction` block in the .phy (`animfrictionmin`, `animfrictionmax`, `animfrictiontimein`,
  `animfrictiontimeout`, `animfrictiontimehold`) is parsed into the ragdoll and drives the joint
  friction ramp after spawn. `[pc:server]` H (keys) and `[sdk]` for the ramp.
* The ragdoll constraint entity (`CRagdollConstraint`) creates a ragdoll joint from the map
  key values (xmin/xmax/ymin/ymax/zmin/zmax, xfriction/yfriction/zfriction). `[pc:server]` H (names)

## 12. Gravity changes per entity

* `sv_gravity` changes propagate to the environment immediately (section 1). `[pc:server]` H
* `trigger_gravity` sets the touching entity's gravity scale (`SetGravity`) when it differs;
  `target_changegravity` changes a player's gravity and resets it. For physics objects the
  binary scales... (not in dump: the SDK applies gravity scale to players and NPC movement only;
  VPhysics objects ignore `m_flGravity`; per-object gravity is only on/off via
  `EnableGravity`). `[pc:server]` H for the trigger, `[sdk]` M for the scope.
* `UTIL_ScaleForGravity` exists (scales a value by sv_gravity / 600 for projectile tosses).
  `[pc:server]` H (name)
* Lua `physenv.SetGravity`, `physenv.GetGravity`, `physenv.SetPerformanceSettings`,
  `physenv.GetPerformanceSettings` are the exposed environment controls; `PhysObj:EnableGravity`,
  `EnableDrag`, `SetDamping`, `SetDragCoefficient`, `SetAngleDragCoefficient`, `SetBuoyancyRatio`,
  `GetDamping`, `GetSpeedDamping`, `GetRotDamping`, `IsGravityEnabled`, `IsDragEnabled`,
  `GetFrictionSnapshot` are all present as redirected bindings in the dump. `[pc:server]` H

## 13. Differences from Source SDK 2013 worth noting

1. Material combination is product/product/sum/sum (friction clamped to 1.0). The SDK game code
   never documents this; it is the IVP material manager behaviour and GMod keeps it. `[pc:vphysics]` H
2. Friction factor has a vehicle wheel override path before the product rule. `[pc:vphysics]` H
3. `physenv.SetPerformanceSettings` and the per-object drag/damping/buoyancy setters are
   reachable from Lua in GMod (not in the SDK). `[pc:server]` H
4. Collision callbacks in GMod build the Lua table described in 10.1 (`GMOD_VPhysicsCollision`)
   in addition to the normal entity callback. `[pc:server]` H
5. The player controller's `Jump()` is a no-op (SDK declares it; GMod never uses it). `[pc:vphysics]` H
6. Nothing in the dump indicates changed numeric defaults vs the SDK for the controller
   constants (dampFactor 1.0, push limits 50000 / 10000, friction mass 10 / 2500). `[pc:vphysics]` H

## 14. What could not be derived

* Numeric values that the compiler put in the data segment: sleep thresholds, extra
  settle damping, contact keeper distance (measured 0.01 m), collision tolerance base,
  shadow update tolerances, player teleport distance (SDK-era 24 u), friction lower clamp,
  friction sound timing, dust/spark speed thresholds.
* The shadow controller's core math function is not in the dump (it is a static helper); its
  behaviour is given from the SDK-era vphysics source and matches the wrapper code and the
  measured physgun fit.
* The exact per-PSI breakable test (impulse integration window) and the hinge motor torque
  units were not followed into the Havok-derived solver.
