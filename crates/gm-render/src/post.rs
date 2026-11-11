//! Bloom post-process (`mat_bloomscale`, the map's `SetBloomScale`): the scene is drawn
//! into an offscreen texture, the bright part is taken at quarter resolution, blurred
//! separably and added back while the frame is composited onto the surface.

use crate::gpu::Gpu;

/// Linear luminance above which a pixel feeds the bloom.
const THRESHOLD: f32 = 0.45;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    dir: [f32; 2],
    scale: f32,
    threshold: f32,
}

pub struct Post {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    bloom_layout: wgpu::BindGroupLayout,
    bright: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    scene_view: wgpu::TextureView,
    scene_tex: wgpu::Texture,
    quarter: [wgpu::TextureView; 2],
    params: [wgpu::Buffer; 4],
    groups: [wgpu::BindGroup; 4],
    bloom_group: wgpu::BindGroup,
}

const QUARTER_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

impl Post {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/post.wgsl").into()),
        });
        let tex_entry = |binding, ty| wgpu::BindGroupLayoutEntry {
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
            label: Some("post src"),
            entries: &[
                tex_entry(0, texture_ty),
                tex_entry(1, sampler_ty),
                tex_entry(
                    2,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });
        let bloom_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post bloom"),
            entries: &[tex_entry(0, texture_ty), tex_entry(1, sampler_ty)],
        });
        let pipeline =
            |label: &str, entry: &str, layouts: &[Option<&wgpu::BindGroupLayout>], target| {
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
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(entry),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: target,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview_mask: None,
                    cache: None,
                })
            };
        let bright = pipeline("post bright", "fs_bright", &[Some(&layout)], QUARTER_FORMAT);
        let blur = pipeline("post blur", "fs_blur", &[Some(&layout)], QUARTER_FORMAT);
        let composite = pipeline(
            "post composite",
            "fs_composite",
            &[Some(&layout), Some(&bloom_layout)],
            format,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let params = std::array::from_fn(|i| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("post params {i}")),
                size: std::mem::size_of::<Params>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let (scene_tex, scene_view, quarter, groups, bloom_group) = Self::targets(
            gpu,
            format,
            width,
            height,
            &layout,
            &bloom_layout,
            &sampler,
            &params,
        );
        Self {
            format,
            layout,
            bloom_layout,
            bright,
            blur,
            composite,
            sampler,
            scene_view,
            scene_tex,
            quarter,
            params,
            groups,
            bloom_group,
        }
    }

    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn targets(
        gpu: &Gpu,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        layout: &wgpu::BindGroupLayout,
        bloom_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        params: &[wgpu::Buffer; 4],
    ) -> (
        wgpu::Texture,
        wgpu::TextureView,
        [wgpu::TextureView; 2],
        [wgpu::BindGroup; 4],
        wgpu::BindGroup,
    ) {
        let device = &gpu.device;
        let make = |label: &str, w: u32, h: u32, f| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w.max(1),
                    height: h.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: f,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let scene_tex = make("post scene", width, height, format);
        let scene = scene_tex.create_view(&Default::default());
        let (qw, qh) = (width.div_ceil(4), height.div_ceil(4));
        let quarter = [
            make("post quarter 0", qw, qh, QUARTER_FORMAT).create_view(&Default::default()),
            make("post quarter 1", qw, qh, QUARTER_FORMAT).create_view(&Default::default()),
        ];
        let texel = [1.0 / qw.max(1) as f32, 1.0 / qh.max(1) as f32];
        let all = [
            Params {
                dir: [0.0; 2],
                scale: 0.0,
                threshold: THRESHOLD,
            },
            Params {
                dir: [texel[0], 0.0],
                scale: 0.0,
                threshold: THRESHOLD,
            },
            Params {
                dir: [0.0, texel[1]],
                scale: 0.0,
                threshold: THRESHOLD,
            },
            Params {
                dir: [0.0; 2],
                scale: 1.0,
                threshold: THRESHOLD,
            },
        ];
        for (b, p) in params.iter().zip(all) {
            gpu.queue.write_buffer(b, 0, bytemuck::bytes_of(&p));
        }
        let group = |label: &str, src: &wgpu::TextureView, buf: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: buf.as_entire_binding(),
                    },
                ],
            })
        };
        let groups = [
            group("post bright", &scene, &params[0]),
            group("post blur h", &quarter[0], &params[1]),
            group("post blur v", &quarter[1], &params[2]),
            group("post composite", &scene, &params[3]),
        ];
        let bloom_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post bloom"),
            layout: bloom_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&quarter[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        (scene_tex, scene, quarter, groups, bloom_group)
    }

    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        let (scene_tex, scene_view, quarter, groups, bloom_group) = Self::targets(
            gpu,
            self.format,
            width,
            height,
            &self.layout,
            &self.bloom_layout,
            &self.sampler,
            &self.params,
        );
        self.scene_view = scene_view;
        self.scene_tex = scene_tex;
        self.quarter = quarter;
        self.groups = groups;
        self.bloom_group = bloom_group;
    }

    /// The offscreen colour target the scene draws into when bloom is on.
    pub fn scene_view(&self) -> &wgpu::TextureView {
        &self.scene_view
    }
    /// The same target as a texture (the water refraction copies it).
    pub fn scene_texture(&self) -> &wgpu::Texture {
        &self.scene_tex
    }

    /// Bright pass -> blur -> composite the scene plus `scale` x bloom onto `target`.
    pub fn apply(
        &self,
        gpu: &Gpu,
        enc: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        scale: f32,
    ) {
        gpu.queue.write_buffer(
            &self.params[3],
            0,
            bytemuck::bytes_of(&Params {
                dir: [0.0; 2],
                scale,
                threshold: THRESHOLD,
            }),
        );
        let passes: [(&wgpu::RenderPipeline, &wgpu::BindGroup, &wgpu::TextureView); 4] = [
            (&self.bright, &self.groups[0], &self.quarter[0]),
            (&self.blur, &self.groups[1], &self.quarter[1]),
            (&self.blur, &self.groups[2], &self.quarter[0]),
            (&self.composite, &self.groups[3], target),
        ];
        for (i, (pipeline, group, view)) in passes.into_iter().enumerate() {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
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
            if i == 3 {
                pass.set_bind_group(1, &self.bloom_group, &[]);
            }
            pass.draw(0..3, 0..1);
        }
    }
}
