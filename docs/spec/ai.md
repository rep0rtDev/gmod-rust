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
- Gait (`NpcMoveDef`, `npc_move_def(class)`), measured on gm_construct over a 400 u
  straight walk (one think = 0.105 s):

  | class | walk u/s | walk accel u/s^2 | run u/s | run accel u/s^2 |
  |---|---|---|---|---|
  | npc_citizen | 79.92 | 129.9 | 209.79 | 259.8 |
  | npc_metropolice | 57.69 | 115.4 | 184.56 | 369.2 |

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
`_citizen_p11`..`_p15` (goal tick phase), `_citizen_turn90` and `_citizen_redirect`
(golden 261102, one scenario per srcds session, 1-tick waits): all eleven equal at 2 u.
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
In `_corner_seen` the probe's `spawnnpc` yaw only turned the bot's eyes, so the game's NPC
spawned facing the bot's body (yaw 180) while ours faces the eye yaw: a probe artifact.
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

Gaits of the other classes (zombie, soldier, ...), idle / combat behaviour, crowd
avoidance, dynamic obstacles (props), off-mesh links (ladders, jumps), addon API for
`npc_goto` / `npc_chase`.
