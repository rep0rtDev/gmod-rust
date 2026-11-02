//! Entity / Player classes and the ents / player / util libraries over the snapshot and queue.
use glam::Vec3;
use gm_script::api::{GestureOp, Payload, QueuedWrite};
use gm_script::snapshot::{EntitySnap, PlayerSnap};
use gm_script::{ActorConfig, ActorSpec, Runtime, Snapshot};
use std::sync::Arc;

fn snapshot() -> Arc<Snapshot> {
    let mut s = Snapshot {
        tick: 7,
        ..Default::default()
    };
    s.entities.insert(
        5,
        EntitySnap {
            id: 5,
            model: "models/props_c17/oildrum001.mdl".into(),
            pos: Vec3::new(100.0, 200.0, 300.0),
            angles: Vec3::new(0.0, 90.0, 0.0),
            frozen: true,
            skin: 1,
            body: "0".into(),
            owner: 1,
            class: "prop_physics".into(),
            health: 0,
            max_health: 0,
            velocity: Vec3::new(1.0, 2.0, 3.0),
            ang_velocity: Vec3::new(1.0, 2.0, 3.0),
            mass: 40.0,
            damping: (0.1, 0.2),
            asleep: false,
            parent: 0,
            color: [10, 20, 30, 40],
            material: "models/debug/debugwhite".into(),
            render_mode: 1,
            render_fx: 0,
            sequence: "idle".into(),
            cycle: 0.0,
            playback_rate: 1.0,
            sequence_finished: false,
            gesture: None,
            pose: Vec::new(),
            mins: glam::Vec3::ZERO,
            maxs: glam::Vec3::ZERO,
            bone_scales: vec![(1, Vec3::new(2.0, 2.0, 2.0))],
            bone_angles: vec![],
            flex: Some((vec![0.5], 1.0)),
            eye_target: None,
            weapon: "weapon_smg1".into(),
            blood: 0,
            npc_target_seen: 200,
            npc_target_first_seen: 0,
            npc_enemy_damage: 0,
            npc_sight: 2048.0,
            friction: 0.6,
            elasticity: 0.1,
            on_fire: true,
            npc_target: 0,
            npc_chase: 0,
            npc_goal: None,
        },
    );
    s.entities.insert(
        9,
        EntitySnap {
            id: 9,
            model: "models/zombie/classic.mdl".into(),
            pos: Vec3::new(0.0, 0.0, 0.0),
            angles: Vec3::ZERO,
            frozen: false,
            skin: 0,
            body: String::new(),
            owner: 1,
            class: "npc_zombie".into(),
            health: 50,
            max_health: 50,
            velocity: Vec3::ZERO,
            ang_velocity: Vec3::new(1.0, 2.0, 3.0),
            mass: 40.0,
            damping: (0.1, 0.2),
            asleep: false,
            parent: 0,
            color: [10, 20, 30, 40],
            material: "models/debug/debugwhite".into(),
            render_mode: 1,
            render_fx: 0,
            sequence: "idle".into(),
            cycle: 0.0,
            playback_rate: 1.0,
            sequence_finished: false,
            gesture: None,
            pose: Vec::new(),
            mins: glam::Vec3::ZERO,
            maxs: glam::Vec3::ZERO,
            bone_scales: vec![(1, Vec3::new(2.0, 2.0, 2.0))],
            bone_angles: vec![],
            flex: Some((vec![0.5], 1.0)),
            eye_target: None,
            weapon: "weapon_smg1".into(),
            blood: 0,
            npc_target_seen: 200,
            npc_target_first_seen: 0,
            npc_enemy_damage: 0,
            npc_sight: 2048.0,
            friction: 0.6,
            elasticity: 0.1,
            on_fire: true,
            npc_target: 1,
            npc_chase: 1,
            npc_goal: Some(Vec3::new(400.0, 0.0, 0.0)),
        },
    );
    s.players.insert(
        1,
        PlayerSnap {
            id: 1,
            counts: vec![("props".to_string(), 3)],
            origin: Vec3::new(10.0, 20.0, 30.0),
            velocity: Vec3::ZERO,
            view_angles: Vec3::new(0.0, 180.0, 0.0),
            held: None,
            carry: Some(777),
            water_level: 2,
            ping: 30.0,
            loss: 2.5,
            timeout: 0.5,
            addr: "127.0.0.1:27005".into(),
            alive: true,
            health: 85,
            armor: 10,
            max_health: 100,
            identity: Some("STEAM_0:1:2".into()),
            model: "models/player/kleiner.mdl".into(),
            skin: 0,
            body: String::new(),
            weapons: vec!["weapon_pistol".into(), "weapon_physgun".into()],
            active_weapon: "weapon_physgun".into(),
            previous_weapon: String::new(),
            ammo: vec![("Pistol".into(), 20), ("SMG1".into(), 45)],
            clips: vec![
                ("weapon_pistol".into(), 18, 0),
                ("weapon_smg1".into(), 45, 1),
            ],
            buttons: 1 | 4,
            buttons_last: 4,
            cmd_move: Vec3::new(250.0, -50.0, 0.0),
            cmd_mouse_x: 3.0,
            cmd_mouse_y: -2.0,
            cmd_impulse: 100,
            cmd_tick: 77,
            on_ground: true,
            ducked: true,
            vehicle: 0,
            driving: 0,
            use_target: 5,
            flashlight: true,
            punch: Vec3::ZERO,
            punch_vel: Vec3::ZERO,
            ladder: false,
            suit: false,
            walk_speed: 200.0,
            run_speed: 400.0,
            jump_power: 200.0,
            step_size: 18.0,
            crouched_walk_speed: 0.3,
            duck_time: 0.4,
            unduck_time: 0.2,
            max_velocity: 3500.0,
            stand_eye: 64.0,
            duck_eye: 28.0,
            hull_half: 16.0,
            stand_height: 72.0,
            duck_height: 36.0,
            frags: 3,
            deaths: 1,
            max_armor: 150,
            no_target: true,
            player_color: Vec3::new(0.5, 0.25, 1.0),
            weapon_color: Vec3::new(1.0, 0.0, 0.0),
            locked: true,
            joined_tick: 40,
            last_hitgroup: 1,
        },
    );
    Arc::new(s)
}

