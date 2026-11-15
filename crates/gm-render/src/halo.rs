//! `halo.Add` (the game's halo library): the given models are drawn into a screen-size mask,
//! the mask's coverage is blurred separably by `size` pixels and added onto the scene in the
//! halo colour outside the models' own pixels. The physgun's held object (`physgun_halo`),
//! the freeze / unfreeze flashes (`effects_freeze`) use it.

use crate::gpu::Gpu;
use crate::world::{Camera, DrawItem, ModelTarget, WorldRenderer};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    dir: [f32; 2],
    alpha: f32,
    pad: f32,
    color: [f32; 4],
}

/// Halos drawn in one frame before the parameter buffers wrap around.
const SLOTS: usize = 8;

struct Slot {
    params: [wgpu::Buffer; 3],
    groups: [wgpu::BindGroup; 3],
}

pub struct Halo {
    width: u32,
    height: u32,
    msaa: u32,
    blur: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    mask_group: wgpu::BindGroup,
    /// Multisampled colour the models draw into (resolved into `mask`) and its depth.
    ms_color: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    mask: wgpu::TextureView,
    blurred: [wgpu::TextureView; 2],
    slots: Vec<Slot>,
    next: std::cell::Cell<usize>,
}

const BLUR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

impl Halo {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat, width: u32, height: u32, msaa: u32) -> Self {
        let device = &gpu.device;
        let (width, height) = (width.max(1), height.max(1));
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("halo"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/halo.wgsl").into()),
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let texture_ty = wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        };
        let sampler_ty = wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering);
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("halo src"),
            entries: &[
                entry(0, texture_ty),
                entry(1, sampler_ty),
                entry(
                    2,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });
        let mask_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("halo mask"),
            entries: &[entry(0, texture_ty), entry(1, sampler_ty)],
        });
        let pipeline = |label: &str,
                        fs: &str,
                        layouts: &[Option<&wgpu::BindGroupLayout>],
                        target: wgpu::TextureFormat,
                        samples: u32,
                        blend: Option<wgpu::BlendState>| {
            let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: layouts,
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: samples,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let blur = pipeline(
            "halo blur",
            "fs_blur",
            &[Some(&layout)],
            BLUR_FORMAT,
            1,
            None,
        );
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let composite = pipeline(
            "halo composite",
            "fs_composite",
            &[Some(&layout), Some(&mask_layout)],
            format,
            msaa,
            Some(additive),
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("halo"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let make = |label: &str, f: wgpu::TextureFormat, samples: u32, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size,
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format: f,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let attach = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let sampled = attach | wgpu::TextureUsages::TEXTURE_BINDING;
        let ms_color = (msaa > 1).then(|| make("halo ms", format, msaa, attach));
        let depth = make(
            "halo depth",
            wgpu::TextureFormat::Depth32Float,
            msaa,
            attach,
        );
        let mask = make("halo mask", format, 1, sampled);
        let blurred = [
            make("halo blur 0", BLUR_FORMAT, 1, sampled),
            make("halo blur 1", BLUR_FORMAT, 1, sampled),
        ];
        let group = |label: &str, src: &wgpu::TextureView, buf: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: buf.as_entire_binding(),
                    },
                ],
            })
        };
        let slots = (0..SLOTS)
            .map(|_| {
                let params = std::array::from_fn(|_| {
                    device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("halo params"),
                        size: std::mem::size_of::<Params>() as u64,
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    })
                });
                let groups = [
                    group("halo blur h", &mask, &params[0]),
                    group("halo blur v", &blurred[0], &params[1]),
                    group("halo composite", &blurred[1], &params[2]),
                ];
                Slot { params, groups }
            })
            .collect();
        let mask_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("halo mask"),
            layout: &mask_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&mask),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        Self {
            width,
            height,
            msaa,
            blur,
            composite,
            mask_group,
            ms_color,
            depth,
            mask,
            blurred,
            slots,
            next: std::cell::Cell::new(0),
        }
    }

    /// `halo.Add(items, color, size, size, 1, additive, ignorez = true)`: the halo of
    /// `items` at `alpha` strength onto `attach` (resolving into `resolve`), under `camera`.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        world: &WorldRenderer,
        camera: &Camera,
        items: &[DrawItem<'_>],
        color: [f32; 3],
        alpha: f32,
        size: f32,
        attach: &wgpu::TextureView,
        resolve: Option<&wgpu::TextureView>,
    ) {
        if items.is_empty() || alpha <= 0.0 {
            return;
        }
        let slot = &self.slots[self.next.get() % SLOTS];
        self.next.set(self.next.get() + 1);
        let texel = [size / self.width as f32, size / self.height as f32];
        let all = [
            Params {
                dir: [texel[0], 0.0],
                alpha,
                pad: 0.0,
                color: [color[0], color[1], color[2], 1.0],
            },
            Params {
                dir: [0.0, texel[1]],
                alpha,
                pad: 0.0,
                color: [color[0], color[1], color[2], 1.0],
            },
            Params {
                dir: [0.0; 2],
                alpha,
                pad: 0.0,
                color: [color[0], color[1], color[2], 1.0],
            },
        ];
        for (b, p) in slot.params.iter().zip(all) {
            gpu.queue.write_buffer(b, 0, bytemuck::bytes_of(&p));
        }
        let model_color = self.ms_color.as_ref().unwrap_or(&self.mask);
        // the mask starts empty (the model pass loads what is there)
        enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("halo clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: model_color,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let target = ModelTarget {
            color: model_color,
            resolve: self.ms_color.as_ref().map(|_| &self.mask),
            depth: &self.depth,
            aspect: self.width as f32 / self.height as f32,
            clear_depth: true,
            alpha: 1.0,
        };
        world.draw_models(gpu, enc, camera, &target, items);
        let passes: [(
            &wgpu::RenderPipeline,
            &wgpu::BindGroup,
            &wgpu::TextureView,
            Option<&wgpu::TextureView>,
            bool,
        ); 3] = [
            (&self.blur, &slot.groups[0], &self.blurred[0], None, true),
            (&self.blur, &slot.groups[1], &self.blurred[1], None, true),
            (&self.composite, &slot.groups[2], attach, resolve, false),
        ];
        for (pipeline, group, view, resolve_target, clear) in passes {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("halo"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: if clear {
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, group, &[]);
            if !clear {
                pass.set_bind_group(1, &self.mask_group, &[]);
            }
            pass.draw(0..3, 0..1);
        }
        let _ = self.msaa;
    }
}
