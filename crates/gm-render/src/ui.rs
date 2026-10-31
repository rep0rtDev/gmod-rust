//! Screen-space text, rectangles and textured quads for the console and HUD: a glyph atlas
//! rasterised from a TrueType font in the mounted content per registered font (the default
//! is Roboto 16 px), VTF textures as images, one alpha-blended pipeline, vertices rebuilt per
//! frame and drawn in batches per texture.

use std::collections::HashMap;

use ab_glyph::{point, Font, FontArc, ScaleFont};
use anyhow::{anyhow, Result};
use bytemuck::{Pod, Zeroable};
use gm_content::Mount;
use wgpu::util::DeviceExt;

use crate::gpu::Gpu;
use crate::material;
use crate::world::upload_rgba;

/// Roboto ships with the game under `resource/fonts`.
pub const DEFAULT_FONT: &str = "resource/fonts/Roboto-Regular.ttf";
pub const DEFAULT_FONT_PX: f32 = 16.0;
const ATLAS_SIZE: u32 = 1024;
/// Images are decoded down to this side at most (HUD textures are small; sprays and the like
/// do not need full mips on screen).
const IMAGE_MAX_SIDE: u32 = 1024;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Screen {
    size: [f32; 2],
    pad: [f32; 2],
}

#[derive(Clone, Copy)]
struct Glyph {
    /// Atlas rectangle in texels.
    uv: [f32; 4],
    /// Pixel size of the glyph box.
    size: [f32; 2],
    /// Offset of the glyph box from the pen (x) and the baseline (y).
    offset: [f32; 2],
    advance: f32,
}

pub enum UiItem {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
    },
    /// `y` is the top of the line; the text sits on the font's baseline below it. `font` is a
    /// name given to `register_font`, the default font when `None` or unknown.
    Text {
        x: f32,
        y: f32,
        text: String,
        color: [f32; 4],
        font: Option<String>,
    },
    /// A VTF (a material's `$basetexture`, or a texture path) stretched over the rectangle and
    /// tinted by `color`; a missing texture draws nothing.
    Image {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        material: String,
        color: [f32; 4],
        /// Quarter turns clockwise of the texture (`DrawTexturedRectRotated` 0/90/180/270).
        rot: u8,
    },
    /// A sub-rectangle `uv` = [u0, v0, u1, v1] of an image (GWEN atlas 9-slice pieces).
    Slice {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        material: String,
        color: [f32; 4],
        uv: [f32; 4],
    },
}

/// What a font's layout needs outside the renderer: the line height and the advance of
/// every glyph in the atlas (`surface.GetTextSize`).
#[derive(Clone, Debug, Default)]
pub struct FontMetrics {
    pub line_height: f32,
    pub advances: HashMap<char, f32>,
    pub fallback: f32,
}

impl FontMetrics {
    pub fn text_width(&self, text: &str) -> f32 {
        text.chars()
            .map(|c| self.advances.get(&c).copied().unwrap_or(self.fallback))
            .sum()
    }
}

struct FontAtlas {
    glyphs: HashMap<char, Glyph>,
    fallback: Glyph,
    white_uv: [f32; 2],
    ascent: f32,
    line_height: f32,
    bind_group: wgpu::BindGroup,
}

