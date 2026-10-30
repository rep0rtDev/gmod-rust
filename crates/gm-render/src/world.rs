//! Static world geometry: BSP faces and displacements, grouped by material.

use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use gm_content::Mount;
use gm_formats::bsp::extra::{build_disp_mesh, disp_infos, disp_verts, DispInfo};
use gm_formats::bsp::lumps::{surf, Face};
use gm_formats::bsp::Bsp;
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
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MaterialParams {
    alpha_test: u32,
    pad: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

/// Source camera: position in units, angles in degrees (pitch, yaw, roll), horizontal FOV.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub angles: Vec3,
    pub fov_x: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn forward(&self) -> Vec3 {
        let (p, y) = (self.angles.x.to_radians(), self.angles.y.to_radians());
        Vec3::new(p.cos() * y.cos(), p.cos() * y.sin(), -p.sin())
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
}

/// `sky_camera` entity: the 3D skybox is a small copy of the world rendered first,
/// with the eye moved to `origin + eye / scale`.
#[derive(Clone, Copy, Debug)]
pub struct SkyCamera {
    pub origin: Vec3,
    pub scale: f32,
}

pub struct WorldRenderer {
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    batches: Vec<Batch>,
    pub sky_camera: Option<SkyCamera>,
    pub stats: Stats,
    material_layout: wgpu::BindGroupLayout,
    base_sampler: wgpu::Sampler,
    white_uv: [f32; 2],
    sun: Vec3,
    sky2d: Option<SkyCube>,
}

/// The 2D sky: worldspawn `skyname` as six `materials/skybox/<name><side>` faces on a cube
/// around the eye, drawn first so sky faces (skipped) and the 3D skybox show over it.
struct SkyCube {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    faces: Vec<wgpu::BindGroup>,
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
            pad: [0; 3],
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
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
            ],
        }));
        let base = verts.len() as u32;
        let (n, sx, ty) = (Vec3::from(n), Vec3::from(sx), Vec3::from(ty));
        for (s, t) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            verts.push(Vertex {
                position: ((n + sx * s + ty * t) * SKY_CUBE_HALF).to_array(),
                uv: [(s + 1.0) / 2.0, (1.0 - t) / 2.0],
                lm_uv: white_uv,
                color: [1.0; 3],
            });
        }
        idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
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
    })
}

const SKY_CUBE_HALF: f32 = 100.0;

/// A model for props that move: meshes in model space with their material bind groups.
/// One mesh of a `DynModel`: material, model-space (position, normal, uv), indices.
type DynPart = (
    wgpu::BindGroup,
    Vec<([f32; 3], [f32; 3], [f32; 2])>,
    Vec<u32>,
);

