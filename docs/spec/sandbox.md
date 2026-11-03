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
- Hold point: the centre of the prop's collision bounds (its `WorldSpaceCenter`, the half-sum
  of the PHY hull's extents in the body frame) goes to `eye + forward * (24 + radius)` where `forward`
  is the view with the pitch clamped to -75..75 and `radius = 22.63 + extent`, 22.63 the
  player hull's horizontal half-diagonal and `extent` the prop hull's support distance along
  the view
  (`CGrabController::UpdateObject`; the trace of `24 + 2 radius` is clear, so the
  `fraction <= 1` branch holds the prop at `distance - radius`; a wall closer than half the
  trace pulls it in to `radius / 2`). The crate (half extent 19.8) sits 72.0 u out at pitch
  20 (67.7 ahead, 24.5 below the eyes) and 66.4 u out at pitch 0 once turned with the player;
  the gas can 58.7, the chair 59.5 (its origin 2.84 u above its bounds centre - hull z
  -19.86..14.18 - so it hangs that much above the ray at any pitch; its mass centre at
  (-1.47, 0, -3.9) would put it 1.5 u to the side and 1.06 u higher than the game), the pop can
  49.6. The point is kept at least `radius` from the
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
- `skin <n>` (the game's `skin` property opens a submenu of the model's skin families and
  `SetSkin`s the pick): offered only when the model has more than one family
  (`ModelInfo::skins`, the MDL's skin table; the plain crate has two, and the game does offer it
  the property, measured in `lua_properties`) and the index is below the count; sets `Prop.skin`, which travels in the entity snapshot (Luau
  `entity.skin`), through the duplicator clip and the save file (`SavedProp.skin`, 0 when the
  file predates it), and the native client draws the prop with that skin family
  (`load_model_variant`).
- `bodygroups <part> <model>` (the game's `bodygroups` property: a submenu per body part;
  its filter wants some part with more than one model, so the plain crate, one part with one
  model, is not offered it - `lua_properties` records the filter as false for the crate):
  offered when `ModelInfo::body_groups` (models per body part from the MDL) has a part above
  one and the pick is a valid part and model index; sets digit `part` of `Prop.body`
  (`SetBodyGroups` string, padded with zeros), which travels like the skin: entity snapshot
  (Luau `entity.body`), duplicator clip, save file (`SavedProp.body`), and the native client's
  `load_model_variant`.
