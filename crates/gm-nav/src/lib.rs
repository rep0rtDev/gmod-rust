//! Navigation mesh for NPCs: Recast builds a walkable polygon mesh from the map's
//! solid brushes and displacements, Detour answers path queries on it.
//!
//! Everything public speaks Source units and Source axes (Z up). Recast is Y up,
//! so vertices are rotated `(x, y, z) -> (x, z, -y)` on the way in and back on the
//! way out; the rotation is proper, so triangle winding (and therefore Recast's
//! up-facing test) survives it.

use glam::Vec3;
use gm_formats::bsp::lumps::{contents, surf};
use gm_formats::bsp::Bsp;
use recastnavigation_sys as rc;
use std::mem::MaybeUninit;
use std::os::raw::{c_int, c_uchar, c_ushort};

/// Agent and voxel parameters, in Source units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavParams {
    /// Horizontal voxel size.
    pub cell: f32,
    /// Vertical voxel size.
    pub cell_height: f32,
    pub agent_radius: f32,
    pub agent_height: f32,
    /// Largest ledge the agent walks over without a jump.
    pub step_height: f32,
    pub max_slope_deg: f32,
}

impl Default for NavParams {
    /// Human NPC hull (32 x 32 x 72) with Source's 18 u step and 45-degree walkable slope.
    fn default() -> Self {
        NavParams {
            cell: 8.0,
            cell_height: 4.0,
            agent_radius: 16.0,
            agent_height: 72.0,
            step_height: 18.0,
            max_slope_deg: 45.0,
        }
    }
}

/// Triangle soup in Source coordinates.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Geometry {
    pub verts: Vec<Vec3>,
    pub tris: Vec<[u32; 3]>,
    pub mins: Vec3,
    pub maxs: Vec3,
}

impl Geometry {
    pub fn push_polygon(&mut self, poly: &[Vec3]) {
        if poly.len() < 3 {
            return;
        }
        let base = self.verts.len() as u32;
        self.verts.extend_from_slice(poly);
        for i in 1..poly.len() - 1 {
            self.tris.push([base, base + i as u32, base + i as u32 + 1]);
        }
    }
}

/// Brush contents an NPC cannot walk through (the solid part of `MASK_NPCSOLID`).
const NPC_SOLID: i32 = contents::SOLID
    | contents::MOVEABLE
    | contents::WINDOW
    | contents::GRATE
    | contents::MONSTERCLIP;

/// Brushes of the world model (reachable from model 0's head node). Brush entities (doors,
/// func_brush, ...) carry their own models and are not part of the static mesh: gm_construct
/// has two 8 u NODRAW slabs through the map centre that would otherwise wall it in two.
fn world_brushes(bsp: &Bsp<'_>) -> Vec<bool> {
    let mut world = vec![false; bsp.brushes.len()];
    let Some(model) = bsp.models.first() else {
        return world;
    };
    let mut stack = vec![model.head_node];
    while let Some(node) = stack.pop() {
        if node >= 0 {
            if let Some(n) = bsp.nodes.get(node as usize) {
                stack.extend(n.children);
            }
            continue;
        }
        let Some(leaf) = bsp.leafs.get((-1 - node) as usize) else {
            continue;
        };
        let first = leaf.first_leaf_brush as usize;
        for &b in bsp
            .leaf_brushes
            .iter()
            .skip(first)
            .take(leaf.num_leaf_brushes as usize)
        {
            if let Some(w) = world.get_mut(b as usize) {
                *w = true;
            }
        }
    }
    world
}