pub struct DynModel {
    parts: Vec<DynPart>,
    /// Drawn at full brightness (beams, markers), not shaded by the sun.
    unlit: bool,
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
    ) -> Result<Self> {
        let device = &gpu.device;
        let atlas_size = 4096u32;
        let mut atlas = Atlas::new(atlas_size);
        let white = atlas.white().ok_or_else(|| anyhow!("atlas too small"))?;
        let mut stats = Stats::default();

        // material -> (texture rgba, flags)
        let mut materials: HashMap<String, Option<(material::Material, material::Rgba)>> =
            HashMap::new();
        let mut per_material: HashMap<String, Vec<u32>> = HashMap::new();
        let mut vertices: Vec<Vertex> = Vec::new();

        let disps = disp_infos(bsp)?;
        let dverts = disp_verts(bsp)?;
        let disp_by_face: HashMap<u16, &DispInfo> = disps.iter().map(|d| (d.map_face, d)).collect();

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
                let leaf = &bsp.leafs[bsp.leaf_at(c)];
                if leaf.area() == area {
                    skybox_faces.insert(fi);
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
                    return Some((
                        m,
                        material::Rgba {
                            width: 1,
                            height: 1,
                            pixels: vec![60, 90, 110, 255],
                        },
                    ));
                }
                let Some(tex) = m.base_texture.clone() else {
                    return Some((
                        m,
                        material::Rgba {
                            width: 1,
                            height: 1,
                            pixels: vec![128, 128, 128, 255],
                        },
                    ));
                };
                match material::load_texture(mount, &tex, 1024) {
                    Ok(rgba) => Some((m, rgba)),
                    Err(_) => {
                        stats.missing_materials.push(format!("{canon} -> {tex}"));
                        None
                    }
                }
            });
            entry
                .as_ref()
                .map(|(_, rgba)| (canon, rgba.width, rgba.height))
        };

        for (fi, face) in bsp.faces.iter().enumerate() {
            if skip_face(bsp, face) {
                continue;
            }
            let Some(name) = bsp.face_material(face) else {
                continue;
            };
            let Some((canon, tw, th)) = resolve(name, &mut stats) else {
                continue;
            };
            let (tw, th) = (tw as f32, th as f32);
            let (placement, lit) = if face.light_ofs >= 0 && face.light_styles() > 0 {
                let (w, h) = face.lightmap_dims();
                let samples = bsp.face_lightmap(face)?;
                match atlas.place(w, h, &samples[..(w * h) as usize]) {
                    Some(p) => (p, true),
                    None => {
                        stats.lightmap_overflow += 1;
                        (white, false)
                    }
                }
            } else {
                (white, false)
            };
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
                    position: [p[0] + off[0], p[1] + off[1], p[2] + off[2]],
                    uv: face_uv(bsp, face, *p, tw, th),
                    lm_uv: if lit {
                        face_lm_uv(bsp, face, *p, &placement, atlas_size as f32)
                    } else {
                        white_uv
                    },
                    color: [1.0; 3],
                });
            }
            for i in 1..poly.len() as u32 - 1 {
                indices.extend_from_slice(&[base, base + i, base + i + 1]);
            }
            stats.faces += 1;
            stats.triangles += poly.len() - 2;
        }

        drop(resolve);
        let sun = crate::props::sun_direction(bsp);
        let prop_stats = crate::props::collect_static_props(
            mount,
            bsp,
            sun,
            white_uv,
            &mut vertices,
            &mut materials,
            &mut per_material,
            &mut per_material_sky,
            &skybox_faces_bounds(bsp, &skybox_faces),
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
            wgpu::TextureFormat::Rgba8Unorm,
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
                    visibility: wgpu::ShaderStages::VERTEX,
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
            ],
        });
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
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
            ],
        });
        let base_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("base"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
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
        for (name, idx, sky) in jobs {
            let Some(Some((mat, rgba))) = materials.get(name) else {
                continue;
            };
            let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("material params"),
                contents: bytemuck::bytes_of(&MaterialParams {
                    alpha_test: mat.translucent as u32,
                    pad: [0; 3],
                }),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let tex = upload_rgba(
                gpu,
                &rgba.pixels,
                rgba.width,
                rgba.height,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                name,
            );
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
                        binding: 2,
                        resource: params.as_entire_binding(),
                    },
                ],
            });
            batches.push(Batch {
                bind_group,
                first_index: indices_all.len() as u32,
                index_count: idx.len() as u32,
                material: name.clone(),
                sky,
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
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("world"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x2, 3 => Float32x3],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        Ok(WorldRenderer {
            pipeline,
            camera_buffer,
            camera_bind_group,
            vertex_buffer,
            index_buffer,
            batches,
            sky_camera,
            stats,
            sky2d: build_sky_cube(gpu, mount, bsp, &material_layout, white_uv),
            material_layout,
            base_sampler,
            white_uv,
            sun,
        })
    }

    pub fn materials(&self) -> impl Iterator<Item = &str> {
        self.batches.iter().map(|b| b.material.as_str())
    }

    /// Render one frame into `color` (cleared to sky grey) and `depth`.
    /// The 3D skybox, if any, is drawn first from the sky camera and the depth cleared after it.
    pub fn draw(
        &self,
        gpu: &Gpu,
        camera: &Camera,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        aspect: f32,
    ) {
        let mut clear = true;
        if self.sky2d.is_some() {
            self.sky_pass(gpu, camera, color, depth, aspect);
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
            self.pass(gpu, &sky_cam, color, depth, aspect, true, clear);
            self.pass(gpu, camera, color, depth, aspect, false, false);
        } else {
            self.pass(gpu, camera, color, depth, aspect, false, clear);
        }
    }

    /// The 2D sky cube around an eye at the origin, clearing colour and depth.
    fn sky_pass(
        &self,
        gpu: &Gpu,
        camera: &Camera,
        color: &wgpu::TextureView,
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
            bytemuck::bytes_of(&CameraUniform {
                view_proj: cam.view_proj(aspect).to_cols_array_2d(),
            }),
        );
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sky2d"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: None,
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, sky.vertex_buffer.slice(..));
            pass.set_index_buffer(sky.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            for (i, bg) in sky.faces.iter().enumerate() {
                pass.set_bind_group(1, bg, &[]);
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
        depth: &wgpu::TextureView,
        aspect: f32,
        sky: bool,
        clear_color: bool,
    ) {
        let vp = camera.view_proj(aspect);
        gpu.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_proj: vp.to_cols_array_2d(),
            }),
        );
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(if sky { "skybox" } else { "world" }),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: None,
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
            for b in self.batches.iter().filter(|b| b.sky == sky) {
                pass.set_bind_group(1, &b.bind_group, &[]);
                pass.draw_indexed(b.first_index..b.first_index + b.index_count, 0, 0..1);
            }
        }
        gpu.queue.submit([enc.finish()]);
    }
}

