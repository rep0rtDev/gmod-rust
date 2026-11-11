//! Diagnostic sweep (ignored by default): every model of the default spawnlists spawned as a
//! prop on gm_construct, with the lowest point of its PHY after a second against the floor.
//! `SWEEP_START=x,y,z` places the player (default the spawn area; `-5000,-3000,251` is the
//! open displacement field), `SWEEP_FILTER=substr` picks models and prints their trajectory,
//! `SWEEP_LIMIT=n` caps the count, `SWEEP_SCAN=1` lists displacement ground spots instead.
//! Irregular bodies still tumbling after a second and models whose PHY reaches far below the
//! hull box (`project_building02`) are reported too, so the figures need reading, not a gate:
//! `GMOD_DIR=... cargo test --release -p gm-dedicated --test prop_spawn_sweep -- --ignored --nocapture`
use glam::Vec3;

#[test]
#[ignore]
fn spawnlist_props_rest_on_the_floor() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mut models = std::collections::BTreeSet::new();
    let dir = inst.game_dir.join("garrysmod/settings/spawnlist_default");
    for e in std::fs::read_dir(&dir).unwrap().flatten() {
        let text = std::fs::read_to_string(e.path()).unwrap_or_default();
        for l in text.lines() {
            let l = l.trim();
            if l.to_ascii_lowercase().starts_with("\"model\"") {
                if let Some(m) = l.split('"').nth(3) {
                    models.insert(m.replace('\\', "/").to_ascii_lowercase());
                }
            }
        }
    }
    let limit: usize = std::env::var("SWEEP_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    if std::env::var_os("SWEEP_SCAN").is_some() {
        // where the map's displacement ground is: spots whose downward trace hits a displacement
        let mut n = 0;
        for xi in -40..40 {
            for yi in -40..40 {
                let a = Vec3::new(xi as f32 * 128.0, yi as f32 * 128.0, 800.0);
                let t =
                    world
                        .collision
                        .trace_ray(a, a - Vec3::Z * 2000.0, gm_collision::MASK_SOLID);
                if t.surface.is_some() && t.fraction < 1.0 && n < 40 {
                    n += 1;
                    eprintln!(
                        "disp at {:.0} {:.0} z {:.1} n {:.2?}",
                        a.x, a.y, t.end.z, t.normal
                    );
                }
            }
        }
        return;
    }
    let start = std::env::var("SWEEP_START")
        .ok()
        .and_then(|s| {
            let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            (v.len() == 3).then(|| Vec3::new(v[0], v[1], v[2]))
        })
        .unwrap_or(Vec3::new(823.235, -500.0, -143.0));
    server.add_player(1, start, Vec3::new(30.0, 0.0, 0.0));
    server.set_weapons(1, &["gmod_tool"]);
    let look = gm_server::PlayerCmd {
        cmd: gm_move::UserCmd {
            view_angles: Vec3::new(30.0, 0.0, 0.0),
            ..Default::default()
        },
        ..Default::default()
    };
    let idle = std::collections::BTreeMap::from([(1, look)]);
    for _ in 0..20 {
        server.tick(&idle);
    }
    let hit = server.tool_hit(1).unwrap();
    if std::env::var_os("SWEEP_FILTER").is_some() {
        eprintln!("tool hit {:.2?} n {:.2?}", hit.pos, hit.normal);
        for z0 in [-100.0, -143.0, -145.0, -150.0, -160.0] {
            let a = Vec3::new(hit.pos.x, hit.pos.y, z0);
            let t = world
                .collision
                .trace_ray(a, a - Vec3::Z * 200.0, gm_collision::MASK_SOLID);
            eprintln!(
                "  down from z {z0}: end z {:.2} n {:.2?} start_solid {} contents {:#x} surface {:?}",
                t.end.z, t.normal, t.start_solid, t.contents, t.surface
            );
        }
    }
    let mut sunk = Vec::new();
    let mut fell = Vec::new();
    let mut n = 0;
    let mut ragdolls = 0;
    let mut skipped = Vec::new();
    let filter = std::env::var("SWEEP_FILTER").unwrap_or_default();
    for m in models.iter().filter(|m| m.contains(&filter)).take(limit) {
        let info = match gm_dedicated::model_info(&mount, m) {
            Ok(i) => i,
            Err(e) => {
                skipped.push(format!("{m}: {e}"));
                continue;
            }
        };
        if info.ragdoll.is_some() {
            ragdolls += 1;
            continue;
        }
        let b = info.bounds;
        let phy: Vec<Vec3> = info.hulls.iter().flatten().copied().collect();
        let phy_min_z = phy.iter().map(|p| p.z).fold(f32::MAX, f32::min);
        if !filter.is_empty() {
            let lo = phy.iter().copied().fold(Vec3::MAX, Vec3::min);
            let hi = phy.iter().copied().fold(Vec3::MIN, Vec3::max);
            eprintln!(
                "{m}: hull box {:.1?}..{:.1?} phy {:.1?}..{:.1?} ({} convexes)",
                b.min,
                b.max,
                lo,
                hi,
                info.hulls.len()
            );
        }
        server.register_model(m, info);
        let Some(id) = server.spawn_prop(1, m) else {
            skipped.push(format!("{m}: spawn refused"));
            continue;
        };
        n += 1;
        let at_spawn = server
            .sandbox
            .props()
            .iter()
            .find(|p| p.id == id)
            .unwrap()
            .pos;
        for t in 0..60 {
            server.tick(&idle);
            if !filter.is_empty() && (t < 10 || t % 10 == 9) {
                if let Some(p) = server.sandbox.props().iter().find(|p| p.id == id) {
                    eprintln!("  tick {t}: pos {:.2?} angles {:.1?}", p.pos, p.angles);
                }
            }
        }
        let Some(p) = server.sandbox.props().iter().find(|p| p.id == id) else {
            fell.push(format!("{m}: gone"));
            continue;
        };
        // the hull box rests on the floor at spawn; a second later the PHY (what really
        // collides) rests on it, so the lowest PHY point must not be under the floor
        let bottom0 = at_spawn.z + b.min.z;
        let rot = gm_sandbox::rotation(p.angles);
        let phy_bottom = p.pos.z + phy.iter().map(|pt| (rot * *pt).z).fold(f32::MAX, f32::min);
        if phy_bottom < hit.pos.z - 64.0 {
            fell.push(format!(
                "{m}: spawn z {:.1} -> {:.1}, phy bottom {:.1}",
                at_spawn.z, p.pos.z, phy_bottom
            ));
        } else if bottom0 < hit.pos.z - 1.0 || phy_bottom < hit.pos.z - 2.0 {
            sunk.push(format!(
                "{m}: hull z {:.1}..{:.1} phy z {:.1}, hull bottom at spawn {:.1}, phy bottom after 1 s {:.1} (floor {:.1})",
                b.min.z, b.max.z, phy_min_z, bottom0, phy_bottom, hit.pos.z
            ));
        }
        server.undo(1);
        for _ in 0..2 {
            server.tick(&idle);
        }
    }
    eprintln!(
        "{n} props spawned ({ragdolls} ragdolls, {} skipped); {} sunk, {} fell",
        skipped.len(),
        sunk.len(),
        fell.len()
    );
    for s in skipped.iter().take(8) {
        eprintln!("  skipped {s}");
    }
    for s in sunk.iter().chain(&fell) {
        eprintln!("  {s}");
    }
}
