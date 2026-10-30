//! Skinned bounding box of a sequence frame: GMOD_DIR=... pose_bbox <model.mdl> <sequence> [time]
use glam::{Mat4, Vec3};
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let install = gm_content::find_install().unwrap();
    let mount = gm_content::Mount::for_install(&install).unwrap();
    let path = &args[0];
    let bytes = mount.read(path).unwrap();
    let (mdl, vvd, _vtx) = gm_render::props::load_model(&mount, path).unwrap();
    let seqs = gm_formats::mdl::sequences(&bytes, &mdl.header);
    let anims = gm_formats::mdl::anim_descs(&bytes, &mdl.header);
    let seq = seqs.iter().find(|s| s.name == args[1]).expect("sequence");
    let anim = &anims[seq.anim as usize];
    let t: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let frame = ((t * anim.fps) as usize) % anim.frames as usize;
    let (start, f) = anim.frame_data(&bytes, frame).expect("block 0");
    println!(
        "desc at {} index {} sections {}x{} start {start} frame {f}",
        anim.offset, anim.index, anim.section_frames, anim.section_index
    );
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
    let skin: Vec<Mat4> = world
        .iter()
        .zip(&mdl.bones)
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
    for v in &vvd.vertices {
        let mut p = Vec3::ZERO;
        for i in 0..v.num_bones as usize {
            p += skin[v.bones[i] as usize].transform_point3(Vec3::from(v.position)) * v.weights[i];
        }
        lo = lo.min(p);
        hi = hi.max(p);
    }
    println!(
        "{path} {} frame {frame}/{}: bbox {lo:?}..{hi:?}",
        seq.name, anim.frames
    );
    for (i, b) in mdl.bones.iter().enumerate().take(6) {
        println!(
            "  bone {i} {} parent {} local {:?} world {:?}",
            b.name,
            b.parent,
            local[i].0,
            world[i].w_axis.truncate()
        );
    }
}
