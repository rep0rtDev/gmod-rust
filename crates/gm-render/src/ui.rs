//! Screen-space text and rectangles for the console and HUD: one glyph atlas
//! rasterised from a TrueType font in the mounted content, one alpha-blended
//! pipeline, vertices rebuilt per frame.

use std::collections::HashMap;

use ab_glyph::{point, Font, FontArc, ScaleFont};
use anyhow::{anyhow, Result};
use bytemuck::{Pod, Zeroable};
use gm_content::Mount;
use wgpu::util::DeviceExt;

use crate::gpu::Gpu;
use crate::world::upload_rgba;

/// Roboto ships with the game under `resource/fonts`.
pub const DEFAULT_FONT: &str = "resource/fonts/Roboto-Regular.ttf";
const ATLAS_SIZE: u32 = 1024;

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
    /// `y` is the top of the line; the text sits on the font's baseline below it.
    Text {
        x: f32,
        y: f32,
        text: String,
        color: [f32; 4],
    },
}

pub struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    screen_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    glyphs: HashMap<char, Glyph>,
    fallback: Glyph,
    white_uv: [f32; 2],
    ascent: f32,
    line_height: f32,
}

impl UiRenderer {
    pub fn new(gpu: &Gpu, mount: &Mount, color_format: wgpu::TextureFormat) -> Result<Self> {
        let bytes = mount.read(DEFAULT_FONT)?;
        Self::with_font(gpu, &bytes, 16.0, color_format)
    }

    pub fn with_font(
        gpu: &Gpu,
        ttf: &[u8],
        px: f32,
        color_format: wgpu::TextureFormat,
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

        let device = &gpu.device;
        let texture = upload_rgba(
            gpu,
            &atlas,
            ATLAS_SIZE,
            ATLAS_SIZE,
            wgpu::TextureFormat::Rgba8Unorm,
            "ui atlas",
        );
        let view = texture.create_view(&Default::default());
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: screen_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
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
        Ok(UiRenderer {
            pipeline,
            screen_buffer,
            bind_group,
            glyphs,
            fallback,
            white_uv,
            ascent,
            line_height,
        })
    }

    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    fn glyph(&self, c: char) -> Glyph {
        self.glyphs.get(&c).copied().unwrap_or(self.fallback)
    }

    pub fn text_width(&self, text: &str) -> f32 {
        text.chars().map(|c| self.glyph(c).advance).sum()
    }

    /// Longest prefix of `text` that fits in `width` pixels, in chars.
    pub fn fit(&self, text: &str, width: f32) -> usize {
        let mut x = 0.0;
        for (i, c) in text.chars().enumerate() {
            x += self.glyph(c).advance;
            if x > width {
                return i;
            }
        }
        text.chars().count()
    }

    pub fn draw(
        &self,
        gpu: &Gpu,
        color: &wgpu::TextureView,
        width: u32,
        height: u32,
        items: &[UiItem],
    ) {
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut quad = |x0: f32, y0: f32, x1: f32, y1: f32, uv: [f32; 4], color: [f32; 4]| {
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
        };
        let s = ATLAS_SIZE as f32;
        for item in items {
            match item {
                UiItem::Rect { x, y, w, h, color } => {
                    let [u, v] = self.white_uv;
                    quad(*x, *y, x + w, y + h, [u, v, u, v], *color);
                }
                UiItem::Text { x, y, text, color } => {
                    let mut pen = x.round();
                    let base = y.round() + self.ascent.round();
                    for c in text.chars() {
                        let g = self.glyph(c);
                        if g.size[0] > 0.0 {
                            let gx = pen + g.offset[0];
                            let gy = base + g.offset[1];
                            quad(
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
                }
            }
        }
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
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        gpu.queue.submit([enc.finish()]);
    }
}