fn run(source: &str) -> (Runtime, Vec<QueuedWrite>) {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "ent".into(),
        priority: 0,
        source: format!("events.on('tick.pre', function() {source} end)"),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 7)
        .unwrap();
    let writes = rt.drain_writes().into_iter().map(|w| w.3).collect();
    (rt, writes)
}

#[test]
fn entities_read_the_snapshot() {
    let (rt, _) = run(
        "local e = Entity(5)
         state.r = {
            tostring(e), e:GetClass(), e:GetModel(), tostring(e:GetPos()), e:GetAngles().y, e:GetVelocity().z,
            e:IsValid(), Entity(77):IsValid(), e:GetOwner():Nick(), e:GetPhysicsObject():IsMotionEnabled(),
            Entity(9):IsNPC(), Entity(9):Health(), e == Entity(5), IsValid(NULL), e:GetForward():IsEqualTol(Vector(0, 1, 0), 1e-5),
         }",
    );
    let r: Vec<mlua::Value> = rt.actors()[0].eval("return state.r").unwrap();
    let s = |i: usize| match &r[i] {
        mlua::Value::String(s) => s.to_string_lossy().to_string(),
        mlua::Value::Boolean(b) => b.to_string(),
        mlua::Value::Number(n) => n.to_string(),
        mlua::Value::Integer(n) => n.to_string(),
        other => format!("{other:?}"),
    };
    assert_eq!(s(0), "Entity [5][prop_physics]");
    assert_eq!(s(1), "prop_physics");
    assert_eq!(s(2), "models/props_c17/oildrum001.mdl");
    assert_eq!(s(3), "100.000000 200.000000 300.000000");
    assert_eq!(s(4), "90");
    assert_eq!(s(5), "3");
    assert_eq!(s(6), "true");
    assert_eq!(s(7), "false");
    assert_eq!(s(8), "STEAM_0:1:2");
    assert_eq!(
        s(9),
        "false",
        "a frozen prop's physics object has motion disabled"
    );
    assert_eq!(s(10), "true");
    assert_eq!(s(11), "50");
    assert_eq!(s(12), "true");
    assert_eq!(s(13), "false");
    assert_eq!(s(14), "true");
}

