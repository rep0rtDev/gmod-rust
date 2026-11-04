//! cargo run -p gm-dedicated --example sched_probe tools/parity/sched_translate_game.txt: for each `<name> <class>
//! <i>g,...>` line (the server_npc_sched_translate_* recordings: SetSchedule(i) then
//! GetCurrentSchedule for i in 0..88) spawns the class, applies the same loop to our server
//! and prints the two mismatch lists where they differ.
use glam::Vec3;

fn main() -> anyhow::Result<()> {
    let table = std::env::args().nth(1).expect("table file");
    let inst = gm_content::find_install().expect("GMOD_DIR");
    let mount = gm_content::Mount::for_install(&inst)?;
    let world = gm_dedicated::load_world(&mount, "gm_construct")?;
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let m2 = gm_content::Mount::for_install(&inst)?;
    server.model_loader = Some(Box::new(move |m: &str| {
        gm_dedicated::model_info(&m2, m).ok()
    }));
    let start = Vec3::new(823.235, -500.0, -143.0);
    server.add_player(1, start, Vec3::new(15.0, 0.0, 0.0));
    server.set_weapons(1, &["gmod_tool"]);
    let look = gm_server::PlayerCmd {
        cmd: gm_move::UserCmd {
            view_angles: Vec3::new(15.0, 0.0, 0.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let idle = std::collections::BTreeMap::from([(1, look)]);
    for _ in 0..20 {
        server.tick(&idle);
    }
    let mut same = 0;
    for line in std::fs::read_to_string(&table)?.lines() {
        let mut it = line.split(' ');
        let (Some(name), Some(class), rest) = (it.next(), it.next(), it.next().unwrap_or(""))
        else {
            continue;
        };
        if class == "None" || rest == "nonpc" {
            continue;
        }
        let Some(id) = server.spawn_npc(1, class, "none") else {
            println!("{name}: {class} does not spawn here");
            continue;
        };
        let mut ours = Vec::new();
        for i in 0..88u16 {
            if !server.npc_set_schedule(id, Some(i)) {
                ours.push(format!("{i}>x"));
                continue;
            }
            let g = server.npc_schedule_id(id);
            if g != i64::from(i) {
                ours.push(format!("{i}>{g}"));
            }
        }
        let ours = ours.join(",");
        if ours == rest {
            same += 1;
        } else {
            println!("{name} ({class})\n  game {rest}\n  ours {ours}");
        }
        server.cleanup(1);
    }
    println!("{same} classes identical");
    Ok(())
}
