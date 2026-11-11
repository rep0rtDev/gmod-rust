//! Static world geometry: BSP faces and displacements, grouped by material.

pub use crate::props::EnvSun;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec3};
use gm_content::Mount;
use gm_formats::bsp::extra::{build_disp_mesh, disp_infos, disp_verts, DispInfo};
use gm_formats::bsp::lumps::{surf, Face};
use gm_formats::bsp::Bsp;
use gm_formats::vtf;
use wgpu::util::DeviceExt;

use crate::gpu::Gpu;
use crate::lightmap::{Atlas, Placement};
use crate::material;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub lm_uv: [f32; 2],
    /// Multiplier on top of the lightmap: 1 for world faces, baked vertex light for props.
    pub color: [f32; 3],
    /// `WorldVertexTransition` blend towards `$basetexture2` (displacement vertex alpha / 255).
    pub alpha: f32,
    /// Bumped faces: atlas u stride between the four lightmap pages (0 when unbumped).
    pub lm_page: f32,
    /// World-space normal for the `$envmap` reflection (zero when unused).
    pub normal: [f32; 3],
    /// Layer of the nearest `env_cubemap` sample in the cube array, -1 for none.
    pub cube: f32,
    /// `$phong` on models: direction towards and colour of the brightest light at the model's
    /// origin (zero direction: the sun).
    pub spec_dir: [f32; 3],
    pub spec_color: [f32; 3],
    /// Models: the player's colour (`$color2` through the PlayerColor proxy), 1 otherwise, and
    /// the dissolve progress (0 = none).
    pub tint: [f32; 4],
}

/// `MaterialParams::unlit` of an unlit material fogged to the fog colour.
const FOG_NORMAL: u32 = 1;
/// ... fogged to black: `$additive` sprites and beams, which Source fogs by fading them out.
const FOG_TO_BLACK: u32 = 2;
/// ... fogged to 0.5 grey: `DecalModulate`, whose neutral (no change) value that is.
const FOG_TO_GRAY: u32 = 3;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MaterialParams {
    alpha_test: u32,
    /// `UnlitGeneric`: the base texture is shown as is, no lightmap (1); 2 / 3: also fogged
    /// to black / 0.5 grey instead of the fog colour (`FOG_TO_BLACK`, `FOG_TO_GRAY`).
    unlit: u32,
    /// `$translucent`: the base alpha is the output alpha.
    translucent: u32,
    /// `WorldVertexTransition`: `$basetexture2` is bound and blended by the vertex alpha.
    blend: u32,
    /// `$detail`: scale, blend factor (0 = none), blend mode, 0.
    detail: [f32; 4],
    /// `$basetexturetransform2` 2x2 (row-major).
    uv2_m: [f32; 4],
    /// `$basetexturetransform2` translation, whether `$blendmodulatetexture` is bound, 0.
    uv2_t: [f32; 4],
    /// `Water` `$fogcolor` (display space) and 1 when set.
    water_color: [f32; 4],
    /// `Water` `$fogstart`, `$fogend`, 0, 0.
    water_range: [f32; 4],
    /// `$envmap`: `$envmaptint` and the mask mode (0 off, 1 none, 2 base alpha, 3 normal alpha).
    envmap: [f32; 4],
    /// `$envmapcontrast`, `$envmapsaturation`, `$selfillum` on.
    envmap2: [f32; 4],
    /// `$envmapfresnelminmaxexp` (xyz) and `$envmapfresnel` on (w).
    envmap3: [f32; 4],
    /// `$selfillumtint` (rgb) and `$envmapmaskscale` (w).
    selfillum_tint: [f32; 4],
    /// `$phong` on, `$phongexponent`, `$phongboost`, `$phongalbedotint`.
    phong: [f32; 4],
    /// `$phongfresnelranges` (xyz).
    phong_fresnel: [f32; 4],
}

/// `LightmappedReflective` that is not water: `func_reflective_glass`, a mirror.
fn is_mirror(mat: &material::Material) -> bool {
    mat.shader.eq_ignore_ascii_case("lightmappedreflective") && !mat.water
}

/// A mirror face: its plane (`normal . p = d`, the normal towards the viewer) and bounds.
#[derive(Clone, Copy, Debug)]
struct Mirror {
    normal: Vec3,
    d: f32,
    min: Vec3,
    max: Vec3,
}

fn envmap_params(mat: &material::Material) -> [f32; 4] {
    // water reuses the slot: reflect / refract amounts and the normal-map scroll
    if mat.water {
        return mat.water_dist;
    }
    match mat.envmap {
        None => [0.0; 4],
        Some(e) => {
            let mode = match e.mask {
                material::EnvMapMask::None => 1.0,
                material::EnvMapMask::BaseAlpha => 2.0,
                material::EnvMapMask::NormalAlpha => 3.0,
                material::EnvMapMask::Texture => 4.0,
            };
            [e.tint[0], e.tint[1], e.tint[2], mode]
        }
    }
}

fn envmap2_params(mat: &material::Material) -> [f32; 4] {
    let (c, s) = mat
        .envmap
        .map_or((0.0, 1.0), |e| (e.contrast, e.saturation));
    [c, s, if mat.selfillum { 1.0 } else { 0.0 }, 0.0]
}

/// `$phong`: on, `$phongexponent`, `$phongboost`, `$phongalbedotint`.
fn phong_params(mat: &material::Material) -> [f32; 4] {
    match &mat.phong {
        // x: 2 with a `$phongexponenttexture` (the shader reads the exponent from it)
        Some(p) => [
            if p.exponent_texture.is_some() {
                2.0
            } else {
                1.0
            },
            p.exponent,
            p.boost,
            p.albedo_tint,
        ],
        None => [0.0; 4],
    }
}

/// `$phongfresnelranges` (xyz).
fn phong_fresnel_params(mat: &material::Material) -> [f32; 4] {
    match &mat.phong {
        Some(p) => [p.fresnel[0], p.fresnel[1], p.fresnel[2], 0.0],
        None => [0.0, 0.5, 1.0, 0.0],
    }
}

fn envmap3_params(mat: &material::Material) -> [f32; 4] {
    match mat.envmap.and_then(|e| e.fresnel) {
        Some(f) => [f[0], f[1], f[2], 1.0],
        None => [0.0; 4],
    }
}

fn selfillum_tint(mat: &material::Material, weapon_color: Vec3) -> [f32; 4] {
    let t = if mat.weapon_color_tint {
        weapon_color.to_array()
    } else {
        mat.selfillum_tint
    };
    // w: `$envmapmaskscale` (the mask texture's uv scale)
    [t[0], t[1], t[2], mat.envmapmask_scale.max(1e-3)]
}

/// Nearest baked `env_cubemap` sample: (origin, layer in the cube array).
pub struct CubeSet {
    samples: Vec<(Vec3, f32)>,
}

impl CubeSet {
    /// The sample named by a vbsp-patched material (`..._<x>_<y>_<z>`), if loaded.
    pub fn from_patched_name(&self, name: &str) -> Option<f32> {
        let mut it = name.rsplit('_');
        let z: i32 = it.next()?.parse().ok()?;
        let y: i32 = it.next()?.parse().ok()?;
        let x: i32 = it.next()?.parse().ok()?;
        let o = Vec3::new(x as f32, y as f32, z as f32);
        self.samples.iter().find(|s| s.0 == o).map(|s| s.1)
    }

    pub fn nearest(&self, p: Vec3) -> f32 {
        self.samples
            .iter()
            .min_by(|a, b| a.0.distance_squared(p).total_cmp(&b.0.distance_squared(p)))
            .map_or(-1.0, |s| s.1)
    }
}

const CUBE_SIDE: u32 = 32;

/// The map's `LUMP_CUBEMAPS` samples from their pakfile VTFs (`c<x>_<y>_<z>.vtf`, the LDR
/// faces in D3D order +x -x +y -y +z -z, sampled with the world vector) as one cube array
/// at the 32-texel mip.
fn load_cubemaps(
    gpu: &Gpu,
    mount: &Mount,
    bsp: &Bsp<'_>,
    hdr: bool,
) -> (CubeSet, wgpu::TextureView) {
    // HDR: the `.hdr.vtf` twins are RGBA16161616F - raw little-endian halves of linear
    // light, uploaded as is (the HDR lightmap atlas holds linear light the same way)
    let (format, bpp) = if hdr {
        (wgpu::TextureFormat::Rgba16Float, 8u32)
    } else {
        (wgpu::TextureFormat::Rgba8UnormSrgb, 4u32)
    };
    let files: HashMap<String, String> = mount
        .find("materials/maps/**/*.vtf")
        .into_iter()
        .filter_map(|p| Some((p.rsplit('/').next()?.to_string(), p.clone())))
        .collect();
    let mut samples = Vec::new();
    let mut pixels = Vec::new();
    for c in gm_formats::bsp::extra::cubemaps(bsp).unwrap_or_default() {
        let ext = if hdr { "hdr.vtf" } else { "vtf" };
        let name = format!("c{}_{}_{}.{ext}", c.origin[0], c.origin[1], c.origin[2]);
        let Some(bytes) = files.get(&name).and_then(|p| mount.read(p).ok()) else {
            continue;
        };
        let Ok(h) = vtf::Header::parse(&bytes) else {
            continue;
        };
        let Some(mip) =
            (0..h.mipmap_count as u32).find(|&m| vtf::decode::mip_dims(&h, m).0 == CUBE_SIDE)
        else {
            continue;
        };
        let faces: Option<Vec<Vec<u8>>> = (0..6)
            .map(|f| {
                let slice = vtf::decode::mip_slice(&h, &bytes, mip, 0, f, 0).ok()?;
                if hdr {
                    (h.high_res_format == vtf::ImageFormat::Rgba16161616F
                        && slice.len() == (CUBE_SIDE * CUBE_SIDE * 8) as usize)
                        .then(|| slice.to_vec())
                } else {
                    vtf::decode::decode_rgba8(h.high_res_format, CUBE_SIDE, CUBE_SIDE, slice).ok()
                }
            })
            .collect();
        let Some(faces) = faces else { continue };
        let layer = samples.len() as f32;
        samples.push((
            Vec3::new(c.origin[0] as f32, c.origin[1] as f32, c.origin[2] as f32),
            layer,
        ));
        pixels.extend(faces.into_iter().flatten());
    }
    let cubes = samples.len().max(1) as u32;
    if samples.is_empty() {
        pixels = vec![0; (CUBE_SIDE * CUBE_SIDE * bpp * 6) as usize];
    }
    let size = wgpu::Extent3d {
        width: CUBE_SIDE,
        height: CUBE_SIDE,
        depth_or_array_layers: cubes * 6,
    };
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("env_cubemaps"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(CUBE_SIDE * bpp),
            rows_per_image: Some(CUBE_SIDE),
        },
        size,
    );
    let view = tex.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::CubeArray),
        ..Default::default()
    });
    eprintln!(
        "env_cubemap: {} samples in the cube array ({})",
        samples.len(),
        if hdr { "hdr" } else { "ldr" }
    );
    (CubeSet { samples }, view)
}

fn uv2_params(mat: &material::Material, modulate: bool, bump: bool) -> ([f32; 4], [f32; 4]) {
    let t = mat
        .base2_transform
        .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    (
        [t[0], t[1], t[2], t[3]],
        // bump: 0 none, 1 normal map, 2 self-shadowed (`$ssbump`) basis weights
        [
            t[4],
            t[5],
            modulate as u32 as f32,
            if bump && mat.ssbump {
                2.0
            } else {
                bump as u32 as f32
            },
        ],
    )
}

fn detail_params(mat: &material::Material) -> [f32; 4] {
    let blend_tint = f32::from(mat.blend_tint_by_base_alpha);
    match &mat.detail {
        Some(d) if d.mode == 0 => [d.scale, d.factor, 0.0, blend_tint],
        _ => [0.0, 0.0, 0.0, blend_tint],
    }
}

fn is_unlit(shader: &str) -> bool {
    shader.eq_ignore_ascii_case("unlitgeneric")
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    /// Output alpha of every fragment (1 opaque; `ModelTarget::alpha` for translucent models).
    alpha: f32,
    /// Monitor gamma (`mat_monitorgamma`): the output is raised to 2.2 / gamma.
    gamma: f32,
    /// Lightmap atlas scale (the atlas stores linear light / lm_scale).
    lm_scale: f32,
    /// Tonemap scale (HDR exposure; 1 in LDR).
    exposure: f32,
    /// Eye position (xyz) and whether fog is on (w).
    eye: [f32; 4],
    /// View forward (xyz): fog depth is the distance along it.
    forward: [f32; 4],
    /// Fog colour in display space (rgb) and `fogmaxdensity` (w).
    fog_color: [f32; 4],
    /// `fogstart`, `fogend` in units.
    fog_range: [f32; 4],
    /// Water reflection pass: clip plane z (x), bit 1 reflection / bit 2 refraction texture
    /// valid (y), clip on (w).
    /// ... and seconds since start (z) for scrolling water normals.
    clip: [f32; 4],
    /// The reflection pass's clip plane: normal and distance.
    clip_plane: [f32; 4],
    /// Dynamic lights: position (xyz) and radius in units (w, 0 = none).
    dlight_pos: [[f32; 4]; MAX_DLIGHTS],
    /// Their linear colours (rgb).
    dlight_color: [[f32; 4]; MAX_DLIGHTS],
    /// Spot direction (xyz) and the cosine of the cone half-angle (w, 0 = a point light).
    dlight_dir: [[f32; 4]; MAX_DLIGHTS],
    /// Direction towards the sun (`light_environment`), for `$phong`.
    sun: [f32; 4],
    /// `SkyPaint` (zero when none): top colour + fade bias, bottom colour + HDR scale, sun
    /// normal + size, sun colour + 1 (set), dusk colour + intensity, dusk scale.
    sky_top: [f32; 4],
    sky_bottom: [f32; 4],
    sky_sun: [f32; 4],
    sky_sun_color: [f32; 4],
    sky_dusk: [f32; 4],
    sky_dusk_scale: [f32; 4],
    /// The view-projection of each flashlight shadow map, and per dynamic light the shadow
    /// map that shades it (-1: none).
    flashlight_vp: [[[f32; 4]; 4]; DEPTH_LAYERS],
    shadow_slot: [[f32; 4]; 2],
    /// Entity sun shadows (render.md 11): on, `r_shadowdist` as a depth-map fraction, the
    /// shadow's opacity, 0.
    sun_shadow: [f32; 4],
}

/// Dynamic lights lit per frame.
pub const MAX_DLIGHTS: usize = 32;

/// A dynamic point light for this frame (the physgun beam's end, a muzzle flash): colour times
/// `(1 - d / radius)^2` times the surface's facing, added to the baked light.
/// Side of a flashlight's shadow map, and how many flashlights (the local player's first)
/// get one.
/// `r_flashlightdepthres` 1024 (render.md 3.4, gap-report #138).
const SHADOW_SIZE: u32 = 1024;
pub const SHADOW_LAYERS: usize = 4;
/// The last depth layer holds the entity shadows: the dynamic models seen from the
/// `light_environment` direction (the client shadow manager's render-to-texture shadows,
/// `r_shadowrendertotexture 1` in GMod), projected onto the world within `r_shadowdist`.
const SUN_LAYER: usize = SHADOW_LAYERS;
const DEPTH_LAYERS: usize = SHADOW_LAYERS + 1;
/// Side of the orthographic sun shadow map around the camera (units) and its depth range.
const SUN_SHADOW_EXTENT: f32 = 1024.0;
const SUN_SHADOW_RANGE: f32 = 4096.0;
/// `r_shadowdist`: how far below the caster the shadow reaches (SDK default, 50 u).
const SUN_SHADOW_DIST: f32 = 50.0;
/// Opacity of the shadow on the surface (an approximation: the game's shadow decal
/// alpha was not resolved from the binary).
const SUN_SHADOW_ALPHA: f32 = 0.6;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DLight {
    pub pos: Vec3,
    pub radius: f32,
    pub color: Vec3,
    /// Spot light: its direction and the cosine of the cone half-angle (0 = point light).
    pub dir: Vec3,
    pub cone: f32,
}

/// `cl_weaponcolor` default: the physgun's glow and beam.
pub const DEFAULT_WEAPON_COLOR: Vec3 = Vec3::new(0.3, 1.8, 2.1);

/// Linear distance fog of an `env_fog_controller` (the world) or a `sky_camera` (its 3D
/// skybox, in skybox units): the fraction `(depth - start) / (end - start)` of the fog
/// colour, clamped to `max_density`, where depth is the view-space distance.
/// A water volume (`LUMP_LEAFWATERDATA` + its `CONTENTS_WATER` leaves): an eye inside it
/// sees the `$bottommaterial`'s fog instead of the map's.
/// `env_skypaint`'s painted sky: the colours at the zenith and the horizon faded by
/// `pow(up, fade_bias)`, a dusk band along the horizon and a sun glow around `sun_normal`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyPaint {
    pub top: Vec3,
    pub bottom: Vec3,
    pub fade_bias: f32,
    pub hdr_scale: f32,
    pub sun_normal: Vec3,
    pub sun_size: f32,
    pub sun_color: Vec3,
    pub dusk_scale: f32,
    pub dusk_intensity: f32,
    pub dusk_color: Vec3,
    /// Stars: tiling scale, fade towards the horizon, layers (0 = none), drift so far.
    pub star_scale: f32,
    pub star_fade: f32,
    pub star_layers: f32,
    pub star_drift: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct WaterVolume {
    pub min: Vec3,
    pub max: Vec3,
    pub surface_z: f32,
    pub fog: Option<Fog>,
}

fn water_volumes(bsp: &Bsp<'_>, mount: &Mount) -> Vec<WaterVolume> {
    let data = gm_formats::bsp::extra::leaf_water_data(bsp).unwrap_or_default();
    let mut boxes: HashMap<usize, (Vec3, Vec3)> = HashMap::new();
    for leaf in &bsp.leafs {
        if leaf.contents & 0x20 == 0 || leaf.leaf_water_data_id < 0 {
            continue;
        }
        let lo = Vec3::new(
            leaf.mins[0] as f32,
            leaf.mins[1] as f32,
            leaf.mins[2] as f32,
        );
        let hi = Vec3::new(
            leaf.maxs[0] as f32,
            leaf.maxs[1] as f32,
            leaf.maxs[2] as f32,
        );
        let e = boxes
            .entry(leaf.leaf_water_data_id as usize)
            .or_insert((lo, hi));
        e.0 = e.0.min(lo);
        e.1 = e.1.max(hi);
    }
    let mut out = Vec::new();
    for (id, (min, max)) in boxes {
        let Some(d) = data.get(id) else { continue };
        let name = bsp
            .tex_infos
            .get(d.surface_tex_info as usize)
            .and_then(|ti| bsp.tex_datas.get(usize::try_from(ti.tex_data).ok()?))
            .and_then(|td| {
                bsp.tex_names
                    .get(usize::try_from(td.name_string_table_id).ok()?)
            });
        let fog = name
            .and_then(|n| material::load(mount, &material::canonical_name(n)).ok())
            .and_then(|m| m.bottom_material)
            .and_then(|b| material::load(mount, &material::canonical_name(&b)).ok())
            .and_then(|m| m.water_fog)
            .map(|(color, start, end)| Fog {
                color: color.map(material::srgb_to_linear),
                start,
                end,
                max_density: 1.0,
            });
        out.push(WaterVolume {
            min,
            max,
            surface_z: d.surface_z,
            fog,
        });
    }
    out
}

#[derive(Clone, Copy, Debug)]
pub struct Fog {
    pub color: [f32; 3],
    pub start: f32,
    pub end: f32,
    pub max_density: f32,
}

impl Fog {
    fn from_entity(e: &gm_formats::bsp::lumps::Entity) -> Option<Fog> {
        if e.get("fogenable").map(|v| v.trim()) != Some("1") {
            return None;
        }
        let f = |k: &str, d: f32| {
            e.get(k)
                .and_then(|v| v.trim().parse::<f32>().ok())
                .unwrap_or(d)
        };
        let c: Vec<f32> = e
            .get("fogcolor")
            .map(|v| {
                v.split_whitespace()
                    .filter_map(|x| x.parse::<f32>().ok())
                    .collect()
            })
            .unwrap_or_default();
        let color = if c.len() >= 3 {
            [c[0] / 255.0, c[1] / 255.0, c[2] / 255.0].map(material::srgb_to_linear)
        } else {
            [0.0; 3]
        };
        Some(Fog {
            color,
            start: f("fogstart", 0.0),
            end: f("fogend", 0.0),
            max_density: f("fogmaxdensity", 1.0),
        })
    }
}

/// Renderer quality from the client's Advanced Video settings.
#[derive(Clone, Copy, Debug)]
pub struct Quality {
    /// Use the HDR lighting lumps when the map has them (`mat_hdr_level` >= 1).
    pub hdr: bool,
    /// `mat_picmip`: top mips skipped on world textures (<= 0 skips none).
    pub picmip: i32,
    /// `mat_trilinear`.
    pub trilinear: bool,
    /// `mat_forceaniso` 1..16.
    pub aniso: u32,
    /// `r_rootlod`: the lowest model LOD drawn (Model Detail High 0 / Medium 1 / Low 2).
    pub root_lod: u32,
    /// `mat_antialias` MSAA samples (1 / 2 / 4 / 8, already clamped to what the adapter does).
    pub msaa: u32,
}

impl Default for Quality {
    fn default() -> Self {
        Self {
            hdr: false,
            picmip: 0,
            trilinear: false,
            aniso: 1,
            root_lod: 0,
            msaa: 1,
        }
    }
}

/// Source camera: position in units, angles in degrees (pitch, yaw, roll), horizontal FOV.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub angles: Vec3,
    pub fov_x: f32,
    pub near: f32,
    pub far: f32,
    /// `mat_monitorgamma` 1.6..2.6, 2.2 = neutral.
    pub gamma: f32,
}

impl Camera {
    pub fn forward(&self) -> Vec3 {
        let (p, y) = (self.angles.x.to_radians(), self.angles.y.to_radians());
        Vec3::new(p.cos() * y.cos(), p.cos() * y.sin(), -p.sin())
    }

    /// The view's up vector (`AngleVectors` up, with the roll).
    pub fn up(&self) -> Vec3 {
        let (p, y, r) = (
            self.angles.x.to_radians(),
            self.angles.y.to_radians(),
            self.angles.z.to_radians(),
        );
        Vec3::new(
            r.cos() * p.sin() * y.cos() + r.sin() * y.sin(),
            r.cos() * p.sin() * y.sin() - r.sin() * y.cos(),
            r.cos() * p.cos(),
        )
    }

    /// Sprite axes of a camera-facing (`vp_parallel`) quad (clientfx.md §10, gap-report
    /// #135): `right = forward x up_view`, `up = right x forward`, never degenerate.
    pub fn sprite_axes(&self) -> (Vec3, Vec3) {
        let fwd = self.forward();
        let mut right = fwd.cross(self.up()).normalize_or_zero();
        if right == Vec3::ZERO {
            right = Vec3::X;
        }
        let up = right.cross(fwd).normalize_or_zero();
        (right, up)
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_to_rh(self.position, self.forward(), Vec3::Z);
        // Source fov is horizontal at 4:3; convert to vertical for the given aspect
        let half_x = (self.fov_x.to_radians() / 2.0).tan();
        let fov_y = 2.0 * (half_x / aspect).atan();
        let proj = Mat4::perspective_rh(fov_y, aspect, self.near, self.far);
        proj * view
    }
}

