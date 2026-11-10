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
    // the reflection pass clips everything behind this plane (normal, distance)
    clip_plane: vec4<f32>,
    dlight_pos: array<vec4<f32>, 32>,
    dlight_color: array<vec4<f32>, 32>,
    dlight_dir: array<vec4<f32>, 32>,
    sun: vec4<f32>,
    // env_skypaint: top colour + fade bias, bottom colour + HDR scale, sun normal + size,
    // sun colour + star drift, dusk colour + intensity, dusk scale + star scale / fade / layers
    sky_top: vec4<f32>,
    sky_bottom: vec4<f32>,
    sky_sun: vec4<f32>,
    sky_sun_color: vec4<f32>,
    sky_dusk: vec4<f32>,
    sky_dusk_scale: vec4<f32>,
    flashlight_vp: array<mat4x4<f32>, 5>,
    shadow_slot: array<vec4<f32>, 2>,
    // entity shadows: on, r_shadowdist as a fraction of the map's depth range, opacity
    sun_shadow: vec4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var lightmap: texture_2d<f32>;
@group(0) @binding(2) var lightmap_sampler: sampler;
@group(0) @binding(3) var cubes: texture_cube_array<f32>;
@group(0) @binding(4) var flashlight_depth: texture_depth_2d_array;
@group(0) @binding(5) var flashlight_sampler: sampler_comparison;
// effects/flashlight001: the flashlight is a projected texture (render.md 3.1 / 3.4)
@group(0) @binding(6) var flashlight_tex: texture_2d<f32>;
@group(0) @binding(7) var flashlight_tex_sampler: sampler;