impl WorldRenderer {
    /// Loads `path` (body group 0, skin 0) for `draw_models`; `None` if it has no drawable mesh.
    pub fn load_model(&self, gpu: &Gpu, mount: &Mount, path: &str) -> Option<DynModel> {
        let (mdl, vvd, vtx) = crate::props::load_model(mount, path).ok()?;
        let mut materials: crate::props::MaterialMap = HashMap::new();
        let mut missing = Vec::new();
        let keys: Vec<Option<String>> = (0..mdl.textures.len())
            .map(|t| {
                crate::props::resolve_model_texture(mount, &mdl, t, &mut materials, &mut missing)
            })
            .collect();
        let mut parts = Vec::new();
        for (bi, bp) in vtx.body_parts.iter().enumerate() {
            let Some(mdl_bp) = mdl.body_parts.get(bi) else {
                continue;
            };
            let (Some(vm), Some(mm)) = (bp.models.first(), mdl_bp.models.first()) else {
                continue;
            };
            let vertex_base = (mm.vertex_index / 48).max(0) as usize;
            let Some(lod) = vm.lods.first() else { continue };
            for (mi, mesh) in lod.meshes.iter().enumerate() {
                let Some(mdl_mesh) = mm.meshes.get(mi) else {
                    continue;
                };
                let tex = mdl
                    .skins
                    .first()
                    .and_then(|s| s.get(mdl_mesh.material.max(0) as usize))
                    .map(|&t| t.max(0) as usize)
                    .unwrap_or(mdl_mesh.material.max(0) as usize);
                let Some(Some(key)) = keys.get(tex) else {
                    continue;
                };
                let Some(Some((mat, rgba))) = materials.get(key) else {
                    continue;
                };
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
                        verts.push((v.position, v.normal, v.uv));
                    }
                    idx.push(li);
                }
                let device = &gpu.device;
                let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("model material params"),
                    contents: bytemuck::bytes_of(&MaterialParams {
                        alpha_test: mat.translucent as u32,
                        pad: [0; 3],
                    }),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
                let t = upload_rgba(
                    gpu,
                    &rgba.pixels,
                    rgba.width,
                    rgba.height,
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                    key,
                );
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
                            binding: 2,
                            resource: params.as_entire_binding(),
                        },
                    ],
                });
                parts.push((bg, verts, idx));
            }
        }
        (!parts.is_empty()).then_some(DynModel {
            parts,
            unlit: false,
        })
    }

    /// A unit cube (-0.5..0.5) of one colour, scaled by the item transform: player boxes,
    /// the physgun beam.
    pub fn solid_box(&self, gpu: &Gpu, rgba: [u8; 4], unlit: bool) -> DynModel {
        let device = &gpu.device;
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("solid params"),
            contents: bytemuck::bytes_of(&MaterialParams {
                alpha_test: 0,
                pad: [0; 3],
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
                    verts.push((p, n, [0.5, 0.5]));
                }
                if sign > 0.0 {
                    idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                } else {
                    idx.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
                }
            }
        }
        DynModel {
            parts: vec![(bg, verts, idx)],
            unlit,
        }
    }

    /// Draws moving models over the frame `draw` left in `color`/`depth`: each item is a model
    /// with its origin and Source angles, transformed on the CPU into one buffer per frame.
    pub fn draw_models(
        &self,
        gpu: &Gpu,
        camera: &Camera,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        aspect: f32,
        items: &[(&DynModel, Mat4)],
    ) {
        let mut verts: Vec<Vertex> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        let mut draws = Vec::new();
        for (m, xf) in items {
            for (bg, pv, pi) in &m.parts {
                let base = verts.len() as u32;
                for (p, n, uv) in pv {
                    let n = xf.transform_vector3(Vec3::from(*n)).normalize_or_zero();
                    let shade = if m.unlit {
                        1.0
                    } else {
                        0.45 + 0.55 * n.dot(self.sun).max(0.0)
                    };
                    verts.push(Vertex {
                        position: xf.transform_point3(Vec3::from(*p)).to_array(),
                        uv: *uv,
                        lm_uv: self.white_uv,
                        color: [shade; 3],
                    });
                }
                let first = idx.len() as u32;
                idx.extend(pi.iter().map(|i| i + base));
                draws.push((bg, first, pi.len() as u32));
            }
        }
        if draws.is_empty() {
            return;
        }
        let device = &gpu.device;
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("model vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("model indices"),
            contents: bytemuck::cast_slice(&idx),
            usage: wgpu::BufferUsages::INDEX,
        });
        gpu.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_proj: camera.view_proj(aspect).to_cols_array_2d(),
            }),
        );
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("models"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: None,
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
            pass.set_pipeline(&self.pipeline);
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
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        size,
    );
    tex
}

/// Create an offscreen color + depth target pair.
pub fn offscreen_targets(gpu: &Gpu, width: u32, height: u32) -> (wgpu::Texture, wgpu::Texture) {
    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let color = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("color"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    (color, depth)
}
