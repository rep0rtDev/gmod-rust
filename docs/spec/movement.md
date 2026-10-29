# Player movement

Realm: shared (server authoritative, client predicted). Crate: `gm-move`, collision through `gm-collision`.
Harness group: `move_*`: 7 harnesses recorded on a dedicated server bot (version 2026.09.17), all bit-exact against gm-move (max position error 0.0000 over 110 to 160 ticks).

## Constants (measured, see convars-defaults.md)

| Name | Value | Source of the number |
|---|---|---|
| tick | 0.015 s | `engine.TickInterval()` on a stock server |
| gravity | 600 u/s^2 | `sv_gravity` |
| friction | 8 | `sv_friction` |
| stop speed | 10 u/s | `sv_stopspeed` |
| ground acceleration | 10 | `sv_accelerate` |
| air acceleration | 10, wish speed capped at 30 u/s | `sv_airaccelerate`, engine constant |
| max velocity | 3500 u/s per axis | `sv_maxvelocity` |
| step size | 18 u | `sv_stepsize` |
| walk / run speed | 200 / 400 u/s | sandbox player defaults |
| crouched speed | 0.3 of walk | sandbox `CrouchedWalkSpeed` |
| jump impulse | 200 u/s | sandbox `JumpPower` |
| standing hull | 32 x 32 x 72, eye 64 | engine hull |
| ducked hull | 32 x 32 x 36, eye 28 | engine hull |
| duck transition | 0.4 s down, instant up when there is room | engine |
| ground normal | z >= 0.7 | engine |
| trace stop distance | 1/32 u | engine `DIST_EPSILON` |

## Tick order (`Mover::tick`)

1. Duck state machine.
2. Categorize: sweep the hull 2 u down; ground if a hit with normal z >= 0.7 and vertical speed <= 180 u/s. On ground the origin snaps to the hit.
3. Jump: on ground, jump pressed and not held since last tick: vertical speed = 200, leave ground. Holding jump does not re-jump (no auto bhop in stock sandbox, bhop is still possible by re-pressing each landing).
4. On ground: vertical speed = 0; friction; accelerate towards wish; step move.
   In the air: half gravity; air accelerate (wish capped at 30); slide move; half gravity.
5. Categorize again; on ground vertical speed = 0.
6. Clamp each axis to +-3500.

## Formulas

- Wish: `forward * fmove + right * smove` with forward/right flattened to the horizontal plane, each input clamped to the current max speed, length clamped to max speed.
- Friction: `speed < 0.1` does nothing; `control = max(speed, stop_speed)`; `drop = control * friction * dt`; `v *= max(speed - drop, 0) / speed`.
- Accelerate: `add = wish_speed - dot(v, wish_dir)`; if `add > 0`: `v += wish_dir * min(accel * dt * wish_speed, add)`.
- Measured: friction runs before acceleration, so a walking player reaches exactly 200 u/s (30 u/s added on the first tick, 24 u/s of friction removed per tick at 200). Releasing the keys decays 200 -> 176 -> 154.88 -> ... with friction alone.
- Air acceleration: the 30 u/s cap limits how much can be added, the rate is `air_accelerate * dt * wish_speed * surface_friction` with the uncapped wish speed. Measured: `surface_friction` is 0.25 while the vertical speed measured before the second half of gravity is positive (rising, and the first tick past the apex), 1 otherwise and on the ground. A rising player gains 7.5 u/s per tick sideways, a falling one 30.
- Jump: vertical speed is set to 200 and the jump itself applies half a tick of gravity, so with the air move's two halves the jump tick ends at `200 - 1.5 * 600 * 0.015 = 186.5`, then minus 9 per tick. Measured and exact.
- Forward jump boost (the HL2 single player rule is active in GMod multiplayer): on the first tick after a jump, `share = 0.1 if ducked else 0.5`; `addition = |forward_move| * share`, clipped so horizontal speed does not exceed `max_speed * (1 + share)`; added along the flattened view direction with the sign of `forward_move`. Walking 200 becomes 300, running 400 becomes 600. The sprint button does not lower the share.
- Ground state: the 2 u downward test is skipped while vertical speed exceeds 140 u/s in either direction; a falling player is grounded only once the move itself touches the floor. CategorizePosition never moves the origin. StayOnGround (up 2 u, then down a step) runs after a walk move and snaps the origin to the floor when the difference is over 1/64 u; a player standing still 1 u above the floor stays there.
- The crouch speed limit uses the duck state from the start of the tick, so the tick in which the duck completes still moves at the standing limit.
- Clip velocity on a plane: `v - n * dot(v, n)`, components below 0.001 zeroed. Slide move tries up to 4 planes; two planes slide along their crease; a velocity turning against the original stops.
- Step move: run the slide flat, run it again from 18 u higher and drop back 18 u; keep whichever went further horizontally, provided the dropped position rests on a ground normal.

## Ducking

- Press: the press tick starts the clock at 0, every following tick adds 0.015; the duck completes when the clock exceeds 0.4 s, which is the 27th tick after the press (0.405 s), or immediately when airborne. In the air the hull shrinks from the top, so the origin rises by half the height difference (18 u), which is what makes crouch jumps clear higher ledges.
- Release: if there is room for the standing hull (in the air the origin first drops by 18 u), an unduck clock starts; standing completes when it exceeds 0.1 s (`GetUnDuckSpeed` = 0.1 in sandbox), the 7th tick after release. Without room, stay ducked and retry every tick. Releasing before the duck completed stands immediately.

## Noclip

Sandbox allows noclip by default: velocity = normalized (forward * fmove + right * smove + up * umove) scaled to 400 u/s, 1000 with the sprint button, no collision, never on ground.

## Not implemented yet

Ladders, water (levels, swim, `sv_waterdist`), moving platforms and props as ground, fall damage, view roll, surface friction modifiers, `sv_maxspeed` style limits set by Lua, prediction error correction.

## Probe

A server bot (`bot` console command) is driven through `StartCommand` (buttons, moves, view angles, so press events and speed changes are seen by the engine) and `SetupMove`; `FinishMove` records origin, velocity, ground, duck and max speed per tick. Scenarios in `tools/parity/scenarios/move/`, golden traces in `tools/parity/golden/260917/`.

Measured since: `move_fall_200` (the 140 u/s rule holds when landing from 200 u), `move_crouch_jump` (ducking in the air raises the origin by the full 36 u, standing lowers it by 36 u), `move_stairs_up` (real 8 u stairs on gm_construct, bit-exact), `move_airstrafe` partially.

`move_air_left`, `move_air_right`, `move_air_alt` (alternating strafes through a whole jump) and `move_airstrafe` pin the air friction rule; `move_stairs_down` recorded.

Open: slopes, ladders, water, moving ground.