struct Batch {
    bind_group: wgpu::BindGroup,
    first_index: u32,
    index_count: u32,
    material: String,
    sky: bool,
    /// `$translucent`: blended after the opaque batches, back to front by `centre`.
    translucent: bool,
    /// `$nocull`: drawn with the two-sided pipelines.
    nocull: bool,
    /// `Water`: drawn in the water pass over the scene depth.
    water: bool,
    /// `$abovewater 0`: only while the eye is below the surface (`centre.z`).
    below_water: bool,
    centre: Vec3,
}

/// `sky_camera` entity: the 3D skybox is a small copy of the world rendered first,
/// with the eye moved to `origin + eye / scale`.
#[derive(Clone, Copy, Debug)]
pub struct SkyCamera {
    pub origin: Vec3,
    pub scale: f32,
}

/// A lightmapped face whose light styles can change at runtime.
struct LmFace {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    bumped: bool,
    styles: Vec<u8>,
    /// Linear luxels per style (`nb * w * h`, the bump pages one after another).
    blocks: Vec<Vec<[f32; 3]>>,
}

/// The styles' luxel blocks summed at their brightness.
fn sum_styles(blocks: &[Vec<[f32; 3]>], styles: &[u8], scale: &[f32]) -> Vec<[f32; 3]> {
    let n = blocks.first().map_or(0, Vec::len);
    let mut out = vec![[0f32; 3]; n];
    for (block, &style) in blocks.iter().zip(styles) {
        let k = scale[style as usize];
        for (dst, src) in out.iter_mut().zip(block) {
            for c in 0..3 {
                dst[c] += src[c] * k;
            }
        }
    }
    out
}

/// The atlas layout of a face's summed luxels: the flat page, or a bumped face's four pages
/// side by side (page 0 first).
fn lm_strip(samples: &[[f32; 3]], w: u32, h: u32, bumped: bool) -> Vec<[f32; 3]> {
    let page = (w * h) as usize;
    if !bumped {
        return samples[..page].to_vec();
    }
    let mut strip = Vec::with_capacity(4 * page);
    for row in 0..h as usize {
        for k in 0..4 {
            let o = k * page + row * w as usize;
            strip.extend_from_slice(&samples[o..o + w as usize]);
        }
    }
    strip
}

pub struct WorldRenderer {
    /// Linear light / `lm_scale` is what the lightmap atlas stores.
    pub lm_scale: f32,
    /// HDR tonemap scale applied in the shader (1 in LDR).
    pub exposure: f32,
    /// Whether the HDR lumps were used.
    pub hdr: bool,
    /// Direction towards the sun (`light_environment`), the `$phong` highlight's light.
    pub sun: Vec3,
    /// The map's `env_sun` glow, if any.
    pub env_sun: Option<EnvSun>,
    pub quality: Quality,
    /// Auto-exposure bounds and rate of the map's env_tonemap_controller.
    pub tm_min: f32,
    pub tm_max: f32,
    pub tm_rate: f32,
    /// The controller's `SetBloomScale` (bloom strength added in the post pass).
    pub tm_bloom: f32,
    /// The bloom amount actually applied: blends toward `tm_bloom` at `mat_bloomamount_rate`
    /// (0.05/s, render.md §5 M; gap-report #149).
    pub bloom_amount: f32,
    /// The local player's `cl_weaponcolor`: `$selfillumtint` of materials with the
    /// PlayerWeaponColor proxy (set before the physgun's model loads).
    pub weapon_color: Vec3,
    /// This frame's dynamic lights (the first four count).
    dlights: Vec<DLight>,
    pipeline: wgpu::RenderPipeline,
    /// The 2D sky cube: no back-face culling (the eye is inside it).
    sky_pipeline: wgpu::RenderPipeline,
    /// The 2D sky as an `env_skypaint` gradient (`fs_skypaint`) while `sky_paint` is set.
    skypaint_pipeline: wgpu::RenderPipeline,
    /// Bound as the detail texture of materials without `$detail` (factor 0).
    detail_fallback: wgpu::TextureView,
    /// Flat normal bound as the bump map of materials without `$bumpmap`.
    bump_fallback: wgpu::TextureView,
    white_fallback: wgpu::TextureView,
    cubes: CubeSet,
    /// The lightmap atlas texture and, for the faces lit by a named style (32..), their
    /// per-style luxels: re-summed and rewritten when a `light` is switched.
    lm_tex: wgpu::Texture,
    lm_faces: Vec<LmFace>,
    style_scale: Vec<f32>,
    color_format: wgpu::TextureFormat,
    reflect: Mutex<ReflectState>,
    reflect_sampler: wgpu::Sampler,
    t0: std::time::Instant,
    /// Some water material asks for `$reflectentities`.
    reflect_entities: bool,
    /// `$nocull` variants of `pipeline` / `translucent_pipeline`.
    nocull_pipeline: wgpu::RenderPipeline,
    translucent_nocull_pipeline: wgpu::RenderPipeline,
    /// Water surfaces: alpha = the volume fog over the scene depth behind them.
    water_pipeline: wgpu::RenderPipeline,
    /// `Refract` sprites (thruster heat-wave): the scene copy offset by a normal map.
    refract_pipeline: wgpu::RenderPipeline,
    depth_layout: wgpu::BindGroupLayout,
    /// Additive, no depth write: beams and glow sprites.
    additive_pipeline: wgpu::RenderPipeline,
    /// Alpha-blended ad-hoc geometry (constraint ropes: `keyframe_rope` cables).
    alpha_pipeline: wgpu::RenderPipeline,
    /// DecalModulate: colour * 2 * decal (bullet holes).
    modulate_pipeline: wgpu::RenderPipeline,
    /// Alpha-blended, no depth write: models drawn with `ModelTarget::alpha` below 1 (ghosts).
    translucent_pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    /// The flashlights (index among the dynamic lights, light) whose shadow maps are drawn,
    /// the local player's first; the first `SHADOW_LAYERS` get one.
    flashlights: Vec<(usize, DLight)>,
    flashlight_vp: std::cell::Cell<[Option<Mat4>; DEPTH_LAYERS]>,
    shadow_pipeline: wgpu::RenderPipeline,
    shadow_layers: Vec<wgpu::TextureView>,
    camera_bind_group: wgpu::BindGroup,
    /// [`camera_bind_group`](Self::camera_bind_group) with a stand-in for the shadow array,
    /// for the pass that renders that array.
    shadow_camera_bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    batches: Vec<Batch>,
    pub sky_camera: Option<SkyCamera>,
    /// The map's `env_fog_controller` fog, and the `sky_camera`'s for the 3D skybox.
    pub fog: Option<Fog>,
    pub sky_fog: Option<Fog>,
    /// An `edit_fog` entity's fog (its `SetupWorldFog` hook returns true): replaces the map's
    /// and the water's while it exists.
    pub fog_override: Option<Fog>,
    /// An `edit_sky` entity's painted sky (`env_skypaint`), drawn instead of the skybox.
    pub sky_paint: Option<SkyPaint>,
    /// The map's own `env_skypaint`, what `sky_paint` falls back to.
    pub map_sky_paint: Option<SkyPaint>,
    pub water_volumes: Vec<WaterVolume>,
    /// `func_reflective_glass` faces (LightmappedReflective): plane and bounds.
    mirrors: Vec<Mirror>,
    pub stats: Stats,
    material_layout: wgpu::BindGroupLayout,
    base_sampler: wgpu::Sampler,
    white_uv: [f32; 2],
    light: crate::light::ModelLight,
    /// Lighting by model position (8 u cells), rebuilt as models move.
    light_cache: std::cell::RefCell<HashMap<[i32; 3], crate::light::LocalLight>>,
    /// Frame counter for [`model_cache`](Self::model_cache), stepped by [`draw`](Self::draw).
    frame: std::cell::Cell<u64>,
    /// CPU time of the last [`draw`](Self::draw) in ms, smoothed: shadow maps, water
    /// reflection, the scene itself.
    pass_ms: std::cell::Cell<[f32; 3]>,
    /// The frame's skinned entity meshes on the GPU: the shadow, reflection and main passes
    /// all draw the same items, so they are skinned and uploaded once per frame, keyed by
    /// the frame and the items slice (pointer, length).
    model_cache: std::cell::RefCell<Option<ModelCacheEntry>>,
    sky2d: Option<SkyCube>,
}

/// The 2D sky: worldspawn `skyname` as six `materials/skybox/<name><side>` faces on a cube
/// around the eye, drawn first so sky faces (skipped) and the 3D skybox show over it.
struct SkyCube {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    faces: Vec<wgpu::BindGroup>,
    /// `skybox/starfield` for the painted sky's stars (None when the texture is missing).
    stars: Option<wgpu::BindGroup>,
}

/// Face suffix and its axes in the Quake/Source convention: position = sx * s + ty * t + n
/// for s, t in -1..1 (texture u = (s + 1) / 2, v = (1 - t) / 2).
/// (suffix, normal, s axis, t axis)
type SkyFace = (&'static str, [f32; 3], [f32; 3], [f32; 3]);
const SKY_FACES: [SkyFace; 6] = [
    ("rt", [1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]),
    ("bk", [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
    ("lf", [0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
    ("ft", [0.0, -1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
    ("up", [0.0, 0.0, 1.0], [0.0, -1.0, 0.0], [-1.0, 0.0, 0.0]),
    ("dn", [0.0, 0.0, -1.0], [0.0, -1.0, 0.0], [1.0, 0.0, 0.0]),
];

fn build_sky_cube(
    gpu: &Gpu,
    mount: &Mount,
    bsp: &Bsp<'_>,
    layout: &wgpu::BindGroupLayout,
    white_uv: [f32; 2],
    detail: &wgpu::TextureView,
) -> Option<SkyCube> {
    let name = bsp
        .entities
        .iter()
        .find(|e| e.classname() == "worldspawn")
        .and_then(|e| e.get("skyname"))
        .or_else(|| {
            eprintln!("sky: no worldspawn skyname");
            None
        })?
        .to_string();
    let device = &gpu.device;
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("sky"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("sky params"),
        contents: bytemuck::bytes_of(&MaterialParams {
            alpha_test: 0,
            unlit: 0,
            translucent: 0,
            blend: 0,
            detail: [0.0; 4],
            uv2_m: [1.0, 0.0, 0.0, 1.0],
            uv2_t: [0.0; 4],
            water_color: [0.0; 4],
            water_range: [0.0; 4],
            envmap: [0.0; 4],
            envmap2: [0.0; 4],
            envmap3: [0.0; 4],
            selfillum_tint: [1.0; 4],
            phong: [0.0; 4],
            phong_fresnel: [0.0, 0.5, 1.0, 0.0],
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let mut verts = Vec::new();
    let mut idx = Vec::new();
    let mut faces = Vec::new();
    for (suffix, n, sx, ty) in SKY_FACES {
        let mat_name = format!("skybox/{name}{suffix}");
        let mat = material::load(mount, &mat_name)
            .inspect_err(|e| eprintln!("sky: {mat_name}: {e}"))
            .ok()?;
        let tex = mat.base_texture.as_deref().unwrap_or(&mat_name);
        let rgba = material::load_texture(mount, tex, 2048)
            .inspect_err(|e| eprintln!("sky: {tex}: {e}"))
            .ok()?;
        let t = upload_rgba(
            gpu,
            &rgba.pixels,
            rgba.width,
            rgba.height,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            &mat_name,
        );
        faces.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&mat_name),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &t.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(detail),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(detail),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(detail),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(detail),
                },
                wgpu::BindGroupEntry {
                    binding: 7,

                    resource: wgpu::BindingResource::TextureView(detail),
                },
                wgpu::BindGroupEntry {
                    binding: 8,

                    resource: wgpu::BindingResource::TextureView(detail),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
            ],
        }));
        let base = verts.len() as u32;
        let (n, sx, ty) = (Vec3::from(n), Vec3::from(sx), Vec3::from(ty));
        // the face uvs are inset by half a texel like the engine's sky box draw: sampled
        // right to the edge, the outermost texel row of a face (which never matches its
        // neighbour's) shows as a dark line along every cube edge
        let (iu, iv) = (
            0.5 / rgba.width.max(1) as f32,
            0.5 / rgba.height.max(1) as f32,
        );
        for (s, t) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            verts.push(Vertex {
                tint: [1.0, 1.0, 1.0, 0.0],
                // faces overlap their neighbours a little: drawn edge to edge, the sub-pixel
                // cracks along the cube edges let the clear colour through as dark lines
                position: ((n + (sx * s + ty * t) * 1.002) * SKY_CUBE_HALF).to_array(),
                uv: [
                    iu + (s + 1.0) / 2.0 * (1.0 - 2.0 * iu),
                    iv + (1.0 - t) / 2.0 * (1.0 - 2.0 * iv),
                ],
                lm_uv: white_uv,
                color: [1.0; 3],
                alpha: 0.0,
                lm_page: 0.0,
                normal: [0.0; 3],
                cube: -1.0,
                spec_dir: [0.0; 3],
                spec_color: [0.0; 3],
            });
        }
        idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    // env_skypaint's stars: the tiled starfield of the painted sky (its own repeat sampler)
    let stars_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("sky stars"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let stars = (|| {
        // env_skypaint `startexture` (its default), tiled by the painted sky's star layers
        let mat_name = bsp
            .entities
            .iter()
            .find(|e| e.classname() == "env_skypaint")
            .and_then(|e| e.get("startexture"))
            .filter(|t| !t.is_empty())
            .unwrap_or("skybox/starfield")
            .to_string();
        let mat = material::load(mount, &mat_name).ok()?;
        let tex = mat.base_texture.as_deref().unwrap_or(&mat_name);
        let rgba = material::load_texture(mount, tex, 1024).ok()?;
        let t = upload_rgba(
            gpu,
            &rgba.pixels,
            rgba.width,
            rgba.height,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            &mat_name,
        );
        Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky stars"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &t.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&stars_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(detail),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(detail),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(detail),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(detail),
                },
                wgpu::BindGroupEntry {
                    binding: 7,

                    resource: wgpu::BindingResource::TextureView(detail),
                },
                wgpu::BindGroupEntry {
                    binding: 8,

                    resource: wgpu::BindingResource::TextureView(detail),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
            ],
        }))
    })();
    Some(SkyCube {
        vertex_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sky vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        }),
        index_buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sky indices"),
            contents: bytemuck::cast_slice(&idx),
            usage: wgpu::BufferUsages::INDEX,
        }),
        faces,
        stars,
    })
}

const SKY_CUBE_HALF: f32 = 100.0;

/// A model for props that move: meshes in model space with their material bind groups.
/// A model-space vertex of a `DynModel` with its skinning weights.
#[derive(Clone, Copy, Debug)]
pub struct DynVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub bones: [u8; 3],
    pub weights: [f32; 3],
    pub num_bones: u8,
    /// `eyes` shader: the iris texture's uv at this vertex (the eyeball's iris projection at
    /// rest), `[-1, -1]` on everything else.
    pub iris_uv: [f32; 2],
}

impl DynVertex {
    fn rigid(position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        DynVertex {
            position,
            normal,
            uv,
            bones: [0; 3],
            weights: [0.0; 3],
            num_bones: 0,
            iris_uv: [-1.0, -1.0],
        }
    }
}

/// One mesh of a `DynModel`: material, vertices, indices.
/// Material bind group, vertices, indices, `$halflambert`.
/// A drawable piece of a [`DynModel`]: material bind group, vertices, indices, `$halflambert`,
/// and whether the material's shader is unlit (`UnlitGeneric`: the toolgun screen) - such parts
/// skip the model's origin lighting instead of only the shader's per-pixel light.
type DynPart = (
    wgpu::BindGroup,
    Vec<DynVertex>,
    Vec<u32>,
    bool,
    bool,
    bool,
    Option<crate::props::EyeInfo>,
);

/// The bones of a `DynModel` and where its animations come from.
struct Skeleton {
    bones: Vec<gm_formats::mdl::Bone>,
    attachments: Vec<gm_formats::mdl::Attachment>,
    /// The model's own animations first, then those of its include models.
    anims: Vec<AnimSource>,
    hitboxes: Vec<gm_formats::mdl::HitBox>,
}

impl Skeleton {
    fn seq(&self, name: &str) -> Option<(&AnimSource, &gm_formats::mdl::Sequence)> {
        self.anims
            .iter()
            .find_map(|a| a.sequences.iter().find(|s| s.name == name).map(|s| (a, s)))
    }
}

/// Animations of a model: its own, or those of the first model it includes (player models
/// animate through a shared animation model), with the `.ani` the tracks may live in and the
/// name map from animation bones to the rendered model's bones.
struct AnimSource {
    mdl: Vec<u8>,
    ani: Vec<u8>,
    blocks: Vec<(usize, usize)>,
    bones: Vec<gm_formats::mdl::Bone>,
    map: Vec<Option<usize>>,
    sequences: Vec<gm_formats::mdl::Sequence>,
    anims: Vec<gm_formats::mdl::AnimDesc>,
    pose_params: Vec<gm_formats::mdl::PoseParam>,
}

type LocalPose = Vec<([f32; 3], [f32; 4])>;

/// Adds a delta pose with weight `w`: positions scaled, the rotation slerped from identity and
/// applied in the bone's own frame (Source's QuaternionMA).
/// Moves `acc` toward `other` by `t` (positions lerp, rotations slerp).
fn mix_local(acc: &mut LocalPose, other: &LocalPose, t: f32) {
    for ((p, q), (p2, q2)) in acc.iter_mut().zip(other) {
        for (a, b) in p.iter_mut().zip(p2) {
            *a += (b - *a) * t;
        }
        let qa = glam::Quat::from_array(*q).normalize();
        let qb = glam::Quat::from_array(*q2).normalize();
        *q = qa.slerp(qb, t).to_array();
    }
}

fn add_delta(local: &mut LocalPose, delta: &LocalPose, w: f32) {
    for ((pos, quat), (dpos, dquat)) in local.iter_mut().zip(delta) {
        for (p, d) in pos.iter_mut().zip(dpos) {
            *p += w * d;
        }
        let dq = glam::Quat::IDENTITY.slerp(glam::Quat::from_array(*dquat).normalize(), w);
        *quat = (glam::Quat::from_array(*quat).normalize() * dq).to_array();
    }
}

impl AnimSource {
    fn new(
        mount: &Mount,
        bytes: Vec<u8>,
        mdl: &gm_formats::mdl::Mdl,
        render_bones: &[gm_formats::mdl::Bone],
    ) -> Self {
        let (ani_name, blocks) = gm_formats::mdl::anim_blocks(&bytes);
        let ani = if blocks.len() > 1 {
            mount.read(&ani_name).unwrap_or_default()
        } else {
            Vec::new()
        };
        let map = mdl
            .bones
            .iter()
            .map(|b| render_bones.iter().position(|r| r.name == b.name))
            .collect();
        AnimSource {
            sequences: gm_formats::mdl::sequences(&bytes, &mdl.header),
            anims: gm_formats::mdl::anim_descs(&bytes, &mdl.header),
            pose_params: gm_formats::mdl::pose_params(&bytes, &mdl.header),
            bones: mdl.bones.clone(),
            map,
            ani,
            blocks,
            mdl: bytes,
        }
    }

    /// The model's own animations followed by those of its include models (a player model
    /// keeps `ragdoll` itself and borrows the rest from `m_anm`).
    fn for_model(mount: &Mount, bytes: Vec<u8>, mdl: &gm_formats::mdl::Mdl) -> Vec<Self> {
        let includes = gm_formats::mdl::include_models(&bytes, &mdl.header);
        let mut out = vec![AnimSource::new(mount, bytes, mdl, &mdl.bones)];
        out.extend(includes.into_iter().filter_map(|inc| {
            // the game's filesystem is case-insensitive: police.mdl names
            // `models/Police_animations.mdl`
            let b = mount
                .read(&inc)
                .or_else(|_| mount.read(&inc.to_ascii_lowercase()))
                .map_err(|e| eprintln!("model include {inc}: {e}"))
                .ok()?;
            let m = gm_formats::mdl::Mdl::parse(&b).ok()?;
            Some(AnimSource::new(mount, b, &m, &mdl.bones))
        }));
        out
    }

    fn data(&self, block: i32) -> Option<(&[u8], usize)> {
        if block == 0 {
            return Some((&self.mdl, 0));
        }
        let (start, _) = *self.blocks.get(usize::try_from(block).ok()?)?;
        Some((&self.ani, start))
    }

    /// Local pose of the animation bones at `cycle` of `anim`.
    fn frame_pose(
        &self,
        anim: &gm_formats::mdl::AnimDesc,
        cycle: f32,
        delta: bool,
    ) -> Option<LocalPose> {
        let frames = anim.frames.max(1) as usize;
        let f = (cycle.clamp(0.0, 1.0) * (frames - 1) as f32).max(0.0);
        let f0 = (f.floor() as usize).min(frames - 1);
        let f1 = (f0 + 1).min(frames - 1);
        let t = f - f0 as f32;
        let sample = |frame: usize| -> Option<LocalPose> {
            let (block, off, frame) = anim.frame_block(&self.mdl, frame)?;
            let (data, base) = self.data(block)?;
            gm_formats::mdl::anim_pose(data, base + off, &self.bones, frame, delta)
        };
        let a = sample(f0)?;
        if f1 == f0 || t <= 0.0 {
            return Some(a);
        }
        // the game interpolates between the two neighbouring frames (CalcBonePosition /
        // CalcBoneQuaternion), a nearest frame steps visibly at low animation frame rates
        let b = sample(f1)?;
        Some(
            a.iter()
                .zip(b.iter())
                .map(|((pa, qa), (pb, qb))| {
                    let p = Vec3::from(*pa).lerp(Vec3::from(*pb), t);
                    let (qa, qb) = (Quat::from_array(*qa), Quat::from_array(*qb));
                    let qb = if qa.dot(qb) < 0.0 { -qb } else { qb };
                    (p.to_array(), qa.slerp(qb, t).normalize().to_array())
                })
                .collect(),
        )
    }