impl FontAtlas {
    fn build(
        gpu: &Gpu,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        screen_buffer: &wgpu::Buffer,
        ttf: &[u8],
        px: f32,
    ) -> Result<Self> {
        let font = FontArc::try_from_vec(ttf.to_vec()).map_err(|e| anyhow!("font: {e}"))?;
        let scaled = font.as_scaled(px);
        let ascent = scaled.ascent();
        let line_height = (scaled.height() + scaled.line_gap()).ceil();

        let mut atlas = vec![0u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize];
        // a 4x4 opaque block for rectangles
        for y in 0..4 {
            for x in 0..4 {
                let i = ((y * ATLAS_SIZE + x) * 4) as usize;
                atlas[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        let white_uv = [2.0 / ATLAS_SIZE as f32, 2.0 / ATLAS_SIZE as f32];
        let (mut pen_x, mut pen_y, mut row_h) = (6u32, 0u32, 4u32);
        let mut glyphs = HashMap::new();
        let chars = (0x20u32..0x7f)
            .chain(0xa0..0x100)
            .chain(0x400..0x460)
            .filter_map(char::from_u32);
        for c in chars {
            let id = font.glyph_id(c);
            if id.0 == 0 && c != ' ' {
                continue;
            }
            let advance = scaled.h_advance(id);
            let glyph = id.with_scale_and_position(px, point(0.0, 0.0));
            let Some(outline) = font.outline_glyph(glyph) else {
                glyphs.insert(
                    c,
                    Glyph {
                        uv: [0.0; 4],
                        size: [0.0; 2],
                        offset: [0.0; 2],
                        advance,
                    },
                );
                continue;
            };
            let b = outline.px_bounds();
            let (w, h) = (b.width().ceil() as u32 + 1, b.height().ceil() as u32 + 1);
            if pen_x + w + 1 > ATLAS_SIZE {
                pen_x = 0;
                pen_y += row_h + 1;
                row_h = 0;
            }
            if pen_y + h + 1 > ATLAS_SIZE {
                break;
            }
            outline.draw(|x, y, cov| {
                let (ax, ay) = (pen_x + x, pen_y + y);
                if ax < ATLAS_SIZE && ay < ATLAS_SIZE {
                    let i = ((ay * ATLAS_SIZE + ax) * 4) as usize;
                    atlas[i..i + 4].copy_from_slice(&[255, 255, 255, (cov * 255.0) as u8]);
                }
            });
            glyphs.insert(
                c,
                Glyph {
                    uv: [
                        pen_x as f32,
                        pen_y as f32,
                        (pen_x + w) as f32,
                        (pen_y + h) as f32,
                    ],
                    size: [w as f32, h as f32],
                    offset: [b.min.x, b.min.y],
                    advance,
                },
            );
            pen_x += w + 1;
            row_h = row_h.max(h);
        }
        let fallback = glyphs
            .get(&'?')
            .copied()
            .ok_or_else(|| anyhow!("font has no '?' glyph"))?;

        let texture = upload_rgba(
            gpu,
            &atlas,
            ATLAS_SIZE,
            ATLAS_SIZE,
            wgpu::TextureFormat::Rgba8Unorm,
            "ui atlas",
        );
        let view = texture.create_view(&Default::default());
        let bind_group = texture_bind_group(gpu, layout, sampler, screen_buffer, &view, "ui font");
        Ok(FontAtlas {
            glyphs,
            fallback,
            white_uv,
            ascent,
            line_height,
            bind_group,
        })
    }

    fn glyph(&self, c: char) -> Glyph {
        self.glyphs.get(&c).copied().unwrap_or(self.fallback)
    }

    fn text_width(&self, text: &str) -> f32 {
        text.chars().map(|c| self.glyph(c).advance).sum()
    }

    fn metrics(&self) -> FontMetrics {
        FontMetrics {
            line_height: self.line_height,
            advances: self.glyphs.iter().map(|(c, g)| (*c, g.advance)).collect(),
            fallback: self.fallback.advance,
        }
    }
}

fn texture_bind_group(
    gpu: &Gpu,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    screen_buffer: &wgpu::Buffer,
    view: &wgpu::TextureView,
    label: &str,
) -> wgpu::BindGroup {
    gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// A TrueType family of the mounted content: `resource/fonts/<Family>.ttf` (Roboto and its
/// weights, `Roboto-Bold`), `resource/<FAMILY>.ttf` (the HL2 icon fonts), looked up by the
/// family name with spaces, hyphens and case ignored; `Roboto` is `Roboto-Regular`.
fn font_file(mount: &Mount, family: &str) -> Option<Vec<u8>> {
    let norm = |s: &str| {
        s.chars()
            .filter(|c| !matches!(c, ' ' | '-' | '_'))
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    if family.to_ascii_lowercase().ends_with(".ttf") {
        return mount.read(family).ok();
    }
    let want = norm(family);
    let direct = [
        format!("resource/fonts/{family}.ttf"),
        format!("resource/fonts/{family}-Regular.ttf"),
        format!("resource/{family}.ttf"),
    ];
    for p in &direct {
        if let Ok(b) = mount.read(p) {
            return Some(b);
        }
    }
    let mut files: Vec<String> = mount.find("resource/fonts/*.ttf").into_iter().collect();
    files.extend(mount.find("resource/*.ttf"));
    let stem = |p: &str| {
        p.rsplit('/')
            .next()
            .unwrap_or(p)
            .trim_end_matches(".ttf")
            .trim_end_matches(".TTF")
            .to_string()
    };
    files
        .iter()
        .find(|p| {
            let s = norm(&stem(p));
            s == want || s == format!("{want}regular")
        })
        .and_then(|p| mount.read(p).ok())
}

/// A run of vertices drawn with one texture.
type Batch<'a> = (&'a wgpu::BindGroup, std::ops::Range<u32>);

pub struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    /// The same items on a plane in the world (3D2D): depth-tested against the scene.
    world_pipeline: wgpu::RenderPipeline,
    plane_layout: wgpu::BindGroupLayout,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    screen_buffer: wgpu::Buffer,
    default: FontAtlas,
    fonts: HashMap<String, FontAtlas>,
    /// Loaded images by name; `None` remembers a texture that is not in the content.
    images: HashMap<String, Option<wgpu::BindGroup>>,
}

impl UiRenderer {
    pub fn new(gpu: &Gpu, mount: &Mount, color_format: wgpu::TextureFormat) -> Result<Self> {
        let bytes = mount.read(DEFAULT_FONT)?;
        Self::with_font(gpu, &bytes, DEFAULT_FONT_PX, color_format)
    }

    pub fn with_font(
        gpu: &Gpu,
        ttf: &[u8],
        px: f32,
        color_format: wgpu::TextureFormat,
    ) -> Result<Self> {
        let device = &gpu.device;
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let screen_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui screen"),
            size: std::mem::size_of::<Screen>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui"),
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
        let default = FontAtlas::build(gpu, &layout, &sampler, &screen_buffer, ttf, px)?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/ui.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let plane_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui plane"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let world_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui world"),
            bind_group_layouts: &[Some(&layout), Some(&plane_layout)],
            ..Default::default()
        });
        let world_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui world"),
            layout: Some(&world_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_world"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Ok(UiRenderer {
            pipeline,
            world_pipeline,
            plane_layout,
            layout,
            sampler,
            screen_buffer,
            default,
            fonts: HashMap::new(),
            images: HashMap::new(),
        })
    }

    pub fn line_height(&self) -> f32 {
        self.default.line_height
    }

    pub fn text_width(&self, text: &str) -> f32 {
        self.default.text_width(text)
    }

    /// Longest prefix of `text` that fits in `width` pixels, in chars.
    pub fn fit(&self, text: &str, width: f32) -> usize {
        let mut x = 0.0;
        for (i, c) in text.chars().enumerate() {
            x += self.default.glyph(c).advance;
            if x > width {
                return i;
            }
        }
        text.chars().count()
    }

    /// `surface.CreateFont`: `family` of the content at `px` pixels under `name` (a name
    /// registered again is replaced). False when no such family is mounted or the font cannot
    /// be rasterised; the name then draws with the default font.
    pub fn register_font(
        &mut self,
        gpu: &Gpu,
        mount: &Mount,
        name: &str,
        family: &str,
        px: f32,
    ) -> bool {
        let Some(ttf) = font_file(mount, family) else {
            return false;
        };
        let px = px.clamp(4.0, 256.0);
        match FontAtlas::build(
            gpu,
            &self.layout,
            &self.sampler,
            &self.screen_buffer,
            &ttf,
            px,
        ) {
            Ok(atlas) => {
                self.fonts.insert(name.to_string(), atlas);
                true
            }
            Err(_) => false,
        }
    }

    pub fn has_font(&self, name: &str) -> bool {
        self.fonts.contains_key(name)
    }

    fn font(&self, name: Option<&str>) -> &FontAtlas {
        name.and_then(|n| self.fonts.get(n))
            .unwrap_or(&self.default)
    }

    /// Metrics of a registered font (the default font's for an unknown name).
    pub fn font_metrics(&self, name: Option<&str>) -> FontMetrics {
        self.font(name).metrics()
    }

    /// A VTF for `Image` items: a material's `$basetexture` when `name` is a material, else
    /// the texture itself; decoded once, a missing one is remembered as missing. Returns
    /// whether it is there.
    pub fn load_image(&mut self, gpu: &Gpu, mount: &Mount, name: &str) -> bool {
        if let Some(entry) = self.images.get(name) {
            return entry.is_some();
        }
        let texture_name = material::load(mount, name)
            .ok()
            .and_then(|m| m.base_texture)
            .unwrap_or_else(|| name.to_string());
        let loaded = material::load_texture(mount, &texture_name, IMAGE_MAX_SIDE)
            .ok()
            .map(|img| {
                let texture = upload_rgba(
                    gpu,
                    &img.pixels,
                    img.width,
                    img.height,
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                    "ui image",
                );
                let view = texture.create_view(&Default::default());
                texture_bind_group(
                    gpu,
                    &self.layout,
                    &self.sampler,
                    &self.screen_buffer,
                    &view,
                    "ui image",
                )
            });
        let present = loaded.is_some();
        self.images.insert(name.to_string(), loaded);
        present
    }

    /// Registers an already rendered texture (a spawn icon) under `name` for `UiItem::Image`.
    pub fn register_image_view(&mut self, gpu: &Gpu, name: &str, view: &wgpu::TextureView) {
        let bg = texture_bind_group(
            gpu,
            &self.layout,
            &self.sampler,
            &self.screen_buffer,
            view,
            "ui icon",
        );
        self.images.insert(name.to_string(), Some(bg));
    }

    /// Remembers `name` as an image that could not be made, so it is not retried.
    pub fn register_missing_image(&mut self, name: &str) {
        self.images.insert(name.to_string(), None);
    }

    /// Whether `name` is a registered image (present or known missing).
    pub fn has_image(&self, name: &str) -> bool {
        self.images.contains_key(name)
    }

    /// Vertices of `items` in order with one batch per run of the same texture.
    fn build(
        &self,
        items: &[UiItem],
    ) -> (Vec<Vertex>, Vec<(&wgpu::BindGroup, std::ops::Range<u32>)>) {
        // vertices in item order, split into batches wherever the texture changes
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut batches: Vec<Batch<'_>> = Vec::new();
        let s = ATLAS_SIZE as f32;
        fn quad(
            vertices: &mut Vec<Vertex>,
            x0: f32,
            y0: f32,
            x1: f32,
            y1: f32,
            uv: [f32; 4],
            color: [f32; 4],
        ) {
            let v = |x, y, u, w| Vertex {
                position: [x, y],
                uv: [u, w],
                color,
            };
            let [u0, v0, u1, v1] = uv;
            vertices.extend_from_slice(&[
                v(x0, y0, u0, v0),
                v(x1, y0, u1, v0),
                v(x1, y1, u1, v1),
                v(x0, y0, u0, v0),
                v(x1, y1, u1, v1),
                v(x0, y1, u0, v1),
            ]);
        }
        // the texture's corners (tl, tr, br, bl) turned clockwise `rot` quarters
        fn quad_rot(
            vertices: &mut Vec<Vertex>,
            x0: f32,
            y0: f32,
            x1: f32,
            y1: f32,
            rot: u8,
            color: [f32; 4],
        ) {
            let c = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
            let at = |i: usize| c[(i + 4 - (rot as usize % 4)) % 4];
            let v = |x: f32, y: f32, uv: [f32; 2]| Vertex {
                position: [x, y],
                uv,
                color,
            };
            vertices.extend_from_slice(&[
                v(x0, y0, at(0)),
                v(x1, y0, at(1)),
                v(x1, y1, at(2)),
                v(x0, y0, at(0)),
                v(x1, y1, at(2)),
                v(x0, y1, at(3)),
            ]);
        }
        fn push_batch<'a>(
            bg: &'a wgpu::BindGroup,
            from: u32,
            to: u32,
            batches: &mut Vec<Batch<'a>>,
        ) {
            if from == to {
                return;
            }
            match batches.last_mut() {
                Some((last, range)) if std::ptr::eq(*last, bg) && range.end == from => {
                    range.end = to
                }
                _ => batches.push((bg, from..to)),
            }
        }
        for item in items {
            let from = vertices.len() as u32;
            let bg: &wgpu::BindGroup = match item {
                UiItem::Rect { x, y, w, h, color } => {
                    let [u, v] = self.default.white_uv;
                    quad(&mut vertices, *x, *y, x + w, y + h, [u, v, u, v], *color);
                    &self.default.bind_group
                }
                UiItem::Text {
                    x,
                    y,
                    text,
                    color,
                    font,
                } => {
                    let f = self.font(font.as_deref());
                    let mut pen = x.round();
                    let base = y.round() + f.ascent.round();
                    for c in text.chars() {
                        let g = f.glyph(c);
                        if g.size[0] > 0.0 {
                            let gx = pen + g.offset[0];
                            let gy = base + g.offset[1];
                            quad(
                                &mut vertices,
                                gx,
                                gy,
                                gx + g.size[0],
                                gy + g.size[1],
                                [g.uv[0] / s, g.uv[1] / s, g.uv[2] / s, g.uv[3] / s],
                                *color,
                            );
                        }
                        pen += g.advance;
                    }
                    &f.bind_group
                }
                UiItem::Slice {
                    x,
                    y,
                    w,
                    h,
                    material,
                    color,
                    uv,
                } => {
                    let Some(Some(bg)) = self.images.get(material) else {
                        continue;
                    };
                    quad(&mut vertices, *x, *y, x + w, y + h, *uv, *color);
                    bg
                }
                UiItem::Image {
                    x,
                    y,
                    w,
                    h,
                    material,
                    color,
                    rot,
                } => {
                    let Some(Some(bg)) = self.images.get(material) else {
                        continue;
                    };
                    quad_rot(&mut vertices, *x, *y, x + w, y + h, *rot, *color);
                    bg
                }
            };
            push_batch(bg, from, vertices.len() as u32, &mut batches);
        }
        (vertices, batches)
    }

    pub fn draw(
        &self,
        gpu: &Gpu,
        color: &wgpu::TextureView,
        width: u32,
        height: u32,
        items: &[UiItem],
    ) {
        let (vertices, batches) = self.build(items);
        if vertices.is_empty() {
            return;
        }
        gpu.queue.write_buffer(
            &self.screen_buffer,
            0,
            bytemuck::bytes_of(&Screen {
                size: [width as f32, height as f32],
                pad: [0.0; 2],
            }),
        );
        let vb = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ui vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, vb.slice(..));
            for (bg, range) in &batches {
                pass.set_bind_group(0, *bg, &[]);
                pass.draw(range.clone(), 0..1);
            }
        }
        gpu.queue.submit([enc.finish()]);
    }

    /// 3D2D: `planes` are (plane-to-world matrix, items) pairs; pixel coordinates on the plane
    /// go through the matrix and the camera, depth-tested against the scene's depth without
    /// writing it (a decal on the world, drawn in order, alpha-blended).
    pub fn draw_world(
        &self,
        gpu: &Gpu,
        color: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        view_proj: glam::Mat4,
        planes: &[(glam::Mat4, Vec<UiItem>)],
    ) {
        let mut all: Vec<Vertex> = Vec::new();
        let mut draws: Vec<(wgpu::BindGroup, Vec<Batch<'_>>)> = Vec::new();
        for (plane, items) in planes {
            let (vertices, batches) = self.build(items);
            if vertices.is_empty() {
                continue;
            }
            let base = all.len() as u32;
            all.extend(vertices);
            let m = (view_proj * *plane).to_cols_array_2d();
            let buffer = gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("ui plane"),
                    contents: bytemuck::cast_slice(&m),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ui plane"),
                layout: &self.plane_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
            let shifted = batches
                .into_iter()
                .map(|(b, r)| (b, r.start + base..r.end + base))
                .collect();
            draws.push((bg, shifted));
        }
        if all.is_empty() {
            return;
        }
        let vb = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ui world vertices"),
                contents: bytemuck::cast_slice(&all),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui world"),
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
            pass.set_pipeline(&self.world_pipeline);
            pass.set_vertex_buffer(0, vb.slice(..));
            for (plane_bg, batches) in &draws {
                pass.set_bind_group(1, plane_bg, &[]);
                for (bg, range) in batches {
                    pass.set_bind_group(0, *bg, &[]);
                    pass.draw(range.clone(), 0..1);
                }
            }
        }
        gpu.queue.submit([enc.finish()]);
    }
}