// the projected texture seen from the light: 0 outside its frustum
fn flashlight_mask(slot: i32, world: vec3<f32>) -> f32 {
    let p = camera.flashlight_vp[slot] * vec4<f32>(world, 1.0);
    if p.w <= 0.0 {
        return 0.0;
    }
    let ndc = p.xyz / p.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 {
        return 0.0;
    }
    let c = textureSampleLevel(flashlight_tex, flashlight_tex_sampler, uv, 0.0).rgb;
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

// the flashlight's shadow: 1 lit, 0 in the shadow of the world seen from the light
fn flashlight_shadow(slot: i32, world: vec3<f32>) -> f32 {
    let p = camera.flashlight_vp[slot] * vec4<f32>(world, 1.0);
    if p.w <= 0.0 {
        return 1.0;
    }
    let ndc = p.xyz / p.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || ndc.z >= 1.0 {
        return 1.0;
    }
    // shadowed texels keep 0.35 of the light (render.md 3.4, gap-report #138)
    let lit = textureSampleCompareLevel(flashlight_depth, flashlight_sampler, uv, slot, ndc.z - 0.0005);
    return 0.35 + 0.65 * lit;
}
// The projected flashlight's falloff (render.md 3.4, gap-report #136 / #140): Source's
// 1 / (constant + linear * d + quadratic * d^2) with 0 / 100 / 0, i.e. 1 / d normalised to
// full at 100 u, cut off at the light's far plane (its radius)
fn flashlight_falloff(d: f32, far: f32) -> f32 {
    if d >= far {
        return 0.0;
    }
    let k = 100.0 / max(d, 100.0);
    // fade into the far plane over its last quarter so the cutoff has no hard edge
    return k * clamp((far - d) / (far * 0.25), 0.0, 1.0);
}
// Entity shadows (render.md 11): the dynamic models' depth from the light_environment
// direction in the last layer; a world point is shaded when a caster lies between it and
// the light within r_shadowdist, fading over the second half of that distance (the two
// comparisons at half and full distance stand in for the blob's linear fade)
fn sun_shadow_factor(world: vec3<f32>) -> f32 {
    if camera.sun_shadow.x < 0.5 {
        return 1.0;
    }
    let p = camera.flashlight_vp[4] * vec4<f32>(world, 1.0);
    let ndc = p.xyz / p.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || ndc.z >= 1.0 || ndc.z <= 0.0 {
        return 1.0;
    }
    let d = camera.sun_shadow.y;
    let lit = textureSampleCompareLevel(flashlight_depth, flashlight_sampler, uv, 4, ndc.z - 0.001);
    let lit_half = textureSampleCompareLevel(flashlight_depth, flashlight_sampler, uv, 4, ndc.z - d * 0.5);
    let lit_full = textureSampleCompareLevel(flashlight_depth, flashlight_sampler, uv, 4, ndc.z - d);
    let shadow = 0.5 * (max(lit_half - lit, 0.0) + max(lit_full - lit, 0.0));
    return 1.0 - camera.sun_shadow.z * shadow;
}
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
    envmap3: vec4<f32>,
    selfillum_tint: vec4<f32>,
    phong: vec4<f32>,
    phong_fresnel: vec4<f32>,
};
@group(1) @binding(2) var<uniform> material: MaterialParams;
@group(1) @binding(3) var detail: texture_2d<f32>;
@group(1) @binding(4) var base2: texture_2d<f32>;
@group(1) @binding(5) var blend_modulate: texture_2d<f32>;
@group(1) @binding(6) var bump: texture_2d<f32>;
@group(1) @binding(7) var envmask: texture_2d<f32>;
@group(1) @binding(8) var phongexp: texture_2d<f32>;
// water pass only: the depth of the opaque scene (multisampled variant substituted at load)
@group(2) @binding(0) var scene_depth: texture_depth_2d;
@group(2) @binding(1) var reflection: texture_2d<f32>;
@group(2) @binding(2) var reflection_sampler: sampler;
@group(2) @binding(3) var refraction: texture_2d<f32>;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) lm_uv: vec2<f32>,
    @location(3) color: vec3<f32>,
    @location(4) alpha: f32,
    @location(5) lm_page: f32,
    @location(6) normal: vec3<f32>,
    @location(7) cube: f32,
    @location(8) spec_dir: vec3<f32>,
    @location(9) spec_color: vec3<f32>,
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
    @location(8) spec_dir: vec3<f32>,
    @location(9) spec_color: vec3<f32>,
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
    out.spec_dir = in.spec_dir;
    out.spec_color = in.spec_color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // the water reflection pass sees nothing below the water plane
    if camera.clip.w > 0.5 && dot(in.world, camera.clip_plane.xyz) < camera.clip_plane.w {
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
    // $ssbump: the texel is the three weights, pre-shadowed by the tool, summed unnormalised
    if material.uv2_t.w > 1.5 {
        bw = textureSample(bump, base_sampler, in.uv).rgb;
    }
    var bumped = (l1 * bw.x + l2 * bw.y + l3 * bw.z) / max(bw.x + bw.y + bw.z, 1e-4);
    if material.uv2_t.w > 1.5 {
        bumped = l1 * bw.x + l2 * bw.y + l3 * bw.z;
    }
    if material.uv2_t.w > 0.5 && in.lm_page > 0.0 {
        light = bumped;
    }
    light = light * camera.lm_scale;
    // dynamic lights: colour * (1 - d / r)^2 * facing (unlit surfaces skip them)
    for (var i = 0u; i < 32u; i++) {
        let lp = camera.dlight_pos[i];
        if lp.w > 0.0 {
            let dv = lp.xyz - in.world;
            let d = length(dv);
            var k = clamp(1.0 - d / lp.w, 0.0, 1.0);
            let sd = camera.dlight_dir[i];
            let slot = i32(camera.shadow_slot[i / 4u][i % 4u]);
            if slot >= 0 {
                // a flashlight: the projected texture gives the cone and its edge, the
                // projected-light law its falloff (#136)
                k = flashlight_falloff(d, lp.w) * flashlight_mask(slot, in.world) * flashlight_shadow(slot, in.world);
            } else if sd.w > 0.0 {
                // spot: full inside the cone, soft 0.08 (cosine) edge
                let c = dot(-dv / max(d, 1e-3), sd.xyz);
                k *= smoothstep(sd.w, min(sd.w + 0.08, 1.0), c);
            }
            // R_AddDynamicLights: world surfaces take a dynamic light by distance alone,
            // only the side of the plane matters (a light sitting on the wall lights it fully)
            var facing = 1.0;
            if dot(in.normal, in.normal) > 0.0 && dot(in.normal, dv) < -1.0 {
                facing = 0.0;
            }
            light += camera.dlight_color[i].rgb * (k * k * facing);
        }
    }
    // the entity shadow decals darken the lit world surface (models carry w = 1: none)
    if material.water_range.w < 0.5 {
        light = light * sun_shadow_factor(in.world);
    }
    if material.unlit != 0u {
        light = vec3<f32>(1.0);
    }
    // $detail mod2x: base * lerp(1, 2 * detail, $detailblendfactor) at base uv * $detailscale
    let d = textureSample(detail, base_sampler, in.uv * material.detail.x).rgb;
    let base = albedo.rgb * mix(vec3<f32>(1.0), d * 2.0, material.detail.y);
    var shaded = base * light * in.color * camera.exposure;
    // DecalModulate (mode 3): the raw texel is the blend factor (colour * 2 * texel), its
    // 128 grey neutral - no exposure, no light, no tint, else every bullet hole brightens
    // its square whenever the auto-exposure is above 1
    if material.unlit == 3u {
        shaded = albedo.rgb;
    }
    // $envmap: the nearest env_cubemap along the reflected view ray, times $envmaptint and
    // the mask (1 - base alpha for $basealphaenvmapmask, normal alpha), added after lighting
    let bump_a = textureSample(bump, base_sampler, in.uv).a;
    let phong_tex = textureSample(phongexp, base_sampler, in.uv);
    let mask_tex = textureSample(envmask, base_sampler, in.uv * material.selfillum_tint.w).rgb;
    // $phongfresnelranges over (1 - n.v)^5: the highlight's and, on $phong materials, the
    // reflection's fresnel (the skin shader scales the envmap by it and the spec mask)
    var phong_fres = 1.0;
    if material.phong.x > 0.5 && dot(in.normal, in.normal) > 0.0 {
        let nvp = clamp(dot(normalize(in.normal), normalize(camera.eye.xyz - in.world)), 0.0, 1.0);
        let fp = pow(1.0 - nvp, 5.0);
        phong_fres = mix(material.phong_fresnel.x, material.phong_fresnel.y, fp * 2.0);
        if fp > 0.5 {
            phong_fres = mix(material.phong_fresnel.y, material.phong_fresnel.z, fp * 2.0 - 1.0);
        }
    }
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
        // $envmapfresnel: min + (max - min) * (1 - n.v)^exp
        if material.envmap3.w > 0.5 {
            let nv = clamp(dot(normalize(camera.eye.xyz - in.world), normalize(in.normal)), 0.0, 1.0);
            env = env * (material.envmap3.x + (material.envmap3.y - material.envmap3.x) * pow(1.0 - nv, material.envmap3.z));
        }
        if material.phong.x > 0.5 {
            env = env * (phong_fres * bump_a);
        }
        shaded = shaded + env * material.envmap.rgb * mask * camera.exposure;
    }
    // $phong: the Blinn-Phong highlight of the sun through the baked light, $phongexponent /
    // $phongboost, $phongfresnelranges over (1 - n.v)^5, masked by the normal map alpha;
    // a $phongexponenttexture gives the exponent (red, 1..150) and the albedo tint mask
    // (green); the dynamic lights add their own highlight with the diffuse loop's falloff
    if material.phong.x > 0.5 && material.unlit == 0u && dot(in.normal, in.normal) > 0.0 {
        let nw = normalize(in.normal);
        let v = normalize(camera.eye.xyz - in.world);
        // models take the highlight from the brightest light at their origin (the map's
        // world lights or the sun through the sky), the rest from the sun
        var l = normalize(camera.sun.xyz);
        var lc = in.color;
        if dot(in.spec_dir, in.spec_dir) > 0.0 {
            l = normalize(in.spec_dir);
            lc = in.spec_color;
        }
        let ndl = max(dot(nw, l), 0.0);
        var exponent = material.phong.y;
        var tint_mask = 1.0;
        if material.phong.x > 1.5 {
            exponent = phong_tex.r * 149.0 + 1.0;
            tint_mask = phong_tex.g;
        }
        let spec = pow(max(dot(nw, normalize(l + v)), 0.0), exponent) * material.phong.z;
        let tint = mix(vec3<f32>(1.0), albedo.rgb, material.phong.w * tint_mask);
        shaded = shaded + lc * (spec * phong_fres * ndl * bump_a) * tint * camera.exposure;
        var local = vec3<f32>(0.0);
        for (var i = 0u; i < 32u; i++) {
            let lp = camera.dlight_pos[i];
            if lp.w > 0.0 {
                let dv = lp.xyz - in.world;
                let d = length(dv);
                let ld = dv / max(d, 1e-3);
                var k = clamp(1.0 - d / lp.w, 0.0, 1.0);
                let sd = camera.dlight_dir[i];
                let slot = i32(camera.shadow_slot[i / 4u][i % 4u]);
                if slot >= 0 {
                    // r_flashlightmodels: the same projected law on models (#140)
                    k = flashlight_falloff(d, lp.w) * flashlight_mask(slot, in.world) * flashlight_shadow(slot, in.world);
                } else if sd.w > 0.0 {
                    k *= smoothstep(sd.w, min(sd.w + 0.08, 1.0), dot(-ld, sd.xyz));
                }
                let h = pow(max(dot(nw, normalize(ld + v)), 0.0), exponent);
                local += camera.dlight_color[i].rgb * (k * k * max(dot(nw, ld), 0.0) * h);
            }
        }
        shaded = shaded + local * (material.phong.z * phong_fres * bump_a) * tint * camera.exposure;
    }
    // $selfillum: the base alpha lerps towards the unlit base colour
    shaded = mix(shaded, albedo.rgb * material.selfillum_tint.rgb * camera.exposure, albedo.a * material.envmap2.z);
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
    // sprite particles: the vertex alpha is their opacity when the material has no $basetexture2
    if material.blend == 0u && in.alpha > 0.0 {
        alpha = alpha * in.alpha;
    }
    // additive sprites fog to black (they fade out), DecalModulate to its neutral 0.5 grey
    var fog_to = camera.fog_color.rgb;
    if material.unlit == 2u {
        fog_to = vec3<f32>(0.0);
    } else if material.unlit == 3u {
        fog_to = vec3<f32>(0.5);
    }
    return vec4<f32>(mix(out, fog_to, fog), alpha);
}

