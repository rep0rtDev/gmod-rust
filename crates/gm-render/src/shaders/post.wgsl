// Bloom post-process: bright pass at quarter resolution, separable blur, composite.

struct Params {
    // blur direction in texels (bright pass / composite ignore it)
    dir: vec2<f32>,
    // bloom strength added in the composite
    scale: f32,
    // bright-pass threshold (linear)
    threshold: f32,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;
@group(0) @binding(2) var<uniform> params: Params;
@group(1) @binding(0) var bloom: texture_2d<f32>;
@group(1) @binding(1) var bloom_sampler: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    var out: VsOut;
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    out.pos = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x, -y) * 0.5 + 0.5;
    return out;
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

@fragment
fn fs_bright(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(src, src_sampler, in.uv).rgb;
    let l = luminance(c);
    let k = max(l - params.threshold, 0.0) / max(l, 1e-4);
    return vec4<f32>(c * k, 1.0);
}

@fragment
fn fs_blur(in: VsOut) -> @location(0) vec4<f32> {
    let w = array<f32, 5>(0.2270270, 0.1945946, 0.1216216, 0.0540541, 0.0162162);
    var acc = textureSample(src, src_sampler, in.uv).rgb * w[0];
    for (var i = 1; i < 5; i++) {
        let o = params.dir * f32(i);
        acc += textureSample(src, src_sampler, in.uv + o).rgb * w[i];
        acc += textureSample(src, src_sampler, in.uv - o).rgb * w[i];
    }
    return vec4<f32>(acc, 1.0);
}

@fragment
fn fs_composite(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(src, src_sampler, in.uv).rgb;
    let b = textureSample(bloom, bloom_sampler, in.uv).rgb;
    return vec4<f32>(c + b * params.scale, 1.0);
}
