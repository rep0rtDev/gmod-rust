# Tool gun

Measured on the real server with the `toolgun` probe scenario (a bot holding `gmod_tool` clicks the real `weld` and `rope` stools through the weapon's own trace; `tools/parity/golden/260917/toolgun_weld_rope.json`). Re-implemented in `gm-sandbox`, checked by `toolgun_weld_rope` (20 ops).

## Trace (`SWEP:DoToolTrace`)

A line from the eye along the view, `MAX_TRACE_LENGTH` (56756 u), mask solid, moveable, monster, window, debris, grate, aux, the player filtered out; if the line hits nothing with an entity a zero-size hull trace is tried. Props are hit on their collision hulls (PHY), not the studio box: the crate's -x face is at 20.058 u from its origin (hit at 879.944 for a crate at 900.005). Our trace casts against the world (brushes and displacements) and the props' PHY convexes, nearest wins; max hit error 0.04 u (the game's crates drift by a few hundredths while settling).

## Objects and stages

Each left click stores the hit as an object (`SetObject`): entity, physics bone, position and normal converted into the entity frame (world space when the world was hit). The first object sets stage 1; the second finishes the tool and clears the objects (stage 0). Reload clears the objects and returns false (no tool effect).

## Weld

On the second click `CheckLimit("constraints")` (default `sbox_maxconstraints` 2000), then `constraint.Weld(Ent1, Ent2, Bone1, Bone2, forcelimit, nocollide)`:
- the same physics object twice: nothing is created (the click still returns true and clears the objects);
- a weld already between the two objects: nothing;
- if the first object is the world the ends are swapped (props are welded to the world, never the world to a prop);
- otherwise a `phys_constraint` (`Type = "Weld"`, `forcelimit`, `nocollide` from the client convars; a bot reads 0 and false), an undo entry `Weld` with custom text `Undone #tool.weld.name`, `AddCount("constraints")`, `AddCleanup("constraints")`.

## Rope

On the second click `CheckLimit("ropeconstraints")` (default `sbox_maxropeconstraints` 1000), then `constraint.Rope` with `LPos1`/`LPos2` the stored local hit points, `length` the distance between their world positions at that moment (44.306 u between the two crates, 49.216 u from a crate to the floor), `addlength` (clamped to +-56756), `forcelimit`, `width`, `material`, `rigid` from the client convars. A `phys_lengthconstraint` sits at the first world point; at width 0 (a bot's default) no `keyframe_rope` is created. Undo entry `Rope` (`Undone #tool.rope.name`), `AddCount("ropeconstraints")`, cleanup list `ropeconstraints`. A rope between the same physics object twice creates no constraint.

## Weld, right click

Three clicks (`toolgun_nocollide_axis_weldworld`): the first must hit a prop (the world is refused) and sets stage 1; the second can hit anything: the prop is turned so that the grabbed face points against the second surface (`Phys1:AlignAngles(Norm1:Angle(), (-Norm2):Angle())`, i.e. the current rotation turned by the rotation taking the first normal's angles to the flipped second normal's; `Vector:Angle()` keeps yaw 0 for a straight up or down vector), moved so that the grabbed point sits on the second hit point, and frozen (stage 2). Measured: a crate grabbed on its -x face at 11.95 u above its centre and dropped onto the floor at 855.719, -518.755 ends at 843.78, -518.75, -123.91 with angles -90, 0, 180 (face down), frozen; ours equal within 0.001 u. The third click anywhere welds the prop to the second object (the world here), unfreezes it, counts and undoes like the left-click weld.

## NoCollide

Two left clicks on props (the world is refused as the first object): `CheckLimit("constraints")`, `constraint.NoCollide`: nothing for the same object or an already no-collided pair, otherwise a `logic_collision_pair` (`Type = "NoCollide"`), undo entry `NoCollide` (`Undone #tool.nocollide.name`), `AddCount("constraints")`, cleanup list `nocollide`.

## Axis

Two left clicks (`toolgun_axis`, frozen crates so nothing drifts): the first must hit a prop (the world is refused, stage stays 0); on the second the first prop is turned like the weld's right click (`AlignAngles` of the hit normals) and moved so that its hit point sits 0.2 u in front of the second hit along the second normal (`TargetPos = WPos2 + (centre - grabbed) + Norm2 * 0.2`); the hinge axis is `LPos1 = Phys1:WorldToLocal(WPos2 + Norm2)` in the moved prop, `LPos2` the stored local hit of the second object (world position for the world). Measured: crate grabbed on its -x face (hit 879.942, -500, -111.74) onto the second crate's -x face at 879.942, -455.696, -105.192 ends at 859.683, -455.696, -117.152 facing yaw 180 with `LPos1` -19.258, 0, 11.96 and `LPos2` -20.058, -15.696, 18.508; onto the floor at 855.719, -518.755 it ends at 836.042, -513.528, -123.857 with angles 0, -90, -90 and `LPos1` 5.227, 19.112, 19.677. Ours equal within 0.001 u and 0.04 degrees. `phys_hinge`, undo `Axis` (`Undone #tool.axis.name`), `AddCount("constraints")`, cleanup `constraints`; forcelimit, torquelimit, hingefriction, nocollide from the client convars (a bot: 0).

## Ballsocket

Two left clicks (`toolgun_ballsocket`, 7 ops); the world may be either object (clicking the same entity twice makes nothing but still returns true). `constraint.Ballsocket(Ent1, Ent2, 0, 0, LPos2, forcelimit, torquelimit, nocollide)`: a `phys_ballsocket` at the second hit point in the second object's frame (world position for the world; with the world as the first object the point is still in the prop's frame: `-20.058, 0, 11.96` for the crate hit on its -x face), `forcelimit` and `nocollide` (spawnflag 1) from the client convars. Undo `BallSocket` (`#tool.ballsocket.name`, one entity), `AddCount("constraints")`, cleanup `constraints`. Reload on an entity removes every ballsocket touching it (`constraint.RemoveConstraints(ent, "Ballsocket")`, true; false on the world). Ours: `ConstraintKind::Ballsocket`, a Jolt `PointConstraint` at the world point, `set_pair_collision` off when nocollide; forcelimit and torquelimit are stored, not simulated (no breaking). Equal on all 7 ops.

Physics (`constraint_ballsocket_*`): a crate hanging from a frozen crate by a ballsocket at its own top point stays put (`_nocollide`, 0.02 u); a crate hung 30 u off to the side swings as a pendulum (`_hang`): the first swing matches (within 2 u to tick 105) but the game's pendulum loses energy faster than ours (7 u apart by tick 115, 13 degrees at rest), open.

## Slider

Two left clicks (`toolgun_slider`, 7 ops): `constraint.Slider(Ent1, Ent2, 0, 0, LPos1, LPos2, width, material)`: a `phys_slideconstraint` between the two local hit points (each in its own object's frame) plus a `keyframe_rope` of `width` between them when `width > 0` (the rope is the undo's second entity; a bot's convars read 0 and "", so a click without settings makes no rope and the undo has one entity). With the world as the first object the game spawns a `gmod_anchor` at the hit point and attaches to it (`LPos1 = 0, 0, 0`); ours keeps the world and reports the anchor in its place. Undo `Slider`, `AddCount("ropeconstraints")`, cleanup `ropeconstraints`, reload removes the sliders touching the hit entity. Ours: `ConstraintKind::Slider`, a Jolt `SliderConstraint` along the axis from the first point to the second (orientation locked, no friction). Equal on all 7 ops.

Physics (`constraint_slider_*`): a crate on a vertical slider under a frozen crate falls at g and lands (0.25 u over 300 ticks); on an axis tilted 43 degrees it slides down at `g * cos` (17.94, -19.20 u/s per 4 ticks along 20, -21.4) and stops on the floor (0.54 u). Both within tolerance.

## Elastic

Two left clicks (`toolgun_elastic`, 8 ops): `constraint.Elastic(Ent1, Ent2, 0, 0, LPos1, LPos2, constant, damping, rdamping, material, width, stretchonly)`: a `phys_spring` between the two hit points with `length` = their distance at creation (the world's point in world space), a `keyframe_rope` of `width` when `width > 0`; `constant`, `damping`, `rdamping`, `width`, `material`, `stretch_only` from the client convars. Undo `Elastic` (two entities with the rope), `AddCount("ropeconstraints")`, reload removes the elastics touching the hit entity. Ours: `ConstraintKind::Elastic`, integrated by `gm-physics` as an explicit per tick impulse `F = -constant * (len - length) - damping * rate` along the line (a Jolt soft constraint at one solver step per tick damps about three times more than the game); `rdamping` is stored only. Equal on all 8 ops.

Physics (`constraint_elastic_*`): a crate under a frozen crate on a 3000 / 10 spring (`_stiff`, stretchonly 0) oscillates about 6 u of stretch with a 41.9 tick period and loses 7% of its amplitude per half period; ours within 0.03 u over 500 ticks. A 500 / 3 spring (`_hang`) is too weak for a 30 kg crate: it reaches the floor at tick 13 like ours, but the game's crate then slides 4.5 u sideways and yaws 5.4 degrees while ours stays put (rest 4.5 u, open).

## Hydraulic, winch, muscle (driven springs)

All three are `constraint.Elastic` (`phys_spring`, the type overwritten) with a `gmod_winch_controller` entity that moves the spring's rest length (`SetSpringLength`) every tick, plus a `keyframe_rope` when `width > 0`. Spring constants are not settings but come from the lighter end's mass (`CalcElasticConsts`; the world does not count): `k = m * 100`, `c = k * 0.2` for a fixed hydraulic / muscle, `k = m * 50`, `c = k * 0.1` otherwise and always for a winch (3000 / 600 and 1500 / 150 for a 30 kg crate). `fixed` adds a `phys_slideconstraint` between the same points with no type of its own (it is not a Slider for `RemoveConstraints`), removed with its owner (`DeleteOnRemove`). The controller's `Think` re-arms every 0.01 s, so it runs every tick with `TimeDiff` = one tick; a stopped one (`direction 0`) does nothing, otherwise `length += direction * speed * dt`, clamped to `[min, max]`; a toggle controller stops at the limit it reached (`isexpanded` at `max`). The key edge sets the direction on that tick and the length starts moving on the next (`constraint_hydraulic_toggle`: key at tick 20, length 22.36 at tick 21, 64 u/s, stops at `length2` after 21 more ticks). Undo entries: constraint, rope (width > 0), controller and the slider of a fixed one (4 for a fixed hydraulic / muscle, 3 for a winch).

- Hydraulic (`toolgun_hydraulic`, 10 ops; settings `width 3, addlength 100, fixed 1, speed 64, toggle 1, material cable/rope, group 37`): `Length1` = distance at creation, `Length2 = Length1 + addlength`, controller `min = Length1`, `max = Length2`. Toggle: `HydraulicToggle` on key down: stopped -> extend (or retract when expanded), moving -> reverse. Not toggle: down sets +1, up sets -1 and the controller keeps retracting at `min` (`direction -1` stays, as in `constraint_hydraulic_free`). Reload on an end removes the hydraulic with its slider and controller. Equal.
- Winch (`toolgun_winch`, 8 ops; `rope_width 3, fwd_speed 64, bwd_speed 64, fwd_group 44, bwd_group 41, rope_material cable/rope`): stretch-only spring, controller `min 1`, no max, `fwd` lengthens at `fwd_speed`, `bwd` shortens at `bwd_speed`; not toggle: down sets the direction, up stops; toggle: the same key again stops, the other reverses. The crate under a winch wound in to 1 u hangs 6 u below the attachment (the spring stretch under its weight, `constraint_winch_pull`). Equal.
- Muscle (`toolgun_muscle`, 8 ops; `width 2, addlength 100, fixed 1, period 1, starton 0, material cable/rope, group 37`): the stool hands the whole `addlength` to the controller as `amplitude` (not half of it); `length = min + clamp((sin(ctime * pi * period) + 1) * amplitude / 2, 0, amplitude)`, `ctime` starts where the sine gives the start length (`asin((len - min) / (2 amp) - 1) / (period pi)` = -0.5 s at `min`) and advances one tick per think after the length is computed (60 ticks after the key: 141.56 for 44.79 .. 144.79). `MuscleToggle` on key down runs / stops; `starton` runs at creation. Equal.

Physics (`constraint_hydraulic_toggle` / `_free`, `constraint_winch_pull`, `constraint_muscle_on`): the driven springs reuse the explicit spring of `gm-physics` with its rest length set from the controller each tick; all four traces within 1.4 u over 300 ticks (the controller lengths bit for bit), with the slider of a fixed one as a Jolt `SliderConstraint`.

## Pulley

Four left clicks (`toolgun_pulley`, 10 ops): entity, two points anywhere (world or entity hit position, stages 1..3), entity; a world hit as the first or last click is refused. `constraint.Pulley(Ent1, Ent4, 0, 0, LPos1, LPos4, WPos2, WPos3, forcelimit, rigid, width, material)`: a `phys_pulleyconstraint` with `Ent1` / `Ent4` (no `Ent2`), `position2 = WPos3`, `ObjOffset1/2` the local points, `addlength = |WPos3 - WPos4|`, `spawnflags 2` when rigid, and three `keyframe_rope`s (`Ent1` -> `WPos2`, `WPos2` -> `WPos3`, `WPos3` -> `Ent4`) when `width > 0`; undo entry of 4, counted under `ropeconstraints`. Settings `width 3, forcelimit 0, rigid 0, material cable/cable`. Equal. Physics (`constraint_pulley_hang` / `_rigid`): a 30 kg crate and a 30 kg oil drum hanging from the two pulley points stay in balance in the game (0.15 u of settling); ours solves the pulley as one velocity constraint per tick (`|p1 - w2| + |p4 - w3| <= total`, pulls only unless rigid, effective masses from Jolt) and stays within 0.15 u.

## Undo and cleanup

`gmod_undo` removes the last constraint and its count drops; removing a prop removes the constraints attached to it. `gmod_cleanup` removes constraints with everything else and leaves the undo entries behind (dead entries are skipped by the next undo).

## Not yet measured

The other stools (thruster, hoverball, balloon, wheel, button, dynamite, lamp, trails, motor, ...), the hull-trace fallback, ragdoll bones, the keyframe_rope's own simulation, force / torque limits breaking a constraint (the pulley's `forcelimit` is stored only), an unbalanced pulley (both measured pairs weigh the same).