// Water surface: the volume fog of the material ($fogcolor / $fogstart / $fogend) over the
// water thickness along the view ray, i.e. between the surface and the opaque scene behind
// it, blended over the already-drawn bottom; the view fog applies on top.
@fragment
fn fs_water(in: VsOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
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
    let flags = u32(camera.clip.y + 0.5);
    let has_refl = f32((flags & 1u) != 0u);
    let has_refr = f32((flags & 2u) != 0u);
    let suv = in.clip.xy / dims + n.xy * material.envmap.x;
    // a vertical mirror's camera gives the reflection mirrored left-right instead
    let flip_x = (flags & 4u) != 0u;
    let refl_uv = select(vec2<f32>(suv.x, 1.0 - suv.y), vec2<f32>(1.0 - suv.x, suv.y), flip_x);
    let refl = textureSample(reflection, reflection_sampler, refl_uv).rgb;
    // LightmappedReflective (func_reflective_glass): the reflection itself
    if material.water_range.z > 0.5 {
        return vec4<f32>(refl, 1.0);
    }
    // _rt_WaterRefraction: the scene behind the surface offset by $refractamount, under the
    // water fog; without the copy the already-drawn bottom shows through the fog alpha
    let ruv = in.clip.xy / dims + n.xy * material.envmap.y;
    let refr = textureSample(refraction, reflection_sampler, ruv).rgb;
    // seen from under the surface ($bottommaterial): the scene above through the refraction,
    // no reflection, the eye's water fog up to the surface
    if !front {
        let above = mix(material.water_color.rgb, refr, has_refr);
        return vec4<f32>(mix(above, camera.fog_color.rgb, fog), 1.0);
    }
    let nv = clamp(dot(-ray / surf_len, n), 0.0, 1.0);
    let fr = pow(1.0 - nv, 5.0) * has_refl;
    let under = mix(material.water_color.rgb, mix(refr, material.water_color.rgb, f), has_refr);
    let col = mix(mix(under, refl, fr), camera.fog_color.rgb, fog);
    let alpha = mix(f + (1.0 - f) * fr, 1.0, has_refr);
    return vec4<f32>(col, alpha);
}

