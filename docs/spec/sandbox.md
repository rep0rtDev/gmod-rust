# Sandbox server core

Measured on the real server with the `sandbox` probe scenario (a bot runs `gm_spawn`, `gmod_undo`, `gmod_cleanup` and `sbox_maxprops` through the real gamemode; `tools/parity/golden/260917/sandbox_spawn_basic.json`). Re-implemented in `gm-sandbox`, checked by `sandbox_spawn_basic` (16 ops, placement within 0.001 u).

## Spawning a prop (`gm_spawn <model>`)

- Ray from the eye (`GetShootPos`, origin + 64 standing) along the view forward, 2048 u, `MASK_SOLID`, player filtered out.
- Angles: pitch 0, roll 0, yaw = player yaw + 180 (the prop faces the player).
- Position: the entity is set at the hit point, then moved so the point of its model box (`OBBMins`/`OBBMaxs`, the studio hull) nearest to a point 512 u behind the surface sits on the hit point. On a floor a crate (hull z from -20.227) ends 20.227 u above the floor; on a wall with normal -x it ends 20.308 u in front of it and keeps the hit height, then falls.
- Then `TryFixPropPosition`: for each of the entity's local axes a trace from its origin to the box extent on each side; if only one side hits the entity is pushed out by the penetration, if both hit nothing happens. On the open floor the downward trace ends exactly on the surface and does not count as a hit, so the position stays.
- Measured samples (eye 823.235, -500, -79): pitch 30 yaw 0 crate at 935.764, -500, -123.742 yaw 180; pitch 45 yaw 90 oil drum (hull z from -0.281) at 823.235, -435.031, -143.688 yaw 270; pitch 20 yaw 180 pop can (hull z from -3.7) at 644.735, -500, -140.281 yaw 0; pitch -10 against the wall at x = 1023.969 crate at 1003.660 (hull x 20.308).
- The player gets `achievements.SpawnedProp()`; an undo entry `#prop_physics (<model>)` is pushed and the entity added to the player's `props` cleanup list.

## Spawning onto props

The spawn ray hits props on their collision hulls (PHY), like the tool gun, and the engine stops it 1/32 u in front of the surface (the same `DIST_EPSILON` as the brush trace): a crate on top of a crate whose top is at -103.985 is traced to -103.985 + 1/32 along the ray. The flush offset then uses the hit normal of the hull face: a prop resting 0.1 degrees off level tilts the normal by about 0.002, and the point 512 u behind the surface moves by about 1 u, so a pop can spawned against a slightly tilted crate lands 0.5 u sideways and 1 u lower than the hit (`sandbox_spawn_on_prop`, 4 spawns onto a crate, a drum and the floor, equal within 0.001 u). Because the props under physics drift while the scenario runs, the runner takes their poses before each op from the recording (`before.placed`, `before.props` in the golden trace) as inputs; the placement itself is ours.

## Limits

`Player:CheckLimit("props")`: refused once `count > sbox_maxprops - 1` where `count` is the number of the player's live props; negative limit means unlimited. A refusal sends `LimitHit("props")` to the player (deduplicated per tick) and nothing is spawned. `sbox_maxprops` is read live, so lowering it does not remove props; undoing one frees a slot.

## Undo (`gmod_undo`)

Per-player stack with increasing ids (the next id is the last id + 1, so ids are reused after an undo). Undo pops the last entry and removes its entities; an entry whose entities are already gone (after a cleanup) counts as nothing undone and the next entry is popped in the same command until one removes something. Cleanup does not touch the undo stack: after `gmod_cleanup` the entries stay (with dead entities) and a later undo walks through them.

## Cleanup (`gmod_cleanup`)

Without a type removes everything on the player's cleanup lists and sends `hook.Run('OnCleanup','all')`. Counts update the next frame in the game; we recount live props directly.

## Not yet measured

Spawning onto other props (the spawn ray only hits the world in our trace), ragdolls and effects, `gmod_undonum`, `gmod_admin_cleanup`, cleanup of one type, `sbox_maxragdolls` and the other limits, prop spawn effects and sounds.