#[test]
fn entity_setters_queue_writes() {
    let (_, writes) = run("local e = Entity(5)
         e:SetPos(Vector(1, 2, 3))
         e:SetAngles(Angle(10, 20, 30))
         e:GetPhysicsObject():EnableMotion(true)
         e:SetHealth(42)
         e:Remove()
         Player(1):SetHealth(50)
         Entity(9):SetEnemy(Player(1))
         Entity(9):NavSetGoalPos(Vector(5, 6, 7))");
    assert!(
        matches!(&writes[0], QueuedWrite::SetPos { ent: 5, pos } if *pos == Vec3::new(1.0, 2.0, 3.0))
    );
    assert!(
        matches!(&writes[1], QueuedWrite::SetAngles { ent: 5, angles } if *angles == Vec3::new(10.0, 20.0, 30.0))
    );
    assert!(matches!(
        &writes[2],
        QueuedWrite::Freeze {
            ent: 5,
            frozen: false
        }
    ));
    assert!(matches!(
        &writes[3],
        QueuedWrite::SetHealth { ent: 5, health: 42 }
    ));
    assert!(matches!(&writes[4], QueuedWrite::Remove { ent: 5 }));
    assert!(matches!(
        &writes[5],
        QueuedWrite::PlayerSetHealth { player: 1, .. }
    ));
    assert!(matches!(
        &writes[6],
        QueuedWrite::NpcTarget {
            ent: 9,
            player: Some(1)
        }
    ));
    assert!(matches!(&writes[7], QueuedWrite::NpcGoto { ent: 9, .. }));
    assert_eq!(writes.len(), 8);
}

#[test]
#[allow(clippy::type_complexity)]
fn ents_and_player_libraries() {
    let (rt, _) = run(
        "local all = ents.GetAll()
         local npcs = ents.FindByClass('npc_*')
         local near = ents.FindInSphere(Vector(0, 0, 0), 10)
         local ply = player.GetAll()[1]
         state.r = { #all, #npcs, npcs[1]:EntIndex(), #near, player.GetCount(), ply:Nick(), ply:Health(), ply:HasWeapon('weapon_pistol'),
                     ply:GetActiveWeapon():GetClass(), #ply:GetWeapons(), ply:GetAimVector():Dot(Vector(-1, 0, 0)), #ents.FindByClass('player'),
                     ents.FindByModel('models/zombie/classic.mdl')[1]:GetClass() }",
    );
    let (all, npcs, idx, near, count, nick, hp, has, active, nweps, aim, players, by_model): (
        i64,
        i64,
        i64,
        i64,
        i64,
        String,
        i64,
        bool,
        String,
        i64,
        f64,
        i64,
        String,
    ) = rt.actors()[0].eval("return unpack(state.r)").unwrap();
    assert_eq!((all, npcs, idx, near, count), (3, 1, 9, 1, 1));
    assert_eq!(nick, "STEAM_0:1:2");
    assert_eq!(hp, 85);
    assert!(has);
    assert_eq!(active, "weapon_physgun");
    assert_eq!(nweps, 2);
    assert!(aim > 0.9999, "aim vector of yaw 180: {aim}");
    assert_eq!(players, 1);
    assert_eq!(by_model, "npc_zombie");
}

#[test]
fn ents_create_binds_to_the_spawned_entity() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "ent".into(),
        priority: 0,
        source: "events.on('tick.pre', function()
                     if state.e == nil then
                         state.e = ents.Create('prop_physics')
                         state.e:SetModel('models/props_junk/wood_crate001a.mdl')
                         state.e:SetPos(Vector(1, 2, 3))
                         state.e:SetAngles(Angle(0, 45, 0))
                         state.valid_before = state.e:IsValid()
                         state.e:Spawn()
                     end
                 end)"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 7)
        .unwrap();
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    let tag = match &writes[0] {
        QueuedWrite::Spawn {
            model,
            pos,
            angles,
            class,
            tag,
            ..
        } => {
            assert_eq!(model, "models/props_junk/wood_crate001a.mdl");
            assert_eq!(*pos, Vec3::new(1.0, 2.0, 3.0));
            assert_eq!(*angles, Vec3::new(0.0, 45.0, 0.0));
            assert!(class.is_none());
            tag.clone()
        }
        other => panic!("{other:?}"),
    };
    let mut p = Payload::default();
    p.ints.push(("player", 1));
    p.ints.push(("entity", 5));
    p.strs.push(("tag", tag));
    rt.actors_mut()[0]
        .dispatch(&snap, "entity.spawned", &p, 8)
        .unwrap();
    let (before, id, valid, class): (bool, i64, bool, String) = rt.actors()[0]
        .eval(
            "return state.valid_before, state.e:EntIndex(), state.e:IsValid(), state.e:GetClass()",
        )
        .unwrap();
    assert!(!before);
    assert_eq!(id, 5);
    assert!(valid);
    assert_eq!(class, "prop_physics");
}