/// `cam.Start3D2D(pos, angles, scale)` as a plane-to-world matrix: pixel x runs along the
/// angles' forward, pixel y along their right (down the plane when it stands up), `scale`
/// world units per pixel; the plane's normal is the angles' up. The axes follow Source's
/// `AngleMatrix` (forward, left, up); the game's own mapping is not recorded yet.
pub fn plane_transform(pos: glam::Vec3, angles: glam::Vec3, scale: f32) -> glam::Mat4 {
    let m = crate::props::angle_matrix(angles.to_array());
    let forward = m.x_axis;
    let left = m.y_axis;
    glam::Mat4::from_cols(
        (forward * scale).extend(0.0),
        (-left * scale).extend(0.0),
        m.z_axis.extend(0.0),
        pos.extend(1.0),
    )
}

#[cfg(test)]
mod plane_tests {
    use super::plane_transform;
    use glam::{Vec3, Vec4};

    #[test]
    fn pixels_map_along_forward_and_right_at_scale() {
        // yaw 90: forward +y, right +x; a pixel (10, 5) at scale 0.5 lands 5 u along +y and
        // 2.5 u along +x from the origin
        let m = plane_transform(Vec3::new(100.0, 0.0, 10.0), Vec3::new(0.0, 90.0, 0.0), 0.5);
        let p = m * Vec4::new(10.0, 5.0, 0.0, 1.0);
        assert!(
            (p.truncate() - Vec3::new(102.5, 5.0, 10.0)).length() < 1e-4,
            "{p}"
        );
        // the plane normal is up for flat angles
        let n = m * Vec4::new(0.0, 0.0, 1.0, 0.0);
        assert!((n.truncate() - Vec3::Z).length() < 1e-4, "{n}");
    }
}
