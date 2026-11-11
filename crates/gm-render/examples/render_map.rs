//! Render a still of a map from the player spawn (or a given camera) to PNG.
//!
//! GMOD_DIR=... cargo run -p gm-render --example render_map -- gm_construct out.png [x y z pitch yaw]

use anyhow::{anyhow, Result};
use glam::Vec3;
use gm_render::world::offscreen_targets;
use gm_render::{Camera, Gpu, WorldRenderer};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let map = args.first().map(String::as_str).unwrap_or("gm_construct");
    let out = args.get(1).map(String::as_str).unwrap_or("render.png");
    let (width, height) = (1280u32, 720u32);

    let install = gm_content::find_install()
        .ok_or_else(|| anyhow!("GMod install not found; set GMOD_DIR"))?;
    let mut mount = gm_content::Mount::for_install(&install)?;
    let data = mount.read(&format!("maps/{map}.bsp"))?;
    if let Ok(pak) = gm_formats::bsp::pakfile(&data) {
        mount.add_pak(&format!("maps/{map}.bsp pak"), pak)?;
    }
    let bsp = gm_formats::bsp::Bsp::parse(&data)?;

    let camera = if args.len() >= 7 {
        let f = |i: usize| args[i].parse::<f32>().unwrap_or(0.0);
        Camera {
            position: Vec3::new(f(2), f(3), f(4)),
            angles: Vec3::new(f(5), f(6), 0.0),
            fov_x: 90.0,
            near: 7.0,
            far: 28000.0,
            gamma: 2.2,
        }
    } else {
        let spawn = bsp
            .entities
            .iter()
            .find(|e| e.classname() == "info_player_start")
            .ok_or_else(|| anyhow!("no info_player_start"))?;
        let o = spawn.origin().unwrap_or([0.0; 3]);
        let a = spawn.angles().unwrap_or([0.0; 3]);
        Camera {
            position: Vec3::new(o[0], o[1], o[2] + 64.0),
            angles: Vec3::new(a[0], a[1], 0.0),
            fov_x: 90.0,
            near: 7.0,
            far: 28000.0,
            gamma: 2.2,
        }
    };

    let t0 = std::time::Instant::now();
    let gpu = Gpu::headless()?;
    eprintln!("adapter: {}", gpu.adapter_name());
    let world = WorldRenderer::new(
        &gpu,
        &mount,
        &bsp,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        // GM_HDR: the HDR lighting lumps and .hdr.vtf cubemaps (mat_hdr_level 2)
        // GM_MSAA: mat_antialias samples (the user's cards run 8; llvmpipe takes it too)
        gm_render::world::Quality {
            hdr: std::env::var_os("GM_HDR").is_some(),
            msaa: std::env::var("GM_MSAA")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1),
            ..Default::default()
        },
    )?;
    eprintln!(
        "world: {} faces ({} in 3D skybox), {} displacements, {} props ({} models), {} triangles, {} materials, {} missing, {} lightmap overflow, built in {:.1}s",
        world.stats.faces,
        world.stats.skybox_faces,
        world.stats.displacements,
        world.stats.props,
        world.stats.prop_models,
        world.stats.triangles,
        world.stats.materials,
        world.stats.missing_materials.len(),
        world.stats.lightmap_overflow,
        t0.elapsed().as_secs_f32()
    );
    for m in world.stats.missing_materials.iter().take(20) {
        eprintln!("  missing: {m}");
    }
    if std::env::var_os("GM_RENDER_LOG").is_some() {
        for m in world.materials() {
            eprintln!("  material: {m}");
        }
    }
    if std::env::var_os("GM_RENDER_LOG").is_some() {
        debug_center_ray(&bsp, &camera);
        for w in &world.water_volumes {
            eprintln!(
                "  water: surface z {} box {:?}..{:?}",
                w.surface_z, w.min, w.max
            );
        }
    }
    let msaa = std::env::var("GM_MSAA")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let (color, _) = offscreen_targets(&gpu, width, height, wgpu::TextureFormat::Rgba8UnormSrgb, 1);
    // with MSAA the passes draw into multisampled targets and resolve into `color`
    let (ms_color, depth) = offscreen_targets(
        &gpu,
        width,
        height,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        msaa,
    );
    let t1 = std::time::Instant::now();
    // GM_PROBE_MODEL=<path>: load a model like the client does and report its animation state
    if let Some(path) = std::env::var_os("GM_PROBE_MODEL") {
        let path = path.to_string_lossy().to_string();
        if let Ok(bytes) = mount.read(&path) {
            if let Ok(mdl) = gm_formats::mdl::Mdl::parse(&bytes) {
                for t in 0..mdl.textures.len() {
                    for mp in mdl.material_paths(t) {
                        eprintln!("PROBE material {mp}: exists {}", mount.exists(&mp));
                    }
                }
                for t in 0..mdl.textures.len() {
                    for mp in mdl.material_paths(t) {
                        if !mount.exists(&mp) {
                            continue;
                        }
                        match gm_render::material::load_path(&mount, &mp, &mp) {
                            Err(e) => eprintln!("PROBE material {mp}: load error {e}"),
                            Ok(m) => {
                                let tex = m.base_texture.clone();
                                let r = tex.as_deref().map(|t| {
                                    gm_render::material::load_texture(&mount, t, 512)
                                        .map(|r| format!("{}x{}", r.width, r.height))
                                        .map_err(|e| e.to_string())
                                });
                                eprintln!(
                                    "PROBE material {mp}: shader {} base {tex:?} -> {r:?}",
                                    m.shader
                                );
                            }
                        }
                    }
                }
                for inc in gm_formats::mdl::include_models(&bytes, &mdl.header) {
                    let seqs = mount
                        .read(&inc)
                        .ok()
                        .and_then(|b| gm_formats::mdl::Mdl::parse(&b).ok())
                        .map(|m| m.bones.len());
                    eprintln!(
                        "PROBE include {inc}: exists {} sequences {seqs:?}",
                        mount.exists(&inc)
                    );
                }
            }
        }
        match world.load_model(&gpu, &mount, &path) {
            None => eprintln!("PROBE {path}: load_model -> None"),
            Some(m) => {
                for seq in ["Idle1", "idle", "ragdoll", "Idle1_SMG1", "batonidle1"] {
                    let len = m.sequence_length(seq);
                    let bones = m.bone_world(seq, 0.0, None, &[]);
                    let summary = bones.as_ref().map(|b| {
                        let nan = b.iter().filter(|m| m.is_nan()).count();
                        let t: Vec<[f32; 3]> = b
                            .iter()
                            .take(3)
                            .map(|m| m.w_axis.truncate().to_array())
                            .collect();
                        format!("{} bones, {nan} nan, first {:?}", b.len(), t)
                    });
                    eprintln!("PROBE {path} seq {seq}: len {len:?} bones {summary:?}");
                }
            }
        }
    }
    let color_view = color.create_view(&Default::default());
    let ms_view = ms_color.create_view(&Default::default());
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    world.draw(
        &gpu,
        &mut enc,
        &camera,
        if msaa > 1 { &ms_view } else { &color_view },
        (msaa > 1).then_some(&color_view),
        &depth.create_view(&Default::default()),
        width as f32 / height as f32,
        // GM_NO_REFRACT: draw the water without the refraction copy (to compare)
        if std::env::var_os("GM_NO_REFRACT").is_some() {
            None
        } else {
            Some(&color)
        },
        &[],
    );
    gpu.queue.submit([enc.finish()]);
    let rgba = gpu.read_texture(&color, width, height)?;
    eprintln!("frame in {:.0} ms", t1.elapsed().as_secs_f32() * 1000.0);
    gm_render::gpu::write_png(std::path::Path::new(out), width, height, &rgba)?;
    eprintln!("wrote {out}");
    Ok(())
}