/// Solid brush faces and displacement triangles of the whole map (world and brush
/// entities alike: doors and func_brush stand where the map put them). Sky faces are
/// left out: the skybox shell is unreachable.
pub fn world_geometry(bsp: &Bsp<'_>) -> Geometry {
    let mut g = Geometry::default();
    let mut planes = Vec::new();
    let mut sky = Vec::new();
    let world = world_brushes(bsp);
    for (bi, brush) in bsp.brushes.iter().enumerate() {
        if brush.contents & NPC_SOLID == 0 || !world.get(bi).copied().unwrap_or(false) {
            continue;
        }
        planes.clear();
        sky.clear();
        let first = brush.first_side.max(0) as usize;
        for side in bsp
            .brush_sides
            .iter()
            .skip(first)
            .take(brush.num_sides.max(0) as usize)
        {
            if let Some(p) = bsp.planes.get(side.plane_num as usize) {
                planes.push((Vec3::from(p.normal), p.dist));
                let flags = usize::try_from(side.tex_info)
                    .ok()
                    .and_then(|i| bsp.tex_infos.get(i))
                    .map_or(0, |ti| ti.flags);
                sky.push(flags & (surf::SKY | surf::SKY2D) != 0);
            }
        }
        for (poly, is_sky) in brush_polygons(&planes).into_iter().zip(&sky) {
            if !is_sky {
                g.push_polygon(&poly);
            }
        }
    }
    use gm_formats::bsp::extra::{build_disp_mesh, disp_infos, disp_verts};
    if let (Ok(infos), Ok(verts)) = (disp_infos(bsp), disp_verts(bsp)) {
        for info in &infos {
            let Ok(mesh) = build_disp_mesh(bsp, info, &verts) else {
                continue;
            };
            let base = g.verts.len() as u32;
            g.verts
                .extend(mesh.positions.iter().map(|p| Vec3::from(*p)));
            for t in mesh.indices.as_chunks::<3>().0 {
                g.tris.push([base + t[0], base + t[1], base + t[2]]);
            }
        }
    }
    g.mins = g.verts.iter().fold(Vec3::MAX, |a, v| a.min(*v));
    g.maxs = g.verts.iter().fold(Vec3::MIN, |a, v| a.max(*v));
    g
}

/// Faces of a convex brush given its side planes (`normal . p = dist`, interior on
/// the negative side), wound counter-clockwise seen from outside; one entry per plane,
/// empty for planes that do not touch the brush (bevels).
pub fn brush_polygons(planes: &[(Vec3, f32)]) -> Vec<Vec<Vec3>> {
    const HUGE: f32 = 65536.0;
    const EPS: f32 = 0.01;
    let mut out = Vec::new();
    for (i, &(n, d)) in planes.iter().enumerate() {
        let u = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
        let u = (u - n * u.dot(n)).normalize();
        let v = n.cross(u);
        let c = n * d;
        let mut poly = vec![
            c + u * HUGE + v * HUGE,
            c - u * HUGE + v * HUGE,
            c - u * HUGE - v * HUGE,
            c + u * HUGE - v * HUGE,
        ];
        for (j, &(cn, cd)) in planes.iter().enumerate() {
            if i == j || poly.is_empty() {
                continue;
            }
            let mut clipped = Vec::with_capacity(poly.len() + 1);
            for k in 0..poly.len() {
                let a = poly[k];
                let b = poly[(k + 1) % poly.len()];
                let da = cn.dot(a) - cd;
                let db = cn.dot(b) - cd;
                if da <= EPS {
                    clipped.push(a);
                }
                if (da > EPS && db < -EPS) || (da < -EPS && db > EPS) {
                    clipped.push(a + (b - a) * (da / (da - db)));
                }
            }
            poly = clipped;
        }
        poly.dedup_by(|a, b| a.distance_squared(*b) < EPS * EPS);
        if poly.len() > 2 && poly[0].distance_squared(poly[poly.len() - 1]) < EPS * EPS {
            poly.pop();
        }
        if poly.len() < 3 {
            poly.clear();
        }
        out.push(poly);
    }
    out
}

fn to_rc(v: Vec3) -> [f32; 3] {
    [v.x, v.z, -v.y]
}

fn from_rc(p: &[f32]) -> Vec3 {
    Vec3::new(p[0], -p[2], p[1])
}

/// A path along the mesh: corner points from start to end (inclusive).
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub points: Vec<Vec3>,
    /// The end was not reachable; the path leads to the closest polygon found.
    pub partial: bool,
}

