# Rigid body physics (gm-physics)

Status: measurements only, no implementation yet. Harness group: `phys_*` (4 traces recorded on 2026.09.17: `phys_drop_crate`, `phys_drop_tilted`, `phys_slide_barrel`, `phys_toss_can`).

Oracle: `prop_physics` spawned by the probe on a dedicated server, `PhysObj` transform, velocity and angular velocity recorded every server tick, plus mass, inertia, mass centre, material, volume, surface area and damping from the physics object.

## Measured on the real game

- Free fall is exactly `sv_gravity` 600 u/s^2 per tick: velocity -9, -18, -27 u/s after ticks 1, 2, 3; the position integrates with the new velocity (semi-implicit Euler: -0.135, -0.405, -0.81 u after ticks 1, 2, 3). Same integration as the player.
- `models/props_junk/wood_crate001a.mdl`: mass 30, material `wood_crate`, one convex, inertia about (3.59, 3.61, 3.62), mass centre offset about (-0.04, 0.006, -0.13), volume 62977, surface area 9497, linear and angular damping 0 reported by the object. Collision AABB about 40 x 40 x 40 (-20.03..19.94).
- A crate dropped flat from 84 u above the floor hits on tick 36 at -300 u/s and rebounds with +46 u/s (effective restitution about 0.15) while picking up a small spin from the off centre mass; it settles 0.4 u above the ideal resting height (-123.6 for a floor at -144 and a 20 u half extent: VPhysics keeps a collision margin) and goes to sleep after about 120 ticks.

## Target

Jolt (through `rolt` / `joltc-sys`, Jolt 5.0) with the world built from BSP brushes and props from their PHY convex hulls, surface properties from `scripts/surfaceproperties*.txt` (friction, elasticity, density), tuned until the `phys_*` traces match within tolerance. Bit exactness is not a goal here (different solver); the bar is the same contact tick, the same resting pose within 0.5 u, the same rebound within 10 percent, the same sleep behaviour.
