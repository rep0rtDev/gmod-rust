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
    clip: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var lightmap: texture_2d<f32>;
@group(0) @binding(2) var lightmap_sampler: sampler;
@group(0) @binding(3) var cubes: texture_cube_array<f32>;
@group(1) @binding(0) var base: texture_2d<f32>;
@group(1) @binding(1) var base_sampler: sampler;

struct MaterialParams {
    alpha_test: u32,
    unlit: u32,
    translucent: u32,
    blend: u32,
    detail: vec4<f32>,
    uv2_m: vec4<f32>,
    uv2_t: vec4<f32>,
    water_color: vec4<f32>,
    water_range: vec4<f32>,
    envmap: vec4<f32>,
    envmap2: vec4<f32>,
};
@group(1) @binding(2) var<uniform> material: MaterialParams;
@group(1) @binding(3) var detail: texture_2d<f32>;
@group(1) @binding(4) var base2: texture_2d<f32>;
@group(1) @binding(5) var blend_modulate: texture_2d<f32>;
@group(1) @binding(6) var bump: texture_2d<f32>;
@group(1) @binding(7) var envmask: texture_2d<f32>;
// water pass only: the depth of the opaque scene (multisampled variant substituted at load)
@group(2) @binding(0) var scene_depth: texture_depth_2d;
@group(2) @binding(1) var reflection: texture_2d<f32>;
@group(2) @binding(2) var reflection_sampler: sampler;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) lm_uv: vec2<f32>,
    @location(3) color: vec3<f32>,
    @location(4) alpha: f32,
    @location(5) lm_page: f32,
    @location(6) normal: vec3<f32>,
    @location(7) cube: f32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) lm_uv: vec2<f32>,
    @location(2) color: vec3<f32>,
    @location(3) world: vec3<f32>,
    @location(4) alpha: f32,
    @location(5) lm_page: f32,
    @location(6) normal: vec3<f32>,
    @location(7) cube: f32,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.uv = in.uv;
    out.lm_uv = in.lm_uv;
    out.color = in.color;
    out.world = in.position;
    out.alpha = in.alpha;
    out.lm_page = in.lm_page;
    out.normal = in.normal;
    out.cube = in.cube;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // the water reflection pass sees nothing below the water plane
    if camera.clip.w > 0.5 && in.world.z < camera.clip.x {
        discard;
    }
    // WorldVertexTransition: lerp($basetexture, $basetexture2, displacement vertex alpha)
    let base1 = textureSample(base, base_sampler, in.uv);
    let uv2 = vec2<f32>(
        material.uv2_m.x * in.uv.x + material.uv2_m.y * in.uv.y + material.uv2_t.x,
        material.uv2_m.z * in.uv.x + material.uv2_m.w * in.uv.y + material.uv2_t.y,
    );
    let base2_s = textureSample(base2, base_sampler, uv2);
    // $blendmodulatetexture: r = half-width, g = centre of the alpha band that transitions
    let modt = textureSample(blend_modulate, base_sampler, in.uv).rg;
    let minb = max(0.0, modt.g - modt.r);
    let maxb = min(1.0, modt.g + modt.r);
    var factor = in.alpha;
    if material.uv2_t.z > 0.5 {
        factor = smoothstep(minb, maxb, in.alpha);
    }
    let albedo = mix(base1, base2_s, factor * f32(material.blend));
    if material.alpha_test != 0u && albedo.a < 0.5 {
        discard;
    }
    // lightmap atlas: linear light (Rgba16Float), lm_scale 1
    var light = textureSample(lightmap, lightmap_sampler, in.lm_uv).rgb;
    // $bumpmap: the three bumped lightmap pages weighted by the squared dot of the
    // tangent-space normal with the bump basis (both live in the face's texture space)
    let nrm = normalize(textureSample(bump, base_sampler, in.uv).xyz * 2.0 - 1.0);
    let l1 = textureSample(lightmap, lightmap_sampler, in.lm_uv + vec2<f32>(in.lm_page, 0.0)).rgb;
    let l2 = textureSample(lightmap, lightmap_sampler, in.lm_uv + vec2<f32>(2.0 * in.lm_page, 0.0)).rgb;
    let l3 = textureSample(lightmap, lightmap_sampler, in.lm_uv + vec2<f32>(3.0 * in.lm_page, 0.0)).rgb;
    var bw = vec3<f32>(
        max(dot(nrm, vec3<f32>(0.81649658, 0.0, 0.57735027)), 0.0),
        max(dot(nrm, vec3<f32>(-0.40824829, 0.70710678, 0.57735027)), 0.0),
        max(dot(nrm, vec3<f32>(-0.40824829, -0.70710678, 0.57735027)), 0.0),
    );
    bw = bw * bw;
    let bumped = (l1 * bw.x + l2 * bw.y + l3 * bw.z) / max(bw.x + bw.y + bw.z, 1e-4);
    if material.uv2_t.w > 0.5 && in.lm_page > 0.0 {
        light = bumped;
    }
    light = light * camera.lm_scale;
    if material.unlit != 0u {
        light = vec3<f32>(1.0);
    }
    // $detail mod2x: base * lerp(1, 2 * detail, $detailblendfactor) at base uv * $detailscale
    let d = textureSample(detail, base_sampler, in.uv * material.detail.x).rgb;
    let base = albedo.rgb * mix(vec3<f32>(1.0), d * 2.0, material.detail.y);
    var shaded = base * light * in.color * camera.exposure;
    // $envmap: the nearest env_cubemap along the reflected view ray, times $envmaptint and
    // the mask (1 - base alpha for $basealphaenvmapmask, normal alpha), added after lighting
    let bump_a = textureSample(bump, base_sampler, in.uv).a;
    let mask_tex = textureSample(envmask, base_sampler, in.uv).rgb;
    if material.envmap.w > 0.5 && in.cube >= 0.0 && dot(in.normal, in.normal) > 0.0 {
        let r = reflect(normalize(in.world - camera.eye.xyz), normalize(in.normal));
        var mask = vec3<f32>(1.0);
        if material.envmap.w > 3.5 {
            mask = mask_tex;
        } else if material.envmap.w > 2.5 {
            mask = vec3<f32>(bump_a);
        } else if material.envmap.w > 1.5 {
            mask = vec3<f32>(1.0 - albedo.a);
        }
        var env = textureSampleLevel(cubes, lightmap_sampler, r, i32(in.cube + 0.5), 0.0).rgb;
        // $envmapcontrast squares, $envmapsaturation desaturates towards the luminance
        env = mix(env, env * env, material.envmap2.x);
        env = mix(vec3<f32>(dot(env, vec3<f32>(0.299, 0.587, 0.114))), env, material.envmap2.y);
        shaded = shaded + env * material.envmap.rgb * mask * camera.exposure;
    }
    // $selfillum: the base alpha lerps towards the unlit base colour
    shaded = mix(shaded, albedo.rgb * camera.exposure, albedo.a * material.envmap2.z);
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