pub struct NavMesh {
    mesh: *mut rc::dtNavMesh,
    query: *mut rc::dtNavMeshQuery,
    filter: Box<rc::dtQueryFilter>,
    poly_count: usize,
    /// Polygon mesh vertices in Source coordinates (for debugging and tests).
    verts: Vec<Vec3>,
    polys: Vec<NavPoly>,
    params: NavParams,
}

unsafe impl Send for NavMesh {}
unsafe impl Sync for NavMesh {}

impl Drop for NavMesh {
    fn drop(&mut self) {
        unsafe {
            rc::dtFreeNavMeshQuery(self.query);
            rc::dtFreeNavMesh(self.mesh);
        }
    }
}

struct Ctx(*mut rc::rcContext);
impl Drop for Ctx {
    fn drop(&mut self) {
        unsafe { rc::DeleteContext(self.0) }
    }
}

macro_rules! guard {
    ($name:ident, $t:ty, $alloc:ident, $free:ident) => {
        struct $name(*mut $t);
        impl $name {
            fn new() -> Result<Self, String> {
                let p = unsafe { rc::$alloc() };
                if p.is_null() {
                    Err(concat!(stringify!($alloc), " failed").to_string())
                } else {
                    Ok($name(p))
                }
            }
        }
        impl Drop for $name {
            fn drop(&mut self) {
                unsafe { rc::$free(self.0) }
            }
        }
    };
}
guard!(Hf, rc::rcHeightfield, rcAllocHeightfield, rcFreeHeightField);
guard!(
    Chf,
    rc::rcCompactHeightfield,
    rcAllocCompactHeightfield,
    rcFreeCompactHeightfield
);
guard!(Cset, rc::rcContourSet, rcAllocContourSet, rcFreeContourSet);
guard!(Pmesh, rc::rcPolyMesh, rcAllocPolyMesh, rcFreePolyMesh);
guard!(
    Dmesh,
    rc::rcPolyMeshDetail,
    rcAllocPolyMeshDetail,
    rcFreePolyMeshDetail
);

const DT_SUCCESS: u32 = 1 << 30;
const DT_PARTIAL_RESULT: u32 = 1 << 6;
const DT_TILE_FREE_DATA: c_int = 1;
const WALKABLE_FLAG: c_ushort = 1;
const MAX_POLYS: usize = 512;

fn ok(status: rc::dtStatus) -> bool {
    status & DT_SUCCESS != 0
}

/// Voxel-space agent parameters shared by every tile.
struct TileConfig {
    tile_cells: c_int,
    border: c_int,
    walkable_height: c_int,
    walkable_climb: c_int,
    walkable_radius: c_int,
}

impl TileConfig {
    fn new(p: &NavParams) -> Self {
        let walkable_radius = (p.agent_radius / p.cell).ceil() as c_int;
        TileConfig {
            tile_cells: 256,
            border: walkable_radius + 3,
            walkable_height: (p.agent_height / p.cell_height).ceil() as c_int,
            walkable_climb: (p.step_height / p.cell_height).floor() as c_int,
            walkable_radius,
        }
    }
}

type TileData = (*mut c_uchar, c_int, Vec<Vec3>, Vec<NavPoly>);

/// One polygon of the mesh: corner indices into [`NavMesh::vertices`] and, per edge, the
/// polygon across it (`None` on an outer or tile-border edge).
#[derive(Clone, Debug, PartialEq)]
pub struct NavPoly {
    pub verts: Vec<u32>,
    pub neis: Vec<Option<u32>>,
}

