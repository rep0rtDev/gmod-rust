# AI and navigation

Our own system, not a port of the engine's `ai_*` schedules or its `.nav` files
(user decision: Recast-based navigation instead of the legacy scheme). GMod is used
only to measure what NPC behaviour looks like from outside (positions per tick);
how the NPC decides is ours.

## Navigation mesh (`gm-nav`)

Built at map load from the BSP with upstream Recast/Detour (`recastnavigation-sys`,
C++ library linked in; the wrapper is ours).

Inputs (Source units, Z up; rotated `(x, y, z) -> (x, z, -y)` for Recast's Y up and
back on output; the rotation is proper so winding survives):

- every world-model brush (model 0's leaves; brush entities keep their own models and
  are left out) whose contents intersect `SOLID | MOVEABLE | WINDOW | GRATE |
  MONSTERCLIP` (the solid part of `MASK_NPCSOLID`; `PLAYERCLIP` is not solid to NPCs),
  as outward-wound polygons from the side planes
  (`brush_polygons`: each plane's huge quad clipped by the others; bevel sides give
  no polygon);
- sides whose texinfo has `SURF_SKY | SURF_SKY2D` are dropped (skybox shell);
- displacement triangles (`build_disp_mesh`).

gm_construct: 63 982 vertices, 46 588 triangles, bounds
`(-14848, -14336, -1024) .. (15360, 14848, 12521)`; the flatgrass ground really spans
the whole box, so one Recast tile cannot hold it (compact-heightfield cell index is
24 bits; a single tile had 33.5 M spans and produced no regions).

Agent (`NavParams::default`, the 32 x 32 x 72 human hull):

| parameter | value | voxels |
|---|---|---|
| cell / cell height | 8 u / 4 u | |
| agent radius | 16 u | 2 (erode) |
| agent height | 72 u | 18 |
| step height | 18 u | 4 (`walkableClimb`) |
| max slope | 45 deg | |

Tiling: 256 x 256 cells (2048 u), border `radius + 3` cells, triangles pre-filtered by
AABB per tile; regions by watershed with `minRegionArea 64`, `mergeRegionArea 400`,
contours `maxError 1.3`, `maxEdgeLen 12`, 6 vertices per polygon, detail sample
`6 cells` / error `1 cell height`. One Detour tile per non-empty Recast tile
(`dtNavMesh_addTile`); poly ref = 32 bit, `maxTiles` next power of two of the tile
count (gm_construct: 15 x 15 -> 256 tiles, 16 384 polys per tile).

gm_construct result: 5 804 polygons, 10 891 vertices, ~2 s release build.

Queries (`NavMesh`):

- `nearest(pos, extents)`: closest point on the mesh within the half-extents box;
- `find_path(start, end, extents) -> Path { points, partial }`: both ends snapped,
  `findPath` (A*, up to 512 polygons) then `findStraightPath` corners; when the end
  is unreachable `partial` is set and the path ends at the closest point of the last
  reachable polygon.

Heights on the path are voxel heights (within one cell height of the floor);
locomotion takes Z from the ground trace, not from the mesh.

Measured on the mesh (unit tests): a 16 u step is walkable, a 40 u ledge is not; a
corner next to a wall stays >= agent radius away from it; an open floor is one
polygon set and a path across it has two points.

## Locomotion (`gm-server`, `Server::npc_goto` / `tick_npc_move`)

Own path following, one step per tick after the physics step and NPC settling:

- `npc_goto(id, target)`: `find_path` from the NPC's feet with query half-extents
  `(32, 32, 72)`; false without a mesh, for a dead NPC or when either end has no
  polygon nearby. A partial path (target off the reachable mesh) is still taken and
  ends at the closest reachable point (`NpcPath::partial`).
- Corners: an intermediate corner counts as passed within 8 u horizontally
  (`NPC_CORNER_REACH`); the final point is walked onto exactly and the goal is cleared in
  that same tick.
