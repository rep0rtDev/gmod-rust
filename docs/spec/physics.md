# Rigid body physics (gm-physics)

Status: first Jolt implementation (`gm-physics`), 1 of 5 `phys_*` harnesses within tolerance (`phys_drop_crate`; `phys_slide_crate` is 10 u short, `phys_drop_tilted`, `phys_slide_barrel`, `phys_toss_can` diverge after contact). Traces recorded on 2026.09.17.

Oracle: `prop_physics` spawned by the probe on a dedicated server, `PhysObj` transform, velocity and angular velocity recorded every server tick, plus mass, inertia, mass centre, material, volume, surface area and damping from the physics object.

## Measured on the real game

- Free fall is exactly `sv_gravity` 600 u/s^2 per tick: velocity -9, -18, -27 u/s after ticks 1, 2, 3; the position integrates with the new velocity (semi-implicit Euler: -0.135, -0.405, -0.81 u after ticks 1, 2, 3). Same integration as the player.
- `models/props_junk/wood_crate001a.mdl`: mass 30, material `wood_crate`, one convex, inertia about (3.59, 3.61, 3.62), mass centre offset about (-0.04, 0.006, -0.13), volume 62977, surface area 9497, linear and angular damping 0 reported by the object. Collision AABB about 40 x 40 x 40 (-20.03..19.94).
- A crate dropped flat from 84 u above the floor hits on tick 36 at -300 u/s and rebounds with +46 u/s (effective restitution about 0.15) while picking up a small spin from the off centre mass; it settles 0.4 u above the ideal resting height (-123.6 for a floor at -144 and a 20 u half extent: VPhysics keeps a collision margin) and goes to sleep after about 120 ticks.

