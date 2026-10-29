# Physics gun

Status: implemented (`gm_physics::physgun`, `World::hold` / `release`), 3 of 4 `physgun_*` harnesses within tolerance; `physgun_hold_crate` matches while held and diverges only after the release when the crate lands (contact response, see physics.md). Harness group: `physgun_*` (4 traces recorded on 2026.09.17: `physgun_hold_crate`, `physgun_pitch_steps`, `physgun_yaw_steps`, `physgun_offcentre`; a bot holding `weapon_physgun` driven through `StartCommand` with `IN_ATTACK` and view angles, the prop's transform and `IsPlayerHolding` recorded every tick in `FinishMove`).

Convars read back from the server: `physgun_maxSpeed` 5000, `physgun_maxAngular` 5000, `physgun_dampingFactor` 0.8, `physgun_timeToArrive` 0.05, `physgun_teleportDistance` 0.

## Measured on the real game

Crate (`wood_crate001a`, mass 30) resting at (823.2, -480, -123.6), bot eye at (823.2, -600, -79.6) looking down 20 degrees along +Y.

- Pickup: `IN_ATTACK` pressed on tick 20 (recorded in that tick's `FinishMove` as not held), the prop reports held from tick 21. Grabbing a resting prop does not move it: it stays at its position and angles while the aim is unchanged (velocity 0.1 u/s up for a few ticks, then 0). The grab keeps the hit point at the same distance along the aim ray and the same offset in the prop.
- Lift (aim pitch 20 -> -10 in one tick, the target point jumps about 60 u): the prop moves 16.5 u in the first tick (1096 u/s), then its speed decays 1096, 660 (2 ticks later), 226, 9.5 at tick 10, settles at the new point by tick 20 with no overshoot. Peak speed is well under `physgun_maxSpeed` 5000; the profile is a critically damped approach with `timeToArrive` 0.05 s (3.3 ticks) and damping 0.8, as a Source shadow controller does.
- Swing (aim yaw 90 -> 120 in one tick): the prop is carried around the player on the aim ray (first tick 1038 u/s, same decay), and its yaw turns with the player, 0 -> 17.1 -> 27.8 -> 29.9 -> 30.0 degrees over the same ticks (the prop keeps its orientation relative to the view).
- Release (`IN_ATTACK` up at tick 260): held is false from tick 262 and the prop falls from rest (velocity -18 u/s at tick 262, -44.8 at 265), i.e. gravity resumes one tick after the release registers; it then lands with the usual contact response.

## Controller (fitted, reproduced)

- Each tick while held: `v' = v * (1 - dampingFactor) + (target - origin) / timeToArrive`, clamped to `maxSpeed`; the position then integrates `v'`. The lift trace gives `1 - damping` = 0.2 and `1 / T` = 20 exactly (1095.7, 985.7, 660.2, 397.4, 225.9 u/s); the game is 7 u/s faster in the first two ticks after a target change only (0.36 u of target), left as is. Angular velocity follows the same law against the orientation error, clamped to `maxAngular`. Gravity and air drag are off while held.
- Target: the grab point (first hit of the view ray on the hull, 106.4 u for the crate's front face) rides the current view ray at the grab distance; the body origin hangs off it by the grab offset kept in *body space*, so it follows the body's actual (lagging) yaw rather than the view; the wanted orientation is the grab orientation turned by the yaw change of the view since the grab (pitch changes do not tilt the prop). Pitch steps of 10, 20, 30, 50 degrees: max error 0.53 u; yaw steps of 10, 20, 60 degrees: 0.10 u; off centre grab with pitch and yaw: 0.50 u.
- Release: gravity is back in the tick the button is up; `IsPlayerHolding` reads false two ticks later.

## Open

More grab geometry (rotated and off axis props, grabbing a moving prop), the E rotate mode, freeze, the hold on ragdolls and multi solid models, and the player moving while holding (the eye moves). A shadow controller on the Jolt body: each tick the target is the aim ray point at the grab distance plus the grab offset rotated by the view delta, velocity and angular velocity are set to reach it within `timeToArrive`, clamped to `maxSpeed` / `maxAngular`, damped by `dampingFactor`, gravity off while held. The `physgun_*` harnesses compare the held prop's position and angles per tick; tolerances per harness in `tools/parity/harnesses.toml` once the implementation exists.
