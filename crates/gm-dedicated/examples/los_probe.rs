//! cargo run -p gm-dedicated --example los_probe: line-of-sight probe on gm_construct for the
//! server_npc_grenade_soldier scene - world plus the four fridges at x=760, from the soldier's
//! eyes toward the bot's eyes / centre / origin (which point the game's FVisible clips on).
fn main() -> anyhow::Result<()> {
    let inst = gm_content::find_install().expect("GMOD_DIR");
    let mount = gm_content::Mount::for_install(&inst)?;
    let world = gm_dedicated::load_world(&mount, "gm_construct")?;
    let model = "models/props_c17/FurnitureFridge001a.mdl";
    let info = gm_dedicated::model_info(&mount, model)?;
    println!(
        "fridge phys z {:.1}..{:.1}",
        info.script.phys_mins.z, info.script.phys_maxs.z
    );
    let bodies: Vec<gm_sandbox::PropBody> = [-551.0f32, -517.0, -483.0, -449.0]
        .iter()
        .enumerate()
        .map(|(i, y)| gm_sandbox::PropBody {
            id: 100 + i as u32,
            bone: 0,
            pos: glam::Vec3::new(760.0, *y, -143.0),
            angles: glam::Vec3::ZERO,
            hulls: info.hull_planes.clone(),
        })
        .collect();
    for (sx, sy) in [(975.0f32, -331.0f32), (971.0, -211.0), (980.0, -32.0)] {
        let a = glam::Vec3::new(sx, sy, -78.0);
        for tz in [-79.0f32, -107.0, -143.0] {
            let b = glam::Vec3::new(700.0, -500.0, tz);
            let hit = gm_sandbox::trace_line(&world.collision, &bodies, a, b);
            println!(
                "from ({sx},{sy}) to z {tz}: {}",
                match hit {
                    Some(h) => format!("hit ent {:?} frac {:.3}", h.ent, h.fraction),
                    None => "clear".to_string(),
                }
            );
        }
    }
    Ok(())
}