/// Recast pipeline for one tile: heightfield -> regions -> contours -> polygon mesh ->
/// Detour tile data. `None` when the tile has no walkable polygon.
#[allow(clippy::too_many_arguments)]
fn build_tile(
    ctx: &Ctx,
    cfg: &TileConfig,
    p: &NavParams,
    verts: &[f32],
    tris: &[c_int],
    bmin: [f32; 3],
    bmax: [f32; 3],
    tx: c_int,
    ty: c_int,
) -> Result<Option<TileData>, String> {
    let nverts = (verts.len() / 3) as c_int;
    let ntris = (tris.len() / 3) as c_int;
    let w = cfg.tile_cells + cfg.border * 2;
    let hf = Hf::new()?;
    unsafe {
        if !rc::rcCreateHeightfield(
            ctx.0,
            hf.0,
            w,
            w,
            bmin.as_ptr(),
            bmax.as_ptr(),
            p.cell,
            p.cell_height,
        ) {
            return Err("rcCreateHeightfield failed".into());
        }
        let mut areas = vec![0 as c_uchar; ntris as usize];
        rc::rcMarkWalkableTriangles(
            ctx.0,
            p.max_slope_deg,
            verts.as_ptr(),
            nverts,
            tris.as_ptr(),
            ntris,
            areas.as_mut_ptr(),
        );
        if !rc::rcRasterizeTriangles(
            ctx.0,
            verts.as_ptr(),
            nverts,
            tris.as_ptr(),
            areas.as_ptr(),
            ntris,
            hf.0,
            cfg.walkable_climb,
        ) {
            return Err("rcRasterizeTriangles failed".into());
        }
        rc::rcFilterLowHangingWalkableObstacles(ctx.0, cfg.walkable_climb, hf.0);
        rc::rcFilterLedgeSpans(ctx.0, cfg.walkable_height, cfg.walkable_climb, hf.0);
        rc::rcFilterWalkableLowHeightSpans(ctx.0, cfg.walkable_height, hf.0);
    }
    let chf = Chf::new()?;
    unsafe {
        if !rc::rcBuildCompactHeightfield(
            ctx.0,
            cfg.walkable_height,
            cfg.walkable_climb,
            hf.0,
            chf.0,
        ) {
            return Err("rcBuildCompactHeightfield failed".into());
        }
    }
    drop(hf);
    let cset = Cset::new()?;
    let pmesh = Pmesh::new()?;
    let dmesh = Dmesh::new()?;
    unsafe {
        if !rc::rcErodeWalkableArea(ctx.0, cfg.walkable_radius, chf.0) {
            return Err("rcErodeWalkableArea failed".into());
        }
        if !rc::rcBuildDistanceField(ctx.0, chf.0) {
            return Err("rcBuildDistanceField failed".into());
        }
        if !rc::rcBuildRegions(ctx.0, chf.0, cfg.border, 8 * 8, 20 * 20) {
            return Err("rcBuildRegions failed".into());
        }
        if !rc::rcBuildContours(ctx.0, chf.0, 1.3, 12, cset.0, 1) {
            return Err("rcBuildContours failed".into());
        }
        if (*cset.0).nconts == 0 {
            return Ok(None);
        }
        if !rc::rcBuildPolyMesh(ctx.0, cset.0, 6, pmesh.0) {
            return Err("rcBuildPolyMesh failed".into());
        }
        if !rc::rcBuildPolyMeshDetail(ctx.0, pmesh.0, chf.0, p.cell * 6.0, p.cell_height, dmesh.0) {
            return Err("rcBuildPolyMeshDetail failed".into());
        }
    }
    let pm = unsafe { &mut *pmesh.0 };
    let dm = unsafe { &*dmesh.0 };
    if pm.npolys == 0 {
        return Ok(None);
    }
    let mut mesh_verts = Vec::with_capacity(pm.nverts as usize);
    for i in 0..pm.nverts as usize {
        let v = unsafe { std::slice::from_raw_parts(pm.verts.add(i * 3), 3) };
        let r = [
            pm.bmin[0] + v[0] as f32 * pm.cs,
            pm.bmin[1] + v[1] as f32 * pm.ch,
            pm.bmin[2] + v[2] as f32 * pm.cs,
        ];
        mesh_verts.push(from_rc(&r));
    }
    for i in 0..pm.npolys as usize {
        unsafe {
            let walkable = *pm.areas.add(i) == rc::RC_WALKABLE_AREA as c_uchar;
            *pm.flags.add(i) = if walkable { WALKABLE_FLAG } else { 0 };
        }
    }
    let nvp = pm.nvp as usize;
    let mut polys = Vec::with_capacity(pm.npolys as usize);
    for i in 0..pm.npolys as usize {
        let q = unsafe { std::slice::from_raw_parts(pm.polys.add(i * 2 * nvp), 2 * nvp) };
        let n = q[..nvp].iter().take_while(|&&v| v != 0xffff).count();
        polys.push(NavPoly {
            verts: q[..n].iter().map(|&v| u32::from(v)).collect(),
            neis: q[nvp..nvp + n]
                .iter()
                .map(|&v| (v != 0xffff && v & 0x8000 == 0).then_some(u32::from(v)))
                .collect(),
        });
    }
    let mut cp: rc::dtNavMeshCreateParams = unsafe { MaybeUninit::zeroed().assume_init() };
    cp.verts = pm.verts;
    cp.vertCount = pm.nverts;
    cp.polys = pm.polys;
    cp.polyAreas = pm.areas;
    cp.polyFlags = pm.flags;
    cp.polyCount = pm.npolys;
    cp.nvp = pm.nvp;
    cp.detailMeshes = dm.meshes;
    cp.detailVerts = dm.verts;
    cp.detailVertsCount = dm.nverts;
    cp.detailTris = dm.tris;
    cp.detailTriCount = dm.ntris;
    cp.walkableHeight = p.agent_height;
    cp.walkableRadius = p.agent_radius;
    cp.walkableClimb = p.step_height;
    cp.tileX = tx;
    cp.tileY = ty;
    cp.tileLayer = 0;
    cp.bmin = pm.bmin;
    cp.bmax = pm.bmax;
    cp.cs = pm.cs;
    cp.ch = pm.ch;
    cp.buildBvTree = true;
    let mut data: *mut c_uchar = std::ptr::null_mut();
    let mut size: c_int = 0;
    unsafe {
        if !rc::dtCreateNavMeshData(&mut cp, &mut data, &mut size) {
            return Err("dtCreateNavMeshData failed".into());
        }
    }
    Ok(Some((data, size, mesh_verts, polys)))
}

