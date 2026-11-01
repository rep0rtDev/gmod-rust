//! `Spawn_Weapon`: a weapon entity lands 32 u along the eye trace's normal, a player walking
//! into it takes it (default clip, no reserve ammo, active weapon unchanged), the entity goes
//! and the undo entry dies; a save keeps the class.
use glam::Vec3;

#[test]
fn spawned_weapon_is_picked_up_by_touch() {
    let Some(inst) = gm_content::find_install() else {
        eprintln!("skipped: no Garry's Mod install");
        return;
    };
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, "gm_construct").unwrap();
    let model = gm_server::hl2_world_model("weapon_357").unwrap();
    let mut server = gm_server::Server::new(&world.collision, world.physics, world.world_body);
    server.register_model(model, gm_dedicated::model_info(&mount, model).unwrap());
    let start = Vec3::new(823.235, -500.0, -143.0);
    server.add_player(1, start, Vec3::new(25.0, -40.0, 0.0));
    server.set_weapons(1, &["gmod_tool"]);
    let idle = std::collections::BTreeMap::from([(1, gm_server::PlayerCmd::default())]);
    for _ in 0..20 {
        server.tick(&idle);
    }
    assert!(server.spawn_weapon(1, "weapon_bogus").is_none());
    assert!(server.sandbox.undos(1).is_empty());
    let hit = server.tool_hit(1).unwrap();
    let ent = server.spawn_weapon(1, "weapon_357").unwrap();
    let prop = server
        .sandbox
        .props()
        .iter()
        .find(|p| p.id == ent)
        .unwrap()
        .clone();
    assert_eq!(prop.model, model);
    assert!((prop.pos - (hit.pos + hit.normal * 32.0)).length() < 0.01);
    assert_eq!(server.world_weapon_class(ent), Some("weapon_357"));
    assert_eq!(server.sandbox.undos(1)[0].name, "SWEP");
    assert_eq!(
        server.saved_prop(&prop, Vec3::ZERO).weapon.as_deref(),
        Some("weapon_357")
    );
    for _ in 0..60 {
        server.tick(&idle);
    }
    let pos = server
        .sandbox
        .props()
        .iter()
        .find(|p| p.id == ent)
        .unwrap()
        .pos;
    server.teleport_player(1, pos + Vec3::new(0.0, 0.0, 8.0));
    for _ in 0..3 {
        server.tick(&idle);
    }
    let p = &server.players[&1];
    assert!(
        p.weapons.iter().any(|w| w == "weapon_357"),
        "{:?}",
        p.weapons
    );
    assert_eq!(p.inventory.clips["weapon_357"], [6, -1]);
    assert_eq!(p.active_weapon, "gmod_tool");
    assert!(!server.sandbox.props().iter().any(|p| p.id == ent));
    assert!(server.world_weapon_class(ent).is_none());
    // the dead entry stays until an undo skips over it
    assert_eq!(server.sandbox.undos(1).len(), 1);
    server.undo(1);
    assert!(server.sandbox.undos(1).is_empty());
}
