# Physics gun

Status: measurements only, no implementation yet. Harness group: `physgun_*` (1 trace recorded on 2026.09.17: `physgun_hold_crate`, a bot holding `weapon_physgun` driven through `StartCommand` with `IN_ATTACK` and view angles, the prop's transform and `IsPlayerHolding` recorded every tick in `FinishMove`).

Convars read back from the server: `physgun_maxSpeed` 5000, `physgun_maxAngular` 5000, `physgun_dampingFactor` 0.8, `physgun_timeToArrive` 0.05, `physgun_teleportDistance` 0.

## Measured on the real game

Crate (`wood_crate001a`, mass 30) resting at (823.2, -480, -123.6), bot eye at (823.2, -600, -79.6) looking down 20 degrees along +Y.

- Pickup: `IN_ATTACK` pressed on tick 20 (recorded in that tick's `FinishMove` as not held), the prop reports held from tick 21. Grabbing a resting prop does not move it: it stays at its position and angles while the aim is unchanged (velocity 0.1 u/s up for a few ticks, then 0). The grab keeps the hit point at the same distance along the aim ray and the same offset in the prop.
- Lift (aim pitch 20 -> -10 in one tick, the target point jumps about 60 u): the prop moves 16.5 u in the first tick (1096 u/s), then its speed decays 1096, 660 (2 ticks later), 226, 9.5 at tick 10, settles at the new point by tick 20 with no overshoot. Peak speed is well under `physgun_maxSpeed` 5000; the profile is a critically damped approach with `timeToArrive` 0.05 s (3.3 ticks) and damping 0.8, as a Source shadow controller does.
- Swing (aim yaw 90 -> 120 in one tick): the prop is carried around the player on the aim ray (first tick 1038 u/s, same decay), and its yaw turns with the player, 0 -> 17.1 -> 27.8 -> 29.9 -> 30.0 degrees over the same ticks (the prop keeps its orientation relative to the view).
- Release (`IN_ATTACK` up at tick 260): held is false from tick 262 and the prop falls from rest (velocity -18 u/s at tick 262, -44.8 at 265), i.e. gravity resumes one tick after the release registers; it then lands with the usual contact response.

## Target

A shadow controller on the Jolt body: each tick the target is the aim ray point at the grab distance plus the grab offset rotated by the view delta, velocity and angular velocity are set to reach it within `timeToArrive`, clamped to `maxSpeed` / `maxAngular`, damped by `dampingFactor`, gravity off while held. The `physgun_*` harnesses compare the held prop's position and angles per tick; tolerances per harness in `tools/parity/harnesses.toml` once the implementation exists.
