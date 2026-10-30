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

## +use pickup

Measured on srcds with the parity probe (`tools/parity/scenarios/use`, golden traces
`tools/parity/golden/261031`, 22 scenarios: a crowbar or physgun in hand, E on a prop ahead,
turn, E again). This is the public SDK's player pickup (`CPlayerPickupController` over
`CGrabController`), and the numbers match it:

- Gate: mass at most 35 and every extent at most 128 (`CanPickupObject(35, 128)`). The wood
  crate (30), oil drum, chair (10), gas can (10) and pop can (1) are picked up; the large
  crate (60), blue barrel (60), washing machine (275), lockers (2000), dumpster and car are
  not. The physgun in hand changes nothing; the weapon does not matter.
- Reach (`FindUseEntity`): a line from the eyes along the view, the hit counted at the
  convex's skin (`REST_GAP` short of the hull), within 80 u horizontally plus any part above
  or below the player's hull: a crate 100 u ahead (face at 79.6) is picked up, 110 u (89.6)
  is not, nor 150..300. When the line finds nothing, seven 16 u box sweeps of 72 u tilted
  down from the view (45, 30, 20, 15, 10, -10, -15 degrees) look for props at the feet: the
  gas can and the pop can below the view ray are used that way (`use_gascan`, `use_popcan`).
- Hold point: the prop's mass centre goes to `eye + forward * (24 + radius)` where `forward`
  is the view with the pitch clamped to -75..75 and `radius = 22.63 + extent`, 22.63 the
  player hull's horizontal half-diagonal and `extent` the prop hull's support distance along
  the view
  (`CGrabController::UpdateObject`; the trace of `24 + 2 radius` is clear, so the
  `fraction <= 1` branch holds the prop at `distance - radius`; a wall closer than half the
  trace pulls it in to `radius / 2`). The crate (half extent 19.8) sits 72.0 u out at pitch
  20 (67.7 ahead, 24.5 below the eyes) and 66.4 u out at pitch 0 once turned with the player;
  the gas can 58.7, the chair 59.5 (its origin 2.9 u above the mass centre, so it hangs that
  much above the ray), the pop can 49.6. The point is kept at least `radius` from the
  player's vertical axis.
- Rotation: at pickup the prop's axes snap to the player's yaw frame when within 30 degrees
  (`DOT_30DEGREE`: the drum's 3.5 degree tilt goes to 0), then the prop turns only with the
  player's yaw (crate yaw 0 at player yaw 90, 60 at player yaw 150); the pitch is ignored.
- Motion: the physgun's shadow law with no damping, `timeToArrive` = one tick and caps of
  2500 u/s and 3600 deg/s (37.5 u and 54 degrees a tick): the crate's 20.5 u pickup takes one
  tick, the pop can's 37 u two (36 then 1), the 71 u swing of a 60 degree turn two (54 then 6
  degrees). Jolt's default per-body angular cap (2700 deg/s) is raised for every prop so the
  turn is not cut short.
- Drop: the second press lets it go where it is with zero velocity; the crate then falls the
  44 u to the floor and plays `impacthard` at 0.30.
- Harness: `use_*` 15 / 22 within 2 u. The seven out: the drum, the pop can and the lockers
  settle differently before the press (the drum tips over in the game and rights itself, the
  can rolls, the lockers tilt), the gas can lands on its base in our world and topples in the game, and the dropped crate (three scenarios) pops 3.9 u up on touchdown in Jolt where
  the game settles at the floor (the same landing-after-release gap as `physgun_hold_crate`;
  the fall itself matches to 0.4 u).
- Not modelled: the `impactsoft` sound the game plays as the prop leaves the floor
  (`wood_crate_impact_soft`, 0.6) and the lag of the prop behind a walking player (not
  measured).

## Not yet measured

Spawning onto other props (the spawn ray only hits the world in our trace), ragdolls and effects, `gmod_undonum`, `gmod_admin_cleanup`, cleanup of one type, `sbox_maxragdolls` and the other limits, prop spawn effects and sounds.
