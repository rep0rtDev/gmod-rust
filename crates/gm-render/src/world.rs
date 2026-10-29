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
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub faces: usize,
    pub displacements: usize,
    pub triangles: usize,
    pub skybox_faces: usize,
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
        let mut skybox_faces: HashSet<usize> = HashSet::new();
        if let Some(sky) = sky_camera {
            let area = bsp.leafs[bsp.leaf_at(sky.origin.into())].area();
            for (fi, face) in bsp.faces.iter().enumerate() {
                let verts = bsp.face_vertices(face);
                if verts.is_empty() {
                    continue;
                }
                let mut c = [0f32; 3];
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

        let mut resolve = |name: &str, stats: &mut Stats| -> Option<(String, u32, u32)> {
            let canon = material::canonical_name(name);
            let entry = materials.entry(canon.clone()).or_insert_with(|| {
                let m = material::load(mount, &canon).ok()?;
                if m.no_draw {
                    return None;
                }
                if m.water {
                    return Some((
                        m,
                        material::Rgba {
                            width: 1,
                            height: 1,
                            pixels: vec![60, 90, 110, 255],
                        },
                    ));
                }
                let tex = m.base_texture.as_deref().unwrap_or("debug/debugempty");
                match material::load_texture(mount, tex, 1024) {
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
            let white_uv = atlas.luxel_center(&white, 0.0, 0.0);
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
            for p in &poly {
                vertices.push(Vertex {
                    position: *p,
                    uv: face_uv(bsp, face, *p, tw, th),
                    lm_uv: if lit {
                        face_lm_uv(bsp, face, *p, &placement, atlas_size as f32)
                    } else {
                        white_uv
                    },
                });
            }
            for i in 1..poly.len() as u32 - 1 {
                indices.extend_from_slice(&[base, base + i, base + i + 1]);
            }
            stats.faces += 1;
            stats.triangles += poly.len() - 2;
        }

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
            let Some(Some((_, rgba))) = materials.get(name) else {
                continue;
            };
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
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x2],
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
        let has_sky = self.sky_camera.is_some() && self.batches.iter().any(|b| b.sky);
        if let (true, Some(sky)) = (has_sky, self.sky_camera) {
            let sky_cam = Camera {
                position: sky.origin + camera.position / sky.scale,
                near: 1.0,
                far: camera.far / sky.scale * 4.0,
                ..*camera
            };
            self.pass(gpu, &sky_cam, color, depth, aspect, true, true);
            self.pass(gpu, camera, color, depth, aspect, false, false);
        } else {
            self.pass(gpu, camera, color, depth, aspect, false, true);
        }
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
                        store: wgpu::StoreOp::Discard,
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
