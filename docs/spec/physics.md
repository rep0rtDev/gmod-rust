# Rigid body physics (gm-physics)

Status: first Jolt implementation (`gm-physics`), 1 of 4 `phys_*` harnesses within tolerance (`phys_drop_crate`; `phys_drop_tilted`, `phys_slide_barrel`, `phys_toss_can` diverge after contact). Traces recorded on 2026.09.17.

Oracle: `prop_physics` spawned by the probe on a dedicated server, `PhysObj` transform, velocity and angular velocity recorded every server tick, plus mass, inertia, mass centre, material, volume, surface area and damping from the physics object.

## Measured on the real game

- Free fall is exactly `sv_gravity` 600 u/s^2 per tick: velocity -9, -18, -27 u/s after ticks 1, 2, 3; the position integrates with the new velocity (semi-implicit Euler: -0.135, -0.405, -0.81 u after ticks 1, 2, 3). Same integration as the player.
- `models/props_junk/wood_crate001a.mdl`: mass 30, material `wood_crate`, one convex, inertia about (3.59, 3.61, 3.62), mass centre offset about (-0.04, 0.006, -0.13), volume 62977, surface area 9497, linear and angular damping 0 reported by the object. Collision AABB about 40 x 40 x 40 (-20.03..19.94).
- A crate dropped flat from 84 u above the floor hits on tick 36 at -300 u/s and rebounds with +46 u/s (effective restitution about 0.15) while picking up a small spin from the off centre mass; it settles 0.4 u above the ideal resting height (-123.6 for a floor at -144 and a 20 u half extent: VPhysics keeps a collision margin) and goes to sleep after about 120 ticks.

- Air drag: while falling the crate gains 9.0, 8.99, 8.9 ... 7.8 u/s per tick, a loss that grows with the square of the speed: `a_drag = k v^2` with `k` about 9.0e-4 per unit for the crate, which is `A / m` with A the projected area of the collision box (1.03 m^2) and m its mass (30), i.e. `drag_coefficient` 1 times area over mass. gm-physics applies it per body axis before each Jolt step (implicit, `v / (1 + k |v| dt)`); after 30 ticks ours is at -261.3 against -260.4.
- Elasticity combines as the average of the two surfaces: wood (0.1, the crate's `Wood_Crate` inherits `Wood`) on concrete (0.2) gives the measured 0.15 rebound (46 u/s from 300). Jolt combines restitution as the max, so the world body carries 0 and each prop carries the average of its own and the world's elasticity.
- Resting bodies sit about 0.4 u (IVP's 1 cm tolerance) above the surface: world brush planes are pushed out by `REST_GAP` = 0.3937 u; ours rests at -123.66, the game at -123.60.
- A flat landing still picks up a lateral kick of 8.7 u/s in x and y in the game (IVP resolves the four corner contacts one after another), which drifts the crate 1.5 u before it sleeps; Jolt lands it dead flat. The rest tolerance is 2 u for that reason.
- Open: an edge landing (`drop_tilted`) tumbles further in Jolt than in the game and comes to rest 12 u away with 20 degrees of yaw difference; the rolling oil drum (`slide_barrel`) keeps 150 u/s in the game but decays to 110 in Jolt and stops 180 u short; the thrown can bounces differently. All three are contact response (friction model and rolling over the hull's edges), not free flight: free flight matches to the tick in every trace.

## Implementation notes

- Jolt 5.0 through `joltc-sys`/`rolt` 0.3.1, metres and Y up inside, Source units and Z up at the API: `jolt = (x, z, -y) * 0.0254`, the same mapping IVP used so PHY data converts one to one.
- World: every solid brush of model 0 (`SOLID`, `WINDOW`, `GRATE` contents) as a Jolt convex hull in one static compound body. Props: PHY ledges as convex hulls (compound when more than one), density from the PHY text section's `mass` and `volume`, friction and elasticity from `scripts/surfaceproperties.txt` with `base` inheritance.
- Gravity goes through the per body gravity factor (`600 * 0.0254 / 9.81`), one Jolt collision step per 0.015 s server tick.

## Target

Jolt (through `rolt` / `joltc-sys`, Jolt 5.0) with the world built from BSP brushes and props from their PHY convex hulls, surface properties from `scripts/surfaceproperties*.txt` (friction, elasticity, density), tuned until the `phys_*` traces match within tolerance. Bit exactness is not a goal here (different solver); the bar is the same contact tick, the same resting pose within 0.5 u, the same rebound within 10 percent, the same sleep behaviour.