#[test]
fn util_trace_and_json() {
    let (rt, _) = run(
        "local tr = util.TraceLine({ start = Vector(0, 0, 0), endpos = Vector(0, 0, -100) })
         local j = util.TableToJSON({ a = 1, b = 'x', v = Vector(1, 2, 3), list = { 1, 2 } })
         local back = util.JSONToTable(j)
         state.r = { tr.Hit, tostring(tr.HitPos), back.a, back.b, tostring(back.v), #back.list, util.Base64Decode(util.Base64Encode('hello world')), util.CRC('abc') }",
    );
    let (hit, pos, a, b, v, n, b64, crc): (bool, String, i64, String, String, i64, String, String) =
        rt.actors()[0].eval("return unpack(state.r)").unwrap();
    assert!(!hit, "no world in the test snapshot");
    assert_eq!(pos, "0.000000 0.000000 -100.000000");
    assert_eq!(
        (a, b.as_str(), v.as_str(), n),
        (1, "x", "1.000000 2.000000 3.000000", 2)
    );
    assert_eq!(b64, "hello world");
    assert_eq!(crc, "891568578");
}

#[test]
fn npc_and_datatable_methods() {
    let (mut rt, writes) = run(
        "local n, e = Entity(9), Entity(5)
         e:InstallDataTable()
         e:NetworkVar('Int', 0, 'Count')
         state.seen = {}
         e:NetworkVarNotify('Count', function(_, name, old, new) state.seen[#state.seen + 1] = name .. ':' .. tostring(old) .. '>' .. tostring(new) end)
         e:SetCount(3); e:SetCount(3); e:SetCount(4)
         e:CallOnRemove('bye', function(ent, tag) state.removed = tag .. ent:EntIndex() end, 'gone')
         n:TakeDamage(30, Entity(1))
         state.r = {
            n:GetEnemy():UserID(), n:IsGoalActive(), n:GetGoalPos().x, n:GetPathDistanceToGoal(), n:GetCurGoalType(),
            n:Classify() == CLASS_ZOMBIE, n:Disposition(Entity(1)) == D_HT, e:GetCount(), #state.seen, state.seen[1], state.seen[2],
            e:GetNetworkVars().Count, e:GetMoveType() == MOVETYPE_NONE, n:GetEnemyLastKnownPos().x, e:IsConstrained(),
            e:GetPhysicsObject():GetEntity() == e, e:GetPhysicsObject():IsGravityEnabled(), select(2, Player(1):GetHull()).z, Player(1):GetMaxSpeed(),
            e:GetPhysicsObject():GetInvMass(),
         }",
    );
    let r: Vec<mlua::Value> = rt.actors()[0].eval("return state.r").unwrap();
    let s = |i: usize| match &r[i] {
        mlua::Value::String(s) => s.to_string_lossy().to_string(),
        mlua::Value::Boolean(b) => b.to_string(),
        mlua::Value::Number(n) => n.to_string(),
        mlua::Value::Integer(n) => n.to_string(),
        other => format!("{other:?}"),
    };
    assert_eq!(s(0), "1");
    assert_eq!(s(1), "true");
    assert_eq!(s(2), "400");
    assert_eq!(s(3), "400");
    assert_eq!(s(4), "2");
    assert_eq!(s(5), "true");
    assert_eq!(s(6), "true");
    assert_eq!(s(7), "4");
    assert_eq!(s(8), "2");
    assert_eq!(s(9), "Count:nil>3");
    assert_eq!(s(10), "Count:3>4");
    assert_eq!(s(11), "4");
    assert_eq!(s(12), "true");
    assert_eq!(s(14), "false");
    assert_eq!(s(15), "true");
    assert_eq!(s(16), "true");
    assert_eq!(s(17), "72");
    assert_eq!(s(18), "200");
    assert_eq!(s(19), "0.025", "GetInvMass over the body mass of 40");
    assert!(
        writes
            .iter()
            .any(|w| matches!(w, QueuedWrite::SetHealth { ent: 9, health: 20 })),
        "TakeDamage should lower the NPC's health through the health op: {writes:?}"
    );
    let mut payload = Payload::default();
    payload.ints.push(("entity", 5));
    rt.actors_mut()[0]
        .dispatch(&snapshot(), "entity.removed", &payload, 8)
        .unwrap();
    let removed: String = rt.actors()[0].eval("return state.removed").unwrap();
    assert_eq!(removed, "gone5");
}

#[test]
fn physics_init_shapes_travel_with_the_spawn_or_rebuild_the_body() {
    let (_, writes) = run("local e = ents.Create(\"prop_physics\")
         e:SetModel(\"models/props_junk/wood_crate001a.mdl\")
         e:PhysicsInitBox(Vector(-8, -8, -8), Vector(8, 8, 8))
         e:Spawn()
         Entity(7):PhysicsInitSphere(12)
         Entity(7):PhysicsDestroy()
         Entity(7):PhysicsInit(SOLID_VPHYSICS)
         Entity(7):PhysWake()");
    assert!(
        matches!(&writes[0], QueuedWrite::Spawn { physics: Some(gm_server::PhysShape::Box { mins, maxs }), .. }
            if *mins == Vec3::splat(-8.0) && *maxs == Vec3::splat(8.0)),
        "{:?}",
        writes[0]
    );
    assert!(matches!(
        &writes[1],
        QueuedWrite::EntityPhysics { ent: 7, shape: gm_server::PhysShape::Sphere { radius } } if *radius == 12.0
    ));
    assert!(matches!(
        &writes[2],
        QueuedWrite::EntityPhysics {
            ent: 7,
            shape: gm_server::PhysShape::None
        }
    ));
    assert!(matches!(
        &writes[3],
        QueuedWrite::EntityPhysics {
            ent: 7,
            shape: gm_server::PhysShape::Model
        }
    ));
    assert!(matches!(
        &writes[4],
        QueuedWrite::SetAsleep {
            ent: 7,
            asleep: false
        }
    ));
}

#[test]
fn animator_writes_carry_sequence_cycle_rate_and_gesture_layer() {
    let (_, writes) = run("Entity(7):SetSequence(\"walk\")
         Entity(7):SetCycle(0.25)
         Entity(7):SetPlaybackRate(2)
         Entity(7):ResetSequenceInfo()
         Entity(7):AddGestureSequence(\"wave\", false)
         Entity(7):SetLayerWeight(0, 0.5)
         Entity(7):SetLayerWeight(3, 0.5)
         Entity(7):RemoveAllGestures()");
    assert!(matches!(&writes[0], QueuedWrite::EntitySequence { ent: 7, name } if name == "walk"));
    assert!(matches!(&writes[1], QueuedWrite::EntityCycle { ent: 7, cycle } if *cycle == 0.25));
    assert!(matches!(&writes[2], QueuedWrite::EntityPlaybackRate { ent: 7, rate } if *rate == 2.0));
    assert!(matches!(
        &writes[3],
        QueuedWrite::EntityResetSequence { ent: 7 }
    ));
    assert!(matches!(
        &writes[4],
        QueuedWrite::EntityGesture { ent: 7, op: GestureOp::Add { sequence, autokill: false } } if sequence == "wave"
    ));
    assert!(matches!(
        &writes[5],
        QueuedWrite::EntityGesture { ent: 7, op: GestureOp::Set(p) } if p.weight == Some(0.5) && p.cycle.is_none()
    ));
    // layer ids other than 0 do not exist: nothing queued, the remove follows directly
    assert!(matches!(
        &writes[6],
        QueuedWrite::EntityGesture {
            ent: 7,
            op: GestureOp::Remove
        }
    ));
    assert_eq!(writes.len(), 7);
}

#[test]
fn pose_parameter_writes_carry_name_and_value() {
    let (_, writes) = run("Entity(7):SetPoseParameter(\"aim_yaw\", 30)
         Entity(7):ClearPoseParameters()");
    assert!(matches!(
        &writes[0],
        QueuedWrite::EntityPose { ent: 7, name, value } if name == "aim_yaw" && *value == 30.0
    ));
    assert!(matches!(
        &writes[1],
        QueuedWrite::EntityPoseClear { ent: 7 }
    ));
}

#[test]
fn activity_enum_round_trips_and_gestures_by_activity_need_a_tagged_sequence() {
    let (rt, writes) = run("state.r = {
            ACT_RESET, ACT_IDLE, ACT_GMOD_SHOWOFF_DUCK_02, ACT_INVALID,
            util.GetActivityIDByName(\"ACT_WALK\"), util.GetActivityIDByName(\"nope\"),
            util.GetActivityNameByID(ACT_RUN), util.GetActivityNameByID(123456),
            Entity(7):SelectWeightedSequence(ACT_IDLE), Entity(7):AddGesture(ACT_IDLE),
            Entity(7):IsPlayingGesture(ACT_IDLE),
         }");
    let (reset, idle, duck, invalid, walk, nope, run_name, none, sel, add, playing): (
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        String,
        String,
        i32,
        i32,
        bool,
    ) = rt.actors()[0].eval("return unpack(state.r)").unwrap();
    assert_eq!((reset, idle, duck, invalid), (0, 1, 2044, -1));
    assert_eq!((walk, nope), (6, -1));
    assert_eq!((run_name.as_str(), none.as_str()), ("ACT_RUN", ""));
    // no model data in this harness: nothing tagged, nothing queued
    assert_eq!((sel, add, playing), (-1, -1, false));
    assert!(writes.is_empty());
}

#[test]
fn collision_bounds_write_carries_both_corners_sorted_by_the_server() {
    let (rt, writes) = run("Entity(7):SetCollisionBounds(Vector(-1, -2, -3), Vector(1, 2, 3))
         Entity(7):SetCollisionBoundsWS(Entity(7):GetPos() + Vector(-5, -5, 0), Entity(7):GetPos() + Vector(5, 5, 10))
         local mn, mx = Entity(5):GetCollisionBounds()
         state.r = { tostring(mn), tostring(mx), tostring(Entity(5):OBBCenter()) }");
    assert!(
        matches!(&writes[0], QueuedWrite::EntityBounds { ent: 7, mins, maxs }
        if *mins == glam::Vec3::new(-1.0, -2.0, -3.0) && *maxs == glam::Vec3::new(1.0, 2.0, 3.0))
    );
    assert!(
        matches!(&writes[1], QueuedWrite::EntityBounds { ent: 7, mins, maxs }
        if *mins == glam::Vec3::new(-5.0, -5.0, 0.0) && *maxs == glam::Vec3::new(5.0, 5.0, 10.0))
    );
    // zero bounds in the snapshot fall back to the model box (none here): still two vectors
    let (mn, mx, c): (String, String, String) =
        rt.actors()[0].eval("return unpack(state.r)").unwrap();
    assert!(!mn.is_empty() && !mx.is_empty() && !c.is_empty());
}

#[test]
fn spawn_counts_and_surface_lookups_come_from_the_snapshot() {
    let (rt, _) = run("local ply = Entity(5):GetOwner()
         state.r = { ply:GetCount(\"props\"), ply:GetCount(\"npcs\"), ply:CheckLimit(\"props\"), ply:IsSprinting(),
                     util.GetSurfaceIndex(\"metal\"), util.GetSurfacePropName(0), util.GetSurfaceData(0) == nil,
                     tostring(game.StartSpot()) }");
    let (props, npcs, ok, sprint, idx, name, nodata, spot): (
        i32,
        i32,
        bool,
        bool,
        i32,
        String,
        bool,
        String,
    ) = rt.actors()[0].eval("return unpack(state.r)").unwrap();
    assert_eq!((props, npcs, ok, sprint), (3, 0, true, false));
    // no surface table / spawn point in this harness
    assert_eq!((idx, name.as_str(), nodata), (-1, "", true));
    assert!(!spot.is_empty());
}

#[test]
fn ammo_table_network_string_pool_weapon_defs_and_addon_lists() {
    let (rt, _) = run("game.AddAmmoType({ name = \"MyAmmo\", dmgtype = 2 })
         local d = __weapon_def(\"weapon_pistol\")
         local a, b = util.AddNetworkString(\"a\"), util.AddNetworkString(\"b\")
         state.r = { game.GetAmmoID(\"ar2\"), game.GetAmmoName(3), game.GetAmmoID(\"MyAmmo\"), game.GetAmmoData(38).dmgtype,
                     game.GetAmmoID(\"nope\"), a ~= b and tonumber(a) == util.NetworkStringToID(\"a\"),
                     util.NetworkIDToString(util.NetworkStringToID(\"b\")), util.NetworkStringToID(\"zz\"),
                     d ~= nil and type(d.slot) == \"number\" and d.ammo1 ~= \"\", #engine.GetAddons(), engine.GetAddons()[1].title }");
    let (ar2, pistol, mine, dmg, nope, pooled, b, zz, def, n, title): (
        i32,
        String,
        i32,
        i32,
        i32,
        bool,
        String,
        i32,
        bool,
        i32,
        String,
    ) = rt.actors()[0].eval("return unpack(state.r)").unwrap();
    assert_eq!(
        (ar2, pistol.as_str(), mine, dmg, nope),
        (1, "Pistol", 38, 2, -1)
    );
    assert!(pooled && b == "b" && zz == 0);
    assert!(def);
    assert_eq!((n, title.as_str()), (1, "glua"));
}
