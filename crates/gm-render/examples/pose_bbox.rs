//! Skinned bounding box of a sequence frame:
//! GMOD_DIR=... pose_bbox <model.mdl> <sequence> [time] [--host <model.mdl> <sequence>]
//! With `--host` the bones the host also has (by name) take the host's pose (bone merge).
use glam::{Mat4, Vec3};

struct Posed {
    mdl: gm_formats::mdl::Mdl,
    vvd: gm_formats::mdl::Vvd,
    local: Vec<([f32; 3], [f32; 4])>,
    world: Vec<Mat4>,
    label: String,
}

fn posed(mount: &gm_content::Mount, path: &str, seq_name: &str, t: f32) -> Posed {
    let bytes = mount.read(path).unwrap();
    let (mdl, vvd, _vtx) = gm_render::props::load_model(mount, path).unwrap();
    let seqs = gm_formats::mdl::sequences(&bytes, &mdl.header);
    let anims = gm_formats::mdl::anim_descs(&bytes, &mdl.header);
    let seq = seqs.iter().find(|s| s.name == seq_name).expect("sequence");
    let anim = &anims[seq.anim as usize];
    let frame = ((t * anim.fps) as usize) % anim.frames as usize;
    let (start, f) = anim.frame_data(&bytes, frame).expect("block 0");
    let local = gm_formats::mdl::anim_pose(&bytes, start, &mdl.bones, f).expect("pose");
    let mut world: Vec<Mat4> = Vec::new();
    for (i, (pos, quat)) in local.iter().enumerate() {
        let m = Mat4::from_rotation_translation(
            glam::Quat::from_array(*quat).normalize(),
            Vec3::from(*pos),
        );
        let p = mdl.bones[i].parent;
        world.push(if p >= 0 { world[p as usize] * m } else { m });
    }
    let label = format!("{path} {} frame {frame}/{}", seq.name, anim.frames);
    Posed {
        mdl,
        vvd,
        local,
        world,
        label,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let install = gm_content::find_install().unwrap();
    let mount = gm_content::Mount::for_install(&install).unwrap();
    let t: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let mut p = posed(&mount, &args[0], &args[1], t);
    if let Some(i) = args.iter().position(|a| a == "--host") {
        let h = posed(&mount, &args[i + 1], &args[i + 2], 0.0);
        let mut merged = 0;
        for j in 0..p.mdl.bones.len() {
            let b = &p.mdl.bones[j];
            if let Some(k) = h.mdl.bones.iter().position(|hb| hb.name == b.name) {
                p.world[j] = h.world[k];
                merged += 1;
            } else {
                let m = Mat4::from_rotation_translation(
                    glam::Quat::from_array(p.local[j].1).normalize(),
                    Vec3::from(p.local[j].0),
                );
                p.world[j] = if b.parent >= 0 {
                    p.world[b.parent as usize] * m
                } else {
                    m
                };
            }
        }
        println!(
            "merged {merged} of {} bones onto {}",
            p.mdl.bones.len(),
            h.label
        );
    }
    let skin: Vec<Mat4> = p
        .world
        .iter()
        .zip(&p.mdl.bones)
        .map(|(w, b)| {
            let r = &b.pose_to_bone;
            *w * Mat4::from_cols_array_2d(&[
                [r[0][0], r[1][0], r[2][0], 0.0],
                [r[0][1], r[1][1], r[2][1], 0.0],
                [r[0][2], r[1][2], r[2][2], 0.0],
                [r[0][3], r[1][3], r[2][3], 1.0],
            ])
        })
        .collect();
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for v in &p.vvd.vertices {
        let mut q = Vec3::ZERO;
        for i in 0..v.num_bones as usize {
            q += skin[v.bones[i] as usize].transform_point3(Vec3::from(v.position)) * v.weights[i];
        }
        lo = lo.min(q);
        hi = hi.max(q);
    }
    println!("{}: bbox {lo:?}..{hi:?}", p.label);
    for (i, b) in p.mdl.bones.iter().enumerate().take(6) {
        println!(
            "  bone {i} {} parent {} local {:?} world {:?}",
            b.name,
            b.parent,
            p.local[i].0,
            p.world[i].w_axis.truncate()
        );
    }
}
