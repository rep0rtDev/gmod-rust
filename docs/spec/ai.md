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

- every brush whose contents intersect `SOLID | MOVEABLE | WINDOW | GRATE |
  MONSTERCLIP` (the solid part of `MASK_NPCSOLID`; `PLAYERCLIP` is not solid to NPCs),
  world and brush entities alike, as outward-wound polygons from the side planes
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
  (`NPC_CORNER_REACH`); the final point is walked onto exactly (the last step is
  shortened to the remaining distance) and the goal is cleared in that same tick, so a
  400 u straight walk at 75 u/s takes exactly `ceil(400 / (75 * 0.015))` = 356 ticks.
- Step: `speed * tick` toward the corner in the XY plane; the hull is swept 18 u above
  the feet (`step_height`) with `MASK_PLAYERSOLID`, stops at the first solid, then is
  dropped onto the floor by the same 256 u box trace that places a spawned NPC. Z is
  therefore the floor's, not the mesh's (gm_construct: the ground west of spawn is 4 u
  lower and the NPC follows it).
- Yaw faces the motion; pitch/roll untouched.
- `speed` per NPC: `NPC_WALK_SPEED` = 75 u/s placeholder. **Not measured yet**: the
  game's per-class walk/run ground speeds come from the model's sequence motion; to
  be recorded on srcds (citizen / metrocop / soldier / zombie, walk and run) and put
  into `NpcDef`.

Test: `gm-dedicated/tests/npc_entity.rs::npc_walks_the_navmesh_to_a_goal` (speed cap
per tick, yaw = motion within 1 deg, feet on the traced floor within 0.5 u, exact
arrival, exact tick count).

## Not yet

Per-class speeds (above), behaviour (idle / chase / combat), crowd avoidance,
dynamic obstacles (props), off-mesh links (ladders, jumps).