- The menu in the native client: holding C frees the cursor (the game's context menu key);
  a right-click asks the server (`Command::PropertyList`) what the prop under the crosshair
  offers right now, the reply (`ServerMsg::Properties`: the prop and the entries in the game's
  menu order - body-group and skin picks first, then keep upright / ignite / remove / gravity
  / collision by state) opens as a list at the cursor, and a left-click on a row sends
  `Command::Property` as the console's `property <name>` does. Entries are the command
  names, not the game's localized labels.
- `drive` (order 1100, offered for any prop nobody drives, to a player not driving; `CanProperty`
  is asked, the game also asks `CanDrive`, which we fold into `player.property_request`).
  Measured (`lua_drive`, `drive_sandbox` on a crate): the prop is frozen (motion disabled) for
  the whole drive and left frozen where it stops, so it hovers; its pitch and yaw follow the
  driver's eye angles every tick, the roll is kept; it moves as a kinematic body with a
  velocity in units per tick: with a move input `v = 0.99 v + 0.07425 * wish` (wish: eye
  forward with its pitch for forward/back, eye right for side, world up for jump, normalized;
  `IN_SPEED` makes the accel tenfold; the steady speed is 7.425 u/tick = 495 u/s), without
  input `v *= 0.9`; duck does nothing. The driver's own body never moves and the game puts
  the camera in chase mode on the prop. A `+use` press ends the drive (the game: three ticks
  after the press, ours: in the tick of the press). Walls are not measured (the game moves the
  frozen prop by position, so it probably passes through them; so does ours).
- Not yet: the game's labels and icons in the list, the chase camera's distance (not measured,
  the client's is a guess).

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
snapshot names a model.

Skins and body groups of the player model: `Command::PlayerSkin { skin, body }` (console
`playerskin <n>`, `playerbodygroups <digits>`) is `cl_playerskin` / `cl_playerbodygroups`:
remembered at once, worn from the next spawn together with the model, the body string cut to
its digits (32 at most). Measured on srcds (`lua_skin_body`): the game stores the skin as set,
unclamped (`SetSkin(7)` and `SetSkin(-1)` on the one-skin kleiner read back 7 and -1, and the
value survives `SetModel`), a respawn goes back to the player's cvars (a bot: skin 0, no body
groups), and `cl_playerskin` / `cl_playerbodygroups` are client userinfo cvars (`GetInfo`), not
server ones; the player models tried (kleiner, group03/male_01, combine_soldier) have a single
body group with one option, so body-group parsing was not observable there. The snapshot carries
skin and body with the model index; the native client draws other players with
`load_model_variant` (an out-of-range skin or digit shows family/model 0, ours). `Command::Kill`
(console `kill`) is the game's `kill`: the player dies and respawns.

## Not yet measured

Spawning onto other props (the spawn ray only hits the world in our trace), ragdolls and effects, `gmod_undonum`, `gmod_admin_cleanup`, cleanup of one type, `sbox_maxragdolls` and the other limits, prop spawn effects and sounds.

### Devices on welded assemblies

The game's weld is a rigid IVP constraint: a thruster's force moves the welded assembly as one body. Our weld is soft (60 Hz translation / 15 Hz rotation, tuned on `constraint_weld_pair`), so a 25 kN thrust on the 1 kg thruster body tore it off its crate (`constraint_device_thruster_crate`: crate tumbled at t26, thruster flew at 3500 u/s, 4225 u error). The thrust impulse is therefore applied at the thruster's point to the heaviest body of the thruster's weld group (`gm_sandbox::weld_group`, transitive over weld constraints; the thruster itself when not welded). Lift now tracks the game within 3.5 % (z 491 vs 509 at t100), error 679 -> 69 u. Open: our pitch rate is twice the game's (-27 vs -14 deg/s at t50) while our lateral drift is smaller (-7 vs -23 u/s) at the same tilt - the game's application point / angular drag differ; one record only, not derivable yet.

### Hoverball drop (open)

Recorded on srcds (`constraint_device_hoverball_drop_m90/m40/p40`: hoverball switched off at t20 from -90/-40/+40 over the floor at -143). The first hit is linear in the approach speed in both: game lateral 0.156·v_in, spin 2.04 deg/s per u/s, rebound 0.193·v_in; ours 0.148 / 2.30 / 0.188. Impulse budget (m40, 15 kg, I = 0.0944 kg m², R = 0.149 m): tangential impulse 18.8 N s ours vs 19.8 game, angular 1.257 vs 1.114 N m s, so the game's effective horizontal lever of the contact vertex is 0.0154 m against our 0.013 m - the game lands the sphere hull on a vertex ~0.25 u further from the mass centre than our first-round pick. The second hit (spin-to-slide conversion, `device_hoverball` t283) is downstream of this split. Open: hull vertex / COM check on the hoverball PHY.

The split is the step granularity of the iterative vertex push (`push_fraction`, 0.075 per step, calibrated across the harness set): on `device_hoverball_drop_m40` the first hit goes 0.025 -> 65.0/-53.6/-624, 0.05 -> 63.4/-50.7/-713, 0.075 -> 62.5/-49.1/-763, 0.15 -> 58.7/-42.3/-975 (vz/vx u/s, spin deg/s; game 64.1/-51.9/-677, i.e. ~0.04). A global 0.04 trades harnesses (filecab corner drop and weapon touch equal -> DIFF, tilted drop DIFF -> equal; drops constraint_device_hoverball 1.184 u, crate 1.492 u, drop_m40 76.160 u, drop_m90 65.370 u, drop_p40 64.604 u), so the default stays; the hoverball residual is bound by this granularity until the vertex push gets a closed form.

### Flat landing rebound: the floor is the body's

The crate material/mass series on srcds (`phys_drop_flat_crate_<material|m3|m300|m2000>_lo*`) says the flat-landing rebound of the cube is a constant 28 u/s for every surface property with friction >= 0.35 and elasticity 0.01-0.3 (dirt, glass, gravel, snow, mud, metal, rubber, tile, slime, wood) and for 3-2000 kg; ice (friction 0.1) gives 18, and only `popcan` (elasticity 0.99) gives a slope (0.19 v + 11). Six more bodies (`phys_drop_flat_{crate2,cardbox1,cardbox3,plasticcrate,harddrive,paintcan}_lo*`, 261103) show every four-vertex box rebounding at its own near-constant level from 44 to 98 u/s: crate 28.2, long wood crate 24.5, cardboard boxes 22-23, hard drive 20.5, ammo can 17.8 (then rising). Not mass, not material (both crates are wood): over the six boxes the level is monotone in the lever product `b*h` over the square radius of gyration `k²` about the body's first tangent axis (`b` the half extent along the other tangent axis, `h` the mass centre height over the face), and the other axis does not order the data at all - the game rocks the landing body about one body axis. The shim's face-landing floor is now `0.36 + 0.264 f (b h / k²)` m/s with the pair friction `f` on the slope (the earlier `0.417 + 0.467 f` is this rule at the crate's `b h / k² = 2.1`); boxes land within 1-3 u/s of the game (crate 28.3, long crate 25.2 vs 24.5, cardboard 23-26 vs 22-26, hard drive 21.5 vs 20.5, ammo can 17.9 vs 17.8; crate on ice 15.9 vs 17.9). No verdict moved over 507 harnesses; the lockers' corner drops improved (22.7 -> 20.7 u). Still open: the lockers' flat landing (forty foot vertices: game 8.8-17.6 u/s and tipping, ours 29 flat), the gas can's edge landing (19-30 with 12-25 u/s lateral), and the ring bodies (drum, pop can, paint can) whose rim-vertex landing follows another path.

The floor never sends a point back faster than it came: a point touching down slower than its own floor keeps plain restitution (the series' slowest landing, 45 u/s, still leaves at 0.62 of it). Without that a chair spawned on the floor hopped 0.8 u at its floor speed every 8 ticks for ever (`server_duplicator_vehicle`: the game's wooden chair sinks 0.25 u in 3 ticks and is still; ours now rests at -144.1 against its -144.14); 387 physics / sandbox / server harnesses unchanged by the cutoff.

### Flat landing side and tipping (open)

The lockers' hull bottom is planar (forty vertices of ten feet at one height), so their tipping on a flat landing (game: 0.19 v up, 0.22 v sideways toward -x, -0.48 deg/s per u/s about y; ours: the face floor, no spin) is not geometry of the feet. The side is the foot row nearer the mass centre (the +x row by 0.13 u), as with the other bodies that tip toward the vertex nearest the mass centre; the filing cabinet's hull is centred to the micrometre, so its side is a tie the game breaks some other way. What orders the size of the effect is slenderness, mass-centre height over the smaller half extent: up to 1.4 (crate, long crate, cardboard, ammo can, filing cabinet) the game's landing is centred within 2-5 % of the half extents and the side speed is under 1 u/s; 2.7 (hard drive) gives 4-6 u/s; 3.5 (gas can) an edge landing with 17 u/s; 5.0 (lockers) the tip. The game's chain looks sequential - one row takes the impulse, the body turns, the other feet slide, friction turns the slide into side speed (friction impulse 0.185 of the normal one) - which a simultaneous face push cannot reproduce; left open with the data (`phys_drop_flat_*_lo*` 261103).

### Spawned into a wall (open)

Recorded on srcds (`server_wall_overlap`: Pod and Jeep spawned by their trace into the gm_construct wall at x = 1024, hit 23 / 8 u above the floor, states every 5 ticks). The game never impacts a body that is created inside world geometry: the Pod keeps its angles to the degree for 25 ticks while it leaves - x untouched for the first 5 ticks, then a constant 1.08 u/tick (5.4 u per 5 ticks, both spawn heights), y and z drifting by whichever other faces it overlaps (+5.4 / tick-5 and a 1.4 u rise at one height, +1.9 at the other), gravity not acting until it is out, the exit speed kept afterwards (it lands upright 32 u from the wall and only then tumbles). The Jeep is different: 0.15 u/tick out, z held at -114 (30 u above the floor) for 60 ticks and the nose sagging to pitch 24 - the rear hangs in the wall by friction, so the game's deep contact carries friction and no impulse. Ours (Jolt): the overlap is resolved on the spawn tick with contact impulses - Pod 4 u and 5 deg at once, Jeep 19 u - and the Pod topples (pitch 68 at t+60). Falsified in the shim: (a) `mMaxPenetrationDistance` 0.05 .. 0.005 m (spawn-tick kick shrinks to 2 u but the Pod is then held in the wall and does not fall, the Jeep still slides 11 .. 20 u); (b) deep static contacts (> 2 u) as sensors with a translation-only exit at 0.036 x depth per tick re-read every tick (Pod exits too slowly and stalls at 10 u); (c) the same with the exit velocity fixed at the first depth (0.078 x the deepest manifold's depth, held, no gravity) - reproduces the Pod's x track to the unit for 30 ticks and lands it upright, but the Jeep's deepest manifold is 27.6 u (its axle stubs) and it is thrown 53 u; (d) deep contacts kept in the solver with the body's inverse inertia scaled to 0 and the correction rate capped at 1.08 u/tick - Pod stalls and topples, Jeep thrown 60 u. A wheel-less body wants (c); a car wants its wheels to resist the push like the game's wheel bodies do - one rule for both is not in this data, so none is in the tree.
