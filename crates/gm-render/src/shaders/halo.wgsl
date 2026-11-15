// halo.Add: the outlined models are drawn into a mask, the mask is blurred separably and
// added back outside the models' own pixels in the halo colour.

struct Params {
    // blur step in uv (one pass horizontal, one vertical)
    dir: vec2<f32>,
    alpha: f32,
    pad: f32,
    color: vec4<f32>,
};

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;
@group(0) @binding(2) var<uniform> params: Params;
@group(1) @binding(0) var mask: texture_2d<f32>;
@group(1) @binding(1) var mask_sampler: sampler;

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

@fragment
fn fs_blur(in: VsOut) -> @location(0) vec4<f32> {
    let w = array<f32, 5>(0.2270270, 0.1945946, 0.1216216, 0.0540541, 0.0162162);
    var acc = textureSample(src, src_sampler, in.uv).a * w[0];
    for (var i = 1; i < 5; i++) {
        let o = params.dir * f32(i);
        acc += textureSample(src, src_sampler, in.uv + o).a * w[i];
        acc += textureSample(src, src_sampler, in.uv - o).a * w[i];
    }
    return vec4<f32>(0.0, 0.0, 0.0, acc);
}

@fragment
fn fs_composite(in: VsOut) -> @location(0) vec4<f32> {
    let glow = textureSample(src, src_sampler, in.uv).a;
    let inside = textureSample(mask, mask_sampler, in.uv).a;
    let k = clamp(glow * (1.0 - inside) * params.alpha, 0.0, 1.0);
    return vec4<f32>(params.color.rgb * k, 0.0);
}
