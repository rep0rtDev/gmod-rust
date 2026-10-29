# Player movement

Realm: shared (server authoritative, client predicted). Crate: `gm-move`, collision through `gm-collision`.
Harness group: `move_*` (not yet recorded; needs a client-side probe, see "Open" below).

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
- Equilibrium walking speed with friction 8 and accelerate 10 is below the 200 cap (friction removes `8 * v * dt`, acceleration adds `10 * 200 * dt` until `v` reaches `wish`), so a stock player does not reach exactly 200 u/s on flat ground; the harness will record the exact plateau.
- Jump apex `v^2 / 2g = 33.3 u`, airtime `2v / g = 0.667 s = 44 ticks`. Unit test: apex within 2 u, airtime within 2 ticks.
- Clip velocity on a plane: `v - n * dot(v, n)`, components below 0.001 zeroed. Slide move tries up to 4 planes; two planes slide along their crease; a velocity turning against the original stops.
- Step move: run the slide flat, run it again from 18 u higher and drop back 18 u; keep whichever went further horizontally, provided the dropped position rests on a ground normal.

## Ducking

- Press: `ducking = true`, timer 0.4 s. When the timer runs out, or immediately when airborne, `ducked = true`. In the air the hull shrinks from the top, so the origin rises by half the height difference (18 u), which is what makes crouch jumps clear higher ledges.
- Release: trace the standing hull in place (in the air the origin first drops by 18 u). If it starts solid, stay ducked and retry every tick.

## Noclip

Sandbox allows noclip by default: velocity = normalized (forward * fmove + right * smove + up * umove) scaled to 400 u/s, 1000 with the sprint button, no collision, never on ground.

## Not implemented yet

Ladders, water (levels, swim, `sv_waterdist`), moving platforms and props as ground, fall damage, view roll, surface friction modifiers, `sv_maxspeed` style limits set by Lua, prediction error correction.

## Open

Recording `move_*` traces needs a client (the usercmd stream and prediction live client-side). On the dedicated server a bot driven through `SetupMove` is the fallback: it exercises the same server movement code with scripted usercmds and records position and velocity per tick. That is the planned first probe.
