//! A prop_ragdoll with poser state survives save_world -> load_world (and so the duplicator).
use glam::Vec3;

#[test]
fn ragdoll_parts_and_poser_state_roundtrip_through_a_save() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let model = "models/kleiner.mdl";
    let info = gm_dedicated::model_info(&mount, model).unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    server.register_model(model, info.clone());
    server.add_existing_props(&[(
        model.to_string(),
        Vec3::new(900.0, -500.0, -100.0),
        Vec3::ZERO,
        true,
    )]);
    let before = server.saved_prop(&server.sandbox.props()[0].clone(), Vec3::ZERO);
    assert_eq!(before.parts.len(), 15, "kleiner has 15 physics objects");
    let seeded = gm_server::save::SavedProp {
        bone_manips: vec![gm_server::save::SavedBoneManip {
            bone: 0,
            scale: [1.03, 1.03, 1.03],
            angles: [0.0; 3],
        }],
        flex: Some(gm_server::save::SavedFlex {
            weights: vec![0.5; before.parts.len()],
            scale: 1.0,
        }),
        eye_target: Some([7.0, -4.0, -25.0]),
        decals: vec![gm_server::save::SavedDecal {
            bone: 0,
            name: "Smile".into(),
            pos1: [1.0, 0.0, 0.0],
            pos2: [-1.0, 0.0, 0.0],
        }],
        ..before.clone()
    };
    server.apply_saved_state(1, &seeded, Vec3::ZERO);
    let save = server.save_world("gm_construct");
    assert_eq!(save.props.len(), 1);
    let sp = &save.props[0];
    assert_eq!(sp.bone_manips, seeded.bone_manips);
    assert_eq!(sp.flex, seeded.flex);
    assert_eq!(sp.eye_target, seeded.eye_target);
    assert_eq!(sp.decals, seeded.decals);

    let world2 = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let mut other = gm_server::Server::new(&world2.collision, world2.physics, world2.world_body);
    other.register_model(model, info);
    let (props, _) = other.load_world(&save, 0);
    assert_eq!(props, 1);
    let restored = other.saved_prop(&other.sandbox.props()[0].clone(), Vec3::ZERO);
    assert_eq!(restored.parts.len(), sp.parts.len());
    for (a, b) in restored.parts.iter().zip(&sp.parts) {
        let d = (Vec3::from(a.pos) - Vec3::from(b.pos)).length();
        assert!(
            d < 0.01,
            "part moved {d} on load: {:?} vs {:?}",
            a.pos,
            b.pos
        );
        assert_eq!(a.frozen, b.frozen);
    }
    assert_eq!(restored.bone_manips, sp.bone_manips);
    assert_eq!(restored.flex, sp.flex);
    assert_eq!(restored.eye_target, sp.eye_target);
    assert_eq!(restored.decals, sp.decals);
}