// Water surface: the volume fog of the material ($fogcolor / $fogstart / $fogend) over the
// water thickness along the view ray, i.e. between the surface and the opaque scene behind
// it, blended over the already-drawn bottom; the view fog applies on top.
@fragment
fn fs_water(in: VsOut) -> @location(0) vec4<f32> {
    let ndc = textureLoad(scene_depth, vec2<i32>(in.clip.xy), 0);
    let near = camera.fog_range.z;
    let far = camera.fog_range.w;
    let scene_z = far * near / max(far - ndc * (far - near), 1e-3);
    let ray = in.world - camera.eye.xyz;
    let surf_len = max(length(ray), 1e-3);
    let cosang = max(dot(ray / surf_len, camera.forward.xyz), 1e-3);
    let thickness = max(scene_z / cosang - surf_len, 0.0);
    let span = max(material.water_range.y - material.water_range.x, 1.0);
    let f = clamp((thickness - material.water_range.x) / span, 0.0, 1.0);
    let depth = dot(ray, camera.forward.xyz);
    let fspan = max(camera.fog_range.y - camera.fog_range.x, 1.0);
    let fog = min(clamp((depth - camera.fog_range.x) / fspan, 0.0, 1.0), camera.fog_color.w) * camera.eye.w;
    // _rt_WaterReflection, drawn by the mirrored camera and read upside down, over the
    // water fog by the Schlick fresnel of the flat surface
    // the $normalmap scrolled by $scroll1 perturbs the reflection lookup by $reflectamount
    // and gives the per-pixel fresnel (the Water shader's expensive path)
    let nuv = in.uv + material.envmap.zw * camera.clip.z;
    let n = normalize(textureSample(bump, base_sampler, nuv).xyz * 2.0 - 1.0);
    let dims = vec2<f32>(textureDimensions(scene_depth));
    let suv = in.clip.xy / dims + n.xy * material.envmap.x;
    let refl = textureSample(reflection, reflection_sampler, vec2<f32>(suv.x, 1.0 - suv.y)).rgb;
    let nv = clamp(dot(-ray / surf_len, n), 0.0, 1.0);
    let fr = pow(1.0 - nv, 5.0) * camera.clip.y;
    let col = mix(mix(material.water_color.rgb, refl, fr), camera.fog_color.rgb, fog);
    return vec4<f32>(col, f + (1.0 - f) * fr);
}