- Air drag: while falling the crate gains 9.0, 8.99, 8.9 ... 7.8 u/s per tick, a loss that grows with the square of the speed: `v -= k |v| v dt` applied to the velocity gravity gives this tick, before the position integrates. `k = A(dir) / m` with `A(dir) = sum_i A_i |dir_i|` the collision box's projected area along the velocity in body axes and `A_i = face_i * dragAxisAreas_i` (the PHY solid header's per axis ratio of hull to box area, 1 for a box, about 0.77 across a cylinder, stored in IVP axis order). Fitted `k` per tick from the traces: crate flat 8.68e-4 per unit, crate tilted (20, 35, 10) 1.24e-3, can 5.25e-4, oil drum 5.5e-4 rising as it tips; gm-physics reproduces all four to the last digit, free flight positions agree within 1e-4 u over 33 ticks.
- Elasticity combines as the average of the two surfaces: wood (0.1, the crate's `Wood_Crate` inherits `Wood`) on concrete (0.2) gives the measured 0.15 rebound (46 u/s from 300). Jolt combines restitution as the max, so the world body carries 0 and each prop carries the average of its own and the world's elasticity.
- Resting bodies sit about 0.4 u (IVP's 1 cm tolerance) above the surface: world brush planes are pushed out by `REST_GAP` = 0.3937 u; ours rests at -123.66, the game at -123.60.
- A flat landing still picks up a lateral kick of 8.7 u/s in x and y in the game (IVP resolves the four corner contacts one after another), which drifts the crate 1.5 u before it sleeps; Jolt lands it dead flat. The rest tolerance is 2 u for that reason.
- Sliding friction: a crate pushed at 300 u/s along the concrete floor loses 6.0 u/s per tick (`slide_crate`), 0.64 g plus drag, i.e. the product of the two frictions (0.8 * 0.8), not Jolt's geometric mean; each body carries its friction squared so the pair comes out as the product. Ours loses 6.3 per tick and stops 104 u out against 112.5; the game's crate also hops once on the first tick and then yaws to 29 degrees while sliding straight in Jolt.
- Contact happens in the tick the hull crosses the surface, with the body left at the contact depth (the tilted crate stops at -115.77 where its corner reaches the floor). Jolt only predicts contacts 2 cm ahead and would land every prop a tick late and 2 to 3 u deep; `SPECULATIVE_CONTACT_M` is raised to 0.15 m through a small C++ shim (JoltC 0.3.1 does not expose `PhysicsSettings`), after which the contact tick matches in all four traces. PHY hulls get a 5 mm convex radius instead of Jolt's 5 cm so corners stay sharp.
- Open: the response after contact. An edge landing (`drop_tilted`) gets its spin mostly about x in the game (-242, -186, -56 deg/s) and about y in Jolt (-87, -231, 0), and comes to rest 10 u away with 23 degrees of yaw difference; the oil drum hitting its bottom edge is kicked to -x with a -206 deg/s yaw spin in the game and to +x with +120 in Jolt (which first vertex of the 24-gon takes the hit); the thrown can bounces differently. These are the solver's contact ordering and friction model, not geometry or free flight; they are kept as failing harnesses with their numbers rather than tuned by hand.

## Constraints (weld, rope)

Harness group `constraint_*` (3 traces on 2026.09.17: `weld_pair` two crates welded side by side and dropped, `rope_hang` a crate on an 80 u rope under a frozen crate, `stack_pair` a crate dropped on a resting crate). The probe spawns several props, applies `constraint.Weld` / `constraint.Rope` (the game creates `phys_constraint` / `phys_lengthconstraint`) and records every prop per tick.

- gm-physics: JoltC 0.3.1 has no constraint bindings, so the shim creates Jolt `FixedConstraint` (weld, auto detected point) and `DistanceConstraint` (rope, world space points, min 0 or the length when rigid) on the raw `PhysicsSystem`; `World::weld`, `World::rope`, `World::remove_constraint`.
- Measured and matched: the welded pair falls as one body with the same air drag per crate (positions equal to the tick until the landing); the roped crate swings with the same period for the first 40 ticks (within 1 u) and keeps the rope taut at 80 u.
- Open: the welded pair's landing (both crates come to rest 5 u away, 3 degrees of yaw) and the stack (the game lets the resting bottom crate sink 4 u under the impact and lands the top one 2 ticks earlier) are the contact response again; the rope swing drifts after half a period (9 u, the game damps the pendulum differently). 0 of 3 within tolerance, kept red with the numbers.

## Impact response (IVP style, clean room)

The real game resolves the first touch of a pair as an *impact* at the contact points, not through the resting contact solver. Behaviour measured on the traces and confirmed by the symbol inventory of `vphysics_srv.so` (`IVP_Impact_Solver`, `IVP_Impact_System`, `IVP_Material_Manager`), implemented in the shim as a Jolt `ContactListener` (`gm-physics/src/shim.cpp`, toggle `World::set_impact_enabled`, runner `GMP_IMPACT=0` for A/B):

- Pair materials: elasticity is the product `e1 * e2`, friction the product `f1 * f2` (both clamped to [0, 1]). The separation speed after the impact is `sqrt(e1 e2) * |vn|`, which is where the measured 0.15 for wood on concrete came from (sqrt(0.1 * 0.2) = 0.141 plus the residual of the push loop).
- Impact set: every manifold point that reaches the surface within the step (a flat face lands on all four corners at once, an edge on one point). Speculative points further away than `|vn| dt` are left to Jolt.
- Push: estimate `-2 vn m_eff` from the virtual masses along the direction, applied in tenths round robin over the points until no point approaches any more (max 100 rounds), then one scaled push per point up to the wanted separation speed. Jolt's own restitution for that contact is zeroed; its solver only keeps the bodies apart afterwards.
- Friction tilt: `tan(angle) = f (1 + sqrt(e))`, push = `cos * n - sin * (tangential part of the unit relative velocity)` unless the motion is already inside that cone (then along the normal). Not renormalised.
- Static bodies get a virtual mass of 1e5 times the other body.
- Persisted contacts count as an impact only above 0.3 m/s approach speed; resting contact stays with Jolt. Repeated impact counting (IVP relaxes the elasticity towards 1 with the impact count) is left out: with Jolt resting contacts it made settling bodies bounce more every tick.

Effect: the tilted crate now lands right (rest 0.9 u and 2 degrees against 5 u and 18 degrees before, 3.5 u peak during the tumble), the flat crate gets the small sideways kick and spin the game shows (6 u/s and 9 deg/s against 8.7 and 11.4) but in a different horizontal direction, so its rest drifts 2.6 u. The barrel and the can still pick a different first vertex of their 24-gons and roll the other way; the push loop is right, the contact geometry is not yet.

## Solver settings do not close the gap (measured)

`World::set_solver` (shim) exposes Jolt's Baumgarte factor, velocity and position iterations and the restitution velocity threshold; the runner takes `GMP_SOLVER=baumgarte,vel_steps,pos_steps,min_restitution_velocity` for experiments. Sweeping Baumgarte 0.02 to 0.2, position steps 0 to 2, velocity steps 4 to 10 and the restitution threshold 0.1 to 1 m/s over all `phys_*`, `physgun_*` and `constraint_*` traces moves the errors by under 1 u (tilted crate stays at 5 to 6 u and 12 to 18 degrees, barrel at 135 to 200 u, can at 19 to 51 u, stack at 4 u). The mismatch is the contact model itself (IVP's impact and friction solver), not the amount of position correction, so the defaults stay and no tuning is baked in.

## Implementation notes

- Jolt 5.0 through `joltc-sys`/`rolt` 0.3.1, metres and Y up inside, Source units and Z up at the API: `jolt = (x, z, -y) * 0.0254`, the same mapping IVP used so PHY data converts one to one.
- World: every solid brush of model 0 (`SOLID`, `WINDOW`, `GRATE` contents) as a Jolt convex hull in one static compound body. Props: PHY ledges as convex hulls (compound when more than one), density from the PHY text section's `mass` and `volume`, friction and elasticity from `scripts/surfaceproperties.txt` with `base` inheritance.
- Gravity goes through the per body gravity factor (`600 * 0.0254 / 9.81`), one Jolt collision step per 0.015 s server tick.

## Target

Jolt (through `rolt` / `joltc-sys`, Jolt 5.0) with the world built from BSP brushes and props from their PHY convex hulls, surface properties from `scripts/surfaceproperties*.txt` (friction, elasticity, density), tuned until the `phys_*` traces match within tolerance. Bit exactness is not a goal here (different solver); the bar is the same contact tick, the same resting pose within 0.5 u, the same rebound within 10 percent, the same sleep behaviour.