    fn param(&self, index: i32, params: &[(&str, f32)]) -> f32 {
        let Some(p) = usize::try_from(index)
            .ok()
            .and_then(|i| self.pose_params.get(i))
        else {
            return 0.0;
        };
        params
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(&p.name))
            .map_or(0.0, |(_, v)| *v)
    }

    /// Pose of a sequence at `cycle`: its blend grid cells weighted bilinearly by the pose
    /// parameters.
    /// The sequence's autolayers: delta animations added with the layer's weight at this cycle.
    fn apply_layers(
        &self,
        seq: &gm_formats::mdl::Sequence,
        cycle: f32,
        params: &[(&str, f32)],
        local: &mut LocalPose,
    ) {
        // bone_setup's AddSequenceLayers: the ramp runs over the host cycle, or over the pose
        // parameter named by the layer (STUDIO_AL_POSE), smoothed by STUDIO_AL_SPLINE
        for layer in &seq.autolayers {
            if layer.flags & 0x1000 != 0 {
                continue;
            }
            let pose_driven = layer.flags & 0x4000 != 0;
            let index = if pose_driven {
                self.param(i32::from(layer.pose), params)
            } else {
                cycle
            };
            let mut w = layer.weight(index);
            if layer.flags & 0x40 != 0 {
                w = w * w * (3.0 - 2.0 * w);
            }
            if layer.flags & 0x200 != 0 && w > 0.0 {
                w = 1.0;
            }
            let Some(lseq) = self.sequences.get(layer.sequence.max(0) as usize) else {
                continue;
            };
            let Some(lanim) = self.anims.get(lseq.anim.max(0) as usize) else {
                continue;
            };
            if w <= 0.0 || lanim.flags & 4 == 0 || lanim.frames <= 0 {
                continue;
            }
            let lcycle = if layer.start != layer.end && !pose_driven {
                ((cycle - layer.start) / (layer.end - layer.start)).clamp(0.0, 1.0)
            } else {
                cycle
            };
            // the layer's own blend grid (aim layers: 3x3 over aim_yaw / aim_pitch)
            if let Some(delta) = self.blended(lseq, lcycle, params, true) {
                add_delta(local, &delta, w);
            }
        }
    }

    fn blended(
        &self,
        seq: &gm_formats::mdl::Sequence,
        cycle: f32,
        params: &[(&str, f32)],
        delta: bool,
    ) -> Option<LocalPose> {
        let (x, fx) = seq.grid_pos(0, self.param(seq.param[0], params));
        let (y, fy) = seq.grid_pos(1, self.param(seq.param[1], params));
        let cells = [
            (x, y, (1.0 - fx) * (1.0 - fy)),
            (x + 1, y, fx * (1.0 - fy)),
            (x, y + 1, (1.0 - fx) * fy),
            (x + 1, y + 1, fx * fy),
        ];
        let mut out: Option<LocalPose> = None;
        let mut total = 0.0;
        for (cx, cy, w) in cells {
            if w <= 0.0001 {
                continue;
            }
            let Some(anim) = seq
                .blend(cx, cy)
                .and_then(|a| self.anims.get(a.max(0) as usize))
            else {
                continue;
            };
            let Some(pose) = self.frame_pose(anim, cycle, delta) else {
                continue;
            };
            total += w;
            match &mut out {
                None => out = Some(pose),
                Some(acc) => {
                    let t = w / total;
                    for ((p, q), (p2, q2)) in acc.iter_mut().zip(&pose) {
                        for (a, b) in p.iter_mut().zip(p2) {
                            *a += (b - *a) * t;
                        }
                        let qa = glam::Quat::from_array(*q).normalize();
                        let qb = glam::Quat::from_array(*q2).normalize();
                        *q = qa.slerp(qb, t).to_array();
                    }
                }
            }
        }
        out
    }
}

pub struct DynModel {
    parts: Vec<DynPart>,
    /// Each part's material key and base texture (`write_part_texture` repaints one: the
    /// toolgun's screen).
    part_textures: Vec<(String, wgpu::Texture)>,
    /// Axis-aligned bounds of the rest-pose vertices (model space).
    bounds: (Vec3, Vec3),
    /// Drawn at full brightness (beams, markers), not shaded by the sun.
    unlit: bool,
    skeleton: Option<Skeleton>,
    /// `Entity:GetBodyGroups`: each MDL body part's name and its submodels' names.
    bodygroups: Vec<(String, Vec<String>)>,
    /// Skinning matrices of the first sequence's first frame, for draws without bones:
    /// the engine always poses a model through its sequence 0, and some reference meshes
    /// differ from it (the crossbow bolt lies along -y in the file, +x once posed). None when
    /// that pose is the identity.
    rest_skin: Option<Vec<Mat4>>,
}

impl DynModel {
    /// Repaints the base texture of every part whose material key contains `key` with
    /// `rgba` (`w` x `h`, which must be the texture's size); every mip level is rewritten.
    /// Returns whether a part matched.
    pub fn write_part_texture(&self, gpu: &Gpu, key: &str, rgba: &[u8], w: u32, h: u32) -> bool {
        let mut hit = false;
        for (k, t) in &self.part_textures {
            if !k.contains(key) || t.width() != w || t.height() != h {
                continue;
            }
            hit = true;
            let (mut data, mut lw, mut lh) = (rgba.to_vec(), w, h);
            for level in 0..t.mip_level_count() {
                gpu.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: t,
                        mip_level: level,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(lw * 4),
                        rows_per_image: Some(lh),
                    },
                    wgpu::Extent3d {
                        width: lw,
                        height: lh,
                        depth_or_array_layers: 1,
                    },
                );
                if lw == 1 && lh == 1 {
                    break;
                }
                let (nw, nh) = ((lw / 2).max(1), (lh / 2).max(1));
                let mut next = vec![0u8; (nw * nh * 4) as usize];
                for y in 0..nh {
                    for x in 0..nw {
                        for c in 0..4 {
                            let mut sum = 0u32;
                            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                                let sx = (x * 2 + dx).min(lw - 1);
                                let sy = (y * 2 + dy).min(lh - 1);
                                sum += data[((sy * lw + sx) * 4 + c) as usize] as u32;
                            }
                            next[((y * nw + x) * 4 + c) as usize] = (sum / 4) as u8;
                        }
                    }
                }
                data = next;
                lw = nw;
                lh = nh;
            }
        }
        hit
    }
}

/// Where `draw_models` renders: the colour and depth views, the aspect ratio of the colour
/// view and whether to clear the depth buffer first (viewmodels draw over the world).
pub struct ModelTarget<'a> {
    pub color: &'a wgpu::TextureView,
    /// Single-sample view `color` resolves into when it is multisampled.
    pub resolve: Option<&'a wgpu::TextureView>,
    pub depth: &'a wgpu::TextureView,
    pub aspect: f32,
    pub clear_depth: bool,
    /// 1 draws opaque; below 1 draws alpha-blended without depth writes (tool ghosts).
    pub alpha: f32,
}

/// Bone-name override for [`DynModel::bone_world`]: the transform another skeleton gives a bone.
pub type BoneMerge<'a> = &'a dyn Fn(&str) -> Option<Mat4>;

/// A `DynModel` to draw: its placement and, for a skinned pose, one matrix per bone from
/// [`DynModel::pose`].
/// A decal ready to place: its bind group, texture rectangle, world size and blend.
pub struct Decal {
    pub bind: wgpu::BindGroup,
    pub uv: [f32; 4],
    pub size: f32,
    pub modulate: bool,
}

pub struct DrawItem<'a> {
    pub model: &'a DynModel,
    pub transform: Mat4,
    pub bones: Option<&'a [Mat4]>,
    /// The player's colour for materials with the PlayerColor proxy (`cl_playercolor`).
    pub tint: Vec3,
    /// Dissolve progress 0..1 (combine ball kill), 0 for none.
    pub dissolve: f32,
    /// World point the model's eyes look at (an NPC watching the player), if any.
    pub look: Option<Vec3>,
}

impl DynModel {
    /// The MDL body parts (name, submodel names) for the bodygroups context menu.
    pub fn bodygroups(&self) -> &[(String, Vec<String>)] {
        &self.bodygroups
    }

    /// Model-space transform of a named attachment given the bones' world (model-space)
    /// matrices from `bone_world`.
    pub fn attachment(&self, name: &str, bones: &[Mat4]) -> Option<Mat4> {
        let sk = self.skeleton.as_ref()?;
        let a = sk
            .attachments
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))?;
        Self::attachment_matrix(a, bones)
    }

    /// [`attachment`](Self::attachment) by index (the MDL order; animation events name them
    /// 1-based, so 6001's brass comes out of index 1).
    pub fn attachment_at(&self, index: usize, bones: &[Mat4]) -> Option<Mat4> {
        let a = self.skeleton.as_ref()?.attachments.get(index)?;
        Self::attachment_matrix(a, bones)
    }

    /// The animation events of sequence `name` with its length: (cycle, event id, event
    /// name, options) each.
    #[allow(clippy::type_complexity)]
    pub fn sequence_events(&self, name: &str) -> Option<(f32, Vec<(f32, i32, String, String)>)> {
        let (len, _) = self.sequence_length(name)?;
        let (_, seq) = self.skeleton.as_ref()?.seq(name)?;
        Some((
            len,
            seq.events
                .iter()
                .map(|e| (e.cycle, e.event, e.name.clone(), e.options.clone()))
                .collect(),
        ))
    }

    fn attachment_matrix(a: &gm_formats::mdl::Attachment, bones: &[Mat4]) -> Option<Mat4> {
        let bone = bones.get(usize::try_from(a.bone).ok()?)?;
        let l = a.local;
        let local = Mat4::from_cols_array_2d(&[
            [l[0][0], l[1][0], l[2][0], 0.0],
            [l[0][1], l[1][1], l[2][1], 0.0],
            [l[0][2], l[1][2], l[2][2], 0.0],
            [l[0][3], l[1][3], l[2][3], 1.0],
        ]);
        Some(*bone * local)
    }

    /// Rest-pose vertex bounds in model space (min, max).
    pub fn bounds(&self) -> (Vec3, Vec3) {
        self.bounds
    }

    /// Names of the model's sequences.
    pub fn sequences(&self) -> impl Iterator<Item = &str> {
        self.skeleton
            .iter()
            .flat_map(|s| s.anims.iter())
            .flat_map(|a| a.sequences.iter().map(|q| q.name.as_str()))
    }

    /// Name of the first sequence tagged with activity `act` (`ACT_VM_SECONDARYATTACK`, ...):
    /// how the game picks a viewmodel animation from `SendWeaponAnim`.
    pub fn sequence_for_activity(&self, act: &str) -> Option<&str> {
        self.skeleton
            .iter()
            .flat_map(|s| s.anims.iter())
            .flat_map(|a| a.sequences.iter())
            .find(|q| q.activity_name.eq_ignore_ascii_case(act))
            .map(|q| q.name.as_str())
    }

    /// Length in seconds of sequence `name` and whether it loops.
    pub fn sequence_length(&self, name: &str) -> Option<(f32, bool)> {
        let (src, seq) = self.skeleton.as_ref()?.seq(name)?;
        let anim = src.anims.get(seq.anim.max(0) as usize)?;
        if anim.fps <= 0.0 {
            return None;
        }
        Some((anim.frames.max(1) as f32 / anim.fps, anim.flags & 1 != 0))
    }

    /// Skinning matrices (model space from bind pose) of sequence `name` at `time` seconds
    /// from its start: looping sequences wrap, others hold their last frame. `None` without a
    /// skeleton, an unknown sequence or tracks in an external `.ani` block.
    pub fn pose(&self, name: &str, time: f32) -> Option<Vec<Mat4>> {
        self.pose_with(name, time, &[])
    }

    /// [`pose`](Self::pose) with pose parameters (`move_x`, `move_y`, ...) driving the
    /// sequence's blend grid.
    pub fn pose_with(&self, name: &str, time: f32, params: &[(&str, f32)]) -> Option<Vec<Mat4>> {
        let world = self.bone_world(name, time, None, params)?;
        Some(self.skin_matrices(&world))
    }

    /// Like [`pose`](Self::pose) with the bones that `host` also has (by name) taken from
    /// `host_world`, the host's bone transforms from [`bone_world`](Self::bone_world): the
    /// hands model follows the weapon's skeleton.
    pub fn pose_merged(
        &self,
        name: &str,
        time: f32,
        host: &DynModel,
        host_world: &[Mat4],
    ) -> Option<Vec<Mat4>> {
        let hs = host.skeleton.as_ref()?;
        let lookup = |bone: &str| {
            hs.bones
                .iter()
                .position(|b| b.name == bone)
                .and_then(|i| host_world.get(i).copied())
        };
        let world = self.bone_world(name, time, Some(&lookup), &[])?;
        Some(self.skin_matrices(&world))
    }

    /// [`pose_merged`](Self::pose_merged) returning the bones' model-space matrices (for
    /// [`attachment`](Self::attachment)) next to the skin matrices.
    pub fn bone_world_merged(
        &self,
        name: &str,
        time: f32,
        host: &DynModel,
        host_world: &[Mat4],
    ) -> Option<(Vec<Mat4>, Vec<Mat4>)> {
        let hs = host.skeleton.as_ref()?;
        let lookup = |bone: &str| {
            hs.bones
                .iter()
                .position(|b| b.name == bone)
                .and_then(|i| host_world.get(i).copied())
        };
        let world = self.bone_world(name, time, Some(&lookup), &[])?;
        let skin = self.skin_matrices(&world);
        Some((world, skin))
    }

    /// The ground speed of sequence `name` at the pose parameters `params` (the root movement
    /// per second of its blended animations), `None` when the model has no such sequence.
    pub fn ground_speed(&self, name: &str, params: &[(&str, f32)]) -> Option<f32> {
        let (src, seq) = self.skeleton.as_ref()?.seq(name)?;
        let (x, fx) = seq.grid_pos(0, src.param(seq.param[0], params));
        let (y, fy) = seq.grid_pos(1, src.param(seq.param[1], params));
        let cells = [
            (x, y, (1.0 - fx) * (1.0 - fy)),
            (x + 1, y, fx * (1.0 - fy)),
            (x, y + 1, (1.0 - fx) * fy),
            (x + 1, y + 1, fx * fy),
        ];
        let (mut speed, mut total) = (0.0, 0.0);
        for (cx, cy, w) in cells {
            let Some(anim) = seq
                .blend(cx, cy)
                .and_then(|a| src.anims.get(a.max(0) as usize))
            else {
                continue;
            };
            speed += w * anim.ground_speed();
            total += w;
        }
        Some(if total > 0.0 {
            speed / total
        } else {
            src.anims.get(seq.anim.max(0) as usize)?.ground_speed()
        })
    }

    /// The model's bones (hierarchy, bind pose, `physics_bone` of each), when it has a skeleton.
    pub fn skeleton_bones(&self) -> Option<&[gm_formats::mdl::Bone]> {
        self.skeleton.as_ref().map(|sk| sk.bones.as_slice())
    }

    /// The model's default hit box set (particles placed on the model).
    pub fn hitboxes(&self) -> &[gm_formats::mdl::HitBox] {
        self.skeleton
            .as_ref()
            .map_or(&[], |sk| sk.hitboxes.as_slice())
    }

    /// Skinning matrices from bone transforms: bone world times pose-to-bone.
    pub fn skin_matrices(&self, world: &[Mat4]) -> Vec<Mat4> {
        let Some(sk) = self.skeleton.as_ref() else {
            return Vec::new();
        };
        world
            .iter()
            .zip(&sk.bones)
            .map(|(w, b)| {
                let r = &b.pose_to_bone;
                let p2b = Mat4::from_cols_array_2d(&[
                    [r[0][0], r[1][0], r[2][0], 0.0],
                    [r[0][1], r[1][1], r[2][1], 0.0],
                    [r[0][2], r[1][2], r[2][2], 0.0],
                    [r[0][3], r[1][3], r[2][3], 1.0],
                ]);
                *w * p2b
            })
            .collect()
    }

    /// See [`DynModel::rest_skin`].
    fn default_pose_skin(&self) -> Option<Vec<Mat4>> {
        let sk = self.skeleton.as_ref()?;
        let first = &sk.anims.first()?.sequences.first()?.name;
        let world = self.bone_world(first, 0.0, None, &[])?;
        let skin = self.skin_matrices(&world);
        let identity = skin.iter().all(|m| m.abs_diff_eq(Mat4::IDENTITY, 1e-3));
        (!identity).then_some(skin)
    }
    /// Model-space transform of every bone at `time` of sequence `name`; `merge` overrides
    /// a bone's transform by name (bone merge onto another skeleton).
    pub fn bone_world(
        &self,
        name: &str,
        time: f32,
        merge: Option<BoneMerge<'_>>,
        params: &[(&str, f32)],
    ) -> Option<Vec<Mat4>> {
        self.bone_world_gesture(name, time, merge, params, None)
    }

    /// [`bone_world`](Self::bone_world) with a one-shot gesture `(sequence, seconds since its
    /// start)` layered on top at full weight, the way GMod plays `range_*` / `reload_*` on a
    /// player: the gesture is a delta sequence of the same animation source; it holds its last
    /// frame once over (the caller drops it after [`sequence_length`](Self::sequence_length)).
    pub fn bone_world_gesture(
        &self,
        name: &str,
        time: f32,
        merge: Option<BoneMerge<'_>>,
        params: &[(&str, f32)],
        gesture: Option<(&str, f32)>,
    ) -> Option<Vec<Mat4>> {
        self.bone_world_full(name, time, merge, params, gesture, None)
    }

    /// `bone_world_gesture` with a `CSequenceTransitioner` layer: the sequence the model just
    /// left (`prev`: name, its cycle time, weight 1 -> 0) blended under the current one.
    pub fn bone_world_gesture_transition(
        &self,
        name: &str,
        time: f32,
        params: &[(&str, f32)],
        gesture: Option<(&str, f32)>,
        prev: Option<(&str, f32, f32)>,
    ) -> Option<Vec<Mat4>> {
        self.bone_world_full(name, time, None, params, gesture, prev)
    }

    /// [`bone_world`](Self::bone_world) with the sequence just left blended in the way
    /// `CSequenceTransitioner` does: `prev` = (sequence, seconds into it when it was left, its
    /// weight now), mixed into the local pose before the bone hierarchy.
    pub fn bone_world_transition(
        &self,
        name: &str,
        time: f32,
        params: &[(&str, f32)],
        prev: Option<(&str, f32, f32)>,
    ) -> Option<Vec<Mat4>> {
        self.bone_world_full(name, time, None, params, None, prev)
    }

    /// The sequence's `fadeouttime` (0.2 s when the model gives none).
    pub fn sequence_fade_out(&self, name: &str) -> f32 {
        self.skeleton
            .as_ref()
            .and_then(|sk| sk.seq(name))
            .map_or(0.2, |(_, s)| s.fade_out.max(0.0))
    }

    /// The sequence's `fadeintime` (0.2 s when the model gives none): `CSequenceTransitioner`
    /// fades the sequence just left over `min(its fadeouttime, the new one's fadeintime)`, so a
    /// `fadein 0` sequence (most weapons' fire) cuts in sharply.
    pub fn sequence_fade_in(&self, name: &str) -> f32 {
        self.skeleton
            .as_ref()
            .and_then(|sk| sk.seq(name))
            .map_or(0.2, |(_, s)| s.fade_in.max(0.0))
    }

    fn bone_world_full(
        &self,
        name: &str,
        time: f32,
        merge: Option<BoneMerge<'_>>,
        params: &[(&str, f32)],
        gesture: Option<(&str, f32)>,
        transition: Option<(&str, f32, f32)>,
    ) -> Option<Vec<Mat4>> {
        let sk = self.skeleton.as_ref()?;
        let (src, seq) = sk.seq(name)?;
        let anim = src.anims.get(seq.anim.max(0) as usize)?;
        if anim.frames <= 0 {
            return None;
        }
        let frames = anim.frames as usize;
        let f = time.max(0.0) * anim.fps;
        let cycle = if anim.flags & 1 != 0 {
            (f / frames as f32).fract()
        } else {
            (f / frames as f32).min(1.0)
        };
        let mut local = src.blended(seq, cycle, params, false)?;
        src.apply_layers(seq, cycle, params, &mut local);
        if let Some((pname, ptime, w)) = transition {
            if let Some((psrc, pseq)) = sk.seq(pname) {
                if let Some(panim) = psrc.anims.get(pseq.anim.max(0) as usize) {
                    if std::ptr::eq(psrc, src) && panim.frames > 0 && w > 0.001 {
                        let pf = ptime.max(0.0) * panim.fps / panim.frames as f32;
                        let pcycle = if panim.flags & 1 != 0 {
                            pf.fract()
                        } else {
                            pf.min(1.0)
                        };
                        if let Some(mut pl) = src.blended(pseq, pcycle, params, false) {
                            src.apply_layers(pseq, pcycle, params, &mut pl);
                            mix_local(&mut local, &pl, w.min(1.0));
                        }
                    }
                }
            }
        }
        // a one-shot gesture (reload, shot) on top: its delta and its own layers at full weight
        if let Some((gname, gtime)) = gesture {
            if let Some((gsrc, gseq)) = sk.seq(gname) {
                if let Some(ganim) = gsrc.anims.get(gseq.anim.max(0) as usize) {
                    if std::ptr::eq(gsrc, src) && ganim.frames > 0 && ganim.flags & 4 != 0 {
                        let gcycle =
                            (gtime.max(0.0) * ganim.fps / ganim.frames as f32).clamp(0.0, 1.0);
                        if let Some(delta) = src.blended(gseq, gcycle, params, true) {
                            add_delta(&mut local, &delta, 1.0);
                        }
                        src.apply_layers(gseq, gcycle, params, &mut local);
                    }
                }
            }
        }
        // onto the rendered skeleton by bone name; bones the animation lacks stay in bind pose
        let mut rlocal: LocalPose = sk.bones.iter().map(|b| (b.pos, b.quat)).collect();
        for (j, l) in local.iter().enumerate() {
            if let Some(Some(i)) = src.map.get(j) {
                rlocal[*i] = *l;
            }
        }
        let mut world: Vec<Mat4> = Vec::with_capacity(rlocal.len());
        for (i, (pos, quat)) in rlocal.iter().enumerate() {
            let bone = &sk.bones[i];
            if let Some(m) = merge.and_then(|f| f(&bone.name)) {
                world.push(m);
                continue;
            }
            let m = Mat4::from_rotation_translation(
                glam::Quat::from_array(*quat).normalize(),
                Vec3::from(*pos),
            );
            let parent = bone.parent;
            let m = if parent >= 0 && (parent as usize) < i {
                world[parent as usize] * m
            } else {
                m
            };
            world.push(m);
        }
        Some(world)
    }
}

fn skin(v: &DynVertex, bones: &[Mat4]) -> (Vec3, Vec3) {
    let p = Vec3::from(v.position);
    let n = Vec3::from(v.normal);
    if v.num_bones == 0 {
        return (p, n);
    }
    let mut sp = Vec3::ZERO;
    let mut sn = Vec3::ZERO;
    for i in 0..(v.num_bones as usize).min(3) {
        let Some(m) = bones.get(v.bones[i] as usize) else {
            continue;
        };
        sp += m.transform_point3(p) * v.weights[i];
        sn += m.transform_vector3(n) * v.weights[i];
    }
    (sp, sn)
}

/// Placement of a `DynModel`: origin plus Source angles.
pub fn model_transform(pos: Vec3, angles: Vec3) -> Mat4 {
    Mat4::from_translation(pos) * Mat4::from_mat3(crate::props::angle_matrix(angles.to_array()))
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub faces: usize,
    pub displacements: usize,
    pub triangles: usize,
    pub skybox_faces: usize,
    pub props: usize,
    pub prop_models: usize,
    pub materials: usize,
    pub missing_materials: Vec<String>,
    pub lightmap_overflow: usize,
}

fn face_uv(bsp: &Bsp<'_>, face: &Face, p: [f32; 3], tex_w: f32, tex_h: f32) -> [f32; 2] {
    let ti = &bsp.tex_infos[face.tex_info as usize];
    let s = ti.texture_vecs[0];
    let t = ti.texture_vecs[1];
    [
        (s[0] * p[0] + s[1] * p[1] + s[2] * p[2] + s[3]) / tex_w,
        (t[0] * p[0] + t[1] * p[1] + t[2] * p[2] + t[3]) / tex_h,
    ]
}

