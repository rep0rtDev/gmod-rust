struct Camera {
    view_proj: mat4x4<f32>,
    alpha: f32,
    gamma: f32,
    lm_scale: f32,
    exposure: f32,
    eye: vec4<f32>,
    forward: vec4<f32>,
    fog_color: vec4<f32>,
    fog_range: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var lightmap: texture_2d<f32>;
@group(0) @binding(2) var lightmap_sampler: sampler;
@group(1) @binding(0) var base: texture_2d<f32>;
@group(1) @binding(1) var base_sampler: sampler;

struct MaterialParams {
    alpha_test: u32,
    unlit: u32,
    translucent: u32,
    pad: u32,
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
    @location(3) world: vec3<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.uv = in.uv;
    out.lm_uv = in.lm_uv;
    out.color = in.color;
    out.world = in.position;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let albedo = textureSample(base, base_sampler, in.uv);
    if material.alpha_test != 0u && albedo.a < 0.5 {
        discard;
    }
    // lightmap atlas: linear light (Rgba16Float), lm_scale 1
    var light = textureSample(lightmap, lightmap_sampler, in.lm_uv).rgb * camera.lm_scale;
    if material.unlit != 0u {
        light = vec3<f32>(1.0);
    }
    let shaded = albedo.rgb * light * in.color * camera.exposure;
    // mat_monitorgamma: 2.2 is neutral, lower brightens
    let out = pow(max(shaded, vec3<f32>(0.0)), vec3<f32>(2.2 / camera.gamma));
    // env_fog_controller: linear in the view depth from fogstart to fogend, capped at
    // fogmaxdensity, the colour given in display space
    let depth = dot(in.world - camera.eye.xyz, camera.forward.xyz);
    let span = max(camera.fog_range.y - camera.fog_range.x, 1.0);
    let fog = min(clamp((depth - camera.fog_range.x) / span, 0.0, 1.0), camera.fog_color.w) * camera.eye.w;
    var alpha = camera.alpha;
    if material.translucent != 0u {
        alpha = albedo.a * camera.alpha;
    }
    return vec4<f32>(mix(out, camera.fog_color.rgb, fog), alpha);
}