// Refract (sprites/heatwave): the scene copy offset by the normal map by $refractamount
// (material.envmap.y), scaled by the beam's vertex alpha (its colour fades to nothing)
@fragment
fn fs_refract(in: VsOut) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(scene_depth));
    let nm = textureSample(base, base_sampler, in.uv).xyz * 2.0 - 1.0;
    let a = max(in.color.r, max(in.color.g, in.color.b));
    let ruv = in.clip.xy / dims + nm.xy * material.envmap.y * a;
    return vec4<f32>(textureSample(refraction, reflection_sampler, ruv).rgb, 1.0);
}

// env_skypaint (an edit_sky entity): an approximation of the game's Sky_Painted shader - the
// bottom colour fades to the top one by pow(up, fade bias), a dusk band sits along the
// horizon (wider with the dusk scale), a glow surrounds the sun normal (wider with the sun
// size); the stars and the HDR scale are not drawn
@fragment
fn fs_skypaint(in: VsOut) -> @location(0) vec4<f32> {
    let dir = normalize(in.world - camera.eye.xyz);
    let up = clamp(dir.z, 0.0, 1.0);
    let t = pow(up, max(camera.sky_top.w, 0.01));
    var col = mix(camera.sky_bottom.rgb, camera.sky_top.rgb, t);
    let band = pow(1.0 - clamp(abs(dir.z), 0.0, 1.0), 4.0 / max(camera.sky_dusk_scale.x, 0.01));
    col = col + camera.sky_dusk.rgb * camera.sky_dusk.w * band;
    var sn = vec3<f32>(0.0, 0.0, 1.0);
    if dot(camera.sky_sun.xyz, camera.sky_sun.xyz) > 1e-6 {
        sn = normalize(camera.sky_sun.xyz);
    }
    let s = clamp(dot(dir, sn), 0.0, 1.0);
    let glow = pow(s, 512.0 / max(camera.sky_sun.w, 0.01));
    col = col + camera.sky_sun_color.rgb * glow;
    // stars: the bound starfield tiled over the upper hemisphere (scale, fading towards the
    // horizon by the star fade), each layer larger and drifting a little faster
    let layers = i32(camera.sky_dusk_scale.w + 0.5);
    if layers > 0 && dir.z > 0.0 {
        let fade = pow(up, max(camera.sky_dusk_scale.z, 0.01));
        var stars = vec3<f32>(0.0);
        for (var i = 0; i < layers; i++) {
            let k = camera.sky_dusk_scale.y * (1.0 + f32(i) * 0.5);
            let drift = vec2<f32>(camera.sky_sun_color.w * (1.0 + f32(i) * 0.3), 0.0);
            let uv = dir.xy / max(dir.z, 0.1) * k + drift;
            stars = stars + textureSampleLevel(base, base_sampler, uv, 0.0).rgb;
        }
        col = col + stars * fade;
    }
    // env_skypaint `hdrscale`: the painted sky is scaled down in HDR (1 in LDR)
    let out = pow(max(col * camera.sky_bottom.w * camera.exposure, vec3<f32>(0.0)), vec3<f32>(2.2 / camera.gamma));
    return vec4<f32>(out, camera.alpha);
}
