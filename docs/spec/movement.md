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
- Forward jump boost (the HL2 single player rule is active in GMod multiplayer): on the first tick after a jump, `share = 0.1 if ducked else 0.5`; `addition = |forward_move| * share` where `forward_move` is the command move already scaled down to `max_speed` (the move vector clamp; forward 10000 counts as 200 for a walker, so a standing walker who jumps with forward held gets +100, measured in `move_step_land`), clipped so horizontal speed does not exceed `max_speed * (1 + share)`; added along the flattened view direction with the sign of `forward_move`. Walking 200 becomes 300, running 400 becomes 600. The sprint button does not lower the share.
- Ground state: the 2 u downward test is skipped while vertical speed exceeds 140 u/s in either direction; a falling player is grounded only once the move itself touches the floor. CategorizePosition never moves the origin. StayOnGround (up 2 u, then down a step) runs after a walk move and snaps the origin to the floor when the difference is over 1/64 u; a player standing still 1 u above the floor stays there.
- The crouch speed limit uses the duck state from the start of the tick, so the tick in which the duck completes still moves at the standing limit.
- Clip velocity on a plane: `v - n * dot(v, n)`, components below 0.001 zeroed. Slide move tries up to 4 planes; two planes slide along their crease; a velocity turning against the original stops.
- Step move: run the slide flat, run it again from 18 u higher and drop back 18 u; keep whichever went further horizontally, provided the dropped position rests on a ground normal.

## Ducking

- Press: the press tick starts the clock at 0, every following tick adds 0.015; the duck completes when the clock exceeds 0.4 s, which is the 27th tick after the press (0.405 s), or immediately when airborne. In the air the hull shrinks from the top, so the origin rises by half the height difference (18 u), which is what makes crouch jumps clear higher ledges.
- Release: if there is room for the standing hull (in the air the origin first drops by 18 u), an unduck clock starts; standing completes when it exceeds 0.1 s (`GetUnDuckSpeed` = 0.1 in sandbox), the 7th tick after release. Without room, stay ducked and retry every tick. Releasing before the duck completed stands immediately.

## Noclip

Sandbox allows noclip by default: velocity = normalized (forward * fmove + right * smove + up * umove) scaled to 400 u/s, 1000 with the sprint button, no collision, never on ground.

## Water

Measured on gm_construct's pond (x -3040..1824, y 2016..4000, surface at z -162, floor at -400 to -176; `move_water_walk`, `move_water_swim`, `lua_drown` in `tools/parity/golden/261031/`, the two movement traces bit-exact: 440 and 660 ticks within 0.0002 u).

- **Levels.** Three point probes of the BSP water contents: feet at origin + 1 (level 1), waist at half the hull height (level 2: 36 standing, 18 ducked), eyes at the view height (level 3). Measured: level 1 at origin z -161.6, level 2 wading the shallow floor at -198.25. Within one tick a probe is looked up again only once it has moved more than 1 u from where it was last looked up; the start-of-tick lookup is reused by the post-move check when the tick moved less (this is what decides the gravity below: ticks 249/381/422 of `move_water_swim` moved over a unit and got no second gravity half, 432/442 moved 0.88 and 0.95 u and did).
- **Level 1.** Ordinary walking, the floor's friction and acceleration; footsteps use the `wade` surface property (`slosh*.wav`) at the normal volume.
- **Level 2 and 3.** No gravity. Wish = forward * fmove + right * smove, plus: jump held adds max speed upward; no input at all adds -60 (sinking); otherwise `up_move` plus `clamp(fmove * forward.z * 2, 0, max speed)` (pitching down 60 degrees and pushing forward sinks at up to -139 u/s, pitching up holds the surface). Wish speed is capped at max speed then scaled by 0.8: 160 u/s forward from the 200 walk speed, -48 u/s sinking. Friction is the ground friction applied to the whole velocity (`speed - dt * speed * friction * surface friction`, the horizontal speed decays by itself), acceleration the ground rule against the scaled wish: 24, 45.1, 63.7, 80.1, 94.5, 107.1 ... 160 u/s. Jump held while swimming sets vertical velocity to 100 u/s and leaves the ground; at the surface the player bobs between level 1 and 2. Falling into the water (air move ending at level 2 or more with a move over 1 u) skips the second gravity half of that tick. On the bottom (ground under the hull) vertical velocity is zeroed after the move. No footsteps at level 2 or more, even walking the bottom.
- **Sounds.** `Player.Swim` (volume 0.5) on going from dry to any level in a tick, and on each tick that starts at level 2 or more with jump held while a 1000 ms timer has run out (it restarts the timer: `move_water_swim` ticks 160, 250, 328; the held jump around 227 came bobbing with the waist out). Not reproduced: `Player.Wade` (volume 0.25) one tick after wading in from dry ground (walk tick 118) and one tick after surfacing from level 3 to 2 (swim tick 209), but not after going from 1 to 2 on the bottom or 2 to 1 in the air; its rule is unknown.
- **Drowning.** None: 60 s at level 3 (`lua_drown`, 4000 ticks) gave no `EntityTakeDamage`, no `PlayerHurt`, hp stayed 100. Not implemented, on purpose.
- Not measured: `sv_waterdist`, water jump out onto a ledge, currents, slime/lava damage.

