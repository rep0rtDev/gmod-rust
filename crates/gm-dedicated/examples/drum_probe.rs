//! cargo run -p gm-dedicated --example drum_probe [model] [x y z]: the model's physics
//! extents and whether a line from a zombie's eyes at (992,-500,-80) to the bot's eyes /
//! origin at x=823 (server_npc_zombie_swat3_a_drum875) clips its hull placed at the point.
use glam::Vec3;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let model = args
        .first()
        .cloned()
        .unwrap_or_else(|| "models/props_c17/oildrum001.mdl".to_string());
    let nums: Vec<f32> = args.iter().skip(1).filter_map(|a| a.parse().ok()).collect();
    let pos = if nums.len() == 3 {
        Vec3::new(nums[0], nums[1], nums[2])
    } else {
        Vec3::new(875.03, -499.98, -143.58)
    };
    let inst = gm_content::find_install().expect("GMOD_DIR");
    let mount = gm_content::Mount::for_install(&inst)?;
    let world = gm_dedicated::load_world(&mount, "gm_construct")?;
    let info = gm_dedicated::model_info(&mount, &model)?;
    println!(
        "{model}: phys {:?}..{:?}, {} hulls",
        info.script.phys_mins,
        info.script.phys_maxs,
        info.hull_planes.len()
    );
    let body = gm_sandbox::PropBody {
        id: 1,
        bone: 0,
        pos,
        angles: Vec3::ZERO,
        hulls: info.hull_planes.clone(),
    };
    let eye = Vec3::new(991.97, -500.0, -79.97);
    for (name, to) in [
        ("bot eyes", Vec3::new(823.23, -500.0, -79.0)),
        ("bot chest", Vec3::new(823.23, -500.0, -107.0)),
        ("bot origin", Vec3::new(823.23, -500.0, -143.0)),
    ] {
        let world_hit = world
            .collision
            .trace_ray(eye, to, gm_collision::MASK_PLAYERSOLID);
        let hit = gm_sandbox::trace_line(&world.collision, std::slice::from_ref(&body), eye, to);
        println!(
            "{name}: world fraction {:.3}, prop line {:?}",
            world_hit.fraction,
            hit.map(|h| (h.fraction, h.ent, h.pos))
        );
    }
    Ok(())
}