impl NavMesh {
    pub fn build(bsp: &Bsp<'_>, params: &NavParams) -> Result<NavMesh, String> {
        Self::from_geometry(&world_geometry(bsp), params)
    }

    pub fn from_geometry(g: &Geometry, p: &NavParams) -> Result<NavMesh, String> {
        if g.tris.is_empty() {
            return Err("no triangles".into());
        }
        let verts: Vec<f32> = g.verts.iter().flat_map(|v| to_rc(*v)).collect();
        let tris: Vec<c_int> = g.tris.iter().flat_map(|t| t.map(|i| i as c_int)).collect();
        let (mut bmin, mut bmax) = ([f32::MAX; 3], [f32::MIN; 3]);
        for v in verts.as_chunks::<3>().0 {
            for k in 0..3 {
                bmin[k] = bmin[k].min(v[k]);
                bmax[k] = bmax[k].max(v[k]);
            }
        }
        let cfg = TileConfig::new(p);
        let tile_world = cfg.tile_cells as f32 * p.cell;
        let (mut gw, mut gh) = (0 as c_int, 0 as c_int);
        unsafe { rc::rcCalcGridSize(bmin.as_ptr(), bmax.as_ptr(), p.cell, &mut gw, &mut gh) };
        let tiles_x = (gw + cfg.tile_cells - 1) / cfg.tile_cells;
        let tiles_y = (gh + cfg.tile_cells - 1) / cfg.tile_cells;
        let max_tiles = ((tiles_x * tiles_y).max(1) as u32).next_power_of_two();
        let tile_bits = max_tiles.trailing_zeros().clamp(1, 14);
        let poly_bits = 22 - tile_bits;
        let mut params: rc::dtNavMeshParams = unsafe { MaybeUninit::zeroed().assume_init() };
        params.orig = bmin;
        params.tileWidth = tile_world;
        params.tileHeight = tile_world;
        params.maxTiles = 1 << tile_bits;
        params.maxPolys = 1 << poly_bits;
        let mesh = unsafe { rc::dtAllocNavMesh() };
        if mesh.is_null() {
            return Err("dtAllocNavMesh failed".into());
        }
        if !ok(unsafe { rc::dtNavMesh_init(mesh, &params) }) {
            unsafe { rc::dtFreeNavMesh(mesh) };
            return Err("dtNavMesh::init failed".into());
        }
        let ctx = Ctx(unsafe { rc::CreateContext(false) });
        let mut poly_count = 0;
        let mut mesh_verts = Vec::new();
        let mut polys: Vec<NavPoly> = Vec::new();
        let mut tile_tris: Vec<c_int> = Vec::new();
        for ty in 0..tiles_y {
            for tx in 0..tiles_x {
                let tmin = [
                    bmin[0] + tx as f32 * tile_world - cfg.border as f32 * p.cell,
                    bmin[1],
                    bmin[2] + ty as f32 * tile_world - cfg.border as f32 * p.cell,
                ];
                let tmax = [
                    bmin[0] + (tx + 1) as f32 * tile_world + cfg.border as f32 * p.cell,
                    bmax[1],
                    bmin[2] + (ty + 1) as f32 * tile_world + cfg.border as f32 * p.cell,
                ];
                tile_tris.clear();
                for t in tris.as_chunks::<3>().0 {
                    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                    for &i in t {
                        let v = &verts[i as usize * 3..i as usize * 3 + 3];
                        for k in 0..3 {
                            lo[k] = lo[k].min(v[k]);
                            hi[k] = hi[k].max(v[k]);
                        }
                    }
                    if lo[0] <= tmax[0] && hi[0] >= tmin[0] && lo[2] <= tmax[2] && hi[2] >= tmin[2]
                    {
                        tile_tris.extend_from_slice(t);
                    }
                }
                if tile_tris.is_empty() {
                    continue;
                }
                let Some((data, size, tverts, tpolys)) =
                    build_tile(&ctx, &cfg, p, &verts, &tile_tris, tmin, tmax, tx, ty)?
                else {
                    continue;
                };
                let mut tref: rc::dtTileRef = 0;
                let st = unsafe {
                    rc::dtNavMesh_addTile(mesh, data, size, DT_TILE_FREE_DATA, 0, &mut tref)
                };
                if !ok(st) {
                    unsafe {
                        rc::dtFree(data as *mut _);
                        rc::dtFreeNavMesh(mesh);
                    }
                    return Err(format!("dtNavMesh::addTile({tx}, {ty}) failed: {st:#x}"));
                }
                poly_count += tpolys.len();
                let (vbase, pbase) = (mesh_verts.len() as u32, polys.len() as u32);
                polys.extend(tpolys.into_iter().map(|q| NavPoly {
                    verts: q.verts.iter().map(|v| v + vbase).collect(),
                    neis: q.neis.iter().map(|n| n.map(|n| n + pbase)).collect(),
                }));
                mesh_verts.extend(tverts);
            }
        }
        if poly_count == 0 {
            unsafe { rc::dtFreeNavMesh(mesh) };
            return Err("empty polygon mesh".into());
        }
        let query = unsafe { rc::dtAllocNavMeshQuery() };
        if query.is_null() || !ok(unsafe { rc::dtNavMeshQuery_init(query, mesh, 4096) }) {
            unsafe {
                rc::dtFreeNavMeshQuery(query);
                rc::dtFreeNavMesh(mesh);
            }
            return Err("dtNavMeshQuery::init failed".into());
        }
        let mut filter =
            Box::new(unsafe { MaybeUninit::<rc::dtQueryFilter>::zeroed().assume_init() });
        unsafe { rc::dtQueryFilter_dtQueryFilter(&mut *filter) };
        filter.m_includeFlags = WALKABLE_FLAG;
        filter.m_excludeFlags = 0;
        Ok(NavMesh {
            mesh,
            query,
            filter,
            poly_count,
            verts: mesh_verts,
            polys,
            params: *p,
        })
    }