fn face_lm_uv(bsp: &Bsp<'_>, face: &Face, p: [f32; 3], pl: &Placement, atlas: f32) -> [f32; 2] {
    let ti = &bsp.tex_infos[face.tex_info as usize];
    let s = ti.lightmap_vecs[0];
    let t = ti.lightmap_vecs[1];
    let lu = s[0] * p[0] + s[1] * p[1] + s[2] * p[2] + s[3] - face.lightmap_mins[0] as f32 + 0.5;
    let lv = t[0] * p[0] + t[1] * p[1] + t[2] * p[2] + t[3] - face.lightmap_mins[1] as f32 + 0.5;
    [(pl.x as f32 + lu) / atlas, (pl.y as f32 + lv) / atlas]
}

/// Brush entities store their faces relative to the entity `origin` (the model's
/// bounds are centred on zero); faces of model 0 and of entities without an
/// origin are already in world space.
/// The brush models (`*N`) of the entities the server moves or toggles: drawn as dynamic
/// models at their networked pose, not baked into the world mesh.
pub fn movable_brush_models(bsp: &Bsp<'_>) -> Vec<usize> {
    bsp.entities
        .iter()
        .filter(|e| !gm_collision::brush_entity_is_static(e))
        .filter_map(|e| e.get("model")?.strip_prefix('*')?.parse::<usize>().ok())
        .filter(|&i| i > 0 && i < bsp.models.len())
        .collect()
}

fn brush_entity_offsets(bsp: &Bsp<'_>) -> HashMap<usize, [f32; 3]> {
    let mut out = HashMap::new();
    for e in &bsp.entities {
        let (Some(model), Some(origin)) = (e.get("model"), e.origin()) else {
            continue;
        };
        let Some(idx) = model
            .strip_prefix('*')
            .and_then(|n| n.parse::<usize>().ok())
        else {
            continue;
        };
        let Some(m) = bsp.models.get(idx) else {
            continue;
        };
        for fi in m.first_face..m.first_face + m.num_faces {
            out.insert(fi as usize, origin);
        }
    }
    out
}

fn skip_face(bsp: &Bsp<'_>, face: &Face) -> bool {
    if face.tex_info < 0 {
        return true;
    }
    let flags = bsp.tex_infos[face.tex_info as usize].flags;
    flags & (surf::NODRAW | surf::SKY | surf::SKY2D | surf::TRIGGER | surf::HINT | surf::SKIP) != 0
}

