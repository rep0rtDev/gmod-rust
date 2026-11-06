//! Death ragdoll spawn pose: the sequences a player model's death ragdoll can start from and
//! the ragdoll part positions after a kill (`death_pose_probe [model]`), to compare the posed
//! ragdoll (hands hanging in `idle_all_01`) against the `ragdoll` reference pose (T-pose).
use glam::Vec3;

fn main() -> anyhow::Result<()> {
    let model = std::env::args()
        .nth(1)
        .unwrap_or_else(|| gm_server::PLAYER_MODEL.to_string());
    let inst = gm_content::find_install().expect("GMOD_DIR");
    let mount = gm_content::Mount::for_install(&inst)?;
    let info = gm_dedicated::model_info(&mount, &model)?;
    let rd = info.ragdoll.as_ref().expect("ragdoll model");
    for d in &rd.death_poses {
        println!("{}: {} frames at {} fps", d.sequence, d.frames.len(), d.fps);
    }
    let world = gm_dedicated::load_world(&mount, "gm_construct")?;
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let m2 = gm_content::Mount::for_install(&inst)?;
    server.model_loader = Some(Box::new(move |m: &str| {
        gm_dedicated::model_info(&m2, m).ok()
    }));
    let start = Vec3::new(823.235, -500.0, -143.0);
    server.add_player(1, start, Vec3::ZERO);
    let idle = std::collections::BTreeMap::from([(1, gm_server::PlayerCmd::default())]);
    for _ in 0..5 {
        server.tick(&idle);
    }
    server.kill_player(1);
    server.tick(&idle);
    let ent = server.players[&1].ragdoll_ent.expect("ragdoll");
    let (bodies, _) = server.ragdoll_parts(ent).expect("parts");
    for (s, &b) in rd.solids.iter().zip(bodies) {
        let st = server.physics.state(b);
        let rel = st.position - start;
        println!(
            "{:<28} rel ({:7.1} {:7.1} {:7.1}) angles ({:6.1} {:6.1} {:6.1})",
            s.bone, rel.x, rel.y, rel.z, st.angles.0, st.angles.1, st.angles.2
        );
    }
    Ok(())
}