    pub fn poly_count(&self) -> usize {
        self.poly_count
    }

    pub fn params(&self) -> &NavParams {
        &self.params
    }

    /// Polygon mesh vertices (Source coordinates).
    /// The polygons of the mesh (the script `CNavArea`s).
    pub fn polys(&self) -> &[NavPoly] {
        &self.polys
    }

    pub fn vertices(&self) -> &[Vec3] {
        &self.verts
    }

    fn nearest_poly(&self, pos: Vec3, extents: Vec3) -> Option<(rc::dtPolyRef, Vec3)> {
        let center = to_rc(pos);
        let half = [extents.x, extents.z, extents.y];
        let mut r: rc::dtPolyRef = 0;
        let mut pt = [0f32; 3];
        let st = unsafe {
            rc::dtNavMeshQuery_findNearestPoly(
                self.query,
                center.as_ptr(),
                half.as_ptr(),
                &*self.filter,
                &mut r,
                pt.as_mut_ptr(),
            )
        };
        (ok(st) && r != 0).then(|| (r, from_rc(&pt)))
    }

    /// Closest point on the mesh within `extents` of `pos`.
    pub fn nearest(&self, pos: Vec3, extents: Vec3) -> Option<Vec3> {
        self.nearest_poly(pos, extents).map(|(_, p)| p)
    }

