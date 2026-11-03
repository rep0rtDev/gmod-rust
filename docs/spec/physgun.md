# Physics gun

Status: implemented (`gm_physics::physgun`, `World::hold` / `release` / `freeze`), 5 of 7 `physgun_*` harnesses within tolerance (pitch steps, yaw steps, off centre grab, freeze, rotate at 1.97 u; held bodies are excluded from the impact listener, the shadow controller owns them), `walk_hold` at 3.0 u while the player accelerates and settles exact, `hold_crate` matches while held and diverges only after the release when the crate lands (contact response, see physics.md). Traces recorded on 2026.09.17 (7 scenarios, bot with `weapon_physgun` driven through `StartCommand`, prop transform and `IsPlayerHolding` / `IsMotionEnabled` recorded every tick in `FinishMove`).

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
- Rotate mode (`IN_USE` held with the gun): mouse x turns the wanted orientation about world up by 0.05 degrees per mouse unit (20 units per tick gave 1 degree per tick, 29.9 degrees after 30 ticks), mouse y about the view's right axis by the same; the body follows through the angular controller (angular velocity ramps 20, 34, 61, 67, 67 deg/s) and its origin swings around the grab point since the offset lives in body space. The mouse y axis is only close (2 degrees of rest angle, 2.04 u), so the exact axis (view right, or something between right and the body's own axis) is still open.
- Freeze (`IN_ATTACK2` while holding): the prop stops dead where it is, `IsPlayerHolding` false and `IsMotionEnabled` false from the next tick, no settling at all. gm-physics switches the Jolt body to kinematic with zero velocity (0.52 u max error, the pre freeze hold error).
- Walking while holding (forward 200 u/s): the prop follows the moving eye with the controller's lag, 5.6 u behind the static hold distance at 200 u/s, and catches up after the player stops; ours lags 3 u more during the acceleration ticks and matches at rest (0.5 u).

## Walking while holding (measured `server_physgun_walk_hold`, crate held 126.4 u ahead, +forward 200 u/s)

The hold target is taken from the player's eye *after* this tick's move (the game's weapon
post-frame runs after player movement): at a steady 200 u/s walk the game's held crate trails
the eye by 121.4 u against 126.4 at rest - a lag of 5.0 u, 1.65 ticks of the walk - and on the
first moving tick it has covered 8.84 of the eye's 12.4 u. Targeting from the pre-move eye put
the crate one tick further behind (118.4 u, 8.0 u lag); with the post-move eye the whole walk
is within 0.49 u of the game. `Server::hold_targets` runs between the player moves and the
physics step.

## Open

`server_physgun_hold_crate`: the crate released with yaw 30 lands flat at 233 u/s (tick 289); the game then slides it 7.5 u (-y / +x) and turns it to yaw 21.5 with a 2-3 deg pitch/roll wobble over 20 ticks, ours lands dead flat and stays (7.9 u, 8.5 deg) - the flat-landing asymmetry family of sandbox.md (sequential vertex chain), not a hold defect.

More grab geometry (rotated and off axis props, grabbing a moving prop), the E rotate mode, freeze, the hold on ragdolls and multi solid models, and the player moving while holding (the eye moves). A shadow controller on the Jolt body: each tick the target is the aim ray point at the grab distance plus the grab offset rotated by the view delta, velocity and angular velocity are set to reach it within `timeToArrive`, clamped to `maxSpeed` / `maxAngular`, damped by `dampingFactor`, gravity off while held. The `physgun_*` harnesses compare the held prop's position and angles per tick; tolerances per harness in `tools/parity/harnesses.toml` once the implementation exists.

## Ragdoll bones (measured `physgun_ragdoll_head_grab` / `physgun_ragdoll_foot_grab`, kleiner with the pelvis frozen)

The physgun's ray hits the ragdoll's physics hulls, not its hitboxes: the probe's `util.TraceLine` with `MASK_SHOT` reports `PhysicsBone` (head 10, foot 14) and `HitBox` 0 for every bone, and a pitch sweep from the bot's eye maps the hulls (yaw 90.7: Spine2 -24.5..-20.5 deg, Pelvis -20..-19, R_Thigh -18.5..-10.5, R_Calf -10..-3, R_Foot -2.5..0.5; the head at pitch -24 is only yaw 82.75..85.5 wide at 137 u). The bot picks up the hit bone on the attack tick (tick 21 in both recordings); `Entity:IsPlayerHolding()` stays false for ragdolls, so the hold shows only in the bone's motion. A held bone that does not have to move (view steady) stays within 0.7 u of its rest; when the view swings 30 deg the held bone is dragged into the chain hanging from the frozen pelvis: the game's head moves 1 u in the first two ticks, 4.6 u after five and 18 u after ten, settling 20 u from its start; ours reaches 4.5 u / 23 u (overshoot) and settles 9.8 u from the game (the foot: 2.1 u) - the Jolt-held body keeps more of the controller velocity against the joint chain than IVP's shadow does (the game's head speeds up by a steady 30 u/s per tick - 41, 60, 90, 117, 147, 179, 211, 242, 267 u/s - until the chain's reach stops it, its foot 212, 333, 384, 368 u/s; ours starts at 146, 351, 571, 731 and 650, 619, 477 and then stalls). Open residual together with the lift on a jointed light body; the tick-0 pose of a frozen-pelvis ragdoll after the probe's 60 settle ticks differs by 5 u at the head (the ragdoll rest-pose residual, docs/status.md).