## Ladders

Measured on gm_construct's `func_useableladder` (the line x -2920, y -1076 from z -104.64 to -7.36 in front of a wall at y -1058, lower floor at -96; the top ends in a 48 u crawlspace, so the top dismount could not be measured). Seven traces in `tools/parity/golden/261031/` (`move_ladder_up/down/jump/pitch/back/angles/thresh`), six bit-exact, `move_ladder_jump` off by one tick at the landing (below).

- **Mount.** A forward press (forward move or `IN_FORWARD`) with the ladder line within 24 u of the origin horizontally (hull 16 + 8), in front of the view and within the ladder's height, grabs it: on the press tick nothing moves and the velocity is zeroed; the following ticks slide the origin 3 u (200 u/s) straight at the line (the game's MOVETYPE_NONE, still grounded, velocity 0); on the line the movetype is LADDER and the climb starts next tick. `move_ladder_back`: press at tick 20 from 24 u, on the line at 28, climbing from 29.
- **Climb.** Only the buttons move: the wish is the view forward for `IN_FORWARD`, minus forward for `IN_BACK`, plus/minus the view right for the strafes (forward move without buttons does nothing). Up at 200 u/s along the ladder unless the unit wish points down steeper than 0.3 (forward at pitch 17 climbs, 18 descends; back at pitch 0 or 25 climbs, at -45 descends; strafes, being level, always climb). The hull sweep along the ladder stops dead on the floor or the ceiling (velocity 0) and the player stays on the ladder at either end; no gravity, never grounded.
- **Jump.** `IN_JUMP` leaves the ladder: velocity = view forward x 200 + 50 up, then the ordinary air tick (gravity, collision). `move_ladder_jump` from the top under the ceiling: the hop is clipped at once, the forward part stops at the wall.
- **Sounds.** `ladder{1..4}.wav` (the `ladder` surface property) at volume 1 on the mount tick, then on the tick after a climbing tick whenever 450 ms have passed since the last climbing step, idle time included (`move_ladder_thresh`: 20, 30, 60, 90).
- **Open.** Landing after the jump, tick 108: the game is still falling 1.13 u above the floor (ground false, velocity -256.5), we ground there with the 2 u probe. The game's own hull trace at that spot (y -1074.03, the hull a float off the wall) reports start-solid even with MASK_SOLID while every point around is empty; something near the top of the wall (solid above z -70 at y -1060 in a slab test) that our world does not have or that the game's sweep ignores. Also unmeasured: the top dismount (`info_ladder_dismount`), `sv_ladder_useonly`, mounting from above.

## Props as ground (measured, `move_prop_*`)

A frozen prop is a solid for the player hull exactly like a brush: the PHY convexes at the prop's pose, planes pushed out by the hull extents, the 1/32 u skin (`move_prop_block`: walking into a frozen crate stops 0.03 u short of its face, the forward speed clipped to 0, bit-exact; `move_prop_step`: a PHX plate on the floor is stepped onto (4 u up) and off, the ground entity is the prop, bit-exact). Footsteps on a prop use the prop's surface property (the plastic plate plays `tile*.wav`); ours still take the map material under the feet. The bump into the frozen crate also makes the crate's impact sound (a physics event, not reproduced).