    /// Corner path from `start` to `end` (both snapped to the mesh within `extents`).
    pub fn find_path(&self, start: Vec3, end: Vec3, extents: Vec3) -> Option<Path> {
        let (sref, spos) = self.nearest_poly(start, extents)?;
        let (eref, epos) = self.nearest_poly(end, extents)?;
        let (s, e) = (to_rc(spos), to_rc(epos));
        let mut polys = vec![0 as rc::dtPolyRef; MAX_POLYS];
        let mut npolys: c_int = 0;
        let st = unsafe {
            rc::dtNavMeshQuery_findPath(
                self.query,
                sref,
                eref,
                s.as_ptr(),
                e.as_ptr(),
                &*self.filter,
                polys.as_mut_ptr(),
                &mut npolys,
                MAX_POLYS as c_int,
            )
        };
        if !ok(st) || npolys == 0 {
            return None;
        }
        let partial = st & DT_PARTIAL_RESULT != 0;
        // When the end is not reachable, finish on the last polygon's closest point.
        let mut target = e;
        if partial {
            let mut closest = [0f32; 3];
            let mut over = false;
            unsafe {
                rc::dtNavMeshQuery_closestPointOnPoly(
                    self.query,
                    polys[npolys as usize - 1],
                    e.as_ptr(),
                    closest.as_mut_ptr(),
                    &mut over,
                );
            }
            target = closest;
        }
        let mut pts = vec![0f32; MAX_POLYS * 3];
        let mut flags = vec![0 as c_uchar; MAX_POLYS];
        let mut refs = vec![0 as rc::dtPolyRef; MAX_POLYS];
        let mut n: c_int = 0;
        let st = unsafe {
            rc::dtNavMeshQuery_findStraightPath(
                self.query,
                s.as_ptr(),
                target.as_ptr(),
                polys.as_ptr(),
                npolys,
                pts.as_mut_ptr(),
                flags.as_mut_ptr(),
                refs.as_mut_ptr(),
                &mut n,
                MAX_POLYS as c_int,
                0,
            )
        };
        if !ok(st) || n == 0 {
            return None;
        }
        let points = pts[..n as usize * 3]
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| from_rc(p))
            .collect();
        Some(Path { points, partial })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_planes(mins: Vec3, maxs: Vec3) -> Vec<(Vec3, f32)> {
        vec![
            (Vec3::X, maxs.x),
            (Vec3::NEG_X, -mins.x),
            (Vec3::Y, maxs.y),
            (Vec3::NEG_Y, -mins.y),
            (Vec3::Z, maxs.z),
            (Vec3::NEG_Z, -mins.z),
        ]
    }