impl WorldRenderer {
    pub fn new(
        gpu: &Gpu,
        mount: &Mount,
        bsp: &Bsp<'_>,
        color_format: wgpu::TextureFormat,
        quality: Quality,
    ) -> Result<Self> {
        let hdr = quality.hdr;
        let device = &gpu.device;
        let atlas_size = 4096u32;
        // the float atlas holds linear light as is (lm_scale stays for the uniform layout)
        let lm_scale: f32 = 1.0;
        let mut atlas = Atlas::new(atlas_size);
        let style_scale = crate::lightmap::style_brightness(bsp);
        let mut lm_faces: Vec<LmFace> = Vec::new();
        // the map's env_tonemap_controller (set by logic_auto OnMapSpawn): the exposure the
        // auto path may use lies between its min and max; without a luminance readback we
        // hold mat_hdr_tonemapscale 1 clamped into that range
        let (mut tm_min, mut tm_max, mut tm_rate, mut tm_bloom) = (0.5f32, 2.0f32, 1.0f32, 1.0f32);
        for e in bsp
            .entities
            .iter()
            .filter(|e| e.classname() == "logic_auto")
        {
            for (k, v) in &e.pairs {
                if k != "OnMapSpawn" {
                    continue;
                }
                let f: Vec<&str> = v.split(',').collect();
                if f.len() >= 3 {
                    match (f[1], f[2].parse::<f32>()) {
                        ("SetAutoExposureMin", Ok(x)) => tm_min = x,
                        ("SetAutoExposureMax", Ok(x)) => tm_max = x,
                        ("SetTonemapRate", Ok(x)) => tm_rate = x,
                        ("SetBloomScale", Ok(x)) => tm_bloom = x,
                        _ => {}
                    }
                }
            }
        }
        let exposure = if hdr {
            1.0f32.clamp(tm_min, tm_max)
        } else {
            1.0
        };
        let white = atlas.white().ok_or_else(|| anyhow!("atlas too small"))?;
        let mut stats = Stats::default();

        // material -> (texture rgba, flags)
        let mut materials: crate::props::MaterialMap = HashMap::new();
        let mut per_material: HashMap<String, Vec<u32>> = HashMap::new();
        let mut vertices: Vec<Vertex> = Vec::new();

        let disps = disp_infos(bsp)?;
        let dverts = disp_verts(bsp)?;
        let disp_by_face: HashMap<u16, &DispInfo> = disps.iter().map(|d| (d.map_face, d)).collect();

        // the map's own env_skypaint: the painted sky (`Sky_Painted`) with its colours
        // as "r g b" floats; an edit_sky entity overrides it on the client
        let map_sky_paint = bsp
            .entities
            .iter()
            .find(|e| e.classname() == "env_skypaint")
            .map(|e| {
                let num =
                    |k: &str, d: f32| e.get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
                let color = |k: &str, d: [f32; 3]| {
                    e.get(k)
                        .and_then(|v| {
                            let p: Vec<f32> = v
                                .split_whitespace()
                                .filter_map(|x| x.parse().ok())
                                .collect();
                            (p.len() == 3).then(|| Vec3::new(p[0], p[1], p[2]))
                        })
                        .unwrap_or(Vec3::from(d))
                };
                SkyPaint {
                    top: color("topcolor", [0.2, 0.5, 1.0]),
                    bottom: color("bottomcolor", [0.8, 1.0, 1.0]),
                    fade_bias: num("fadebias", 1.0),
                    hdr_scale: num("hdrscale", 0.66),
                    sun_normal: color("sunnormal", [0.0, 0.0, 1.0]).normalize_or_zero(),
                    sun_size: num("sunsize", 2.0),
                    sun_color: color("suncolor", [0.2, 0.1, 0.0]),
                    dusk_scale: num("duskscale", 1.0),
                    dusk_intensity: num("duskintensity", 1.0),
                    dusk_color: color("duskcolor", [1.0, 0.2, 0.0]),
                    star_scale: num("starscale", 0.5),
                    star_fade: num("starfade", 1.5),
                    star_layers: if num("drawstars", 1.0) > 0.5 {
                        num("starlayers", 1.0)
                    } else {
                        0.0
                    },
                    star_drift: 0.0,
                }
            });
        let sky_camera = bsp
            .entities
            .iter()
            .find(|e| e.classname() == "sky_camera")
            .and_then(|e| {
                let o = e.origin()?;
                let scale = e
                    .get("scale")
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(16.0);
                Some(SkyCamera {
                    origin: Vec3::from(o),
                    scale,
                })
            });
        let fog = bsp
            .entities
            .iter()
            .filter(|e| e.classname() == "env_fog_controller")
            .find_map(Fog::from_entity);
        let water_volumes = water_volumes(bsp, mount);
        let sky_fog = bsp
            .entities
            .iter()
            .find(|e| e.classname() == "sky_camera")
            .and_then(Fog::from_entity);
        // Faces whose centre lies in the sky camera's area belong to the 3D skybox.
        // Displacement faces are not in leaf face lists, so classify by position.
        let face_offset = brush_entity_offsets(bsp);
        let mut skybox_faces: HashSet<usize> = HashSet::new();
        if let Some(sky) = sky_camera {
            let area = bsp.leafs[bsp.leaf_at(sky.origin.into())].area();
            for (fi, face) in bsp.faces.iter().enumerate() {
                let verts = bsp.face_vertices(face);
                if verts.is_empty() {
                    continue;
                }
                let mut c = face_offset.get(&fi).copied().unwrap_or([0.0; 3]);
                for v in &verts {
                    for k in 0..3 {
                        c[k] += v[k] / verts.len() as f32;
                    }
                }
                // the centre lies on the face's plane, which is also a leaf boundary: looked
                // up as is, a skybox ground face may land in the solid leaf behind it (area
                // 0), get drawn with the world at z 11000 and show through the (skipped) sky
                // as a thin dark corner high in the sky (#144); test a point just in front
                // (and, for a concave face whose centre lies outside it, each vertex pulled
                // 1 u towards the centre)
                let mut nudge = [0.0f32; 3];
                if let Some(plane) = bsp.planes.get(face.plane_num as usize) {
                    let sign = if face.side == 0 { 1.0 } else { -1.0 };
                    nudge = plane.normal.map(|n| n * sign);
                }
                let off = face_offset.get(&fi).copied().unwrap_or([0.0; 3]);
                let samples = std::iter::once(c).chain(verts.iter().map(|v| {
                    let mut p = [0.0f32; 3];
                    for k in 0..3 {
                        let vk = v[k] + off[k];
                        let d = c[k] - vk;
                        let len = ((0..3).map(|j| (c[j] - v[j] - off[j]).powi(2)).sum::<f32>())
                            .sqrt()
                            .max(1e-3);
                        p[k] = vk + d / len;
                    }
                    p
                }));
                if samples.into_iter().any(|mut p| {
                    for k in 0..3 {
                        p[k] += nudge[k];
                    }
                    bsp.leafs[bsp.leaf_at(p)].area() == area
                }) {
                    skybox_faces.insert(fi);
                }
            }
            // faces into a sealed pocket beside the skybox ground keep area 0 (never seen in
            // the game: their leaf is in no PVS); the skybox and the world never overlap in
            // space, so whatever lies within the skybox faces' bounds is skybox too
            if let Some((mins, maxs)) = skybox_faces_bounds(bsp, &skybox_faces) {
                for (fi, face) in bsp.faces.iter().enumerate() {
                    if skybox_faces.contains(&fi) {
                        continue;
                    }
                    let off = face_offset.get(&fi).copied().unwrap_or([0.0; 3]);
                    let verts = bsp.face_vertices(face);
                    if !verts.is_empty()
                        && verts.iter().all(|v| {
                            (0..3).all(|k| {
                                v[k] + off[k] >= mins[k] - 1.0 && v[k] + off[k] <= maxs[k] + 1.0
                            })
                        })
                    {
                        skybox_faces.insert(fi);
                    }
                }
            }
        }
        let mut per_material_sky: HashMap<String, Vec<u32>> = HashMap::new();
        let white_uv = atlas.luxel_center(&white, 0.0, 0.0);

        let mut resolve = |name: &str, stats: &mut Stats| -> Option<(String, u32, u32)> {
            let canon = material::canonical_name(name);
            let entry = materials.entry(canon.clone()).or_insert_with(|| {
                let m = material::load(mount, &canon).ok()?;
                if m.no_draw {
                    return None;
                }
                if m.water || m.shader.contains("reflective") || m.shader.contains("refract") {
                    // no refraction/reflection targets yet: water shows its own volume fog
                    // colour (what the game converges to at fogend), other RT shaders a grey
                    let px = match m.water_fog {
                        Some((c, _, _)) => c.map(|v| (v * 255.0).round() as u8),
                        None => [60, 90, 110],
                    };
                    return Some((
                        m,
                        material::Rgba {
                            width: 1,
                            height: 1,
                            pixels: vec![px[0], px[1], px[2], 255],
                            mips: Vec::new(),
                        },
                        None,
                    ));
                }
                let Some(tex) = m.base_texture.clone() else {
                    return Some((
                        m,
                        material::Rgba {
                            width: 1,
                            height: 1,
                            pixels: vec![128, 128, 128, 255],
                            mips: Vec::new(),
                        },
                        None,
                    ));
                };
                match material::load_texture_lod(mount, &tex, 1024, quality.picmip.max(0) as u32) {
                    Ok(rgba) => Some((m, rgba, None)),
                    Err(_) => {
                        stats.missing_materials.push(format!("{canon} -> {tex}"));
                        None
                    }
                }
            });
            entry
                .as_ref()
                .map(|(_, rgba, _)| (canon, rgba.width, rgba.height))
        };

        let (cubes, cube_view) = load_cubemaps(gpu, mount, bsp, hdr);
        let reflect_sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("water reflection"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mut mirror_cache: HashMap<String, bool> = HashMap::new();
        let mut mirrors: Vec<Mirror> = Vec::new();
        let movable_faces: HashSet<usize> = movable_brush_models(bsp)
            .into_iter()
            .filter_map(|i| bsp.models.get(i))
            .flat_map(|m| m.first_face as usize..(m.first_face + m.num_faces) as usize)
            .collect();
        for (fi, face) in bsp.faces.iter().enumerate() {
            if skip_face(bsp, face) || movable_faces.contains(&fi) {
                continue;
            }
            let Some(name) = bsp.face_material(face) else {
                continue;
            };
            let Some((canon, tw, th)) = resolve(name, &mut stats) else {
                continue;
            };
            let is_mirror_face = *mirror_cache
                .entry(canon.clone())
                .or_insert_with(|| material::load(mount, &canon).is_ok_and(|m| is_mirror(&m)));
            if is_mirror_face {
                let mut n = Vec3::from(bsp.planes[face.plane_num as usize].normal);
                if face.side != 0 {
                    n = -n;
                }
                // func_reflective_glass is a brush entity: its faces sit relative to its origin
                let off = face_offset.get(&fi).map_or(Vec3::ZERO, |o| Vec3::from(*o));
                let poly: Vec<Vec3> = bsp
                    .face_vertices(face)
                    .into_iter()
                    .map(|v| Vec3::from(v) + off)
                    .collect();
                if let Some(first) = poly.first().copied() {
                    let (lo, hi) = poly
                        .iter()
                        .fold((first, first), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
                    mirrors.push(Mirror {
                        normal: n,
                        d: n.dot(first),
                        min: lo,
                        max: hi,
                    });
                }
            }
            let (tw, th) = (tw as f32, th as f32);
            let mut lm_page = 0.0f32;
            let (placement, lit) = if face.light_ofs >= 0 && face.light_styles() > 0 {
                let (w, h) = face.lightmap_dims();
                let raw = bsp.face_lightmap(face, hdr)?;
                let page = (w * h) as usize;
                let bumped = bsp.tex_infos[face.tex_info as usize].flags & surf::BUMPLIGHT != 0;
                let nb = if bumped { 4 } else { 1 };
                // the lump holds one block per light style (each block its bump pages); the
                // styles sum at their brightness (gm_construct's sun is named: style 33)
                let styles: Vec<u8> = face
                    .styles
                    .iter()
                    .copied()
                    .take(face.light_styles())
                    .collect();
                let blocks: Vec<Vec<[f32; 3]>> = (0..styles.len())
                    .map(|si| {
                        let mut block_s = vec![[0f32; 3]; nb * page];
                        for b in 0..nb {
                            let o = (si * nb + b) * page;
                            if let Some(block) = raw.get(o..o + page) {
                                for (dst, src) in
                                    block_s[b * page..(b + 1) * page].iter_mut().zip(block)
                                {
                                    *dst = src.to_linear();
                                }
                            }
                        }
                        block_s
                    })
                    .collect();
                let samples = sum_styles(&blocks, &styles, &style_scale);
                // bumped faces: the four pages side by side, page 0 (flat) first
                let bumped = bumped && samples.len() >= 4 * page;
                let strip = lm_strip(&samples, w, h, bumped);
                let placed = atlas.place(if bumped { 4 * w } else { w }, h, &strip);
                match placed {
                    Some(p) => {
                        if bumped {
                            lm_page = w as f32 / atlas_size as f32;
                        }
                        if styles.iter().any(|&s| s >= 32) {
                            lm_faces.push(LmFace {
                                x: p.x,
                                y: p.y,
                                w,
                                h,
                                bumped,
                                styles,
                                blocks,
                            });
                        }
                        (p, true)
                    }
                    None => {
                        stats.lightmap_overflow += 1;
                        (white, false)
                    }
                }
            } else {
                (white, false)
            };
            // the plane + `side` flip is not reliable for every face (walls came out facing
            // inwards, so the flashlight lit floors only): take the winding's normal instead -
            // faces wind clockwise seen from the front, so the Newell normal points away
            let face_normal = {
                let poly = bsp.face_vertices(face);
                let mut n = Vec3::ZERO;
                for (i, a) in poly.iter().enumerate() {
                    let b = poly[(i + 1) % poly.len()];
                    let (a, b) = (Vec3::from(*a), Vec3::from(b));
                    n += Vec3::new(
                        (a.y - b.y) * (a.z + b.z),
                        (a.z - b.z) * (a.x + b.x),
                        (a.x - b.x) * (a.y + b.y),
                    );
                }
                let pn = bsp.planes[face.plane_num as usize].normal;
                let pn = Vec3::from(pn) * if face.side != 0 { -1.0 } else { 1.0 };
                let n = n.normalize_or_zero();
                if n == Vec3::ZERO { pn } else { -n }.to_array()
            };
            // vbsp bakes the chosen sample into the face's patched material name
            // (`maps/<map>/<material>_<x>_<y>_<z>`); nearest to the face centre otherwise
            let face_cube = cubes.from_patched_name(name).unwrap_or_else(|| {
                let poly = bsp.face_vertices(face);
                let c = poly.iter().fold(Vec3::ZERO, |a, v| a + Vec3::from(*v))
                    / poly.len().max(1) as f32;
                cubes.nearest(c)
            });
            let in_sky = skybox_faces.contains(&fi);
            if in_sky {
                stats.skybox_faces += 1;
            }
            let indices = if in_sky {
                per_material_sky.entry(canon).or_default()
            } else {
                per_material.entry(canon).or_default()
            };
            if let Some(disp) = disp_by_face.get(&(fi as u16)) {
                let mesh = build_disp_mesh(bsp, disp, &dverts)?;
                let base = vertices.len() as u32;
                let side = disp.side_verts() as f32 - 1.0;
                let (lw, lh) = face.lightmap_dims();
                for (i, p) in mesh.positions.iter().enumerate() {
                    let gx = (i % disp.side_verts()) as f32 / side;
                    let gy = (i / disp.side_verts()) as f32 / side;
                    vertices.push(Vertex {
                        tint: [1.0, 1.0, 1.0, 0.0],
                        position: *p,
                        uv: face_uv(bsp, face, *p, tw, th),
                        lm_uv: if lit {
                            atlas.luxel_center(
                                &placement,
                                gx * (lw as f32 - 1.0),
                                gy * (lh as f32 - 1.0),
                            )
                        } else {
                            white_uv
                        },
                        color: [1.0; 3],
                        alpha: mesh.alphas.get(i).copied().unwrap_or(0.0) / 255.0,
                        lm_page,
                        normal: face_normal,
                        cube: face_cube,
                        spec_dir: [0.0; 3],
                        spec_color: [0.0; 3],
                    });
                }
                indices.extend(mesh.indices.iter().map(|i| base + i));
                stats.displacements += 1;
                stats.triangles += mesh.indices.len() / 3;
                continue;
            }
            let poly = bsp.face_vertices(face);
            if poly.len() < 3 {
                continue;
            }
            let base = vertices.len() as u32;
            let off = face_offset.get(&fi).copied().unwrap_or([0.0; 3]);
            for p in &poly {
                vertices.push(Vertex {
                    tint: [1.0, 1.0, 1.0, 0.0],
                    position: [p[0] + off[0], p[1] + off[1], p[2] + off[2]],
                    uv: face_uv(bsp, face, *p, tw, th),
                    lm_uv: if lit {
                        face_lm_uv(bsp, face, *p, &placement, atlas_size as f32)
                    } else {
                        white_uv
                    },
                    color: [1.0; 3],
                    alpha: 0.0,
                    lm_page,
                    normal: face_normal,
                    cube: face_cube,
                    spec_dir: [0.0; 3],
                    spec_color: [0.0; 3],
                });
            }
            for i in 1..poly.len() as u32 - 1 {
                indices.extend_from_slice(&[base, base + i, base + i + 1]);
            }
            stats.faces += 1;
            stats.triangles += poly.len() - 2;
        }

        drop(resolve);
        let light = crate::light::ModelLight::new(bsp, hdr);
        let prop_stats = crate::props::collect_static_props(
            mount,
            bsp,
            &light,
            white_uv,
            &mut vertices,
            &mut materials,
            &mut per_material,
            &mut per_material_sky,
            &skybox_faces_bounds(bsp, &skybox_faces),
            sky_camera.map(|s| bsp.leafs[bsp.leaf_at(s.origin.into())].area()),
            quality.root_lod as usize,
            &|p| cubes.nearest(p),
        )?;
        stats.props = prop_stats.0;
        stats.prop_models = prop_stats.1;
        stats.triangles += prop_stats.2;
        stats.missing_materials.extend(prop_stats.3);

        // GPU resources
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/world.wgsl").into()),
        });
        let lm_tex = upload_rgba(
            gpu,
            &atlas.pixels,
            atlas_size,
            atlas_size,
            wgpu::TextureFormat::Rgba16Float,
            "lightmap",
        );
        let lm_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("lightmap"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::CubeArray,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // the flashlight's shadow map: depth from the light, compared in the world shader
        let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("flashlight depth"),
            size: wgpu::Extent3d {
                width: SHADOW_SIZE,
                height: SHADOW_SIZE,
                depth_or_array_layers: DEPTH_LAYERS as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let shadow_layers: Vec<wgpu::TextureView> = (0..DEPTH_LAYERS as u32)
            .map(|layer| {
                shadow_texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: layer,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        // the flashlight's projected texture (effects/flashlight001; white when absent)
        let (fl_pixels, fl_w, fl_h) =
            match crate::material::load_texture(mount, "effects/flashlight001", 512) {
                Ok(t) => (t.pixels, t.width, t.height),
                Err(_) => (vec![255u8; 4], 1, 1),
            };
        let flashlight_tex = upload_rgba(
            gpu,
            &fl_pixels,
            fl_w,
            fl_h,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            "flashlight001",
        );
        let flashlight_tex_view = flashlight_tex.create_view(&Default::default());
        let flashlight_tex_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("flashlight texture"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("flashlight depth"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera"),
            layout: &camera_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &lm_tex.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&lm_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&cube_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&flashlight_tex_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::Sampler(&flashlight_tex_sampler),
                },
            ],
        });
        // the flashlight depth pass writes a layer of the shadow array, so its camera bind
        // group must not hold that array (wgpu: a texture is not a resource and a depth
        // attachment of one pass): a 1x1 stand-in takes binding 4 there
        let shadow_dummy = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("flashlight depth stand-in"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_dummy_view = shadow_dummy.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let shadow_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera (flashlight depth)"),
            layout: &camera_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &lm_tex.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&lm_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&cube_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&shadow_dummy_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&flashlight_tex_view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::Sampler(&flashlight_tex_sampler),
                },
            ],
        });
        // mat_trilinear / mat_forceaniso (anisotropic filtering implies trilinear)
        let aniso = quality.aniso.clamp(1, 16) as u16;
        let mip_filter = if quality.trilinear || aniso > 1 {
            wgpu::MipmapFilterMode::Linear
        } else {
            wgpu::MipmapFilterMode::Nearest
        };
        let base_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("base"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: mip_filter,
            anisotropy_clamp: aniso,
            ..Default::default()
        });

        let mut indices_all: Vec<u32> = Vec::new();
        let mut batches = Vec::new();
        let mut jobs: Vec<(&String, &Vec<u32>, bool)> = Vec::new();
        for (n, idx) in &per_material_sky {
            jobs.push((n, idx, true));
        }
        for (n, idx) in &per_material {
            jobs.push((n, idx, false));
        }
        jobs.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(b.0)));
        let detail_fallback = upload_rgba_mips(
            gpu,
            &material::Rgba {
                width: 1,
                height: 1,
                pixels: vec![128, 128, 128, 255],
                mips: Vec::new(),
            },
            wgpu::TextureFormat::Rgba8Unorm,
            "detail fallback",
        )
        .create_view(&Default::default());
        let mut detail_cache: HashMap<String, Option<wgpu::TextureView>> = HashMap::new();
        let mut base2_cache: HashMap<String, Option<wgpu::TextureView>> = HashMap::new();
        let mut modulate_cache: HashMap<String, Option<wgpu::TextureView>> = HashMap::new();
        let mut bump_cache: HashMap<String, Option<wgpu::TextureView>> = HashMap::new();
        let mut envmask_cache: HashMap<String, Option<wgpu::TextureView>> = HashMap::new();
        let mut phongexp_cache: HashMap<String, Option<wgpu::TextureView>> = HashMap::new();
        let bump_fallback = upload_rgba_mips(
            gpu,
            &material::Rgba {
                width: 1,
                height: 1,
                pixels: vec![128, 128, 255, 255],
                mips: Vec::new(),
            },
            wgpu::TextureFormat::Rgba8Unorm,
            "bump fallback",
        )
        .create_view(&Default::default());
        let white_fallback = upload_rgba_mips(
            gpu,
            &material::Rgba {
                width: 1,
                height: 1,
                pixels: vec![255, 255, 255, 255],
                mips: Vec::new(),
            },
            wgpu::TextureFormat::Rgba8Unorm,
            "white fallback",
        )
        .create_view(&Default::default());
        let mut reflect_entities = false;
        for (name, idx, sky) in jobs {
            let Some(Some((mat, rgba, _))) = materials.get(name) else {
                continue;
            };
            reflect_entities |= mat.water && mat.reflect_entities;
            let base2_view = mat
                .base_texture2
                .as_ref()
                .filter(|_| mat.shader == "worldvertextransition")
                .and_then(|t| {
                    base2_cache
                        .entry(t.clone())
                        .or_insert_with(|| {
                            let rgba = material::load_texture(mount, t, 1024).ok()?;
                            let tex = upload_rgba_mips(
                                gpu,
                                &rgba,
                                wgpu::TextureFormat::Rgba8UnormSrgb,
                                t,
                            );
                            Some(tex.create_view(&Default::default()))
                        })
                        .as_ref()
                });
            let blend = base2_view.is_some() as u32;
            let base2_view = base2_view.unwrap_or(&detail_fallback);
            let modulate_view = mat
                .blend_modulate
                .as_ref()
                .filter(|_| blend != 0)
                .and_then(|t| {
                    modulate_cache
                        .entry(t.clone())
                        .or_insert_with(|| {
                            let rgba = material::load_texture(mount, t, 1024).ok()?;
                            let tex =
                                upload_rgba_mips(gpu, &rgba, wgpu::TextureFormat::Rgba8Unorm, t);
                            Some(tex.create_view(&Default::default()))
                        })
                        .as_ref()
                });
            let bump_view = mat.bumpmap.as_ref().and_then(|t| {
                bump_cache
                    .entry(t.clone())
                    .or_insert_with(|| {
                        let rgba = material::load_texture(mount, t, 1024).ok()?;
                        let tex = upload_rgba_mips(gpu, &rgba, wgpu::TextureFormat::Rgba8Unorm, t);
                        Some(tex.create_view(&Default::default()))
                    })
                    .as_ref()
            });
            let envmask_view = mat.envmapmask.as_ref().and_then(|t| {
                envmask_cache
                    .entry(t.clone())
                    .or_insert_with(|| {
                        let rgba = material::load_texture(mount, t, 1024).ok()?;
                        let tex = upload_rgba_mips(gpu, &rgba, wgpu::TextureFormat::Rgba8Unorm, t);
                        Some(tex.create_view(&Default::default()))
                    })
                    .as_ref()
            });
            let uv2 = uv2_params(mat, modulate_view.is_some(), bump_view.is_some());
            let bump_view = bump_view.unwrap_or(&bump_fallback);
            let envmask_view = envmask_view.unwrap_or(&white_fallback);
            let phongexp_view = mat
                .phong
                .as_ref()
                .and_then(|p| p.exponent_texture.as_ref())
                .and_then(|t| {
                    phongexp_cache
                        .entry(t.clone())
                        .or_insert_with(|| {
                            let rgba = material::load_texture(mount, t, 1024).ok()?;
                            let tex =
                                upload_rgba_mips(gpu, &rgba, wgpu::TextureFormat::Rgba8Unorm, t);
                            Some(tex.create_view(&Default::default()))
                        })
                        .as_ref()
                })
                .unwrap_or(&white_fallback);
            let modulate_view = modulate_view.unwrap_or(&detail_fallback);
            let water = match mat.water_fog {
                Some((c, start, end)) => {
                    let c = c.map(material::srgb_to_linear);
                    ([c[0], c[1], c[2], 1.0], [start, end, 0.0, 0.0])
                }
                None => ([0.0; 4], [0.0; 4]),
            };
            let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("material params"),
                contents: bytemuck::bytes_of(&MaterialParams {
                    alpha_test: mat.alpha_test as u32,
                    unlit: is_unlit(&mat.shader) as u32,
                    translucent: mat.translucent as u32,
                    blend,
                    detail: detail_params(mat),
                    uv2_m: uv2.0,
                    uv2_t: uv2.1,
                    water_color: water.0,
                    water_range: [
                        water.1[0],
                        water.1[1],
                        f32::from(u8::from(is_mirror(mat))),
                        0.0,
                    ],
                    envmap: envmap_params(mat),
                    envmap2: envmap2_params(mat),
                    envmap3: envmap3_params(mat),
                    selfillum_tint: selfillum_tint(mat, DEFAULT_WEAPON_COLOR),
                    phong: phong_params(mat),
                    phong_fresnel: phong_fresnel_params(mat),
                }),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let tex = upload_rgba_mips(gpu, rgba, wgpu::TextureFormat::Rgba8UnormSrgb, name);
            let detail_view = mat
                .detail
                .as_ref()
                .and_then(|d| {
                    detail_cache
                        .entry(d.texture.clone())
                        .or_insert_with(|| {
                            let rgba = material::load_texture(mount, &d.texture, 1024).ok()?;
                            let t = upload_rgba_mips(
                                gpu,
                                &rgba,
                                wgpu::TextureFormat::Rgba8UnormSrgb,
                                &d.texture,
                            );
                            Some(t.create_view(&Default::default()))
                        })
                        .as_ref()
                })
                .unwrap_or(&detail_fallback);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(name),
                layout: &material_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &tex.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&base_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(detail_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(base2_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(modulate_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(bump_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,

                        resource: wgpu::BindingResource::TextureView(envmask_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 8,

                        resource: wgpu::BindingResource::TextureView(phongexp_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: params.as_entire_binding(),
                    },
                ],
            });
            let centre = if idx.is_empty() {
                Vec3::ZERO
            } else {
                idx.iter()
                    .map(|&i| Vec3::from(vertices[i as usize].position))
                    .sum::<Vec3>()
                    / idx.len() as f32
            };
            batches.push(Batch {
                bind_group,
                first_index: indices_all.len() as u32,
                index_count: idx.len() as u32,
                material: name.clone(),
                sky,
                translucent: mat.translucent,
                nocull: mat.nocull,
                water: mat.water || is_mirror(mat),
                below_water: mat.below_water,
                centre,
            });
            indices_all.extend_from_slice(idx);
        }
        stats.materials = batches.len();

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("world vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("world indices"),
            contents: bytemuck::cast_slice(&indices_all),
            usage: wgpu::BufferUsages::INDEX,
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world"),
            bind_group_layouts: &[Some(&camera_layout), Some(&material_layout)],
            ..Default::default()
        });
        let make_pipeline = |label: &str,
                             fs: &str,
                             blend: Option<wgpu::BlendState>,
                             depth_write: bool,
                             cull: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x2, 3 => Float32x3, 4 => Float32, 5 => Float32, 6 => Float32x3, 7 => Float32, 8 => Float32x3, 9 => Float32x3, 10 => Float32x4],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                // BSP faces wind clockwise seen from their front; the 2D sky cube is seen
                // from inside, so it is the one pass without back-face culling
                front_face: wgpu::FrontFace::Cw,
                cull_mode: if cull { Some(wgpu::Face::Back) } else { None },
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(depth_write),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: quality.msaa,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
        };
        let pipeline = make_pipeline("world", "fs_main", None, true, true);
        let sky_pipeline = make_pipeline("sky", "fs_main", None, true, false);
        let skypaint_pipeline = make_pipeline("skypaint", "fs_skypaint", None, true, false);
        let nocull_pipeline = make_pipeline("world nocull", "fs_main", None, true, false);
        // depth only, from the flashlight, into the (single-sampled) shadow map
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("flashlight depth"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x2, 3 => Float32x3, 4 => Float32, 5 => Float32, 6 => Float32x3, 7 => Float32, 8 => Float32x3, 9 => Float32x3, 10 => Float32x4],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        // water: the scene depth of the main pass, read in a second pass with a read-only
        // depth attachment
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene depth"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: quality.msaa > 1,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let water_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("water"),
            bind_group_layouts: &[
                Some(&camera_layout),
                Some(&material_layout),
                Some(&depth_layout),
            ],
            ..Default::default()
        });
        let water_src = include_str!("shaders/world.wgsl");
        let water_src = if quality.msaa > 1 {
            // only the scene depth is multisampled; the flashlight shadow array
            // (`texture_depth_2d_array`) shares the prefix and has no multisampled form
            water_src.replace(
                "scene_depth: texture_depth_2d;",
                "scene_depth: texture_depth_multisampled_2d;",
            )
        } else {
            water_src.to_string()
        };
        let water_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("water"),
            source: wgpu::ShaderSource::Wgsl(water_src.into()),
        });
        let water_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("water"),
            layout: Some(&water_layout),
            vertex: wgpu::VertexState {
                module: &water_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x2, 3 => Float32x3, 4 => Float32, 5 => Float32, 6 => Float32x3, 7 => Float32, 8 => Float32x3, 9 => Float32x3, 10 => Float32x4],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                // BSP faces wind clockwise seen from their front; the 2D sky cube is seen
                // from inside, so it is the one pass without back-face culling
                front_face: wgpu::FrontFace::Cw,
                // both sides: the surface is seen from under the water too
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: quality.msaa,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &water_shader,
                entry_point: Some("fs_water"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
;
        let refract_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("refract"),
            layout: Some(&water_layout),
            vertex: wgpu::VertexState {
                module: &water_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x2, 3 => Float32x3, 4 => Float32, 5 => Float32, 6 => Float32x3, 7 => Float32, 8 => Float32x3, 9 => Float32x3, 10 => Float32x4],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                // BSP faces wind clockwise seen from their front; the 2D sky cube is seen
                // from inside, so it is the one pass without back-face culling
                front_face: wgpu::FrontFace::Cw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: quality.msaa,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &water_shader,
                entry_point: Some("fs_refract"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
;
        let translucent_nocull_pipeline = make_pipeline(
            "translucent nocull",
            "fs_main",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            false,
        );
        let translucent_pipeline = make_pipeline(
            "models translucent",
            "fs_main",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            true,
        );
        let alpha_pipeline = make_pipeline(
            "alpha",
            "fs_main",
            Some(wgpu::BlendState::ALPHA_BLENDING),
            false,
            false,
        );
        let additive_pipeline = make_pipeline(
            "additive",
            "fs_main",
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            }),
            false,
            false,
        );
        let modulate_pipeline = make_pipeline(
            "modulate",
            "fs_main",
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Dst,
                    dst_factor: wgpu::BlendFactor::Src,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            }),
            false,
            true,
        );

        let sky2d = build_sky_cube(
            gpu,
            mount,
            bsp,
            &material_layout,
            white_uv,
            &detail_fallback,
        );

        Ok(WorldRenderer {
            weapon_color: DEFAULT_WEAPON_COLOR,
            dlights: Vec::new(),
            lm_scale,

            exposure,

            hdr,
            sun: crate::props::sun_direction(bsp),
            env_sun: crate::props::env_sun(bsp),
            quality,
            tm_min,
            tm_max,
            tm_rate,
            tm_bloom,
            bloom_amount: tm_bloom,
            pipeline,
            sky_pipeline,
            skypaint_pipeline,
            detail_fallback,
            bump_fallback,
            white_fallback,
            cubes,
            lm_tex,
            lm_faces,
            style_scale,
            color_format,
            reflect: Mutex::new(ReflectState::default()),
            reflect_sampler,
            t0: std::time::Instant::now(),
            reflect_entities,
            nocull_pipeline,
            translucent_nocull_pipeline,
            water_pipeline,
            refract_pipeline,
            depth_layout,
            additive_pipeline,
            alpha_pipeline,
            modulate_pipeline,
            translucent_pipeline,
            camera_buffer,
            flashlights: Vec::new(),
            flashlight_vp: std::cell::Cell::new([None; DEPTH_LAYERS]),
            shadow_pipeline,
            shadow_layers,
            camera_bind_group,
            shadow_camera_bind_group,
            vertex_buffer,
            index_buffer,
            batches,
            sky_camera,
            fog,
            sky_fog,
            fog_override: None,
            sky_paint: map_sky_paint,
            map_sky_paint,
            water_volumes,
            mirrors,
            stats,
            sky2d,
            material_layout,
            base_sampler,
            white_uv,
            light,
            light_cache: Default::default(),
            frame: Default::default(),
            pass_ms: Default::default(),
            model_cache: Default::default(),
        })
    }

    pub fn materials(&self) -> impl Iterator<Item = &str> {
        self.batches.iter().map(|b| b.material.as_str())
    }

    /// Render one frame into `color` (cleared to sky grey) and `depth`.
    /// The 3D skybox, if any, is drawn first from the sky camera and the depth cleared after it.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        gpu: &Gpu,
        camera: &Camera,
        color: &wgpu::TextureView,
        resolve: Option<&wgpu::TextureView>,
        depth: &wgpu::TextureView,
        aspect: f32,
        scene_src: Option<&wgpu::Texture>,
        reflect_items: &[DrawItem<'_>],
    ) {
        self.frame.set(self.frame.get() + 1);
        let t0 = std::time::Instant::now();
        let mut vps = [None; DEPTH_LAYERS];
        for (slot, (_, l)) in self.flashlights.iter().take(SHADOW_LAYERS).enumerate() {
            vps[slot] =
                Some(self.draw_flashlight_depth(gpu, l, &self.shadow_layers[slot], reflect_items));
        }
        vps[SUN_LAYER] =
            self.draw_sun_depth(gpu, camera, &self.shadow_layers[SUN_LAYER], reflect_items);
        self.flashlight_vp.set(vps);
        let t1 = std::time::Instant::now();
        self.draw_reflection(gpu, camera, aspect, reflect_items);
        let t2 = std::time::Instant::now();
        self.draw_inner(
            gpu,
            camera,
            color,
            resolve,
            depth,
            aspect,
            scene_src,
            reflect_items,
        );
        let t3 = std::time::Instant::now();
        let ms = |a: std::time::Instant, b: std::time::Instant| (b - a).as_secs_f32() * 1000.0;
        let now = [ms(t0, t1), ms(t1, t2), ms(t2, t3)];
        let old = self.pass_ms.get();
        self.pass_ms.set(std::array::from_fn(|k| {
            if old[k] <= 0.0 {
                now[k]
            } else {
                old[k] * 0.9 + now[k] * 0.1
            }
        }));
    }

    /// The plane the reflection pass mirrors in: the water surface under the eye, else the
    /// nearest `func_reflective_glass` facing the eye within 3000 u (one reflection a frame).
    /// CPU ms of the last [`draw`](Self::draw), smoothed: shadow maps, water reflection,
    /// scene (`cl_showfps 2`).
    pub fn pass_ms(&self) -> [f32; 3] {
        self.pass_ms.get()
    }

    fn reflected_plane(&self, eye: Vec3) -> Option<(Vec3, f32)> {
        self.reflected_surface(eye).map(|(n, d, _, _)| (n, d))
    }

    /// The reflecting surface for an eye: its plane (normal, distance) and the bounds of the
    /// water volume or mirror it belongs to.
    fn reflected_surface(&self, eye: Vec3) -> Option<(Vec3, f32, Vec3, Vec3)> {
        // the nearer of the two wins: gm_construct's basement mirror sits far below the
        // pool's surface, which must not claim the pass from the other end of the map
        let dist = |m: &Mirror| eye.clamp(m.min, m.max).distance_squared(eye);
        let mirror = self
            .mirrors
            .iter()
            .filter(|m| m.normal.z.abs() < 0.5 && eye.dot(m.normal) - m.d > 0.0)
            .min_by(|a, b| dist(a).total_cmp(&dist(b)))
            .filter(|m| dist(m) < 3000.0 * 3000.0);
        let water = self
            .water_volumes
            .iter()
            .filter(|w| w.surface_z < eye.z)
            .map(|w| (eye.clamp(w.min, w.max).distance_squared(eye), w))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        match (mirror, water) {
            (Some(m), Some((wd, _))) if dist(m) < wd => Some((m.normal, m.d, m.min, m.max)),
            // the surface rectangle alone decides the visibility test, not the depth below
            (_, Some((_, w))) => Some((
                Vec3::Z,
                w.surface_z,
                w.min.with_z(w.surface_z),
                w.max.with_z(w.surface_z),
            )),
            (Some(m), None) => Some((m.normal, m.d, m.min, m.max)),
            (None, None) => None,
        }
    }
    /// `_rt_WaterReflection`: the scene above the water plane seen from the eye mirrored in
    /// it, drawn by a real camera below the plane (pitch and roll negated, so the winding
    /// stays and the image is read upside down); everything below the plane is clipped.
    fn draw_reflection(&self, gpu: &Gpu, camera: &Camera, aspect: f32, items: &[DrawItem<'_>]) {
        if std::env::var_os("GM_RENDER_LOG").is_some() {
            eprintln!(
                "reflection: eye {:?} plane {:?} mirrors {:?}",
                camera.position,
                self.reflected_plane(camera.position),
                self.mirrors
                    .iter()
                    .map(|m| (m.normal, m.d, m.min, m.max))
                    .collect::<Vec<_>>()
            );
        }
        // GM_NO_REFLECT: draw the water without the reflection (to compare); a surface
        // wholly outside the view is not reflected either (the game only renders
        // _rt_WaterReflection while a water surface is in the visible leaves)
        let vp = camera.view_proj(aspect);
        let plane = self
            .reflected_surface(camera.position)
            .filter(|_| std::env::var_os("GM_NO_REFLECT").is_none())
            .filter(|&(_, _, min, max)| aabb_in_frustum(vp, min, max));
        let Some((n, zs, _, _)) = plane else {
            self.reflect.lock().unwrap().valid = false;
            return;
        };
        let vertical = n.z.abs() < 0.5;
        let w = REFLECT_WIDTH;
        let h = ((w as f32 / aspect).round() as u32).max(1);
        {
            let mut state = self.reflect.lock().unwrap();
            if state.target.as_ref().is_none_or(|t| t.size != (w, h)) {
                state.target = Some(ReflectTarget::new(
                    gpu,
                    w,
                    h,
                    self.color_format,
                    self.quality.msaa,
                ));
            }
            state.clip = Some(zs);
            state.normal = n;
            state.flip_x = vertical;
            state.valid = false;
        }
        let mirrored = if vertical {
            // a vertical mirror (normal at yaw th): the eye reflected over the plane, the yaw
            // mirrored (2 th + 180 - yaw), the roll negated; the image is the reflection
            // mirrored left-right, which the shader undoes
            let p = camera.position;
            let th = n.y.atan2(n.x).to_degrees();
            Camera {
                position: p - n * (2.0 * (p.dot(n) - zs)),
                angles: Vec3::new(
                    camera.angles.x,
                    2.0 * th + 180.0 - camera.angles.y,
                    -camera.angles.z,
                ),
                ..*camera
            }
        } else {
            Camera {
                position: Vec3::new(
                    camera.position.x,
                    camera.position.y,
                    2.0 * zs - camera.position.z,
                ),
                angles: Vec3::new(-camera.angles.x, camera.angles.y, -camera.angles.z),
                ..*camera
            }
        };
        let (color, resolve, depth) = {
            let state = self.reflect.lock().unwrap();
            let t = state.target.as_ref().unwrap();
            (t.color.clone(), t.resolve.clone(), t.depth.clone())
        };
        self.draw_inner(
            gpu,
            &mirrored,
            &color,
            resolve.as_ref(),
            &depth,
            aspect,
            None,
            // `$reflectentities`: the entity models too, still under the clip plane
            if self.reflect_entities { items } else { &[] },
        );
        let mut state = self.reflect.lock().unwrap();
        state.clip = None;
        state.valid = true;
    }

    /// The clip plane of the running reflection pass, in skybox units for the 3D skybox.
    fn clip_for(&self, sky: bool) -> Option<f32> {
        let (z, up) = {
            let st = self.reflect.lock().unwrap();
            (st.clip?, st.normal.z > 0.99)
        };
        if sky {
            let s = self.sky_camera?;
            // a mirror's plane means nothing in skybox units: the skybox is not clipped
            Some(if up {
                s.origin.z + z / s.scale
            } else {
                f32::MIN
            })
        } else {
            Some(z)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_inner(
        &self,
        gpu: &Gpu,
        camera: &Camera,
        color: &wgpu::TextureView,
        resolve: Option<&wgpu::TextureView>,
        depth: &wgpu::TextureView,
        aspect: f32,
        scene_src: Option<&wgpu::Texture>,
        items: &[DrawItem<'_>],
    ) {
        let mut clear = true;
        if self.sky2d.is_some() {
            self.sky_pass(gpu, camera, color, resolve, depth, aspect);
            clear = false;
        }
        let has_sky = self.sky_camera.is_some() && self.batches.iter().any(|b| b.sky);
        if let (true, Some(sky)) = (has_sky, self.sky_camera) {
            let sky_cam = Camera {
                position: sky.origin + camera.position / sky.scale,
                near: 1.0,
                far: camera.far / sky.scale * 4.0,
                ..*camera
            };
            // under water the skybox takes the water fog too, in skybox units
            let view = self.view_fog(camera);
            let under = view.filter(|v| {
                self.fog
                    .is_none_or(|f| v.start != f.start || v.end != f.end || v.color != f.color)
            });
            let sky_fog = match under {
                Some(u) => Some(Fog {
                    start: u.start / sky.scale,
                    end: u.end / sky.scale,
                    ..u
                }),
                None => self.sky_fog,
            };
            self.pass(
                gpu, &sky_cam, color, resolve, depth, aspect, true, clear, sky_fog, scene_src,
                items,
            );
            self.pass(
                gpu, camera, color, resolve, depth, aspect, false, false, None, scene_src, items,
            );
        } else {
            self.pass(
                gpu, camera, color, resolve, depth, aspect, false, clear, None, scene_src, items,
            );
        }
    }

    /// The fog an eye at `camera.position` sees: the `$bottommaterial`'s when it is inside a
    /// water volume below its surface, else the map's.
    pub fn view_fog(&self, camera: &Camera) -> Option<Fog> {
        // the reflection pass renders from the mirrored eye, which sits under the surface
        // whenever the real eye is near it; the fog is the real eye's (the game's reflection
        // is the scene as seen from above the water, never the water's own fog)
        let mut p = camera.position;
        if let Some(zs) = self.reflect.lock().unwrap().clip {
            p.z = 2.0 * zs - p.z;
        }
        let base = self
            .water_volumes
            .iter()
            .find(|w| {
                p.z < w.surface_z && p.cmpge(w.min).all() && p.cmple(w.max).all() && w.fog.is_some()
            })
            .and_then(|w| w.fog)
            .or(self.fog);
        self.fog_override.or(base)
    }

    fn camera_uniform(
        &self,
        cam: &Camera,
        aspect: f32,
        alpha: f32,
        fog: Option<Fog>,
        clip: Option<f32>,
    ) -> CameraUniform {
        let valid = {
            let st = self.reflect.lock().unwrap();
            (st.valid as u32 | ((st.refr_valid as u32) << 1) | ((st.flip_x as u32) << 2)) as f32
        };
        let f = fog.unwrap_or(Fog {
            color: [0.0; 3],
            start: 0.0,
            end: 1.0,
            max_density: 0.0,
        });
        let fw = cam.forward();
        let sp = self.sky_paint;
        let v4 = |v: Vec3, w: f32| [v.x, v.y, v.z, w];
        CameraUniform {
            sun: [self.sun.x, self.sun.y, self.sun.z, 0.0],
            sky_top: sp.map_or([0.0; 4], |s| v4(s.top, s.fade_bias)),
            sky_bottom: sp.map_or([0.0; 4], |s| {
                v4(s.bottom, if self.hdr { s.hdr_scale } else { 1.0 })
            }),
            sky_sun: sp.map_or([0.0; 4], |s| v4(s.sun_normal, s.sun_size)),
            sky_sun_color: sp.map_or([0.0; 4], |s| v4(s.sun_color, s.star_drift)),
            sky_dusk: sp.map_or([0.0; 4], |s| v4(s.dusk_color, s.dusk_intensity)),
            flashlight_vp: {
                let vps = self.flashlight_vp.get();
                std::array::from_fn(|s| vps[s].unwrap_or(Mat4::IDENTITY).to_cols_array_2d())
            },
            sun_shadow: [
                f32::from(self.flashlight_vp.get()[SUN_LAYER].is_some()),
                SUN_SHADOW_DIST / SUN_SHADOW_RANGE,
                SUN_SHADOW_ALPHA,
                0.0,
            ],
            shadow_slot: {
                let vps = self.flashlight_vp.get();
                let mut slots = [[-1.0f32; 4]; 2];
                for (s, (i, _)) in self.flashlights.iter().take(SHADOW_LAYERS).enumerate() {
                    if vps[s].is_some() && *i < MAX_DLIGHTS {
                        slots[i / 4][i % 4] = s as f32;
                    }
                }
                slots
            },
            sky_dusk_scale: sp.map_or([0.0; 4], |s| {
                // no star texture: no star layers (the sky face must not stand in for it)
                let layers = if self.sky2d.as_ref().is_some_and(|k| k.stars.is_some()) {
                    s.star_layers
                } else {
                    0.0
                };
                [s.dusk_scale, s.star_scale, s.star_fade, layers]
            }),
            view_proj: cam.view_proj(aspect).to_cols_array_2d(),
            alpha,
            gamma: cam.gamma,
            lm_scale: self.lm_scale,
            exposure: self.exposure,
            eye: [
                cam.position.x,
                cam.position.y,
                cam.position.z,
                if fog.is_some() { 1.0 } else { 0.0 },
            ],
            forward: [fw.x, fw.y, fw.z, 0.0],
            fog_color: [f.color[0], f.color[1], f.color[2], f.max_density],
            fog_range: [f.start, f.end, cam.near, cam.far],
            dlight_pos: std::array::from_fn(|i| match self.dlights.get(i) {
                Some(l) => [l.pos.x, l.pos.y, l.pos.z, l.radius],
                None => [0.0; 4],
            }),
            dlight_color: std::array::from_fn(|i| match self.dlights.get(i) {
                Some(l) => [l.color.x, l.color.y, l.color.z, 0.0],
                None => [0.0; 4],
            }),
            dlight_dir: std::array::from_fn(|i| match self.dlights.get(i) {
                Some(l) => [l.dir.x, l.dir.y, l.dir.z, l.cone],
                None => [0.0; 4],
            }),
            clip: {
                let t = self.t0.elapsed().as_secs_f32();
                match clip {
                    Some(z) => [z, valid, t, 1.0],
                    None => [0.0, valid, t, 0.0],
                }
            },
            clip_plane: {
                let st = self.reflect.lock().unwrap();
                let n = if st.normal == Vec3::ZERO {
                    Vec3::Z
                } else {
                    st.normal
                };
                [n.x, n.y, n.z, clip.unwrap_or(0.0)]
            },
        }
    }

    /// Rasterizes `text` with a TTF at `px` into an 8-bit coverage bitmap: (width, height,
    /// coverage, baseline y). For textures painted on the CPU (the toolgun's screen).
    pub fn raster_text(ttf: &[u8], px: f32, text: &str) -> Option<(u32, u32, Vec<u8>, f32)> {
        crate::ui::raster_text(ttf, px, text)
    }

    /// The dynamic lights of the next frame (the first `MAX_DLIGHTS` are lit).
    /// `render.ComputeLighting`: the baked light at `pos` on a surface facing `normal` (the
    /// leaf's ambient cube and the direct lights, as a model's origin is lit).
    pub fn light_at(&self, pos: Vec3, normal: Vec3) -> Vec3 {
        self.light.at(pos, 4).shade(normal, false)
    }

    pub fn set_dlights(&mut self, lights: &[DLight]) {
        self.dlights.clear();
        self.dlights
            .extend(lights.iter().filter(|l| l.radius > 0.0).take(MAX_DLIGHTS));
    }

    /// The 2D sky cube around an eye at the origin, clearing colour and depth.
    fn sky_pass(
        &self,
        gpu: &Gpu,
        camera: &Camera,
        color: &wgpu::TextureView,
        resolve: Option<&wgpu::TextureView>,
        depth: &wgpu::TextureView,
        aspect: f32,
    ) {
        let Some(sky) = &self.sky2d else { return };
        let cam = Camera {
            position: Vec3::ZERO,
            near: 1.0,
            far: SKY_CUBE_HALF * 4.0,
            ..*camera
        };
        gpu.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&self.camera_uniform(&cam, aspect, 1.0, None, None)),
        );
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sky2d"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: resolve,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(if self.sky_paint.is_some() {
                &self.skypaint_pipeline
            } else {
                &self.sky_pipeline
            });
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, sky.vertex_buffer.slice(..));
            pass.set_index_buffer(sky.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            let stars = self
                .sky_paint
                .is_some_and(|s| s.star_layers > 0.0)
                .then_some(sky.stars.as_ref())
                .flatten();
            for (i, bg) in sky.faces.iter().enumerate() {
                pass.set_bind_group(1, stars.unwrap_or(bg), &[]);
                let first = i as u32 * 6;
                pass.draw_indexed(first..first + 6, 0, 0..1);
            }
        }
        gpu.queue.submit([enc.finish()]);
    }

    #[allow(clippy::too_many_arguments)]
    fn pass(
        &self,
        gpu: &Gpu,
        camera: &Camera,
        color: &wgpu::TextureView,
        resolve: Option<&wgpu::TextureView>,
        depth: &wgpu::TextureView,
        aspect: f32,
        sky: bool,
        clear_color: bool,
        sky_fog: Option<Fog>,
        scene_src: Option<&wgpu::Texture>,
        items: &[DrawItem<'_>],
    ) {
        // the refraction copy is taken in this pass when it has the resolved scene
        self.reflect.lock().unwrap().refr_valid = scene_src.is_some() && !sky;
        let cam_uniform = self.camera_uniform(
            camera,
            aspect,
            1.0,
            if sky { sky_fog } else { self.view_fog(camera) },
            self.clip_for(sky),
        );
        gpu.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&cam_uniform));
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(if sky { "skybox" } else { "world" }),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: resolve,
                    ops: wgpu::Operations {
                        load: if clear_color {
                            wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.35,
                                g: 0.45,
                                b: 0.6,
                                a: 1.0,
                            })
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            for b in self
                .batches
                .iter()
                .filter(|b| b.sky == sky && !b.translucent && !b.water)
            {
                pass.set_pipeline(if b.nocull {
                    &self.nocull_pipeline
                } else {
                    &self.pipeline
                });
                pass.set_bind_group(1, &b.bind_group, &[]);
                pass.draw_indexed(b.first_index..b.first_index + b.index_count, 0, 0..1);
            }
            // the frame's entity models go between the opaque world and the translucent
            // surfaces (glass blends over them, the refraction copy has them); their draw
            // writes the camera uniform itself, so ours is restored after
            drop(pass);
            gpu.queue.submit([enc.finish()]);
            if !sky && !items.is_empty() {
                self.draw_models(
                    gpu,
                    camera,
                    &ModelTarget {
                        color,
                        resolve,
                        depth,
                        aspect,
                        clear_depth: false,
                        alpha: 1.0,
                    },
                    items,
                );
                gpu.queue
                    .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&cam_uniform));
            }
            enc = gpu.device.create_command_encoder(&Default::default());
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("translucent"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: resolve,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            // translucent surfaces after everything opaque, farthest first, no depth write
            let mut trans: Vec<&Batch> = self
                .batches
                .iter()
                .filter(|b| b.sky == sky && b.translucent)
                .collect();
            trans.sort_by(|a, b| {
                let da = a.centre.distance_squared(camera.position);
                let db = b.centre.distance_squared(camera.position);
                db.total_cmp(&da)
            });
            for b in trans {
                pass.set_pipeline(if b.nocull {
                    &self.translucent_nocull_pipeline
                } else {
                    &self.translucent_pipeline
                });
                pass.set_bind_group(1, &b.bind_group, &[]);
                pass.draw_indexed(b.first_index..b.first_index + b.index_count, 0, 0..1);
            }
        }
        let water: Vec<&Batch> = self
            .batches
            .iter()
            .filter(|b| {
                b.sky == sky && b.water && !(b.below_water && camera.position.z > b.centre.z)
            })
            .collect();
        if !water.is_empty() && self.clip_for(false).is_none() {
            let mut state = self.reflect.lock().unwrap();
            if let Some(src) = scene_src {
                // `_rt_WaterRefraction`: the opaque scene behind the surface, copied before
                // the water is drawn over it
                let size = src.size();
                if state
                    .refraction
                    .as_ref()
                    .is_none_or(|(t, _)| t.size() != size || t.format() != src.format())
                {
                    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("water refraction"),
                        size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: src.format(),
                        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    });
                    let view = tex.create_view(&Default::default());
                    state.refraction = Some((tex, view));
                }
                let (tex, _) = state.refraction.as_ref().unwrap();
                enc.copy_texture_to_texture(src.as_image_copy(), tex.as_image_copy(), size);
            }
            let state = &*state;
            let reflection = match (&state.target, state.valid) {
                (Some(t), true) => &t.sampled,
                _ => &self.bump_fallback,
            };
            let refraction = match (&state.refraction, scene_src) {
                (Some((_, v)), Some(_)) => v,
                _ => &self.white_fallback,
            };
            let depth_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scene depth"),
                layout: &self.depth_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(depth),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(reflection),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.reflect_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(refraction),
                    },
                ],
            });
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("water"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: resolve,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: None,
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.water_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(2, &depth_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            for b in water {
                pass.set_bind_group(1, &b.bind_group, &[]);
                pass.draw_indexed(b.first_index..b.first_index + b.index_count, 0, 0..1);
            }
        }
        gpu.queue.submit([enc.finish()]);
    }
}