- Cadence (measured per tick, `server_npc_walk_*`): the NPC only moves on its think,
  every 7 ticks (`NPC_THINK_TICKS`) on a grid locked to its spawn tick: the ticks
  `NPC_THINK_PHASE` = 3 past the spawn modulo 7. A goal set between ticks starts moving
  on the first grid tick after it (1..7 ticks later; `server_npc_walk_citizen_p11`..`_p15`
  shift the goal tick by one each and all land on the game's tick). Between thinks the
  origin does not change. While no player's PVS contains the NPC the game thinks every
  13 ticks instead (`NPC_THINK_TICKS_FAR`; `server_npc_walk_citizen_corner` /
  `_corner_seen` switch at the same spot of the same line behind the building whether
  the bot looks at the NPC or not, while `_turn90` stays at 7 in the open 370 u away);
  the step and the yaw clamp scale with the actual interval (15.58 u / 29.3 deg per 13
  ticks at walk speed). Ours uses the map's PVS from the player's eye cluster to the NPC's.
- Open (`server_npc_walk_citizen_corner_seen`, re-recorded with the probe bot's body yaw
  following its eye yaw - the game's spawns read `GetAngles`, so the earlier recording had
  the NPC facing the other way): an NPC spawned facing away from its goal turns in place
  in the game slowly and late (yaw 16 deg 13 ticks after the goto, then 63 / 94 / 110 /
  142 / 157 / 177 every 8 ticks while it creeps), then walks straight -x along y = -491
  and corners at x = 75 (yaw 156) and x = -8 (140); ours turns at once at a steady 2
  deg/tick, settles on the direct bearing (168) and walks the diagonal. Per think (7
  ticks): the game does nothing on the think after the goto where ours already turns
  15.75 and steps 0.6 u; then its yaw advances 15.7 / 31.5 / 15.8 / 31.5 / 15.7 / 31.5 /
  15.8 / 19.6 (one and two yaw clamps alternating, 1.5x our steady 15.75) while its step
  grows 0.7 / 2.0 / 3.1 / 4.3 then holds 5.1 / 5.3 / 5.5 until the yaw is within 20 deg
  of the motion and only then 6.2 / 7.0 toward the 8.4 of walk speed (ours reaches 8.2
  unthrottled). The first leg along y = -491 is the node graph's, not the direct bearing.
  Not derived yet: the alternating double clamp, the speed hold while turning, the leg.
- Gait (`NpcMoveDef`, `npc_move_def(class)`), measured on gm_construct over a 400 u
  straight walk (one think = 0.105 s):

  | class | walk u/s | walk accel u/s^2 | run u/s | run accel u/s^2 |
  |---|---|---|---|---|
  | npc_citizen | 79.92 | 129.9 | 209.79 | 259.8 |
  | npc_metropolice | 57.69 | 115.4 | 184.56 | 369.2 |
  | npc_combine_s | 71.84 | 121.8 | 214.78 | 264.8 |

  Every other class takes the citizen row until measured (the zombie recordings were
  contaminated by its combat state even under `ai_ignoreplayers 1`).
- Speed law per think (`u` the step in units, `s0` the previous speed per think, `A` the
  accel per think squared, `D` the remaining path length):
  - ramp: `u = (s0 + min(s0 + A, ideal)) / 2`, a trapezoid of the end-of-think speed
    (citizen: 0.716, 2.149, 3.581 ...; switching to run mid-ramp continues from the walk
    speed: 0.716, 2.864, 5.729 ...);
  - brake onset: `u <= sqrt(2 * 0.755 * A * D)` (`NPC_BRAKE_CURVE`; citizen walk 8.199 and
    metrocop run 18.106 exact, citizen run 20.78 vs the game's 21.58);
  - from then on `u` is the largest step whose staircase `u, u - A, u - 2A, ...` sums to
    `D` (`brake_staircase`; the game's second braking step solves this exactly in every
    recording), moving at the step itself (no trapezoid), never shedding more than `A`
    per think, and the last step is the remainder, so the walk arrives on the game's tick
    (the citizen's 400 u take 54 thinks: the first on the grid, then 53 every 7 ticks).
- `npc_goto(id, target, run)` picks the gait; `npc_set_run(id, run)` switches it from the
  next think on (the oracle probe forces `ACT_RUN` once the NPC moves; the runner mirrors
  that by switching as soon as the NPC has moved).
- A new goal while moving (`server_npc_walk_citizen_redirect`, goal behind-right at full
  walk speed): the game halts along its old path first, shedding the accel per think as
  a trapezoid (7.675 / 6.055 / 4.657 / 3.322 / 2.002 / 0.833; ours 7.676 / 6.243 / 4.811 /
  3.379 / 1.947 / 0.615), while the body already turns toward the new goal faster than
  usual (17.8 -> 15.7 deg per think as it slows; `NPC_YAW_RATE_PER_SPEED` = 0.267 deg/s
  per u/s), then starts the new path from zero on the next think (0.548 vs our 0.716).
  `NpcPath::halt` + `Npc::next_goal`; the new path is queried from the halt point.
- Step: along the path in the XY plane (a step longer than the distance to the corner
  continues along the next segment); the hull is swept 18 u above the feet
  (`step_height`) with `MASK_PLAYERSOLID`, stops at the first solid, then is dropped onto
  the floor by the same 256 u box trace that places a spawned NPC. Z is therefore the
  floor's, not the mesh's (gm_construct: the ground west of spawn is 4 u lower and the
  NPC follows it).
- Yaw turns toward the motion at the think, 150 deg/s (`NPC_YAW_RATE`, 15.75 deg per
  think: the game goes 180 -> 164.2 -> 148.5 -> 132.7 -> 117.0 -> 101.2 -> 90 for a goal
  due north, `server_npc_walk_citizen_turn90`); the NPC walks along the path from the
  first think whatever it faces, with the accel scaled by 0.89 (`NPC_TURN_ACCEL`) while
  the body still turns (ramp 2.004 / 3.184 / 4.446 / 5.753 / 7.091 instead of 2.149 /
  3.581 / 5.013 / 6.446 / 7.732; 2.7 u if ignored). Pitch/roll untouched.

Harnesses `server_npc_walk_citizen` / `_citizen_run` / `_metrocop` / `_metrocop_run`,
`_citizen_p11`..`_p15` (goal tick phase), `_citizen_turn90`, `_citizen_redirect`,
`_soldier` and `_soldier_run` (golden 261102, one scenario per srcds session, 1-tick
waits): all thirteen equal at 2 u.
A `wait 17` of the probe lasted 18 ticks in one session (other waits exact), so that phase
was not kept. `_citizen_corner` / `_corner_seen` (1300 u to (-300, -300) round the
building) are red by construction: the game walks its node graph (175 -> 180 -> 147 ->
140 -> 136 -> 168 deg legs with a ~1 u brake dip at a mid-leg node), ours string-pulls the
Detour mesh and leaves south-about instead of north-about, so the routes part at the first
step; they stay as recordings of the 13-tick regime. Chasing why Detour first went
south-about found a mesh bug: `world_geometry` took every brush of the BSP, including the
brush entities' own models, and gm_construct has two 8 u NODRAW `CONTENTS_SOLID` slabs
through the map centre (x = -4..4, y = -968..968, z = -204..204) that walled the mesh in
two. Only the world model's brushes (model 0's leaves) are meshed now; the route is the
straight 1317 u diagonal, the game's node-graph legs total about 1300 u.
Residuals inside the tolerance: the game's first braking step is gentler than the curve
on the citizen run (0.8 u, it arrives one think after us), and the metrocop walk sheds
0.06-0.3 u per think over four thinks before its brake onset (1.3 u).

Test: `gm-dedicated/tests/npc_entity.rs::npc_walks_the_navmesh_to_a_goal` (step cap per
think, yaw = motion within 1 deg, feet on the traced floor within 0.5 u, exact arrival, the game's
think grid: first think after the goal + 53 x 7 ticks).

## Chase (own behaviour, `Server::npc_chase` / `npc_stop`)

Our design on top of the measured locomotion; the game is no oracle here (its chase is a
schedule we do not port). `npc_chase(id, player, run)` makes the NPC follow a player: on
each think, if the player is more than `NPC_CHASE_REPATH` = 16 u off the end of the
current path and more than `NPC_CHASE_REACH` = 64 u away, a fresh Detour path from the
NPC's feet replaces the goal without touching the speed (no halt, unlike a re-issued
`npc_goto`); the braking staircase runs on the path length minus the reach, so the NPC
comes to rest exactly 64 u short of the player and starts again when the player moves
off. `npc_stop` drops goal and chase. A player who leaves ends the chase. Test:
`npc_entity.rs::npc_chases_a_moving_player` (rests 64 +- 1 u away, follows a teleported
player with monotonically growing steps, i.e. no halt).

## Not yet

Gaits of the other classes (zombie, antlion, ...), idle / combat behaviour, crowd
avoidance, dynamic obstacles (props), off-mesh links (ladders, jumps), addon API for
`npc_goto` / `npc_chase`.

## Combat: measured inputs (soldier, SMG1)

Oracle recording `server_npc_fire_soldier` (gm_construct, `npc_combine_s` with `weapon_smg1` spawned 178 u east of a standing player, `ai_ignoreplayers 0`, `sbox_godmode 0`, 1200 ticks; `.local/oracle/npc_fire/`). Only the numbers below are taken from it; the behaviour itself (target choice, states, transitions) is our own and is not a port of the game's schedules.

- The soldier enters combat on its first think after spawn (state 1 -> 3 at tick +28), crouches and never moves for the whole run.
- Every shot is one `FireBullets` call: `Num 1`, `Damage 0` (the ammo's NPC damage, `sk_npc_dmg_smg1` = 3), `Spread 0` (the spread is already folded into `Dir`, |Dir| up to 1.008), source = the crouched shoot position (origin + 35 z), range = max trace length.
- Cadence: one shot per NPC think (7 ticks), 5 shots per burst, 28 ticks between bursts (56-tick period, i.e. 3.6 s of `shootSMG1c` + `CrouchIdle`); the very first burst is 2 shots followed by a 21-tick gap. One 637-tick pause occurred mid-run, after which bursts were 2 shots long.
- Accuracy: the shots inside a burst repeat the same direction; against the standing player at 178 u that direction passes ~15 u beside the body (the first shot of each burst goes ~50 u high). A straight `MASK_SHOT` trace from the shoot position to the player's centre does hit (hitbox 15, hitgroup 3), yet in 1200 ticks the player took no damage at all - the game's NPC aim at this range misses a motionless player systematically.

Consequences for our combat model: fire cadence = 5 x 7 ticks on, 28 off; damage per hit = the ammo's NPC damage; aim offset is not copied (it is a hidden proficiency table, not a measurable law) - our soldier aims at the chest with a fixed cone and the hit rate is documented as a divergence.

## Combat: our model

Implemented in `Server::npc_combat_think`, run once per NPC think right after the body moved. It is our own state machine; only the cadence and the per-hit damage above come from the game.

- Who: classes in `npc_hostile` (combine, metropolice, zombies, headcrabs, antlions, hunters, turrets, ...) that hold a hitscan weapon (`npc_weapon_shot`: pistol / SMG1 / AR2 / shotgun 7 pellets / .357). Melee and projectile NPCs do not engage yet.
- Target: the current target while visible, otherwise the nearest alive player within `NPC_SIGHT_RANGE` (2048 u) whose chest (origin + 36) is on an unobstructed `MASK_PLAYERSOLID` ray from the NPC's eye (origin + 64). Lowest player id breaks ties.
- A hostile NPC shot from across the room walks at the shooter (`server_npc_damage`: the zombie
  180 u from the bot, facing it): the game moves 4.4 u in the 15 ticks after the first hit, then
  7.6 u per 15 ticks (0.5 u/tick) along a node-graph leg that drifts +y (yaw 170 -> 199 as it
  closes), and the later pistol shots at its centre miss while it moves (48 / 45 / 45 / 45 /
  43 ...). Ours, with the mesh (the harness runner now builds it for any scenario that spawns
  an NPC, and `ai_disabled 1` freezes every NPC as in the game), chases straight at 0.6 u/tick
  from the first think and is hit by every shot - reaction delay, route and pace open. Note
  the pace: the zombie that sees its target from the start (`server_npc_melee_zombie`) walks
  44.6 u/s straight at it, the one woken by a bullet here 33.8 u/s along a curving leg - two
  AI states (combat chase vs alert / investigate), only the first is modelled.
- Facing: standing still the NPC turns toward the target at `NPC_YAW_RATE` (150 deg/s per think); while walking a path the motion owns the yaw. Fire starts once the yaw is within 15 deg of the target.
- Fire: one shot per think, `NPC_BURST_SHOTS` = 5 per burst, `NPC_BURST_GAP_TICKS` = 28 between bursts; the first shot comes one think after the target is acquired (the game also opens fire one think after entering combat). Each pellet leaves the eye toward the chest inside a 5 deg cone (`NPC_AIM_CONE_DEG`, a hash of NPC id and tick, deterministic) and the nearest of the world, a prop and a player hull on the ray takes it; a player takes `weapons::npc_shot_damage` (3; .357 15) as DMG_BULLET through the normal damage hooks. NPCs do not hit each other.
- Lost sight: the NPC runs after the player (`npc_chase`, run gait) until the player is seen again, which ends the chase and resumes fire; after `NPC_TARGET_LOSE_TICKS` (330) without sight the target is dropped and the NPC stops. A chase or goto issued by an addon is not overridden while the target stays in view.

Tests (`gm-dedicated/tests/npc_entity.rs`): `armed_soldier_fires_bursts_at_a_visible_player` - a soldier with an SMG1 178 u from a standing player fires 27 shots in 300 ticks with gaps 7/7/7/7/28, the player goes 100 -> 19 (every shot hits the 32 u hull from that range; the game's soldier hit nothing in 1200 ticks - the aim offset of its proficiency table is a documented divergence, our model is deliberately more lethal at short range). `soldier_chases_a_target_out_of_sight_then_gives_up` - a hidden navmesh spot 1120 u away makes the soldier chase without firing, a return to view re-engages, a player beyond sight range is dropped after 335 ticks with chase and goal cleared.

Addon control: `npc_target` (`Server::npc_target(id, Some(player))`) forces the target, `npc_target` with no value drops it and sets `Npc::passive`, which keeps the NPC out of combat until the next `npc_target`; `npc_goto` / `npc_chase` / `npc_stop` are untouched.

Crouch pose and hits taken (measured, `server_npc_flinch_soldier`: two pistol bullets into a firing soldier): in combat and standing still the soldier fires from a crouch - eyes at origin + 40 (standing + 66), the hull stays 13 x 13 x 72; the sequence is `shootSMG1c` from the first shot for 7 ticks per shot (35 for a full burst) and `CrouchIdle` for the rest of the gap (21). A bullet hit plays no flinch; a hit inside a burst ends the burst (3 shots instead of 5, the next burst `NPC_BURST_GAP_TICKS` after the last shot), a hit in the gap delays the next burst by one think (gap 35). Ours: `NPC_CROUCH_EYE_HEIGHT` = 40 for sight and bullet origin while crouched, `npc_combat_sequences` (soldier only, other classes keep their sequence), `Npc::rest_sequence` restores the idle when the target is lost, dropped or cleared, the flinch rule in `damage_npc`. Test `soldier_hit_mid_burst_ends_it_and_crouches`.

## Squads and cover: our model

No oracle here - the game's squad slots and hint-node cover are not measurable from outside and are not copied.

- Squad: NPCs of the same spawn key within `NPC_SQUAD_RANGE` (512 u). On its think an NPC without a target takes a squadmate's target (`npc_share_targets`) and, not seeing it, chases it under the usual loss rules. Test `squad_shares_the_target`: a second soldier placed on a navmesh spot the player cannot see takes the target within two thinks and starts the chase.
- Cover: a bullet hit that leaves an armed NPC at `NPC_COVER_HEALTH` (40%) of its max health or less asks for cover (the game's soldier at 60% kept firing, see the flinch recording). On the next think `npc_take_cover` picks, among navmesh vertices within `NPC_COVER_RANGE` (512 u) of the NPC, those the target's eye cannot see at chest height, not within `NPC_COVER_SPACING` (32 u) of a squadmate's cover, reachable by a complete path, ranked by path length (capped at 1.5 x range: a point 65 u away across a wall can be 1000 u on foot); the NPC runs there without firing, holds `NPC_COVER_TICKS` (200) out of sight without chasing, then the usual rules return (chase if still unseen, fire if seen). `NPC_COVER_COOLDOWN` (660) between cover runs. Test `soldier_takes_cover_when_hit`: the soldier at 15/50 runs a 6-corner path to a vertex hidden from the player, stops within 8 u of it, holds, re-engages.
- Cover from threats that are not hits (`npc_threatened`, same gating as above: armed hostile NPC, not passive, not already in cover, off cooldown; a target-less NPC takes the threat's author as its target first so it has someone to hide from): suppressing fire - a player's bullet whose segment (eye to its first hit) passes within `NPC_SUPPRESS_RADIUS` (48 u) of the NPC's chest without hitting it counts as a near miss, `NPC_SUPPRESS_SHOTS` (3) of them inside `NPC_SUPPRESS_WINDOW` (66 ticks) ask for cover and reset the count; a live exploding projectile (grenade, AR2 grenade, rocket - any `weapons::explosion` kind, in flight or going off that tick) inside `NPC_GRENADE_RANGE` (256 u) of the NPC asks for cover from its owner. Tests `soldier_takes_cover_under_suppressing_fire` (30 u beside the chest: 1 miss nothing, the window lapses and the count restarts, 3 misses -> cover wanted, a shot 120 u past does not count) and `soldier_takes_cover_from_a_live_grenade` (an AR2 grenade dropped 100 u beside a full-health soldier -> cover on the next think).
- Formation (`formation_slots`): squadmates chasing the same player are ordered by entity id; slot 0 goes straight at them, slot k paths to a point ceil(k / 2) * `NPC_FORMATION_SPACING` (96 u) to its own left (odd k) or right (even k) of the player, perpendicular to its own line of approach, recomputed at every re-path; the 64 u chase rest still applies to that point. Test `squad_fans_out_on_the_chase` (two soldiers forced on one player 400 u away end >= 64 u apart, both within 200 u of him).

Locomotion fixes that fell out of the cover run: a path corner within `NPC_CORNER_REACH` (8 u) is passed only when the hull can already sweep to the next corner (cutting it against a wall end jams: the mesh keeps 16 u, the box hull 13); a think-step sweep blocked after passing a corner is retried to the corner itself; the remainder of a blocked step slides along the hit plane (the box hull's 18.4 u half-diagonal reaches past the mesh's 16 u margin at wall ends); a body that covers less than a tenth of its step three thinks running gives the path up (`Npc::stuck`).

Antlion and poison zombie (scenarios `server/npc_melee_antlion`, `server/npc_melee_poisonzombie`, `server/npc_run_antlion`, `server/npc_melee_poisonzombie_long`). Measured - antlion: hull 16 x 16 x 64, eyes +24, 30 hp; combat at +28, one think of `run_all`, a 66 u ground lunge (`pounce2`), then at ~93 u `attack3` (98 ticks) whose swipe lands at +34 for 5 DMG_SLASH with a 7490 force (the player is thrown ~30 u; the swipe reaches ~100 u), then `pounce` (63 ticks): an 84 u lunge hitting at 38 u at +34 for 5 (773 up), a 21 u recoil, and `attack3` again - swipe and pounce alternate on a 168-tick period. From 1100 u (the bot teleported west after the spawn) it runs `run_all` with steps of 3.91, 11.7, 19.49, 27.32, 34.05, 36.15, 37.23, 37.26 u per 7-tick think (0.105 s): 354.9 u/s top, 706 u/s^2 ramp with a soft cap over the last two thinks; at ~597 u it stops for 42 ticks and `fly_in` carries it 51.23 u per think (483 u/s) for 13 thinks - landing ~120 u PAST the player - then `jump_stop` (35 ticks), `turn_left` (35), `pounce`, `attack4`. Poison zombie: hull 13 x 13 x 72, eyes +64, 175 hp; walks at 5.85 u per think (55.2 u/s, 104 u/s^2 ramp); at ~140 u stops for `headcrab2Leap` (168 ticks) and throws a poison headcrab - a real `npc_headcrab_poison` entity (the damage attacker/inflictor), which bites the player 187 ticks after the throw began for 5 DMG_SLASH plus 94 DMG_POISON leaving it at 1 hp (the player then recovers 10 hp 134 ticks later - poison regen, one sample); then from ~66 u `melee_02` (98 ticks) with the claw at +62 for 25 DMG_SLASH. In the long run the zombie threw again 859 ticks after the first throw (busy in between: the claws killed the bot, who respawned 1072 u away), this time from 795 u with `ThrowWarning` (77 ticks) + `Throw` (161), the crab crossing the distance and biting 341 ticks after `Throw` began.

Ours: `npc_melee_moves` gives each class a list of moves cycled in order (restarting at a per-class index): a move starts inside `start_range`, may dash `lunge` u toward the player over the first 21 ticks of the swing (hull-swept, floor-dropped), lands `damage` at `hit_tick` if the player is still inside `reach`, back-steps `recoil` u. Antlion: swipe (reach 100, 98 ticks, hit 34) then pounce (start 120, lunge 84, reach 55, 63 ticks, hit 34, recoil 21), both 5 damage with a 250 u/s punch, run gait `NPC_MOVE_ANTLION` 354.9 u/s / 706 u/s^2; the long jump (`npc_jump`, our reading of `fly_in`): a visible target 300..600 u away (one sample at 597) makes it stop and wind up 42 ticks (`jump_at`), then `launch_npc` puts it on the arc (483 u/s horizontal, the vertical speed that lands on the player's position - the game's 120 u overshoot is left as a residual), the landing holds it 70 ticks (`stand_ticks`: jump_stop + turn), then the melee cycle; next jump no sooner than `NPC_JUMP_COOLDOWN` 660 ticks (own value). Poison zombie: the throw move (start/reach `NPC_CRAB_THROW_RANGE` 1000 u, 168 ticks) releases a real crab at tick 150 (`throw_crab`: a `npc_headcrab_black`-table NPC with `thrown` set, launched from the zombie's shoulder at 380 u/s to land `NPC_CRAB_THROW_SHORT` 48 u short of the player, already targeting them; a thrown landing takes no rest and no run-away, so it leaps on its next think and the touch is `NpcLeap.poison`: 5 DMG_SLASH + up to 94 DMG_POISON, never below 1 hp); `NPC_POISON_CRABS` 3 per zombie, `NPC_CRAB_THROW_COOLDOWN` 860 ticks between throws, otherwise the claw (start/reach 70, 98 ticks, hit 62, 25 damage, 325 u/s) at `NPC_MOVE_POISONZOMBIE` 55.2 u/s. Tests `antlion_alternates_swipe_and_pounce` (swipes land from 93-98 u, pounces from 51-53 u, 5 each; same-move period 182 ticks against the game's 168 - our run-up between moves is longer, recorded, not tuned), `antlion_runs_and_jumps_from_afar` (ramp 7.8 u per think^2 to 37.26, wind-up inside 600 u, lands within 60 u, stands 63+ ticks, then hits) and `poison_zombie_throws_then_claws` (throw on the first think from 178 u - the game waited until 140, residual; the crab entity appears at +150, the bite leaves 1 hp at +177, then the claw; one crab in 700 ticks). The thrown crab is not registered to a player's cleanup/undo and does not count toward the NPC limit (the game's crab is the zombie's, not the player's).

Pending: the poison zombie's throw cadence when idle (859 is a lower bound), the game's crab chasing across long distances (ours flies to 48 u short in one arc), poison regen on the player, combat sequences of other armed classes, dynamic obstacle (prop) avoidance.

## Melee: measured inputs and our model

Oracle recording `server_npc_melee_zombie` (`npc_zombie`, no weapon, spawned 178 u east of a standing player, 600 ticks; scenario `tools/parity/scenarios/server/npc_melee_zombie.json`): the zombie is in combat on its first think and walks straight at the player at 4.73 u per 7-tick think (44.6 u/s, ramp 1.57 / 2.62 / 3.67 / 4.45 / 4.72, ~93 u/s^2), starts `attackE` when the 2D distance is 52.1 / 51.6 / 50.4 u (the first think inside a 55 u reach), stands through the 84-tick swing, and 48 ticks in the player takes 25 DMG_SLASH (hitgroup 0, damage force forward x 25 x 300 = 7500) and leaves at ~325 u/s (the game's DamageForce 25 x 5 = 125 away from the inflictor plus a 200 u/s claw punch), sliding ~64 u before the zombie walks up (`walk4`) and swings again; hits 196 ticks apart.

Our model (`npc_melee_think`, `tick_npc_melee_hits`): a hostile class in `npc_melee` without a hitscan weapon (zombie, zombine; others unmeasured) takes the same sight rules as armed NPCs, chases the target with the chase resting at `reach - 3` (so the last step lands inside the reach like the game's), swings when the 2D distance is within `reach` (facing snaps to the player, the body stands for `swing_ticks`), and on `hit_tick` a target still within `reach + 16` takes `damage` as DMG_SLASH through the damage hooks and has `punch` u/s added to its velocity away from the NPC; after the swing it swings again if the player is still in reach, otherwise walks up again. `NPC_MOVE_ZOMBIE` = 44.6 u/s at 93 u/s^2 (no run gait).

Fast zombie (`server_npc_melee_fastzombie`): runs from the first think (+2.95 u/think per think = 262 u/s^2, peak 21.37 u/think = 201.5 u/s, two brake steps 20.89 / 17.95), stops at 46-50 u, `Melee` (35 ticks) claws twice - at the swing's think tick and 14 ticks later, 3 DMG_SLASH each (force 900, the player moves ~4 u) - then `BR2_Roar` for 98 ticks (drifting back ~25 u, not modelled) and runs in again; double hits 161 ticks apart. Ours: `NpcMelee { hit_gap: Some(14), run: true }` with `NPC_MOVE_FASTZOMBIE`; test `fast_zombie_runs_in_and_claws_twice` lands the pairs 14 apart and 161 apart like the game; our run peaks at 18.4 u/think on this 178 u approach because the brake onset curve fires a think earlier than the game's two-step run brake (the known softer-run-brake residual).

Headcrab (`server_npc_melee_headcrab`): hull 12 x 12 x 24, eyes +16, 10 hp; on its second think it leaps - a ballistic arc at 379.5 u/s horizontal with 272 u/s up under gravity 600 (an aim point ~62 u above the player's origin), touches the player's hull 26 ticks later for 5 DMG_SLASH (force 1500, the player barely moves), hangs on the hull ~12 ticks and drops straight down; after landing it runs away at 49.5 u/s for ~168 ticks to ~150 u, turns (63 ticks) and leaps again, period 328 ticks. Ours (`npc_leap_think`, `tick_npc_flight`, `NpcLeap`): a visible player inside `NPC_LEAP_RANGE` (256 u) is leapt at - horizontal `speed` toward the player and the vertical speed that meets `aim_height` above its origin under `NPC_GRAVITY`; each tick the arc is swept against the world (a wall drops the horizontal speed, a floor lands), touching the player's hull (expanded by the crab's) lands `damage` through the damage hooks with a `punch`, pins the crab on that hull face for `hang_ticks`, then it falls; the landing starts an `npc_goto` `retreat` u straight away from the player at `NPC_MOVE_HEADCRAB` (49.5 u/s) and the next leap comes `rest_ticks` (238) later. Farther players are walked toward. Test `headcrab_leaps_hits_lands_and_leaps_again`: the hit lands 26 ticks into the leap at the hull (28 u out, 59 u up), landing 67 ticks after take-off on the floor, run-away to 180 u, leaps 308 ticks apart (game 328: our turn is instant where the game spends 63 ticks turning - recorded, not tuned).

Test `zombie_closes_in_and_claws_the_player`: steady steps 4.68 u/think, swings start at 53.3 / 54.5 / 53.4 u, three 25-hp hits at 325 u/s, the player ends pushed west. Residual: our hits come 154-161 ticks apart against the game's 196 - the launched player slides a shorter way on our ground friction, so the zombie's second approach is shorter; recorded, not tuned.