A frozen prop teleported with `SetPos` every tick does not carry the player: he stays put and loses the ground once the plate has slid out from under him (`move_prop_setpos`). The tick that loses the ground ends with the second gravity half (-4.5) although it started grounded: `FullWalkMove` finishes with gravity when the move left the ground; we do that too. Open: the game drops him one tick earlier than our sweep does (its ground probe misses the plate while 0.6 u of the hull is still over it, tick 82), and he lands at -143.26 instead of the floor's -143.97.

An unfrozen floating prop (gravity off, velocity set every tick) with the player standing on it is a vphysics shadow interaction: the plate sinks and tilts under him, he is carried at about half its speed and slides off (`move_prop_ride`, recorded, not modelled, no harness).

## Not implemented yet

Moving platforms that carry the player (brush entities, the velocity of a physics ground) and props as ground, fall damage, view roll, surface friction modifiers, `sv_maxspeed` style limits set by Lua, prediction error correction, `Player.Wade`.

## Probe

A server bot (`bot` console command) is driven through `StartCommand` (buttons, moves, view angles, so press events and speed changes are seen by the engine) and `SetupMove`; `FinishMove` records origin, velocity, ground, duck and max speed per tick. Scenarios in `tools/parity/scenarios/move/`, golden traces in `tools/parity/golden/260917/`.

Measured since: `move_fall_200` (the 140 u/s rule holds when landing from 200 u), `move_crouch_jump` (ducking in the air raises the origin by the full 36 u, standing lowers it by 36 u), `move_stairs_up` (real 8 u stairs on gm_construct, bit-exact), `move_airstrafe` partially.

`move_air_left`, `move_air_right`, `move_air_alt` (alternating strafes through a whole jump) and `move_airstrafe` pin the air friction rule; `move_stairs_down` recorded.

Open: slopes, carrying ground.

## Displacements

