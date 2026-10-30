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

## Properties (context menu)

Measured on srcds (`tools/parity/scenarios/lua/lua_properties`): for a `prop_physics` crate the
game's context menu offers, in menu order, `skin` (601, only with more than one skin),
`keepupright` (900), `ignite` (999), `remove` (1000), `gravity` (1001, a toggle), `drive`
(1100) and `collision_off` (1500); `collision_on`, `keepupright_stop` and `extinguish` take
their place once the prop is in that state, and the rest of the list (persist, bone
manipulation, bodygroups, statue, NPC scale, motion control, edit entity) does not apply to a
plain prop. Nothing applies to players or the world. Every filter asks
`GM:CanProperty(ply, name, ent)` (sandbox: always true), our `player.property_request`.

Implemented, by `Command::Property { name }` on the prop under the crosshair:

- `remove`: the prop and its constraints go (no undo entry).
- `gravity`: `PhysObj:EnableGravity` toggled. Measured (`lua_property_effects`): a weightless
  crate pushed up at 50 u/s keeps rising, only the air drag slows it (46.4 u/s after 120
  ticks); a physgun release keeps it weightless.
- `collision_off` / `collision_on`: `COLLISION_GROUP_WORLD`. Measured: other props fall
  straight through such a prop and it falls through them, only the world holds it; a prop
  already resting on it stays until something wakes it.
- `keepupright` / `keepupright_stop`: `constraint.Keepupright(ent, ent:GetAngles(), 0,
  999999)`. Measured (`lua_property_effects3`): the orientation is driven to the reference
  angles within two ticks (pitch 30 -> 1.1 -> 0, roll 60 -> 4.2 -> 0) and a 40 degree kick on
  a kept prop is undone as fast; `angularlimit` is an angular acceleration cap (60 gives
  ~0.9 degrees per tick per tick with ringing), the property uses the huge one. Ours drives
  the orientation with the physgun's angular law, no damping, one tick to arrive, 3700 deg/s.
- `ignite` is `Entity:Ignite(360)` (measured `lua_fire`, three crates): an `entityflame` on the
  prop plays `General.BurningObject` (`ambient/fire/fire_small_loop2.wav`, a loop) and deals 1
  point of `DMG_BURN | DMG_DIRECT` 7 ticks after the ignition, then every 13 ticks (0.2 s
  thinks), to the prop's `prop_data` health: the crate is `Wooden.Medium` = 30 through
  `scripts/propdata.txt` (the gas can says 20 in its own `prop_data`, metal has 0 and burns
  unharmed). The think that finds the lifetime over (`Ignite(3)`: 200 ticks) plays
  `General.StopBurning` and the flame lingers 0.5 s (33 ticks, still on fire) before it is gone,
  so `Ignite(3)` is 15 points of damage. `extinguish` (offered while burning) removes the flame
  at once and keeps the health. Any damage on a prop goes through its `prop_data` modifier by type first (`Wooden.Base`: bullets
  0.75, club 2.0, explosive 1.5, fire 1) and is truncated to whole points: a pistol's 5 takes 3
  off the crate, the crowbar's 10 takes 20, the .357's 40 breaks it (`shoot_prop_*`). Health 0
  breaks the prop: the surface's `break` sound
  (`Wood_Crate.Break`, volume 0.8 and level 85 from the script), the prop is gone - 30 points
  after `Ignite(360)`, 394 ticks from the ignition. Ours: `Server::ignite / extinguish /
  damage_prop`, `Prop.health / fire`, `entity.break {entity}` to Luau. Not measured: fire on
  players and on props without health, gibs (the game leaves wood chunks), fire spreading.
- Not yet: `drive`, `skin`, `bodygroups`, the menu itself in
  the native client (the console's `property <name>` sends the command).

## Player models

Measured on srcds (`lua_models_full`): `player_manager.AllValidModels()` has 87 names
(`kleiner`, `alyx`, `combine`, `male01`..., `models/player/...`), each with view hands
(`TranslatePlayerHands`: `c_arms_citizen` for 39 of them, `c_arms_refugee` 30, `c_arms_cstrike`
8, `c_arms_combine` 7, `c_arms_dod` 2, `c_arms_chell` 1, with a skin and bodygroups); an
unknown name or an empty `cl_playermodel` translates to `models/player/kleiner.mdl` with the
citizen hands, which is what a fresh bot wears. The table is `gm_server::player_models`,
recorded verbatim. `Command::PlayerModel { name }` (console `playermodel <name>`) is the
player's `cl_playermodel`: remembered at once, worn from the next spawn, when
`player.set_model {player, model}` (`PlayerSetModel`) may return another path. The snapshot
carries the table index, so other clients draw and animate that model; a path outside the
table draws as kleiner. The native client's view model wears the hands of the local player's
model from the same table (`gm_server::player_models::hands`: model path, skin family and
`SetBodyGroups` digit string, loaded with `WorldRenderer::load_model_variant`; one digit per
body part, a missing or out-of-range digit means model 0, a skin past the table means 0), e.g.
`c_arms_refugee` skin 1 body `0100000` for the refugee set, citizen hands until the first
snapshot names a model. Not yet: skins and bodygroups of the player model itself.

## Not yet measured

Spawning onto other props (the spawn ray only hits the world in our trace), ragdolls and effects, `gmod_undonum`, `gmod_admin_cleanup`, cleanup of one type, `sbox_maxragdolls` and the other limits, prop spawn effects and sounds.