/// The model a body part shows for Source's `SetBodyGroups` string: digit `part` of `body`,
/// when it is a digit below the part's model count `n`, else 0.
pub fn body_group_choice(body: &str, part: usize, n: usize) -> usize {
    body.as_bytes()
        .get(part)
        .filter(|b| b.is_ascii_digit())
        .map(|b| (b - b'0') as usize)
        .filter(|&i| i < n)
        .unwrap_or(0)
}

impl WorldRenderer {
    /// Loads `path` (body group 0, skin 0) for `draw_models`; `None` if it has no drawable mesh.
    /// The named light styles at their brightness (`light` TurnOn / TurnOff / SetPattern from
    /// the server): the faces they light are re-summed and rewritten into the atlas.
    pub fn set_lightstyles(&mut self, gpu: &Gpu, styles: &[(u8, f32)]) {
        let mut changed: Vec<u8> = Vec::new();
        for &(style, b) in styles {
            if (self.style_scale[style as usize] - b).abs() > 1e-4 {
                self.style_scale[style as usize] = b;
                changed.push(style);
            }
        }
        if changed.is_empty() {
            return;
        }
        for f in &self.lm_faces {
            if !f.styles.iter().any(|s| changed.contains(s)) {
                continue;
            }
            let sum = sum_styles(&f.blocks, &f.styles, &self.style_scale);
            let strip = lm_strip(&sum, f.w, f.h, f.bumped);
            let sw = if f.bumped { 4 * f.w } else { f.w };
            let bytes = crate::lightmap::encode_block(sw, f.h, &strip);
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.lm_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: f.x - 1,
                        y: f.y - 1,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some((sw + 2) * 8),
                    rows_per_image: Some(f.h + 2),
                },
                wgpu::Extent3d {
                    width: sw + 2,
                    height: f.h + 2,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    /// The flashlights among the dynamic lights (index, light), the local player's first:
    /// the first `SHADOW_LAYERS` of them cast shadows from their own shadow map.
    pub fn set_flashlights(&mut self, flashlights: &[(usize, DLight)]) {
        self.flashlights = flashlights.to_vec();
    }

    /// Depth of the opaque world from the flashlight (`r_flashlightfov` 53.7 over its
    /// radius) into the shadow map; models do not cast yet.
    fn draw_flashlight_depth(
        &self,
        gpu: &Gpu,
        light: &DLight,
        view: &wgpu::TextureView,
        items: &[DrawItem<'_>],
    ) -> Mat4 {
        let dir = light.dir.normalize_or_zero();
        let cam = Camera {
            position: light.pos,
            angles: Vec3::new(
                (-dir.z).clamp(-1.0, 1.0).asin().to_degrees(),
                dir.y.atan2(dir.x).to_degrees(),
                0.0,
            ),
            fov_x: 53.7,
            near: 4.0,
            far: light.radius.max(8.0),
            gamma: 2.2,
        };
        let vp = cam.view_proj(1.0);
        let mut u: CameraUniform = bytemuck::Zeroable::zeroed();
        u.view_proj = vp.to_cols_array_2d();
        gpu.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&u));
        let model_buffers = self.model_buffers(gpu, items);
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("flashlight depth"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("flashlight depth"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.shadow_pipeline);
            pass.set_bind_group(0, &self.shadow_camera_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            for b in &self.batches {
                if b.sky || b.translucent || b.water || b.index_count == 0 {
                    continue;
                }
                pass.set_bind_group(1, &b.bind_group, &[]);
                pass.draw_indexed(b.first_index..b.first_index + b.index_count, 0, 0..1);
            }
            if let Some((bufs, mdraws)) = &model_buffers {
                pass.set_vertex_buffer(0, bufs.vb.slice(..));
                pass.set_index_buffer(bufs.ib.slice(..), wgpu::IndexFormat::Uint32);
                for (bg, first, count) in mdraws {
                    pass.set_bind_group(1, *bg, &[]);
                    pass.draw_indexed(*first..*first + *count, 0, 0..1);
                }
            }
        }
        gpu.queue.submit(Some(encoder.finish()));
        vp
    }

    /// The entity shadow map: the dynamic models alone, orthographic along the light's travel
    /// direction (the sun's opposite) over `SUN_SHADOW_EXTENT` around the camera; None when
    /// nothing casts.
    fn draw_sun_depth(
        &self,
        gpu: &Gpu,
        camera: &Camera,
        view: &wgpu::TextureView,
        items: &[DrawItem<'_>],
    ) -> Option<Mat4> {
        let dir = (-self.sun).normalize_or_zero();
        if items.is_empty() || dir == Vec3::ZERO {
            return None;
        }
        let (bufs, mdraws) = self.model_buffers(gpu, items)?;
        let up = if dir.z.abs() > 0.99 { Vec3::X } else { Vec3::Z };
        let eye = camera.position - dir * (SUN_SHADOW_RANGE * 0.5);
        let vp = Mat4::orthographic_rh(
            -SUN_SHADOW_EXTENT,
            SUN_SHADOW_EXTENT,
            -SUN_SHADOW_EXTENT,
            SUN_SHADOW_EXTENT,
            0.0,
            SUN_SHADOW_RANGE,
        ) * Mat4::look_to_rh(eye, dir, up);
        let mut u: CameraUniform = bytemuck::Zeroable::zeroed();
        u.view_proj = vp.to_cols_array_2d();
        gpu.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&u));
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("sun shadow depth"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sun shadow depth"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(&self.shadow_pipeline);
            pass.set_bind_group(0, &self.shadow_camera_bind_group, &[]);
            pass.set_vertex_buffer(0, bufs.vb.slice(..));
            pass.set_index_buffer(bufs.ib.slice(..), wgpu::IndexFormat::Uint32);
            for (bg, first, count) in &mdraws {
                pass.set_bind_group(1, *bg, &[]);
                pass.draw_indexed(*first..*first + *count, 0, 0..1);
            }
        }
        gpu.queue.submit(Some(encoder.finish()));
        Some(vp)
    }

    /// Auto-exposure step from a frame histogram (render.md §5, `mat_tonemap_algorithm 1`;
    /// gap-report #148): `bright` is where the brightest 2 % of pixels start
    /// (`mat_tonemap_percent_bright_pixels`), aimed at 60 % (`mat_tonemap_percent_target`);
    /// `avg` is the 50 % location, kept at or above 3 % (`mat_tonemap_min_avglum`); the larger
    /// scale wins, times the current scale, clamped to the controller's min / max and
    /// approached at its rate, three times faster when going down
    /// (`mat_accelerate_adjust_exposure_down`). Also blends the bloom amount (#149).
    pub fn update_exposure(&mut self, bright: f32, avg: f32, dt: f32) {
        if !self.hdr || bright <= 0.0 || !bright.is_finite() || dt <= 0.0 {
            return;
        }
        let step = 0.05 * dt;
        self.bloom_amount += (self.tm_bloom - self.bloom_amount).clamp(-step, step);
        let scalar = (0.6 / bright.max(1e-4)).max(0.03 / avg.max(1e-4));
        let goal = (self.exposure * scalar)
            .max(0.001)
            .clamp(self.tm_min, self.tm_max);
        let rate = if goal < self.exposure {
            self.tm_rate * 3.0
        } else {
            self.tm_rate
        };
        self.exposure += (goal - self.exposure) * (rate * dt).min(1.0);
    }

    /// One drawn material of a dynamic model: its params buffer, textures and bind group.
    fn material_part(
        &self,
        gpu: &Gpu,
        key: &str,
        mat: &material::Material,
        rgba: &material::Rgba,
        bump: Option<&material::Rgba>,
        iris: Option<&material::Rgba>,
    ) -> (wgpu::BindGroup, wgpu::Texture) {
        let device = &gpu.device;
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("model material params"),
            contents: bytemuck::bytes_of(&MaterialParams {
                water_range: [0.0, 0.0, 0.0, 1.0],
                alpha_test: mat.alpha_test as u32,
                unlit: is_unlit(&mat.shader) as u32,
                translucent: mat.translucent as u32,
                // 2: `eyes`, the iris (binding 4) projected by the vertices' iris uv
                blend: if iris.is_some() { 2 } else { 0 },
                detail: detail_params(mat),
                uv2_m: [1.0, 0.0, 0.0, 1.0],
                uv2_t: [0.0; 4],
                water_color: [0.0; 4],
                envmap: envmap_params(mat),
                envmap2: envmap2_params(mat),
                envmap3: envmap3_params(mat),
                selfillum_tint: selfillum_tint(mat, self.weapon_color),
                phong: phong_params(mat),
                phong_fresnel: phong_fresnel_params(mat),
            }),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let t = upload_rgba_mips(gpu, rgba, wgpu::TextureFormat::Rgba8UnormSrgb, key);
        let bump_t = bump.map(|b| upload_rgba_mips(gpu, b, wgpu::TextureFormat::Rgba8Unorm, key));
        let bump_view = bump_t.as_ref().map(|t| t.create_view(&Default::default()));
        let iris_t =
            iris.map(|b| upload_rgba_mips(gpu, b, wgpu::TextureFormat::Rgba8UnormSrgb, key));
        let iris_view = iris_t.as_ref().map(|t| t.create_view(&Default::default()));
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(key),
            layout: &self.material_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &t.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.base_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.detail_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(
                        iris_view.as_ref().unwrap_or(&self.detail_fallback),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&self.detail_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(
                        bump_view.as_ref().unwrap_or(&self.bump_fallback),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 7,

                    resource: wgpu::BindingResource::TextureView(&self.white_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 8,

                    resource: wgpu::BindingResource::TextureView(&self.white_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
            ],
        });
        (bg, t)
    }

    /// A brush model (`*N`, a moving brush entity) as a dynamic model in its own space
    /// (the BSP stores brush entity geometry relative to the entity origin), one part per
    /// material, drawn by `draw_models` at the entity's pose.
    pub fn brush_model_dyn(
        &self,
        gpu: &Gpu,
        mount: &Mount,
        bsp: &Bsp<'_>,
        idx: usize,
    ) -> Option<DynModel> {
        let m = bsp.models.get(idx)?;
        let mut by_mat: HashMap<String, (Vec<DynVertex>, Vec<u32>)> = HashMap::new();
        let mut sizes: HashMap<String, (material::Material, material::Rgba)> = HashMap::new();
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for fi in m.first_face..m.first_face + m.num_faces {
            let Some(face) = bsp.faces.get(fi as usize) else {
                continue;
            };
            if skip_face(bsp, face) {
                continue;
            }
            let name = bsp
                .tex_infos
                .get(face.tex_info as usize)
                .and_then(|ti| bsp.tex_datas.get(usize::try_from(ti.tex_data).ok()?))
                .and_then(|td| {
                    bsp.tex_names
                        .get(usize::try_from(td.name_string_table_id).ok()?)
                })
                .map(|n| material::canonical_name(n))?;
            if !sizes.contains_key(&name) {
                let Ok(mat) = material::load(mount, &name) else {
                    continue;
                };
                let tex = mat.base_texture.clone().unwrap_or_else(|| name.clone());
                let Ok(rgba) = material::load_texture(mount, &tex, 2048) else {
                    continue;
                };
                sizes.insert(name.clone(), (mat, rgba));
            }
            let (_, rgba) = &sizes[&name];
            let (tw, th) = (rgba.width as f32, rgba.height as f32);
            let plane = &bsp.planes[face.plane_num as usize];
            let mut normal = Vec3::from(plane.normal);
            if face.side != 0 {
                normal = -normal;
            }
            let verts = bsp.face_vertices(face);
            if verts.len() < 3 {
                continue;
            }
            let (vs, is) = by_mat.entry(name.clone()).or_default();
            let base = vs.len() as u32;
            for v in &verts {
                let p = Vec3::from(*v);
                lo = lo.min(p);
                hi = hi.max(p);
                vs.push(DynVertex {
                    position: *v,
                    normal: normal.to_array(),
                    uv: face_uv(bsp, face, *v, tw, th),
                    bones: [0; 3],
                    weights: [1.0, 0.0, 0.0],
                    num_bones: 1,
                    iris_uv: [-1.0, -1.0],
                });
            }
            for k in 1..verts.len() as u32 - 1 {
                is.extend([base, base + k, base + k + 1]);
            }
        }
        if by_mat.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        let mut part_textures = Vec::new();
        for (name, (verts, idx)) in by_mat {
            let (mat, rgba) = &sizes[&name];
            let (bg, t) = self.material_part(gpu, &name, mat, rgba, None, None);
            part_textures.push((name.clone(), t));
            parts.push((
                bg,
                verts,
                idx,
                mat.half_lambert,
                is_unlit(&mat.shader),
                mat.player_color_tint,
                None,
            ));
        }
        Some(DynModel {
            parts,
            part_textures,
            bounds: (lo, hi),
            unlit: false,
            skeleton: None,
            bodygroups: Vec::new(),
            rest_skin: None,
        })
    }

    pub fn load_model(&self, gpu: &Gpu, mount: &Mount, path: &str) -> Option<DynModel> {
        self.load_model_variant(gpu, mount, path, 0, "")
    }

    /// `load_model` with a skin family and body groups: `body` is Source's `SetBodyGroups`
    /// string, one digit per body part choosing its model (missing digits and out-of-range
    /// values fall back to 0); a skin past the table falls back to 0.
    pub fn load_model_variant(
        &self,
        gpu: &Gpu,
        mount: &Mount,
        path: &str,
        skin: u8,
        body: &str,
    ) -> Option<DynModel> {
        if path.is_empty() {
            return None;
        }
        let (mdl, vvd, vtx, lod_index) =
            crate::props::load_model(mount, path, self.quality.root_lod as usize)
                .map_err(|e| eprintln!("model {path}: {e}"))
                .ok()?;
        let skin_table = mdl.skins.get(skin as usize).or_else(|| mdl.skins.first());
        let body_choice = |bi: usize, n: usize| body_group_choice(body, bi, n);
        let mut materials: crate::props::MaterialMap = HashMap::new();
        let mut missing = Vec::new();
        let keys: Vec<Option<String>> = (0..mdl.textures.len())
            .map(|t| {
                crate::props::resolve_model_texture(mount, &mdl, t, &mut materials, &mut missing)
            })
            .collect();
        let mut parts = Vec::new();
        let mut part_textures: Vec<(String, wgpu::Texture)> = Vec::new();
        for (bi, bp) in vtx.body_parts.iter().enumerate() {
            let Some(mdl_bp) = mdl.body_parts.get(bi) else {
                continue;
            };
            let choice = body_choice(bi, mdl_bp.models.len().min(bp.models.len()));
            let (Some(vm), Some(mm)) = (bp.models.get(choice), mdl_bp.models.get(choice)) else {
                continue;
            };
            let vertex_base = (mm.vertex_index / 48).max(0) as usize;
            let Some(lod) = vm.lods.get(lod_index).or(vm.lods.last()) else {
                continue;
            };
            for (mi, mesh) in lod.meshes.iter().enumerate() {
                let Some(mdl_mesh) = mm.meshes.get(mi) else {
                    continue;
                };
                let tex = skin_table
                    .and_then(|s| s.get(mdl_mesh.material.max(0) as usize))
                    .map(|&t| t.max(0) as usize)
                    .unwrap_or(mdl_mesh.material.max(0) as usize);
                let Some(Some(key)) = keys.get(tex) else {
                    continue;
                };
                let Some(Some((mat, rgba, bump))) = materials.get(key) else {
                    continue;
                };
                // an eyeball mesh (materialtype 1) under an `eyes` material: the iris uv per
                // vertex from the eyeball's frame (mstudioeyeball_t, in its bone's space)
                let eye = (mdl_mesh.material_type == 1)
                    .then(|| mm.eyeballs.get(mdl_mesh.material_param.max(0) as usize))
                    .flatten()
                    .filter(|_| mat.iris.is_some());
                let iris_rgba =
                    eye.and_then(|_| material::load_texture(mount, mat.iris.as_deref()?, 512).ok());
                let eye_frame = eye.and_then(|e| {
                    let b = mdl.bones.get(e.bone.max(0) as usize)?;
                    Some(crate::props::EyeFrame::new(e, &b.pose_to_bone))
                });
                let eye_info = eye
                    .zip(eye_frame.as_ref())
                    .map(|(e, f)| f.eye_info(e.bone.max(0) as usize));
                let mut remap: HashMap<u32, u32> = HashMap::new();
                let mut verts = Vec::new();
                let mut idx = Vec::with_capacity(mesh.indices.len());
                for &i in &mesh.indices {
                    let next = remap.len() as u32;
                    let li = *remap.entry(i).or_insert(next);
                    if li == next {
                        let Some(v) = vvd.vertices.get(vertex_base + i as usize) else {
                            continue;
                        };
                        let iris_uv = eye_frame
                            .as_ref()
                            .map_or([-1.0, -1.0], |f| f.iris_uv(v.position));
                        verts.push(DynVertex {
                            position: v.position,
                            normal: v.normal,
                            uv: v.uv,
                            bones: v.bones,
                            weights: v.weights,
                            num_bones: v.num_bones,
                            iris_uv,
                        });
                    }
                    idx.push(li);
                }
                let (bg, t) =
                    self.material_part(gpu, key, mat, rgba, bump.as_ref(), iris_rgba.as_ref());
                part_textures.push((key.to_string(), t));
                parts.push((
                    bg,
                    verts,
                    idx,
                    mat.half_lambert,
                    is_unlit(&mat.shader),
                    mat.player_color_tint,
                    eye_info,
                ));
            }
        }
        let skeleton = mount.read(path).ok().map(|bytes| Skeleton {
            bones: mdl.bones.clone(),
            attachments: mdl.attachments.clone(),
            hitboxes: gm_formats::mdl::hitboxes(&bytes, &mdl.header),
            anims: AnimSource::for_model(mount, bytes, &mdl),
        });
        let mut bounds = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for (_, verts, _, _, _, _, _) in &parts {
            for v in verts {
                let p = Vec3::from(v.position);
                bounds.0 = bounds.0.min(p);
                bounds.1 = bounds.1.max(p);
            }
        }
        let bodygroups = mdl
            .body_parts
            .iter()
            .map(|bp| {
                (
                    bp.name.clone(),
                    bp.models.iter().map(|m| m.name.clone()).collect(),
                )
            })
            .collect();
        let mut model = DynModel {
            part_textures,
            bounds,
            parts,
            unlit: false,
            skeleton,
            bodygroups,
            rest_skin: None,
        };
        model.rest_skin = model.default_pose_skin();
        (!model.parts.is_empty()).then_some(model)
    }

    /// A unit cube (-0.5..0.5) of one colour, scaled by the item transform: player boxes,
    /// the physgun beam.
    /// A `Sprite` / `UnlitGeneric` material (`materials/<name>.vmt`) as a bind group for
    /// [`draw_additive`](Self::draw_additive): the base texture, full bright.
    pub fn sprite_material(&self, gpu: &Gpu, mount: &Mount, name: &str) -> Option<wgpu::BindGroup> {
        self.sprite_material_fmt(gpu, mount, name, wgpu::TextureFormat::Rgba8UnormSrgb)
    }

    /// [`sprite_material`](Self::sprite_material) with the base texture's width in texels (an
    /// `env_sprite` is its texels times `scale` across).
    pub fn sprite_material_sized(
        &self,
        gpu: &Gpu,
        mount: &Mount,
        name: &str,
    ) -> Option<(wgpu::BindGroup, f32, bool)> {
        let mat = crate::material::load(mount, name).ok()?;
        let tex = mat.base_texture.as_deref()?;
        let rgba = crate::material::load_texture(mount, tex, 512).ok()?;
        let w = rgba.width as f32;
        let fog = if mat.additive {
            FOG_TO_BLACK
        } else {
            FOG_NORMAL
        };
        let bind = self.sprite_bind_group(
            gpu,
            &rgba,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            name,
            0.0,
            fog,
        );
        Some((bind, w, mat.additive))
    }

    /// A `DecalModulate` material for [`draw_modulate`](Self::draw_modulate): the texture is
    /// kept linear, so its 128 grey is the blend's neutral 0.5 (colour * 2 * decal) as in the
    /// game, which modulates in gamma space; read as sRGB it would darken every decal to a
    /// square.
    pub fn decal_material(&self, gpu: &Gpu, mount: &Mount, name: &str) -> Option<wgpu::BindGroup> {
        self.sprite_material_fmt(gpu, mount, name, wgpu::TextureFormat::Rgba8Unorm)
    }

    /// A decal material as the game places it: a `Subrect` resolves to its atlas with the
    /// sub-rectangle as uvs and `$Size` as its texels, else the base texture whole; the world
    /// size is the texels times `$decalscale` (1 without); `DecalModulate` ones go through
    /// [`draw_modulate`](Self::draw_modulate) (kept linear), the rest alpha-blend.
    pub fn decal(&self, gpu: &Gpu, mount: &Mount, name: &str) -> Option<Decal> {
        let mat = crate::material::load(mount, name).ok()?;
        let (tex_mat, rect, texels) = match &mat.subrect {
            Some((atlas, pos, size)) => {
                let atlas_mat = crate::material::load(mount, atlas).ok()?;
                let tex = atlas_mat.base_texture.clone()?;
                let full = crate::material::load_texture(mount, &tex, 4096).ok()?;
                let (w, h) = (full.width as f32, full.height as f32);
                (
                    atlas_mat,
                    Some([pos[0] / w, pos[1] / h, size[0] / w, size[1] / h]),
                    size[0],
                )
            }
            None => {
                let tex = mat.base_texture.clone()?;
                let full = crate::material::load_texture(mount, &tex, 4096).ok()?;
                (mat.clone(), None, full.width as f32)
            }
        };
        let modulate = tex_mat.shader.eq_ignore_ascii_case("decalmodulate");
        let format = if modulate {
            wgpu::TextureFormat::Rgba8Unorm
        } else {
            wgpu::TextureFormat::Rgba8UnormSrgb
        };
        let tex = tex_mat.base_texture.as_deref()?;
        let mut rgba = crate::material::load_texture(mount, tex, 1024).ok()?;
        // a Subrect is cut out of its atlas into a texture of its own: sampled in place, the
        // atlas' lower mips (and the edge texels under linear filtering) mix the neighbouring
        // decals in, which draws a square frame around every bullet hole
        if let Some(r) = rect {
            rgba = crop_rgba(&rgba, r);
        }
        Some(Decal {
            bind: self.sprite_bind_group(
                gpu,
                &rgba,
                format,
                name,
                0.0,
                if modulate { FOG_TO_GRAY } else { FOG_NORMAL },
            ),
            uv: [0.0, 0.0, 1.0, 1.0],
            // a Subrect without its own `$decalscale` takes the atlas material's
            size: texels * mat.decal_scale.or(tex_mat.decal_scale).unwrap_or(1.0),
            modulate,
        })
    }

    fn sprite_material_fmt(
        &self,
        gpu: &Gpu,
        mount: &Mount,
        name: &str,
        format: wgpu::TextureFormat,
    ) -> Option<wgpu::BindGroup> {
        let mat = crate::material::load(mount, name).ok()?;
        // a `Refract` sprite has no base: its normal map (kept linear) drives `fs_refract`
        let refract = mat.base_texture.is_none() && mat.shader.eq_ignore_ascii_case("refract");
        let (tex, format, amount) = if refract {
            (
                mat.bumpmap.as_deref()?,
                wgpu::TextureFormat::Rgba8Unorm,
                mat.water_dist[1],
            )
        } else {
            (mat.base_texture.as_deref()?, format, 0.0)
        };
        let rgba = crate::material::load_texture(mount, tex, 512).ok()?;
        // additive sprites fog to black like the game's `$additive` materials: fogged towards
        // the fog colour, a far one (the env_sun glow) adds that colour over its whole quad,
        // a lighter square around the glow
        let fog = if mat.additive {
            FOG_TO_BLACK
        } else {
            FOG_NORMAL
        };
        Some(self.sprite_bind_group(gpu, &rgba, format, name, amount, fog))
    }

    /// The bind group of a sprite image (unlit, no lightmap) in `format`.
    fn sprite_bind_group(
        &self,
        gpu: &Gpu,
        rgba: &crate::material::Rgba,
        format: wgpu::TextureFormat,
        name: &str,
        refract: f32,
        fog: u32,
    ) -> wgpu::BindGroup {
        let device = &gpu.device;
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sprite params"),
            contents: bytemuck::bytes_of(&MaterialParams {
                water_range: [0.0, 0.0, 0.0, 1.0],
                alpha_test: 0,
                unlit: fog,
                translucent: 1,
                blend: 0,
                detail: [0.0; 4],
                uv2_m: [1.0, 0.0, 0.0, 1.0],
                uv2_t: [0.0; 4],
                water_color: [0.0; 4],
                envmap: [0.0, refract, 0.0, 0.0],
                envmap2: [0.0; 4],
                envmap3: [0.0; 4],
                selfillum_tint: [1.0; 4],
                phong: [0.0; 4],
                phong_fresnel: [0.0, 0.5, 1.0, 0.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let t = upload_rgba_mips(gpu, rgba, format, name);
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(name),
            layout: &self.material_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &t.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.base_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.detail_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&self.detail_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&self.detail_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&self.bump_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 7,

                    resource: wgpu::BindingResource::TextureView(&self.white_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 8,

                    resource: wgpu::BindingResource::TextureView(&self.white_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
            ],
        })
    }

    /// Every frame of an animated sprite (`sprites/zerogxplode`) as its own material.
    pub fn sprite_frames(&self, gpu: &Gpu, mount: &Mount, name: &str) -> Vec<wgpu::BindGroup> {
        let Ok(mat) = crate::material::load(mount, name) else {
            return Vec::new();
        };
        let Some(tex) = mat.base_texture.as_deref() else {
            return Vec::new();
        };
        crate::material::load_texture_frames(mount, tex, 512)
            .unwrap_or_default()
            .iter()
            .map(|f| {
                self.sprite_bind_group(
                    gpu,
                    f,
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                    name,
                    0.0,
                    FOG_TO_BLACK,
                )
            })
            .collect()
    }

    /// Camera-facing strip along `points` (world space), `width` units wide, the texture's
    /// u across the beam and v along it (`u0` is the scroll offset along the length, one
    /// repeat per `u_per_unit` units; the game's beam sprites are 64 across x 128 along);
    /// `color` multiplies the texture (vertex `color`, alpha 1).
    pub fn beam_strip(
        &self,
        points: &[Vec3],
        width: f32,
        color: [f32; 3],
        eye: Vec3,
        u0: f32,
        u_per_unit: f32,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut verts = Vec::with_capacity(points.len() * 2);
        let mut idx = Vec::new();
        let mut u = u0;
        for (i, p) in points.iter().enumerate() {
            let dir = if i + 1 < points.len() {
                points[i + 1] - *p
            } else if i > 0 {
                *p - points[i - 1]
            } else {
                Vec3::X
            };
            if i > 0 {
                u += (*p - points[i - 1]).length() * u_per_unit;
            }
            let to_eye = (eye - *p).normalize_or_zero();
            let side = dir.cross(to_eye).normalize_or_zero() * (width * 0.5);
            let base = verts.len() as u32;
            for (s, across) in [(-1.0f32, 0.0f32), (1.0, 1.0)] {
                verts.push(Vertex {
                    tint: [1.0, 1.0, 1.0, 0.0],
                    position: (*p + side * s).to_array(),
                    uv: [across, u],
                    lm_uv: self.white_uv,
                    color,
                    alpha: 0.0,
                    lm_page: 0.0,
                    normal: [0.0; 3],
                    cube: -1.0,
                    spec_dir: [0.0; 3],
                    spec_color: [0.0; 3],
                });
            }
            if i > 0 {
                idx.extend([base - 2, base - 1, base, base - 1, base + 1, base]);
            }
        }
        (verts, idx)
    }

    /// `render.StartBeam/AddBeam/EndBeam`: a camera-facing strip through `points`, each with
    /// its own width, colour (alpha premultiplied for the additive pass) and texture v.
    pub fn beam_strip_var(
        &self,
        points: &[(Vec3, f32, [f32; 4], f32)],
        eye: Vec3,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut verts = Vec::with_capacity(points.len() * 2);
        let mut idx = Vec::new();
        for (i, (p, width, rgba, v)) in points.iter().enumerate() {
            let dir = if i + 1 < points.len() {
                points[i + 1].0 - *p
            } else if i > 0 {
                *p - points[i - 1].0
            } else {
                Vec3::X
            };
            let to_eye = (eye - *p).normalize_or_zero();
            let side = dir.cross(to_eye).normalize_or_zero() * (width * 0.5);
            let base = verts.len() as u32;
            let color = [rgba[0] * rgba[3], rgba[1] * rgba[3], rgba[2] * rgba[3]];
            for (s, across) in [(-1.0f32, 0.0f32), (1.0, 1.0)] {
                verts.push(Vertex {
                    tint: [1.0, 1.0, 1.0, 0.0],
                    position: (*p + side * s).to_array(),
                    uv: [across, *v],
                    lm_uv: self.white_uv,
                    color,
                    alpha: 0.0,
                    lm_page: 0.0,
                    normal: [0.0; 3],
                    cube: -1.0,
                    spec_dir: [0.0; 3],
                    spec_color: [0.0; 3],
                });
            }
            if i > 0 {
                idx.extend([base - 2, base - 1, base, base - 1, base + 1, base]);
            }
        }
        (verts, idx)
    }

    /// A particle quad: camera-facing, or in the plane normal to `axis`, rolled by `roll`
    /// radians; `rgba[3]` is the vertex opacity (the shader multiplies the texture alpha by
    /// it in the alpha pass; pass premultiplied colour and 0 for the additive pass).
    pub fn sprite_quad_var(
        &self,
        pos: Vec3,
        size: f32,
        rgba: [f32; 4],
        roll: f32,
        axis: Option<Vec3>,
        camera: &Camera,
    ) -> (Vec<Vertex>, Vec<u32>) {
        // camera-facing quads take the view's axes (#135); an explicit axis orients the quad
        // around it
        let (right, up) = match axis {
            None => camera.sprite_axes(),
            Some(fwd) => {
                let mut right = fwd.cross(Vec3::Z).normalize_or_zero();
                if right == Vec3::ZERO {
                    right = Vec3::X;
                }
                (right, right.cross(fwd).normalize_or_zero())
            }
        };
        let (sr, cr) = roll.sin_cos();
        let (right, up) = (right * cr + up * sr, up * cr - right * sr);
        let h = size * 0.5;
        let verts = [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .into_iter()
            .map(|(x, y)| Vertex {
                tint: [1.0, 1.0, 1.0, 0.0],
                position: (pos + right * (x * h) + up * (y * h)).to_array(),
                uv: [(x + 1.0) * 0.5, 1.0 - (y + 1.0) * 0.5],
                lm_uv: self.white_uv,
                color: [rgba[0], rgba[1], rgba[2]],
                alpha: rgba[3],
                lm_page: 0.0,
                normal: [0.0; 3],
                cube: -1.0,
                spec_dir: [0.0; 3],
                spec_color: [0.0; 3],
            })
            .collect();
        (verts, vec![0, 1, 2, 0, 2, 3])
    }

    /// `sprite_quad_var` over a sub-rectangle `uv` (`[u_min, v_min, u_max, v_max]`) of the
    /// material: one frame of a sprite sheet.
    #[allow(clippy::too_many_arguments)]
    pub fn sprite_quad_uv(
        &self,
        pos: Vec3,
        size: f32,
        rgba: [f32; 4],
        roll: f32,
        axis: Option<Vec3>,
        uv: [f32; 4],
        camera: &Camera,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let (mut verts, idx) = self.sprite_quad_var(pos, size, rgba, roll, axis, camera);
        for v in &mut verts {
            v.uv = [
                uv[0] + (uv[2] - uv[0]) * v.uv[0],
                uv[1] + (uv[3] - uv[1]) * v.uv[1],
            ];
        }
        (verts, idx)
    }

    /// Camera-facing square of `size` units at `pos` (a `vp_parallel` sprite).
    pub fn sprite_quad(
        &self,
        pos: Vec3,
        size: f32,
        color: [f32; 3],
        camera: &Camera,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let (right, up) = camera.sprite_axes();
        let h = size * 0.5;
        let verts = [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .into_iter()
            .map(|(x, y)| Vertex {
                tint: [1.0, 1.0, 1.0, 0.0],
                position: (pos + right * (x * h) + up * (y * h)).to_array(),
                uv: [(x + 1.0) * 0.5, 1.0 - (y + 1.0) * 0.5],
                lm_uv: self.white_uv,
                color,
                alpha: 0.0,
                lm_page: 0.0,
                normal: [0.0; 3],
                cube: -1.0,
                spec_dir: [0.0; 3],
                spec_color: [0.0; 3],
            })
            .collect();
        (verts, vec![0, 1, 2, 0, 2, 3])
    }

    /// A quad of `size` lying on the surface at `pos` with `normal`, lifted 0.3 u off it
    /// (decals).
    pub fn decal_quad(
        &self,
        pos: Vec3,
        normal: Vec3,
        size: f32,
        color: [f32; 3],
    ) -> (Vec<Vertex>, Vec<u32>) {
        self.decal_quad_uv(pos, normal, size, color, [0.0, 0.0, 1.0, 1.0])
    }

    /// [`decal_quad`](Self::decal_quad) over the texture rectangle `uv` (u0, v0, u1, v1).
    pub fn decal_quad_uv(
        &self,
        pos: Vec3,
        normal: Vec3,
        size: f32,
        color: [f32; 3],
        uv: [f32; 4],
    ) -> (Vec<Vertex>, Vec<u32>) {
        let n = normal.normalize_or_zero();
        let helper = if n.z.abs() < 0.9 { Vec3::Z } else { Vec3::X };
        let right = n.cross(helper).normalize_or_zero();
        let up = right.cross(n).normalize_or_zero();
        let h = size * 0.5;
        let p = pos + n * 0.3;
        let verts = [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
            .into_iter()
            .map(|(x, y)| Vertex {
                tint: [1.0, 1.0, 1.0, 0.0],
                position: (p + right * (x * h) + up * (y * h)).to_array(),
                uv: [
                    uv[0] + (uv[2] - uv[0]) * (x + 1.0) * 0.5,
                    uv[1] + (uv[3] - uv[1]) * (1.0 - (y + 1.0) * 0.5),
                ],
                lm_uv: self.white_uv,
                color,
                alpha: 0.0,
                lm_page: 0.0,
                normal: n.to_array(),
                cube: -1.0,
                spec_dir: [0.0; 3],
                spec_color: [0.0; 3],
            })
            .collect();
        (verts, vec![0, 1, 2, 0, 2, 3])
    }

    /// Draws additive geometry (beams, glows) over the current colour with the current depth
    /// (tested, not written): each item is a material bind group with world-space vertices.
    pub fn draw_additive(
        &self,
        gpu: &Gpu,
        camera: &Camera,
        target: &ModelTarget<'_>,
        items: &[(&wgpu::BindGroup, Vec<Vertex>, Vec<u32>)],
    ) {
        self.draw_blended(gpu, camera, target, items, &self.additive_pipeline);
    }

    /// `Refract` sprites (the thruster flame's heat-wave): the scene so far is copied into
    /// the refraction texture (`render.UpdateRefractTexture`) and each item redraws it offset
    /// by its normal map; the items are [`sprite_material`](Self::sprite_material) bind groups
    /// of a `Refract` material with world-space vertices.
    pub fn draw_refract(
        &self,
        gpu: &Gpu,
        camera: &Camera,
        target: &ModelTarget<'_>,
        scene_src: &wgpu::Texture,
        items: &[(&wgpu::BindGroup, Vec<Vertex>, Vec<u32>)],
    ) {
        let mut verts: Vec<Vertex> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        let mut draws = Vec::new();
        for (bg, v, i) in items {
            let base = verts.len() as u32;
            let first = idx.len() as u32;
            verts.extend_from_slice(v);
            idx.extend(i.iter().map(|k| k + base));
            draws.push((*bg, first, i.len() as u32));
        }
        if idx.is_empty() {
            return;
        }
        let device = &gpu.device;
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("refract vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("refract indices"),
            contents: bytemuck::cast_slice(&idx),
            usage: wgpu::BufferUsages::INDEX,
        });
        let cam = self.camera_uniform(
            camera,
            target.aspect,
            1.0,
            self.view_fog(camera),
            self.clip_for(false),
        );
        gpu.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&cam));
        let mut enc = device.create_command_encoder(&Default::default());
        let mut state = self.reflect.lock().unwrap();
        let size = scene_src.size();
        if state
            .refraction
            .as_ref()
            .is_none_or(|(t, _)| t.size() != size || t.format() != scene_src.format())
        {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("water refraction"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: scene_src.format(),
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let view = tex.create_view(&Default::default());
            state.refraction = Some((tex, view));
        }
        let (tex, refraction) = state.refraction.as_ref().unwrap();
        enc.copy_texture_to_texture(scene_src.as_image_copy(), tex.as_image_copy(), size);
        let depth_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("refract scene"),
            layout: &self.depth_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(target.depth),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.bump_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.reflect_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(refraction),
                },
            ],
        });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("refract"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.color,
                    depth_slice: None,
                    resolve_target: target.resolve,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: target.depth,
                    depth_ops: None,
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.refract_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_bind_group(2, &depth_bind_group, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            for (bg, first, count) in draws {
                pass.set_bind_group(1, bg, &[]);
                pass.draw_indexed(first..first + count, 0, 0..1);
            }
        }
        drop(state);
        gpu.queue.submit([enc.finish()]);
    }

    /// Alpha-blended geometry over the current colour with the current depth (constraint
    /// ropes); each item is a material bind group with world-space vertices.
    pub fn draw_alpha(
        &self,
        gpu: &Gpu,
        camera: &Camera,
        target: &ModelTarget<'_>,
        items: &[(&wgpu::BindGroup, Vec<Vertex>, Vec<u32>)],
    ) {
        self.draw_blended(gpu, camera, target, items, &self.alpha_pipeline);
    }

    fn draw_blended(
        &self,
        gpu: &Gpu,
        camera: &Camera,
        target: &ModelTarget<'_>,
        items: &[(&wgpu::BindGroup, Vec<Vertex>, Vec<u32>)],
        pipeline: &wgpu::RenderPipeline,
    ) {
        let mut verts: Vec<Vertex> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        let mut draws = Vec::new();
        for (bg, v, i) in items {
            let base = verts.len() as u32;
            let first = idx.len() as u32;
            verts.extend_from_slice(v);
            idx.extend(i.iter().map(|k| k + base));
            draws.push((*bg, first, i.len() as u32));
        }
        if idx.is_empty() {
            return;
        }
        let device = &gpu.device;
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("additive vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("additive indices"),
            contents: bytemuck::cast_slice(&idx),
            usage: wgpu::BufferUsages::INDEX,
        });
        let cam = self.camera_uniform(
            camera,
            target.aspect,
            1.0,
            self.view_fog(camera),
            self.clip_for(false),
        );
        gpu.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&cam));
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("additive"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.color,
                    depth_slice: None,
                    resolve_target: target.resolve,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            for (bg, first, count) in draws {
                pass.set_bind_group(1, bg, &[]);
                pass.draw_indexed(first..first + count, 0, 0..1);
            }
        }
        gpu.queue.submit([enc.finish()]);
    }

    /// Draws modulated geometry (DecalModulate decals: colour * 2 * texel) over the current
    /// colour with the current depth
    /// (tested, not written): each item is a material bind group with world-space vertices.
    pub fn draw_modulate(
        &self,
        gpu: &Gpu,
        camera: &Camera,
        target: &ModelTarget<'_>,
        items: &[(&wgpu::BindGroup, Vec<Vertex>, Vec<u32>)],
    ) {
        let mut verts: Vec<Vertex> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        let mut draws = Vec::new();
        for (bg, v, i) in items {
            let base = verts.len() as u32;
            let first = idx.len() as u32;
            verts.extend_from_slice(v);
            idx.extend(i.iter().map(|k| k + base));
            draws.push((*bg, first, i.len() as u32));
        }
        if idx.is_empty() {
            return;
        }
        let device = &gpu.device;
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("additive vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("additive indices"),
            contents: bytemuck::cast_slice(&idx),
            usage: wgpu::BufferUsages::INDEX,
        });
        let cam = self.camera_uniform(
            camera,
            target.aspect,
            1.0,
            self.view_fog(camera),
            self.clip_for(false),
        );
        gpu.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&cam));
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("additive"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.color,
                    depth_slice: None,
                    resolve_target: target.resolve,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.modulate_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            for (bg, first, count) in draws {
                pass.set_bind_group(1, bg, &[]);
                pass.draw_indexed(first..first + count, 0, 0..1);
            }
        }
        gpu.queue.submit([enc.finish()]);
    }

    pub fn solid_box(&self, gpu: &Gpu, rgba: [u8; 4], unlit: bool) -> DynModel {
        let device = &gpu.device;
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("solid params"),
            contents: bytemuck::bytes_of(&MaterialParams {
                water_range: [0.0, 0.0, 0.0, 1.0],
                alpha_test: 0,
                unlit: 0,
                translucent: 0,
                blend: 0,
                detail: [0.0; 4],
                uv2_m: [1.0, 0.0, 0.0, 1.0],
                uv2_t: [0.0; 4],
                water_color: [0.0; 4],
                envmap: [0.0; 4],
                envmap2: [0.0; 4],
                envmap3: [0.0; 4],
                selfillum_tint: [1.0; 4],
                phong: [0.0; 4],
                phong_fresnel: [0.0, 0.5, 1.0, 0.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let t = upload_rgba(
            gpu,
            &rgba,
            1,
            1,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            "solid",
        );
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("solid"),
            layout: &self.material_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &t.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.base_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.detail_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&self.detail_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&self.detail_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&self.bump_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 7,

                    resource: wgpu::BindingResource::TextureView(&self.white_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 8,

                    resource: wgpu::BindingResource::TextureView(&self.white_fallback),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
            ],
        });
        let mut verts = Vec::new();
        let mut idx = Vec::new();
        for axis in 0..3 {
            for sign in [-1.0f32, 1.0] {
                let mut n = [0.0f32; 3];
                n[axis] = sign;
                let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                let base = verts.len() as u32;
                for (a, b) in [(-0.5f32, -0.5f32), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
                    let mut p = [0.0f32; 3];
                    p[axis] = 0.5 * sign;
                    p[u] = a;
                    p[v] = b;
                    verts.push(DynVertex::rigid(p, n, [0.5, 0.5]));
                }
                if sign > 0.0 {
                    idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                } else {
                    idx.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
                }
            }
        }
        DynModel {
            part_textures: Vec::new(),
            bounds: (Vec3::splat(-0.5), Vec3::splat(0.5)),
            parts: vec![(bg, verts, idx, false, false, false, None)],
            unlit,
            skeleton: None,
            bodygroups: Vec::new(),
            rest_skin: None,
        }
    }

    /// Draws moving models over the frame `draw` left in `color`/`depth`: each item is a model
    /// with its origin and Source angles, transformed on the CPU into one buffer per frame.
    /// The items skinned, placed and lit into world vertices: the vertices, indices and one
    /// (material bind group, first index, count) per part.
    #[allow(clippy::type_complexity)]
    fn model_mesh(&self, items: &[DrawItem<'_>]) -> (Vec<Vertex>, Vec<u32>, Vec<ModelDraw>) {
        let mut verts: Vec<Vertex> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        let mut draws = Vec::new();
        let mut cache = self.light_cache.borrow_mut();
        if cache.len() > 4096 {
            cache.clear();
        }
        for (item_index, item) in items.iter().enumerate() {
            let (m, xf) = (item.model, &item.transform);
            // the game lights a model from its origin: ambient cube of the leaf + direct lights
            let origin = xf.transform_point3(Vec3::ZERO);
            let model_cube = self.cubes.nearest(origin);
            let local = if m.unlit {
                None
            } else {
                let key = [
                    (origin.x / 8.0).floor() as i32,
                    (origin.y / 8.0).floor() as i32,
                    (origin.z / 8.0).floor() as i32,
                ];
                Some(
                    cache
                        .entry(key)
                        .or_insert_with(|| self.light.at(origin, 4))
                        .clone(),
                )
            };
            let spec = match &local {
                Some(l) => l
                    .lights
                    .iter()
                    .max_by(|a, b| a.0.length_squared().total_cmp(&b.0.length_squared()))
                    .map(|&(c, d)| (d, c))
                    .unwrap_or((Vec3::Z, Vec3::ZERO)),
                None => (Vec3::ZERO, Vec3::ZERO),
            };
            for (part, (_, pv, pi, half_lambert, part_unlit, player_tint, eye)) in
                m.parts.iter().enumerate()
            {
                let tint = if *player_tint { item.tint } else { Vec3::ONE };
                let tint = tint.extend(item.dissolve);
                // an eyeball looking at the item's target: the iris re-projected per frame
                // towards the target brought into the model's rest pose space
                let look = eye.zip(item.look).and_then(|(e, target)| {
                    let skin = item.bones.or(m.rest_skin.as_deref())?.get(e.bone)?;
                    let to_rest = (*xf * *skin).inverse();
                    Some((e, to_rest.transform_point3(target)))
                });
                let base = verts.len() as u32;
                for v in pv {
                    let (p, n) = match item.bones.or(m.rest_skin.as_deref()) {
                        Some(b) => skin(v, b),
                        None => (Vec3::from(v.position), Vec3::from(v.normal)),
                    };
                    let n = xf.transform_vector3(n).normalize_or_zero();
                    let shade = match &local {
                        Some(l) if !*part_unlit => l.shade(n, *half_lambert),
                        _ => Vec3::ONE,
                    };
                    verts.push(Vertex {
                        tint: tint.to_array(),
                        position: xf.transform_point3(p).to_array(),
                        uv: v.uv,
                        lm_uv: if v.iris_uv[0] > -0.5 {
                            match look {
                                Some((e, target)) => {
                                    e.iris_uv_towards(Vec3::from(v.position), target)
                                }
                                None => v.iris_uv,
                            }
                        } else {
                            self.white_uv
                        },
                        color: shade.to_array(),
                        alpha: 0.0,
                        lm_page: 0.0,
                        normal: n.to_array(),
                        cube: model_cube,
                        spec_dir: spec.0.to_array(),
                        spec_color: spec.1.to_array(),
                    });
                }
                let first = idx.len() as u32;
                idx.extend(pi.iter().map(|i| i + base));
                draws.push(ModelDraw {
                    item: item_index,
                    part,
                    first,
                    count: pi.len() as u32,
                });
            }
        }
        (verts, idx, draws)
    }

    /// The items' skinned meshes as GPU buffers plus their draws with the part's material
    /// bind group, built once per frame for a given items slice (see `model_cache`); None
    /// when there is nothing to draw.
    fn model_buffers<'a>(
        &self,
        gpu: &Gpu,
        items: &[DrawItem<'a>],
    ) -> Option<(std::rc::Rc<ModelBuffers>, Vec<PartDraw<'a>>)> {
        if items.is_empty() {
            return None;
        }
        let key = (self.frame.get(), items.as_ptr() as usize, items.len());
        let cached = self
            .model_cache
            .borrow()
            .as_ref()
            .filter(|(f, p, n, _)| (*f, *p, *n) == key)
            .map(|(_, _, _, b)| b.clone());
        let bufs = match cached {
            Some(b) => b,
            None => {
                let (verts, idx, draws) = self.model_mesh(items);
                if draws.is_empty() {
                    return None;
                }
                let vb = gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("model vertices"),
                        contents: bytemuck::cast_slice(&verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                let ib = gpu
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("model indices"),
                        contents: bytemuck::cast_slice(&idx),
                        usage: wgpu::BufferUsages::INDEX,
                    });
                let b = std::rc::Rc::new(ModelBuffers { vb, ib, draws });
                *self.model_cache.borrow_mut() = Some((key.0, key.1, key.2, b.clone()));
                b
            }
        };
        let draws = bufs
            .draws
            .iter()
            .map(|d| (&items[d.item].model.parts[d.part].0, d.first, d.count))
            .collect();
        Some((bufs, draws))
    }

    pub fn draw_models(
        &self,
        gpu: &Gpu,
        camera: &Camera,
        target: &ModelTarget<'_>,
        items: &[DrawItem<'_>],
    ) {
        let ModelTarget {
            color,
            depth,
            aspect,
            clear_depth,
            alpha,
            ..
        } = *target;
        let Some((bufs, draws)) = self.model_buffers(gpu, items) else {
            return;
        };
        let device = &gpu.device;
        gpu.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&self.camera_uniform(
                camera,
                aspect,
                alpha,
                self.view_fog(camera),
                self.clip_for(false),
            )),
        );
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("models"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: target.resolve,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations {
                        load: if clear_depth {
                            wgpu::LoadOp::Clear(1.0)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(if alpha < 1.0 {
                &self.translucent_pipeline
            } else {
                &self.pipeline
            });
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, bufs.vb.slice(..));
            pass.set_index_buffer(bufs.ib.slice(..), wgpu::IndexFormat::Uint32);
            for (bg, first, count) in draws {
                pass.set_bind_group(1, bg, &[]);
                pass.draw_indexed(first..first + count, 0, 0..1);
            }
        }
        gpu.queue.submit([enc.finish()]);
    }
}