The trace stops on displacement surfaces, not on the brush under them (the brush below gm_construct's ground at 600, -500 tops out at -152 and carries contents 0x100; the displacement holds the player at -147.969). `gm_collision` sweeps the player box against displacement triangles after the brush pass, one-sided, with the same 1/32 epsilon. Measured: `move_walk_disp` (walk +x from 600, -500 over the displacement and up the 4 u step onto the brush at 823) is bit-exact against the server, 160 ticks.

Each triangle is swept as a thin bevelled convex: its face plane, a back plane 4 u behind it (so the space under a displacement stays open while a hull that has slipped a unit behind the extended face plane of a neighbour, resting on an edge bevel, still finds the floor: `move_water_walk` tick 61), the axis planes of its bounds and the edge-by-axis bevels. A point ray skips the bevels (they are the face plane again to float precision on flat ground); a box keeps even the nearly parallel ones: on the pond shore a 1 degree bevel keeps the hull off the extended plane of the next triangle, a 0.93 u step the game does not take (`move_water_walk` tick 48).

The surface under the feet for footsteps is the triangle the hull's ground sweep clipped against, not the one under the origin (ticks 68 and 116: the hull rests on a higher edge). A blend material's triangle whose three vertex alphas sum past 382.5 is the second material (`$surfaceprop2`; `grass-sand` gives `grass3, grass2, sand3, sand4` like the game).

## Open: standing on a displacement edge (`move_settle_edge`)

A bot placed at (600, -500, -143) on gm_construct and left alone: the game falls to z -147.32 and stands there (not grounded yet at tick 5, z -146.307, vz -63), ours grounds at tick 5 at -146.307 (the 2 u ground probe hits our floor at -147.969) and stays 1.01 u higher. The only solid brush under the 32 x 32 footprint is the slab with its top at z -152; the surface the player stands on is a displacement: a pad at -147.969 (rays at (600, -500) and (620, -500)) dropping to the slab (rays at (580, -500), (600, -480), (600, -520) hit -151.969), so the box straddles the pad's edge. The game's box-against-displacement sweep neither sees the pad within 2 u from -146.307 nor rests on it at -147.969 but stops at -147.32, which looks like the box's bottom edge resting on the edge bevels of the pad's side triangles; ours treats the pad top as the contact. When a prop or player falls fast onto the same spot both stop at -147.969 (`damage_fall_*`). Resolved. The displacement mesh and the map were innocent (the surface there is flat at -148; two zero-thickness planes of an axis-aligned triangle did reject point rays and are skipped now, see above). Three more settle recordings (`move_settle_disp2` on a displacement where the start is already on the ground, `move_settle_disp3` on the same displacement, `move_settle_brush` over the brush floor at (823, -657)) show the same gap over brushes, so it is the ground probe of `categorize`, not displacement collision: the game settles 0.65-0.68 u above the floor one tick after we did, both from 1.66 u at 63 u/s. The probe reach: 0.97 u above the floor is grounded (the scenario starts), 1.66 u at 63 u/s is not, 1.32 u at 191.5 u/s (jump landings) is not, 0.3 u at 1089 u/s is (`damage_fall_progressive`), so the probe is between 1.0 and 1.32 u, not 2 u, and only rising faster than 140 u/s rules the ground out (the old `|vz|` guard was compensating for the 2 u probe on fast landings). `MoveConfig::ground_probe` = 1.2 (every value in 1.1-1.3 reproduces all recordings); movement 20/20, `damage_fall_progressive` and `damage_fall_threshold` equal with no other change.

## Footsteps (measured, `move_step_*`)

Recorded on srcds with an `EntityEmitSound` hook on gm_construct's concrete spawn floor (`move_step_walk/run/slow/duck/land`):

- The server emits the wave directly, not the script: `player/footsteps/concrete{1..4}.wav`, soundlevel 75, pitch 100, channel 4 (body). Left and right alternate: `Concrete.StepLeft` is `{1,3}`, `Concrete.StepRight` is `{2,4}`, picked at random inside the side.
- Volume: 0.2 walking, 0.5 running; ducked multiplies by 0.65 (0.13 / 0.325). The walk/run split is the SDK one: standing `velrun = 220` (200 u/s is walking, 310 is running), ducked `velrun = 80` (60 u/s ducked walks, 176 ducked runs).
- A step needs the horizontal speed entering the tick to be at least `velwalk` (standing: between 85.4 and 90.2, i.e. the SDK's 90): the slow walk (forward 100) steps first on the tick after speed reaches 90.2.
- Interval, in ticks between steps: walking 24, running 17, duck-walking 27. This is GMod's `PlayerStepSoundTime` (max speed <= 100: 400 ms, <= 300: 350 ms, else 250 ms; +50 ms crouching; `GetMaxSpeed` stays 200 while ducked) counted down 15 ms per tick: 350 -> 24, 250 -> 17, 400 -> 27.
- Jumping plays a step at volume 1.0 on the jump tick; landing itself plays nothing (the next ordinary step follows the timer).

Landing (`move_step_fall_50/100/200/300/500/900`, drops onto the spawn floor; fall speeds 243, 342, 486, 594, 774, 1035 u/s): a fall of 526.5 u/s or more (the one that hurts, see fall damage) lands with a step at volume 1.0 and `Player.FallDamage` (script volume 0.75, pitch 90-110 from the script, soundlevel 75) on the same tick, in that order; more than half of that (342, 486) lands with a step at 0.85 and nothing else; 243 (a standing jump) makes no landing sound. No pain sound is emitted by the server on top.

Implementation: `gm_move::Footsteps` holds the timer and the foot and is run before each player's move with the state entering the tick (`before_move`) and on a jump (`on_jump`). `gm-server` resolves the wave set: a 4 u ray down from the origin finds the floor brush side (`gm_collision::World::ground_material`; an idle player on the spawn stands 1 u above the floor), its material's `$surfaceprop` (through `patch` includes, `default` when missing) gives `stepleft`/`stepright` through the surfaceproperties base chain, and the soundscript name goes to clients as a world sound with the measured volume as its factor (the step scripts have volume 1.0). The five `move_step_*` harnesses compare tick, volume and wave set; which foot a recording starts on is whatever the bot's pre-roll left, so the compare may swap feet once for the whole list ("feet swapped").