    fn room(walls: &[(Vec3, Vec3)]) -> Geometry {
        let mut g = Geometry {
            mins: Vec3::new(-600.0, -600.0, -100.0),
            maxs: Vec3::new(600.0, 600.0, 300.0),
            ..Default::default()
        };
        for p in brush_polygons(&box_planes(
            Vec3::new(-512.0, -512.0, -64.0),
            Vec3::new(512.0, 512.0, 0.0),
        )) {
            g.push_polygon(&p);
        }
        for (mins, maxs) in walls {
            for p in brush_polygons(&box_planes(*mins, *maxs)) {
                g.push_polygon(&p);
            }
        }
        g
    }

    #[test]
    fn box_brush_has_six_quads_facing_out() {
        let polys = brush_polygons(&box_planes(Vec3::ZERO, Vec3::splat(64.0)));
        assert_eq!(polys.len(), 6);
        let mut bevel = box_planes(Vec3::ZERO, Vec3::splat(64.0));
        bevel.push((Vec3::new(1.0, 1.0, 0.0).normalize(), 128.0 / 2f32.sqrt()));
        assert!(brush_polygons(&bevel)[6].is_empty());
        for p in &polys {
            assert_eq!(p.len(), 4, "{p:?}");
            let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize();
            let c = p.iter().sum::<Vec3>() / 4.0;
            assert!(
                n.dot(c - Vec3::splat(32.0)) > 0.99 * 32.0,
                "normal {n} at {c}"
            );
        }
    }

    #[test]
    fn open_floor_is_one_walkable_mesh() {
        let nav = NavMesh::from_geometry(&room(&[]), &NavParams::default()).unwrap();
        assert!(nav.poly_count() > 0);
        let path = nav
            .find_path(
                Vec3::new(-400.0, -400.0, 2.0),
                Vec3::new(400.0, 400.0, 2.0),
                Vec3::splat(32.0),
            )
            .unwrap();
        assert!(!path.partial);
        assert_eq!(path.points.len(), 2, "{:?}", path.points);
        // Heights come from the voxel grid: within one cell height of the floor.
        let end = path.points[1];
        assert!(
            end.truncate().distance(glam::Vec2::new(400.0, 400.0)) < 1.0,
            "{end}"
        );
        assert!((0.0..=4.0).contains(&end.z), "{end}");
    }

    #[test]
    fn path_goes_around_a_wall_and_keeps_the_agent_radius() {
        let wall = (Vec3::new(-8.0, -300.0, 0.0), Vec3::new(8.0, 300.0, 96.0));
        let nav = NavMesh::from_geometry(&room(&[wall]), &NavParams::default()).unwrap();
        let path = nav
            .find_path(
                Vec3::new(-200.0, 0.0, 2.0),
                Vec3::new(200.0, 0.0, 2.0),
                Vec3::splat(32.0),
            )
            .unwrap();
        assert!(!path.partial);
        assert!(path.points.len() >= 3, "{:?}", path.points);
        let corner = path
            .points
            .iter()
            .find(|p| p.x.abs() < 24.0)
            .expect("corner near the wall");
        assert!(
            corner.y.abs() > 300.0 + 16.0 - 8.5,
            "corner {corner} inside the agent radius"
        );
    }

    #[test]
    fn step_vs_ledge() {
        let step = (Vec3::new(0.0, -512.0, 0.0), Vec3::new(512.0, 512.0, 16.0));
        let nav = NavMesh::from_geometry(&room(&[step]), &NavParams::default()).unwrap();
        let p = nav
            .find_path(
                Vec3::new(-300.0, 0.0, 2.0),
                Vec3::new(300.0, 0.0, 18.0),
                Vec3::splat(32.0),
            )
            .unwrap();
        assert!(!p.partial, "16 u step must be walkable: {:?}", p.points);
        let ledge = (Vec3::new(0.0, -512.0, 0.0), Vec3::new(512.0, 512.0, 40.0));
        let nav = NavMesh::from_geometry(&room(&[ledge]), &NavParams::default()).unwrap();
        let p = nav
            .find_path(
                Vec3::new(-300.0, 0.0, 2.0),
                Vec3::new(300.0, 0.0, 42.0),
                Vec3::splat(32.0),
            )
            .unwrap();
        assert!(p.partial, "40 u ledge must not be walkable: {:?}", p.points);
    }
}