/// One part of a skinned item in a frame's model buffers: `items[item].model.parts[part]`
/// over `count` indices from `first`.
struct ModelDraw {
    item: usize,
    part: usize,
    first: u32,
    count: u32,
}

/// A frame's skinned entity meshes (see `World::model_buffers`).
struct ModelBuffers {
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    draws: Vec<ModelDraw>,
}

/// `World::model_cache`: (frame, items pointer, items length, buffers).
type ModelCacheEntry = (u64, usize, usize, std::rc::Rc<ModelBuffers>);

/// A part draw: its material bind group, first index, index count.
type PartDraw<'a> = (&'a wgpu::BindGroup, u32, u32);

/// Whether the box can be in view: false only when all its corners lie beyond one clip
/// plane (conservative; wgpu clip z runs 0..w).
fn aabb_in_frustum(vp: Mat4, min: Vec3, max: Vec3) -> bool {
    let mut outside = [0u8; 6];
    for i in 0..8 {
        let c = Vec3::new(
            if i & 1 == 0 { min.x } else { max.x },
            if i & 2 == 0 { min.y } else { max.y },
            if i & 4 == 0 { min.z } else { max.z },
        );
        let p = vp * c.extend(1.0);
        let tests = [
            p.x < -p.w,
            p.x > p.w,
            p.y < -p.w,
            p.y > p.w,
            p.z < 0.0,
            p.z > p.w,
        ];
        for (o, t) in outside.iter_mut().zip(tests) {
            *o += t as u8;
        }
    }
    outside.iter().all(|&o| o < 8)
}

