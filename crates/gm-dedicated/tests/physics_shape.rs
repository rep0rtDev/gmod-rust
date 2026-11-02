//! `ENT:PhysicsInit*` shapes: a scripted entity's body is built from the shape it asked for
//! instead of its model's PHY (box hull rests on its own half height, static never falls, none has
//! no body at all).

use glam::Vec3;

fn prop_z(server: &gm_server::Server, id: u32) -> f32 {
    server
        .sandbox
        .props()
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.pos.z)
        .unwrap()
}

#[test]
fn script_shapes_replace_the_model_hulls() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_flatgrass").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    let model = "models/props_junk/wood_crate001a.mdl";
    let info = gm_dedicated::model_info(&mount, model).unwrap();
    server.register_model(model, info);
    let start = Vec3::new(1600.0, 384.0, -12799.0);
    server.add_player(1, start, Vec3::ZERO);
    let idle = std::collections::BTreeMap::from([(1, gm_server::PlayerCmd::default())]);
    let floor = -12800.0;
    let drop = Vec3::new(1700.0, 384.0, floor + 120.0);

    // a 48 u box: rests with its centre 24 u over the floor (the crate's own hull is ~41 u tall)
    let boxed = server
        .script_spawn_prop(1, model, drop, Vec3::ZERO, false)
        .unwrap();
    assert!(server.set_script_shape(
        boxed,
        gm_server::PhysShape::Box {
            mins: Vec3::splat(-24.0),
            maxs: Vec3::splat(24.0),
        }
    ));
    // static: stays where it was spawned
    let fixed = server
        .script_spawn_prop(
            1,
            model,
            drop + Vec3::new(200.0, 0.0, 0.0),
            Vec3::ZERO,
            false,
        )
        .unwrap();
    assert!(server.set_script_shape(fixed, gm_server::PhysShape::Static));
    // none: no body is ever built
    let ghost = server
        .script_spawn_prop(
            1,
            model,
            drop + Vec3::new(400.0, 0.0, 0.0),
            Vec3::ZERO,
            false,
        )
        .unwrap();
    assert!(server.set_script_shape(ghost, gm_server::PhysShape::None));
    assert!(!server.set_script_shape(9999, gm_server::PhysShape::None));

    for _ in 0..200 {
        server.tick(&idle);
    }
    let z = prop_z(&server, boxed) - floor;
    assert!(
        (22.0..26.5).contains(&z),
        "box rests on its half height: {z}"
    );
    let z = prop_z(&server, fixed) - floor;
    assert!((z - 120.0).abs() < 1.0, "static stays: {z}");
    assert!(!server.has_body(ghost), "no body for SOLID_NONE");
    let z = prop_z(&server, ghost) - floor;
    assert!(
        (z - 120.0).abs() < 1.0,
        "bodiless entity does not move: {z}"
    );

    // a sphere of 20 u on a live body: dropped and rebuilt, rests at 20
    assert!(server.set_script_shape(boxed, gm_server::PhysShape::Sphere { radius: 20.0 }));
    for _ in 0..200 {
        server.tick(&idle);
    }
    let z = prop_z(&server, boxed) - floor;
    assert!((17.0..23.0).contains(&z), "sphere rests on its radius: {z}");
}
