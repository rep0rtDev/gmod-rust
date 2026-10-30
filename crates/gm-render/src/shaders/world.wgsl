struct Camera {
    view_proj: mat4x4<f32>,
    alpha: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var lightmap: texture_2d<f32>;
@group(0) @binding(2) var lightmap_sampler: sampler;
@group(1) @binding(0) var base: texture_2d<f32>;
@group(1) @binding(1) var base_sampler: sampler;

struct MaterialParams {
    alpha_test: u32,
    unlit: u32,
    pad1: u32,
    pad2: u32,
};
@group(1) @binding(2) var<uniform> material: MaterialParams;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) lm_uv: vec2<f32>,
    @location(3) color: vec3<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) lm_uv: vec2<f32>,
    @location(2) color: vec3<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.uv = in.uv;
    out.lm_uv = in.lm_uv;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let albedo = textureSample(base, base_sampler, in.uv);
    if material.alpha_test != 0u && albedo.a < 0.5 {
        discard;
    }
    // lightmap atlas stores linear light / 2 so the Source overbright range fits in 8 bits
    var light = textureSample(lightmap, lightmap_sampler, in.lm_uv).rgb * 2.0;
    if material.unlit != 0u {
        light = vec3<f32>(1.0);
    }
    return vec4<f32>(albedo.rgb * light * in.color, camera.alpha);
}