/// Bounding box of the 3D skybox faces, used to sort props into the skybox pass.
fn skybox_faces_bounds(bsp: &Bsp<'_>, faces: &HashSet<usize>) -> Option<([f32; 3], [f32; 3])> {
    let mut mins = [f32::MAX; 3];
    let mut maxs = [f32::MIN; 3];
    let mut any = false;
    for &fi in faces {
        for v in bsp.face_vertices(&bsp.faces[fi]) {
            for k in 0..3 {
                mins[k] = mins[k].min(v[k]);
                maxs[k] = maxs[k].max(v[k]);
            }
            any = true;
        }
    }
    any.then_some((mins, maxs))
}

/// Upload a decoded texture with its mip chain.
/// `rect` (x, y, w, h as fractions of the image) of `src` as an image of its own with a
/// box-filtered mip chain down to 1x1.
fn crop_rgba(src: &material::Rgba, rect: [f32; 4]) -> material::Rgba {
    let x0 = ((rect[0] * src.width as f32).round() as u32).min(src.width.saturating_sub(1));
    let y0 = ((rect[1] * src.height as f32).round() as u32).min(src.height.saturating_sub(1));
    let w = ((rect[2] * src.width as f32).round() as u32).clamp(1, src.width - x0);
    let h = ((rect[3] * src.height as f32).round() as u32).clamp(1, src.height - y0);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in y0..y0 + h {
        let row = ((y * src.width + x0) * 4) as usize;
        pixels.extend_from_slice(&src.pixels[row..row + (w * 4) as usize]);
    }
    let mut mips = Vec::new();
    let (mut pw, mut ph, mut prev) = (w, h, pixels.clone());
    while pw > 1 || ph > 1 {
        let (nw, nh) = ((pw / 2).max(1), (ph / 2).max(1));
        let mut next = Vec::with_capacity((nw * nh * 4) as usize);
        for y in 0..nh {
            for x in 0..nw {
                let mut acc = [0u32; 4];
                let mut n = 0u32;
                for dy in 0..2u32 {
                    for dx in 0..2u32 {
                        let (sx, sy) = ((x * 2 + dx).min(pw - 1), (y * 2 + dy).min(ph - 1));
                        let i = ((sy * pw + sx) * 4) as usize;
                        for c in 0..4 {
                            acc[c] += prev[i + c] as u32;
                        }
                        n += 1;
                    }
                }
                next.extend(acc.iter().map(|a| ((a + n / 2) / n) as u8));
            }
        }
        mips.push(next.clone());
        (pw, ph, prev) = (nw, nh, next);
    }
    material::Rgba {
        width: w,
        height: h,
        pixels,
        mips,
    }
}

pub fn upload_rgba_mips(
    gpu: &Gpu,
    rgba: &material::Rgba,
    format: wgpu::TextureFormat,
    label: &str,
) -> wgpu::Texture {
    let size = wgpu::Extent3d {
        width: rgba.width,
        height: rgba.height,
        depth_or_array_layers: 1,
    };
    let levels = (1 + rgba.mips.len() as u32).min(size.max_mips(wgpu::TextureDimension::D2));
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let bpp = format.block_copy_size(None).unwrap_or(4);
    for (level, px) in std::iter::once(&rgba.pixels)
        .chain(rgba.mips.iter())
        .take(levels as usize)
        .enumerate()
    {
        let (w, h) = ((rgba.width >> level).max(1), (rgba.height >> level).max(1));
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            px,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * bpp),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }
    tex
}

pub fn upload_rgba(
    gpu: &Gpu,
    pixels: &[u8],
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    label: &str,
) -> wgpu::Texture {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * format.block_copy_size(None).unwrap_or(4)),
            rows_per_image: Some(height),
        },
        size,
    );
    tex
}

/// A square icon target: a color texture in the world pipeline's (swapchain) format that can
/// also be sampled (spawn-menu icons), and its depth buffer.
pub fn icon_targets(
    gpu: &Gpu,
    size: u32,
    format: wgpu::TextureFormat,
) -> (wgpu::Texture, wgpu::Texture) {
    let extent = wgpu::Extent3d {
        width: size,
        height: size,
        depth_or_array_layers: 1,
    };
    let color = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("icon"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("icon depth"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    (color, depth)
}

/// Create an offscreen color + depth target pair.
/// `_rt_WaterReflection` width (the engine's default); the height follows the aspect.
const REFLECT_WIDTH: u32 = 1024;

#[derive(Default)]
struct ReflectState {
    target: Option<ReflectTarget>,
    /// Normal of the clip plane (`clip` is its distance along it); the water's is +z.
    normal: Vec3,
    /// The mirrored camera of a vertical mirror gives the reflection mirrored left-right.
    flip_x: bool,
    /// Clip plane z while the reflection pass runs.
    clip: Option<f32>,
    /// `target.sampled` holds this frame's reflection.
    valid: bool,
    /// `_rt_WaterRefraction`: a copy of the resolved scene, same size and format.
    refraction: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// The running pass has a scene to copy for refraction.
    refr_valid: bool,
}

struct ReflectTarget {
    size: (u32, u32),
    color: wgpu::TextureView,
    resolve: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    /// The view to sample: the resolve target under MSAA, else the colour target.
    sampled: wgpu::TextureView,
}

impl ReflectTarget {
    fn new(gpu: &Gpu, w: u32, h: u32, format: wgpu::TextureFormat, msaa: u32) -> Self {
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let tex = |label, samples, format, usage| {
            gpu.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size,
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let bind = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let color = tex("water reflection", msaa, format, bind);
        let resolve = (msaa > 1).then(|| tex("water reflection resolve", 1, format, bind));
        let depth = tex(
            "water reflection depth",
            msaa,
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let sampled = resolve.clone().unwrap_or_else(|| color.clone());
        ReflectTarget {
            size: (w, h),
            color,
            resolve,
            depth,
            sampled,
        }
    }
}

pub fn offscreen_targets(
    gpu: &Gpu,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    msaa: u32,
) -> (wgpu::Texture, wgpu::Texture) {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let color = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("color"),
        size,
        mip_level_count: 1,
        sample_count: msaa,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size,
        mip_level_count: 1,
        sample_count: msaa,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    (color, depth)
}

#[cfg(test)]
mod body_group_tests {
    use super::body_group_choice;

    #[test]
    fn digits_pick_models_and_everything_else_means_the_first() {
        // the refugee hands' `0100000`: part 1 shows model 1, the rest model 0
        assert_eq!(body_group_choice("0100000", 0, 2), 0);
        assert_eq!(body_group_choice("0100000", 1, 2), 1);
        assert_eq!(body_group_choice("0100000", 6, 3), 0);
        // past the string, a non-digit, or a model the part does not have
        assert_eq!(body_group_choice("01", 5, 4), 0);
        assert_eq!(body_group_choice("0x", 1, 4), 0);
        assert_eq!(body_group_choice("03", 1, 2), 0);
        assert_eq!(body_group_choice("", 0, 1), 0);
    }
}
