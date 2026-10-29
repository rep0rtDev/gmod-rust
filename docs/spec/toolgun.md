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

## Undo and cleanup

`gmod_undo` removes the last constraint and its count drops; removing a prop removes the constraints attached to it. `gmod_cleanup` removes constraints with everything else and leaves the undo entries behind (dead entries are skipped by the next undo).

## Not yet measured

Right click of the weld tool (weld to the world under the hit), the other stools (axis, ballsocket, nocollide, slider, ...), the hull-trace fallback, ragdoll bones, ropes with a width (keyframe_rope), the limits when hit.
