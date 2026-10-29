use glam::Vec3;
use gm_collision::{World, MASK_PLAYERSOLID, MASK_SOLID};

fn load_construct() -> Option<(Vec<u8>, Vec<[f32; 3]>)> {
    let install = gm_content::find_install()?;
    let mount = gm_content::Mount::for_install(&install).ok()?;
    let data = mount.read("maps/gm_construct.bsp").ok()?;
    let bsp = gm_formats::bsp::Bsp::parse(&data).ok()?;
    let spawns = bsp
        .entities
        .iter()
        .filter(|e| e.classname() == "info_player_start")
        .filter_map(|e| e.origin())
        .collect();
    Some((data, spawns))
}

#[test]
fn spawns_stand_on_ground() {
    let Some((data, spawns)) = load_construct() else {
        return;
    };
    let bsp = gm_formats::bsp::Bsp::parse(&data).unwrap();
    let world = World::from_bsp(&bsp);
    let mins = Vec3::new(-16.0, -16.0, 0.0);
    let maxs = Vec3::new(16.0, 16.0, 72.0);
    assert!(spawns.len() > 10);
    for s in &spawns {
        let o = Vec3::from(*s);
        // the spawn origin is at the feet; the player hull dropped from 8 units up lands at the floor
        let t = world.trace_box(
            o + Vec3::Z * 8.0,
            o - Vec3::Z * 64.0,
            mins,
            maxs,
            MASK_PLAYERSOLID,
        );
        assert!(t.hit(), "spawn {s:?} has no floor");
        assert!(!t.start_solid, "spawn {s:?} starts in solid");
        assert!(
            (t.end.z - o.z).abs() < 0.5,
            "spawn {s:?} floor at {}",
            t.end.z
        );
        assert!(t.normal.z > 0.99, "floor normal {:?}", t.normal);
        // and the ray straight down from eye height hits the same floor
        let r = world.trace_ray(o + Vec3::Z * 64.0, o - Vec3::Z * 64.0, MASK_SOLID);
        assert!(
            r.hit() && (r.end.z - o.z).abs() < 0.1,
            "ray floor {}",
            r.end.z
        );
        // standing position is open space
        assert_eq!(world.point_contents(o + Vec3::Z * 36.0) & MASK_SOLID, 0);
    }
}

#[test]
fn walls_block_long_sweeps() {
    let Some((data, spawns)) = load_construct() else {
        return;
    };
    let bsp = gm_formats::bsp::Bsp::parse(&data).unwrap();
    let world = World::from_bsp(&bsp);
    let o = Vec3::from(spawns[0]) + Vec3::Z * 36.0;
    let mut hits = 0;
    for dir in [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y] {
        let t = world.trace_ray(o, o + dir * 20000.0, MASK_SOLID);
        if t.hit() {
            hits += 1;
            assert!(
                t.normal.dot(dir) < 0.0,
                "hit normal {:?} faces away from travel {dir:?}",
                t.normal
            );
        }
    }
    assert_eq!(hits, 4, "a sealed map blocks every horizontal ray");
    // going up eventually hits the sky brush ceiling
    let up = world.trace_ray(o, o + Vec3::Z * 20000.0, MASK_SOLID);
    assert!(up.hit());
}
