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

## Undo and cleanup

`gmod_undo` removes the last constraint and its count drops; removing a prop removes the constraints attached to it. `gmod_cleanup` removes constraints with everything else and leaves the undo entries behind (dead entries are skipped by the next undo).

## Not yet measured

The other stools (ballsocket, slider, ...), the hull-trace fallback, ragdoll bones, ropes with a width (keyframe_rope), the limits when hit.