/// Print the face hit by the view ray, with its material and lightmap, to explain what the centre pixel shows.
fn debug_center_ray(bsp: &gm_formats::bsp::Bsp<'_>, camera: &Camera) {
    let o = camera.position;
    let d = camera.forward();
    let mut best: Option<(f32, usize)> = None;
    for (fi, face) in bsp.faces.iter().enumerate() {
        let verts = bsp.face_vertices(face);
        if verts.len() < 3 {
            continue;
        }
        let plane = &bsp.planes[face.plane_num as usize];
        let mut n = Vec3::from(plane.normal);
        let mut dist = plane.dist;
        if face.side != 0 {
            n = -n;
            dist = -dist;
        }
        let denom = n.dot(d);
        if denom.abs() < 1e-6 {
            continue;
        }
        let t = (dist - n.dot(o)) / denom;
        if t <= 0.0 || best.is_some_and(|(bt, _)| t >= bt) {
            continue;
        }
        let p = o + d * t;
        let mut inside = true;
        for i in 0..verts.len() {
            let a = Vec3::from(verts[i]);
            let b = Vec3::from(verts[(i + 1) % verts.len()]);
            if (b - a).cross(p - a).dot(n) < -1e-3 {
                inside = false;
                break;
            }
        }
        if inside {
            best = Some((t, fi));
        }
    }
    let Some((t, fi)) = best else {
        eprintln!("centre ray: nothing hit");
        return;
    };
    let face = &bsp.faces[fi];
    let ti = &bsp.tex_infos[face.tex_info.max(0) as usize];
    let lm = bsp.face_lightmap(face, false).unwrap_or_default();
    let avg = lm
        .iter()
        .take(lm.len().max(1))
        .fold([0f32; 3], |mut acc, c| {
            let l = c.to_linear();
            for k in 0..3 {
                acc[k] += l[k] / lm.len().max(1) as f32;
            }
            acc
        });
    eprintln!(
        "centre ray: face {fi} at {t:.0} units, material {:?}, texinfo flags {:#x}, light_ofs {}, lightmap {:?} ({} samples, avg {:.3?}), styles {:?}",
        bsp.face_material(face),
        ti.flags,
        face.light_ofs,
        face.lightmap_dims(),
        lm.len(),
        avg,
        face.styles
    );
}
