//! net, ConVar, concommand, file, CTakeDamageInfo and the GM hook bridge.
use glam::Vec3;
use gm_script::api::{Payload, QueuedWrite};
use gm_script::snapshot::PlayerSnap;
use gm_script::{ActorConfig, ActorSpec, Runtime, Snapshot};
use std::sync::Arc;

fn snapshot() -> Arc<Snapshot> {
    let mut s = Snapshot::default();
    s.cvars.insert("sv_gravity".into(), "600".into());
    s.mounted = vec!["garrysmod".into(), "hl2".into()];
    s.entities.insert(
        5,
        gm_script::snapshot::EntitySnap {
            name: String::new(),
            id: 5,
            model: "models/props_c17/oildrum001.mdl".into(),
            pos: Vec3::new(0.0, 0.0, 0.0),
            angles: Vec3::ZERO,
            frozen: false,
            skin: 0,
            body: String::new(),
            owner: 1,
            class: "prop_physics".into(),
            health: 0,
            max_health: 0,
            velocity: Vec3::ZERO,
            ang_velocity: Vec3::new(1.0, 2.0, 3.0),
            mass: 40.0,
            damping: (0.1, 0.2),
            inertia: glam::Vec3::ZERO,
            asleep: false,
            drag: false,
            parent: 0,
            color: [10, 20, 30, 40],
            material: "models/debug/debugwhite".into(),
            render_mode: 1,
            render_fx: 0,
            model_scale: 1.0,
            transmit_block: Vec::new(),
            transmit_with_parent: false,
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
            bone_positions: vec![],
            sub_materials: vec![],
            flex: Some((vec![0.5], 1.0)),
            eye_target: None,
            weapon: "weapon_smg1".into(),
            blood: 0,
            npc_target_seen: 200,
            npc_target_first_seen: 0,
            npc_ai_state: 0,
            npc_ai_schedule: None,
            npc_ai_task: None,
            npc_ai_task_status: 0,
            npc_ai_scripted: false,
            npc_ai_conditions: Vec::new(),
            npc_ai_squad: None,
            npc_ai_proficiency: 0,
            npc_ai_expression: String::new(),
            npc_ai_behavior: None,
            npc_ai_goal_stage: None,
            npc_ai_script_stage: None,
            npc_ai_save_position: None,
            npc_ai_fail_schedule: None,
            npc_ai_unforgettable: false,
            npc_stuck: 0,
            npc_chase_reach: 0.0,
            npc_max_yaw: 0.0,
            npc_ideal_yaw: None,
            npc_move_yaw_locked: false,
            npc_view_offset: None,
            npc_relation: None,
            npc_ignore: Vec::new(),
            npc_move_speed: 0.0,
            npc_next_goal: None,
            npc_caps: 0,
            nextbot: None,
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
        6,
        gm_script::snapshot::EntitySnap {
            name: String::new(),
            id: 6,
            model: "models/props_c17/oildrum001.mdl".into(),
            pos: Vec3::new(10.0, 20.0, 30.0),
            angles: Vec3::ZERO,
            frozen: false,
            skin: 0,
            body: String::new(),
            owner: 1,
            class: "prop_physics".into(),
            health: 0,
            max_health: 0,
            velocity: Vec3::ZERO,
            ang_velocity: Vec3::new(1.0, 2.0, 3.0),
            mass: 40.0,
            damping: (0.1, 0.2),
            inertia: glam::Vec3::ZERO,
            asleep: false,
            drag: false,
            parent: 5,
            color: [10, 20, 30, 40],
            material: "models/debug/debugwhite".into(),
            render_mode: 1,
            render_fx: 0,
            model_scale: 1.0,
            transmit_block: Vec::new(),
            transmit_with_parent: false,
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
            bone_positions: vec![],
            sub_materials: vec![],
            flex: Some((vec![0.5], 1.0)),
            eye_target: None,
            weapon: "weapon_smg1".into(),
            blood: 0,
            npc_target_seen: 200,
            npc_target_first_seen: 0,
            npc_ai_state: 0,
            npc_ai_schedule: None,
            npc_ai_task: None,
            npc_ai_task_status: 0,
            npc_ai_scripted: false,
            npc_ai_conditions: Vec::new(),
            npc_ai_squad: None,
            npc_ai_proficiency: 0,
            npc_ai_expression: String::new(),
            npc_ai_behavior: None,
            npc_ai_goal_stage: None,
            npc_ai_script_stage: None,
            npc_ai_save_position: None,
            npc_ai_fail_schedule: None,
            npc_ai_unforgettable: false,
            npc_stuck: 0,
            npc_chase_reach: 0.0,
            npc_max_yaw: 0.0,
            npc_ideal_yaw: None,
            npc_move_yaw_locked: false,
            npc_view_offset: None,
            npc_relation: None,
            npc_ignore: Vec::new(),
            npc_move_speed: 0.0,
            npc_next_goal: None,
            npc_caps: 0,
            nextbot: None,
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
    s.players.insert(
        1,
        PlayerSnap {
            id: 1,
            counts: vec![("props".to_string(), 3)],
            origin: Vec3::ZERO,
            velocity: Vec3::ZERO,
            view_angles: Vec3::ZERO,
            held: None,
            carry: Some(777),
            water_level: 2,
            ping: 30.0,
            loss: 2.5,
            timeout: 0.5,
            addr: "127.0.0.1:27005".into(),
            alive: true,
            health: 100,
            armor: 0,
            fov: 0.0,
            max_health: 100,
            identity: Some("STEAM_0:1:2".into()),
            model: String::new(),
            skin: 0,
            body: String::new(),
            weapons: vec!["weapon_pistol".into(), "weapon_smg1".into()],
            active_weapon: String::new(),
            previous_weapon: "weapon_smg1".into(),
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
            ragdoll: 0,
            use_target: 5,
            flashlight: true,
            punch: Vec3::ZERO,
            punch_vel: Vec3::ZERO,
            ladder: false,
            suit: false,
            walk_speed: 200.0,
            ladder_speed: 200.0,
            lagged_movement: 1.0,
            sprint_enabled: true,
            run_speed: 400.0,
            slow_walk_speed: 100.0,
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
            frags: 0,
            deaths: 0,
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

fn runtime(source: &str) -> Runtime {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "srv".into(),
        priority: 0,
        source: source.into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    rt
}

#[test]
fn net_messages_round_trip_through_the_host_transport() {
    let mut rt = runtime(
        "net.Receive('hello', function(len, ply)
             state.got = { net.ReadString(), net.ReadUInt(8), net.ReadBool(), tostring(net.ReadVector()), net.ReadEntity():EntIndex(), net.ReadTable().k, ply:UserID() }
         end)
         events.on('tick.pre', function()
             util.AddNetworkString('hello')
             net.Start('hello')
             net.WriteString('hi') net.WriteUInt(200, 8) net.WriteBool(true) net.WriteVector(Vector(1, 2, 3)) net.WriteEntity(NULL) net.WriteTable({ k = 'v' })
             net.Send(Player(1))
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let out = rt.drain_net();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, 1);
    assert_eq!(out[0].1, "hello");
    // deliver it back as an incoming message
    let mut p = Payload::default();
    p.ints.push(("player", 1));
    p.net = Some(("hello".into(), out[0].2.clone()));
    rt.actors_mut()[0]
        .dispatch(&snap, "net.message", &p, 2)
        .unwrap();
    let (s, n, b, v, e, k, who): (String, i64, bool, String, i64, String, i64) =
        rt.actors()[0].eval("return unpack(state.got)").unwrap();
    assert_eq!((s.as_str(), n, b), ("hi", 200, true));
    assert_eq!(v, "1.000000 2.000000 3.000000");
    assert_eq!(e, 0);
    assert_eq!(k, "v");
    assert_eq!(who, 1);
}

#[test]
fn legacy_nwvars_reach_the_client_as_networkedvar_messages() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             if state.set then return end
             state.set = true
             Entity(5):SetNWInt('hp', 7)
             Entity(5):SetNWString('long', string.rep('x', 300))
             Entity(5):SetNWVector('at', Vector(1, -2.5, 3))
         end)",
    );
    let snap = snapshot();
    // the cycle sends a changed var once CurTime has passed its entity's base (0.5..1 s)
    for tick in [1u64, 200] {
        let mut later = (*snap).clone();
        later.tick = tick;
        rt.actors_mut()[0]
            .dispatch(&Arc::new(later), "tick.pre", &Payload::default(), tick)
            .unwrap();
    }
    let out = rt.drain_net();
    let nw: Vec<_> = out
        .iter()
        .filter(|m| m.0 == 0 && m.1 == "NetworkedVar")
        .collect();
    assert!(nw.len() >= 3, "{out:?}");
    // the client realm stores them under the entity
    let mut client = Runtime::client();
    client
        .load(ActorSpec {
            addon: "glua".into(),
            name: "cl".into(),
            priority: 0,
            source: "".into(),
            config: ActorConfig {
                cpu_budget: std::time::Duration::from_millis(50),
                ..Default::default()
            },
        })
        .unwrap();
    for m in &nw {
        let mut p = Payload::default();
        p.ints.push(("player", 0));
        p.net = Some((m.1.clone(), m.2.clone()));
        client.actors_mut()[0]
            .dispatch(&snap, "net.message", &p, 41)
            .unwrap();
    }
    let (hp, len, at): (i64, i64, String) = client.actors()[0]
        .eval(
            "return Entity(5):GetNWInt('hp'), #Entity(5):GetNWString('long'), tostring(Entity(5):GetNWVector('at'))",
        )
        .unwrap();
    assert_eq!(
        (hp, len, at.as_str()),
        (7, 200, "1.000000 -2.500000 3.000000")
    );
}

#[test]
fn convars_read_the_snapshot_and_hooks_bridge_events() {
    let mut rt = runtime(
        "state.log = {}
         hook.Add('Initialize', 'a', function() table.insert(state.log, 'init') end)
         hook.Add('PlayerHurt', 'a', function(ply, att, hp, dmg) table.insert(state.log, 'hurt ' .. ply:UserID() .. ' ' .. dmg) end)
         hook.Add('EntityTakeDamage', 'a', function(ply, dmg) if dmg:GetDamage() > 50 then return true end dmg:ScaleDamage(0.5) end)
         hook.Add('PlayerSay', 'a', function(ply, text) if text == 'drop' then return '' end return text:upper() end)
         hook.Add('KeyPress', 'a', function(ply, key) table.insert(state.log, 'key ' .. key) end)
         events.on('tick.pre', function()
             local g = GetConVar('sv_gravity')
             state.cv = { g:GetInt(), g:GetFloat(), g:GetBool(), GetConVar('nope') == nil, cvars.Number('sv_gravity', 1) }
             concommand.Add('say_hi', function(ply, cmd, args) state.cmd = cmd .. ':' .. table.concat(args, ',') end)
             RunConsoleCommand('say_hi', 'a', 2)
             local d = DamageInfo() d:SetDamage(10) d:SetDamageType(DMG_BULLET) d:AddDamage(5)
             state.dmg = { d:GetDamage(), d:IsBulletDamage(), d:IsFallDamage() }
         end)",
    );
    let snap = snapshot();
    let a = &mut rt.actors_mut()[0];
    a.dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let mut p = Payload::default();
    p.ints.push(("player", 1));
    p.floats.push(("amount", 30.0));
    p.ints.push(("type", 2));
    a.dispatch(&snap, "player.damaged", &p, 1).unwrap();
    let mut k = Payload::default();
    k.ints.push(("player", 1));
    k.strs.push(("key", "IN_JUMP".into()));
    a.dispatch(&snap, "player.key_press", &k, 1).unwrap();
    // requests: EntityTakeDamage blocks over 50, halves otherwise
    let mut big = Payload::default();
    big.ints.push(("player", 1));
    big.floats.push(("amount", 80.0));
    big.ints.push(("type", 2));
    let blocked = a.dispatch_reply(&snap, "player.damage", &big, 1).unwrap();
    let mut small = Payload::default();
    small.ints.push(("player", 1));
    small.floats.push(("amount", 20.0));
    small.ints.push(("type", 2));
    let halved = a.dispatch_reply(&snap, "player.damage", &small, 1).unwrap();
    let mut chat = Payload::default();
    chat.ints.push(("player", 1));
    chat.strs.push(("text", "drop".into()));
    let dropped = a.dispatch_reply(&snap, "player.chat", &chat, 1).unwrap();
    chat.strs.clear();
    chat.strs.push(("text", "hello".into()));
    let replaced = a.dispatch_reply(&snap, "player.chat", &chat, 1).unwrap();
    eprintln!("blocked {blocked:?} halved {halved:?} dropped {dropped:?} replaced {replaced:?}");
    assert!(matches!(blocked, gm_script::Reply::Cancel));
    assert!(matches!(halved, gm_script::Reply::Number(x) if (x - 10.0).abs() < 1e-6));
    assert!(matches!(dropped, gm_script::Reply::Cancel));
    assert!(matches!(replaced, gm_script::Reply::Text(ref t) if t == "HELLO"));
    let (log, cv, cmd, dmg): (String, Vec<mlua::Value>, String, Vec<mlua::Value>) = rt.actors()[0]
        .eval("return table.concat(state.log, '|'), state.cv, state.cmd, state.dmg")
        .unwrap();
    assert_eq!(log, "init|hurt 1 30|key 2");
    let num = |v: &mlua::Value| match v {
        mlua::Value::Integer(i) => *i as f64,
        mlua::Value::Number(n) => *n,
        mlua::Value::Boolean(b) => f64::from(u8::from(*b)),
        _ => f64::NAN,
    };
    assert_eq!(
        cv.iter().map(num).collect::<Vec<_>>(),
        vec![600.0, 600.0, 1.0, 1.0, 600.0]
    );
    assert_eq!(cmd, "say_hi:a,2");
    assert_eq!(
        format!("{dmg:?}"),
        "[Integer(15), Boolean(true), Boolean(false)]"
    );
}

#[test]
fn gm_hooks_bridge_more_events_and_defaults() {
    let mut rt = runtime(
        "state.log = {}
         hook.Add('PlayerInitialSpawn', 'a', function(ply) table.insert(state.log, 'initial ' .. ply:UserID()) end)
         hook.Add('PlayerTick', 'a', function(ply) table.insert(state.log, 'tick') end)
         hook.Add('EntityNetworkedVarChanged', 'a', function(ent, name, old, new)
             table.insert(state.log, 'nw ' .. name .. ' ' .. tostring(old) .. '>' .. tostring(new))
         end)
         hook.Add('PlayerShouldTakeDamage', 'a', function(ply, att) return false end)
         hook.Add('PreRegisterSENT', 'a', function(ent, class) if class == 'blocked_ent' then return false end end)
         events.on('tick.pre', function()
             Player(1):SetNWInt('score', 7)
             Player(1):SetNWInt('score', 7)
             state.desc = hook.Run('GetGameDescription')
             state.suicide = hook.Run('CanPlayerSuicide', Player(1))
             scripted_ents.Register({ Type = 'anim', Model = 'models/props_c17/oildrum001.mdl', PrintName = 'B' }, 'blocked_ent')
             scripted_ents.Register({ Type = 'anim', Model = 'models/props_c17/oildrum001.mdl', PrintName = 'O' }, 'ok_ent')
             state.reg = { scripted_ents.Get('blocked_ent') == nil, scripted_ents.Get('ok_ent') ~= nil }
         end)",
    );
    let snap = snapshot();
    let a = &mut rt.actors_mut()[0];
    a.dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    a.dispatch(&snap, "tick.pre", &Payload::default(), 2)
        .unwrap();
    let mut dmg = Payload::default();
    dmg.ints.push(("player", 1));
    dmg.floats.push(("amount", 20.0));
    dmg.ints.push(("type", 2));
    let refused = a.dispatch_reply(&snap, "player.damage", &dmg, 2).unwrap();
    assert!(matches!(refused, gm_script::Reply::Cancel));
    let log: Vec<String> = a.eval("return state.log").unwrap();
    assert_eq!(
        log.iter().filter(|l| l.starts_with("initial")).count(),
        1,
        "PlayerInitialSpawn once per player: {log:?}"
    );
    assert_eq!(log.iter().filter(|l| *l == "tick").count(), 2, "{log:?}");
    assert_eq!(
        log.iter().filter(|l| l.starts_with("nw")).count(),
        1,
        "{log:?}"
    );
    assert!(log.contains(&"nw score nil>7".to_string()), "{log:?}");
    let desc: String = a.eval("return state.desc").unwrap();
    assert_eq!(desc, "Sandbox");
    let suicide: bool = a.eval("return state.suicide").unwrap();
    assert!(suicide);
    let reg: Vec<bool> = a.eval("return state.reg").unwrap();
    assert_eq!(reg, vec![true, true]);
}

#[test]
fn navmesh_over_a_flat_recast_mesh() {
    let mut g = gm_nav::Geometry::default();
    let sq = [
        Vec3::new(-1024.0, -1024.0, 0.0),
        Vec3::new(1024.0, -1024.0, 0.0),
        Vec3::new(1024.0, 1024.0, 0.0),
        Vec3::new(-1024.0, 1024.0, 0.0),
    ];
    g.push_polygon(&sq);
    let mut rev = sq;
    rev.reverse();
    g.push_polygon(&rev);
    g.mins = Vec3::new(-1024.0, -1024.0, -8.0);
    g.maxs = Vec3::new(1024.0, 1024.0, 8.0);
    let mesh = gm_nav::NavMesh::from_geometry(&g, &gm_nav::NavParams::default()).unwrap();
    let mut rt = runtime(
        "events.on('tick.pre', function()
             if not navmesh.IsLoaded() then return end
             local a = navmesh.GetNearestNavArea(Vector(100, 100, 40))
             local found = navmesh.Find(Vector(1000, 1000, 0), 300, 18, 100)
             local gz = navmesh.GetGroundHeight(Vector(500, 500, 900))
             state.z = { a:GetCenter().z, a:GetZ(Vector(300, 300, 500)), gz }
             state.r = {
                 navmesh.IsLoaded(), navmesh.GetNavAreaCount() > 0, a:IsValid(), math.abs(a:GetCenter().z) <= 8,
                 math.abs(a:GetZ(Vector(300, 300, 500))) <= 8, #found > 0, found[1]:GetID() == navmesh.GetNavAreaByID(found[1]:GetID()):GetID(),
                 math.abs(gz) <= 8, a:Contains(a:GetCenter()), navmesh.GetNearestNavArea(Vector(5000, 5000, 0), false, 100) == NULL,
                 #navmesh.GetAllNavAreas() == #navmesh.FindInBox(Vector(-2000, -2000, -10), Vector(2000, 2000, 10)),
             }
         end)",
    );
    let mut snap = (*snapshot()).clone();
    snap.nav = gm_script::snapshot::NavRef(&mesh as *const _);
    let snap = Arc::new(snap);
    let a = &mut rt.actors_mut()[0];
    a.dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let z: Vec<f32> = a.eval("return state.z").unwrap();
    let r: Vec<bool> = a.eval("return state.r").unwrap();
    // the mesh surface sits one voxel (cell_height 4) above the geometry
    assert_eq!(r, vec![true; 11], "{r:?} z {z:?}");
    let empty = snapshot();
    a.dispatch(&empty, "tick.pre", &Payload::default(), 2)
        .unwrap();
    let loaded: bool = a.eval("return navmesh.IsLoaded()").unwrap();
    assert!(!loaded);
}

#[test]
fn libs_team_list_numpad_duplicator() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             team.SetUp(1, 'Red', Color(255, 0, 0))
             Player(1):SetTeam(1)
             list.Set('Desktop Widgets', 'clock', { x = 1 })
             numpad.Register('Toggle', function(ply, ent, on) state.np = (state.np or '') .. ent .. tostring(on) end)
             local id = numpad.OnDown(Player(1), KEY_PAD_1, 'Toggle', 'door', true)
             numpad.OnUp(Player(1), KEY_PAD_1, 'Toggle', 'door', false)
             numpad.Activate(Player(1), KEY_PAD_1)
             numpad.Deactivate(Player(1), KEY_PAD_1)
             numpad.Remove(id)
             numpad.Activate(Player(1), KEY_PAD_1)
             duplicator.StoreEntityModifier(Entity(5), 'colour', { r = 1 })
             local dupe = duplicator.Copy(Entity(5))
             local hit, far = util.IntersectRayWithSphere(Vector(-10, 0, 0), Vector(1, 0, 0), Vector(0, 0, 0), 2)
             player_manager.AddValidModel('kleiner', 'models/player/kleiner.mdl')
             state.r = {
                 team.GetName(1), team.NumPlayers(1), team.GetPlayers(1)[1]:UserID(), list.Get('Desktop Widgets').clock.x,
                 state.np, dupe.Entities[5].Class, dupe.Entities[5].EntityMods.colour.r, hit, far,
                 player_manager.TranslatePlayerModel('kleiner'), math.ease.InQuad(0.5), HexToColor('#ff0080').g, STNDRD(22),
                 system.IsLinux(), CUserCmd ~= nil, Tool ~= nil,
             }
         end)",
    );
    let snap = snapshot();
    let a = &mut rt.actors_mut()[0];
    a.dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<String> = a.eval("return { tostring(state.r[1]), tostring(state.r[2]), tostring(state.r[3]), tostring(state.r[4]), tostring(state.r[5]), tostring(state.r[6]), tostring(state.r[7]), tostring(state.r[8]), tostring(state.r[9]), tostring(state.r[10]), tostring(state.r[11]), tostring(state.r[12]), tostring(state.r[13]), tostring(state.r[14]) }").unwrap();
    assert_eq!(
        r,
        vec![
            "Red",
            "1",
            "1",
            "1",
            "doortruedoorfalse",
            "prop_physics",
            "1",
            "8",
            "12",
            "models/player/kleiner.mdl",
            "0.25",
            "0",
            "nd",
            "true"
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>()
    );
}

#[test]
fn tail_libraries_stack_sound_cookie_gamemode() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local st = util.Stack()
             st:Push(1) st:Push(2) st:Push(3)
             local top, size = st:Top(), st:Size()
             local popped = st:Pop()
             local snd = CreateSound(Entity(5), 'ambient/wind1.wav')
             snd:PlayEx(0.5, 90)
             cookie.Set('seen', 3)
             properties.Add('bonk', { MenuLabel = 'Bonk', Order = 1 })
             gamemode.Register({ Name = 'Mine' }, 'mine', 'base')
             local sched = ai_schedule.New('patrol')
             sched:EngTask('TASK_WAIT', 2)
             state.r = { top, size, popped, st:Size(), snd:IsPlaying() and 1 or 0, snd:GetPitch(), cookie.GetNumber('seen', 0),
                 properties.List.bonk.Order, gamemode.Get('mine').Name == 'Mine' and 1 or 0, sched:NumTasks(), properties.CanBeTargeted(Entity(5), Player(1)) and 1 or 0 }
         end)",
    );
    let snap = snapshot();
    let a = &mut rt.actors_mut()[0];
    a.dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<f64> = a.eval("return state.r").unwrap();
    assert_eq!(
        r,
        vec![3.0, 3.0, 3.0, 2.0, 1.0, 90.0, 3.0, 1.0, 1.0, 1.0, 1.0]
    );
}

#[test]
fn player_ammo_and_clips_over_the_inventory() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local ply = Player(1)
             local w = ply:GetWeapon('weapon_pistol')
             state.r = { ply:GetAmmoCount('Pistol'), ply:GetAmmoCount('AR2'), ply:GetAmmo().SMG1, w:Clip1(), w:Clip2(), ply:GetWeapon('weapon_smg1'):Clip2() }
             ply:SetAmmo(7, 'Pistol')
             ply:GiveAmmo(5, 'SMG1')
             ply:RemoveAmmo(3, 'SMG1')
             w:SetClip1(2)
             ply:StripWeapon('weapon_smg1')
             ply:StripWeapons()
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<i64> = rt.actors()[0].eval("return state.r").unwrap();
    assert_eq!(r, vec![20, 0, 45, 18, 0, 1]);
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(
        matches!(&writes[0], QueuedWrite::PlayerSetAmmo { player: 1, ammo, count: 7, add: false } if ammo == "Pistol")
    );
    assert!(
        matches!(&writes[1], QueuedWrite::PlayerSetAmmo { ammo, count: 5, add: true, .. } if ammo == "SMG1")
    );
    assert!(matches!(
        &writes[2],
        QueuedWrite::PlayerSetAmmo {
            count: -3,
            add: true,
            ..
        }
    ));
    assert!(
        matches!(&writes[3], QueuedWrite::PlayerSetClip { class, slot: 0, count: 2, .. } if class == "weapon_pistol")
    );
    assert!(
        matches!(&writes[4], QueuedWrite::PlayerStripWeapon { class: Some(c), .. } if c == "weapon_smg1")
    );
    assert!(matches!(
        &writes[5],
        QueuedWrite::PlayerStripWeapon { class: None, .. }
    ));
}

#[test]
fn physobj_mass_angular_velocity_damping_over_the_body() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local ph = Entity(5):GetPhysicsObject()
             local w = ph:GetAngleVelocity()
             local l, a = ph:GetDamping()
             local f, t = ph:CalculateForceOffset(Vector(0, 0, 10), Entity(5):GetPos() + Vector(2, 0, 0))
             local al = ph:AlignAngles(Angle(0, 0, 0), Angle(0, 90, 0))
             state.r = { ph:GetMass(), w.x, w.y, w.z, l, a, ph:IsAsleep() and 1 or 0, f.z, t.y, math.Round(al.y) }
             ph:SetMass(80)
             ph:SetDamping(0.5, 0.6)
             ph:SetAngleVelocity(Vector(0, 0, 90))
             ph:Sleep()
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<f64> = rt.actors()[0].eval("return state.r").unwrap();
    assert_eq!(r[..4], [40.0, 1.0, 2.0, 3.0]);
    assert!((r[4] - 0.1).abs() < 1e-5 && (r[5] - 0.2).abs() < 1e-5);
    assert_eq!(r[6..], [0.0, 10.0, -20.0, 90.0]);
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(matches!(&writes[0], QueuedWrite::SetMass { ent: 5, mass } if *mass == 80.0));
    assert!(
        matches!(&writes[1], QueuedWrite::SetDamping { ent: 5, linear, angular } if *linear == 0.5 && *angular == 0.6)
    );
    assert!(
        matches!(&writes[2], QueuedWrite::SetVelocity { ent: 5, angular, .. } if angular.z == 90.0)
    );
    assert!(matches!(
        &writes[3],
        QueuedWrite::SetAsleep {
            ent: 5,
            asleep: true
        }
    ));
}

#[test]
fn parenting_reads_the_link_and_queues_set_parent() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local c, p = Entity(6), Entity(5)
             local lp = c:GetLocalPos()
             state.r = { c:GetParent():EntIndex(), #p:GetChildren(), p:GetChildren()[1]:EntIndex(), lp.x, lp.y, lp.z,
                 IsValid(p:GetParent()) and 1 or 0, c:GetParentWorldTransformMatrix():GetTranslation().x }
             p:SetParent(Player(1))
             c:SetParent(nil)
             c:SetLocalPos(Vector(1, 2, 3))
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<f64> = rt.actors()[0].eval("return state.r").unwrap();
    assert_eq!(r, vec![5.0, 1.0, 6.0, 10.0, 20.0, 30.0, 0.0, 0.0]);
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(matches!(
        &writes[0],
        QueuedWrite::SetParent {
            ent: 5,
            parent: Some(1)
        }
    ));
    assert!(matches!(
        &writes[1],
        QueuedWrite::SetParent {
            ent: 6,
            parent: None
        }
    ));
    assert!(
        matches!(&writes[2], QueuedWrite::SetPos { ent: 6, pos } if *pos == Vec3::new(1.0, 2.0, 3.0)),
        "local pos of a parented child is parent-relative: {:?}",
        writes[2]
    );
}

#[test]
fn render_state_reads_the_prop_and_queues_parts() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local e = Entity(5)
             local c = e:GetColor()
             state.r = { c.r, c.g, c.b, c.a, e:GetMaterial(), e:GetRenderMode(), e:GetRenderFX() }
             e:SetColor(Color(1, 2, 3, 4))
             e:SetMaterial('models/wireframe')
             e:SetRenderMode(RENDERMODE_TRANSALPHA or 4)
             e:SetRenderFX(kRenderFxPulseSlow or 1)
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<String> = rt.actors()[0]
        .eval("local o = {} for i, v in ipairs(state.r) do o[i] = tostring(v) end return o")
        .unwrap();
    assert_eq!(
        r,
        ["10", "20", "30", "40", "models/debug/debugwhite", "1", "0"]
    );
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(matches!(
        &writes[0],
        QueuedWrite::SetRender {
            ent: 5,
            color: Some([1, 2, 3, 4]),
            material: None,
            ..
        }
    ));
    assert!(
        matches!(&writes[1], QueuedWrite::SetRender { material: Some(m), color: None, .. } if m == "models/wireframe")
    );
    assert!(matches!(
        &writes[2],
        QueuedWrite::SetRender { mode: Some(4), .. }
    ));
    assert!(matches!(
        &writes[3],
        QueuedWrite::SetRender { fx: Some(1), .. }
    ));
}

fn oildrum_models() -> std::collections::BTreeMap<String, gm_server::ModelScriptData> {
    let mut models = std::collections::BTreeMap::new();
    models.insert(
        "models/props_c17/oildrum001.mdl".to_string(),
        gm_server::ModelScriptData {
            body_groups: vec![("body".into(), 1), ("hat".into(), 3)],
            skins: 2,
            bones: vec![
                "ValveBiped.Bip01_Pelvis".into(),
                "ValveBiped.Bip01_Head1".into(),
            ],
            bone_parents: vec![-1, 0],
            phys_bones: vec!["ValveBiped.Bip01_Pelvis".into()],
            bone_bind: vec![
                (Vec3::new(0.0, 0.0, 10.0), Vec3::new(0.0, 0.0, 0.0)),
                (Vec3::new(5.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0)),
            ],
            sequences: vec!["ragdoll".into(), "idle".into()],
            hitboxes: 4,
            hitbox_sets: 1,
            hitbox_list: vec![(0, 1, Vec3::new(-4.0, -4.0, -4.0), Vec3::new(4.0, 4.0, 4.0))],
            flexes: vec!["smile".into()],
            sequence_anim: vec![(30.0, 31, 1), (24.0, 49, 0)],
            anims: vec![
                ("ragdoll".into(), 30.0, 31, 1),
                ("idle".into(), 24.0, 49, 0),
            ],
            mins: Vec3::new(-8.0, -8.0, 0.0),
            maxs: Vec3::new(8.0, 8.0, 40.0),
            friction: 0.0,
            elasticity: 0.0,
            attachments: vec![("eyes".into(), 1, Vec3::new(0.0, 0.0, 36.0), Vec3::X)],
            ..Default::default()
        },
    );
    models
}

#[test]
fn model_data_comes_from_the_server_tables() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local e = Entity(5)
             local bg = e:GetBodyGroups()
             local mn, mx = e:GetModelBounds()
             local at = e:GetAttachment(e:LookupAttachment('eyes'))
             state.r = { e:GetNumBodyGroups(), bg[2].name, e:GetBodygroupCount(1), e:FindBodygroupByName('hat'), e:SkinCount(), e:GetBoneCount(),
                 e:GetBoneName(1), e:LookupBone('ValveBiped.Bip01_Head1'), e:GetBoneParent(1), e:GetSequenceCount(), e:LookupSequence('idle'),
                 e:GetSequenceList()[1], e:GetHitBoxCount(0), e:GetFlexNum(), e:GetFlexName(0), mn.z, mx.z, math.Round(e:GetModelRadius(), 3), at.Pos.z, tostring(Entity(6):GetBoneCount()) }
         end)",
    );
    let models = oildrum_models();
    let mut snap = (*snapshot()).clone();
    snap.models = gm_script::snapshot::ModelsRef(&models as *const _);
    let snap = Arc::new(snap);
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<String> = rt.actors()[0]
        .eval("local o = {} for i, v in ipairs(state.r) do o[i] = tostring(v) end return o")
        .unwrap();
    assert_eq!(
        r,
        [
            "2",
            "hat",
            "3",
            "1",
            "2",
            "2",
            "ValveBiped.Bip01_Head1",
            "1",
            "0",
            "2",
            "1",
            "idle",
            "4",
            "1",
            "smile",
            "0",
            "40",
            "22.978",
            "36",
            "2"
        ],
        "entity 6 shares entity 5's model"
    );
}

#[test]
fn player_mover_state_and_ops() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local ply = Player(1)
             state.r = { ply:KeyDown(IN_ATTACK), ply:KeyPressed(IN_ATTACK), ply:KeyPressed(IN_DUCK), ply:KeyDown(IN_DUCK), ply:KeyReleased(IN_JUMP),
                 ply:IsOnGround(), ply:Crouching(), ply:InVehicle(), ply:FlashlightIsOn(), ply:GetUseEntity():EntIndex() == 5, ply:IsWalking() }
             ply:Flashlight(false)
             ply:SetFrags(3)
             ply:AddDeaths(2)
             ply:EnterVehicle(Entity(5))
             ply:ExitVehicle()
             ply:Kick('bye')
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<bool> = rt.actors()[0].eval("return state.r").unwrap();
    assert_eq!(
        r,
        vec![true, true, false, true, false, true, true, false, true, true, false]
    );
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(matches!(
        &writes[0],
        QueuedWrite::PlayerSetFlashlight {
            player: 1,
            on: false
        }
    ));
    assert!(matches!(
        &writes[1],
        QueuedWrite::PlayerSetScore {
            frags: Some(3),
            deaths: None,
            ..
        }
    ));
    assert!(
        matches!(&writes[2], QueuedWrite::PlayerSetScore { frags: None, deaths: Some(d), .. } if *d == snap.players[&1].deaths + 2)
    );
    assert!(matches!(
        &writes[3],
        QueuedWrite::PlayerVehicle {
            vehicle: Some(5),
            ..
        }
    ));
    assert!(matches!(
        &writes[4],
        QueuedWrite::PlayerVehicle { vehicle: None, .. }
    ));
    assert!(matches!(&writes[5], QueuedWrite::PlayerKick { player: 1 }));
}

#[test]
fn animation_and_bounds_math_over_the_model() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local e = Entity(5)
             local info = e:GetSequenceInfo()
             local lo, hi = e:WorldSpaceAABB()
             local np = e:NearestPoint(Vector(100, 0, 100))
             state.r = { e:GetSequence(), e:SequenceDuration(), e:SequenceDuration(0), info.label, info.numframes, e:GetAnimCount(), e:GetAnimInfo(0).label,
                 e:OBBMaxs().z, lo.z, hi.z, np.x, np.z, e:IsPointInBounds(Vector(0, 0, 10)) and 1 or 0, e:IsPointInBounds(Vector(0, 0, 50)) and 1 or 0, e:GetNoDraw() and 1 or 0 }
             e:SetNoDraw(true)
         end)",
    );
    let mut models = std::collections::BTreeMap::new();
    models.insert(
        "models/props_c17/oildrum001.mdl".to_string(),
        gm_server::ModelScriptData {
            sequences: vec!["ragdoll".into(), "idle".into()],
            sequence_anim: vec![(30.0, 31, 1), (24.0, 49, 0)],
            anims: vec![
                ("ragdoll".into(), 30.0, 31, 1),
                ("idle".into(), 24.0, 49, 0),
            ],
            mins: Vec3::new(-8.0, -8.0, 0.0),
            maxs: Vec3::new(8.0, 8.0, 40.0),
            ..Default::default()
        },
    );
    let mut snap = (*snapshot()).clone();
    snap.models = gm_script::snapshot::ModelsRef(&models as *const _);
    let snap = Arc::new(snap);
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<String> = rt.actors()[0]
        .eval("local o = {} for i, v in ipairs(state.r) do o[i] = tostring(v) end return o")
        .unwrap();
    assert_eq!(
        r,
        ["1", "2", "1", "idle", "49", "2", "ragdoll", "40", "0", "40", "8", "40", "1", "0", "0"]
    );
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(matches!(
        &writes[0],
        QueuedWrite::SetRender { mode: Some(10), .. }
    ));
}

#[test]
fn poser_state_reads_and_writes() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local e = Entity(5)
             state.r = { e:GetManipulateBoneScale(1).x, e:GetManipulateBoneScale(0).x, e:GetManipulateBoneAngles(1).y, e:HasBoneManipulations() and 1 or 0,
                 e:GetFlexWeight(0), e:GetFlexWeight(3), e:GetFlexScale(), e:HasFlexManipulatior() and 1 or 0, e:GetEyeTarget() == nil and 1 or 0 }
             e:ManipulateBoneScale(1, Vector(3, 3, 3))
             e:ManipulateBoneAngles(2, Angle(10, 20, 30))
             e:SetFlexWeight(0, 0.75)
             e:SetFlexScale(2)
             e:SetEyeTarget(Vector(1, 2, 3))
             e:SetEyeTarget(nil)
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<f64> = rt.actors()[0].eval("return state.r").unwrap();
    assert_eq!(r, vec![2.0, 1.0, 0.0, 1.0, 0.5, 0.0, 1.0, 1.0, 1.0]);
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(
        matches!(&writes[0], QueuedWrite::SetBoneManip { ent: 5, bone: 1, scale: Some(s), angles: None, .. } if s.x == 3.0)
    );
    assert!(
        matches!(&writes[1], QueuedWrite::SetBoneManip { bone: 2, scale: None, angles: Some(a), .. } if a.y == 20.0)
    );
    assert!(
        matches!(&writes[2], QueuedWrite::SetFlex { id: Some(0), value, .. } if *value == 0.75)
    );
    assert!(matches!(&writes[3], QueuedWrite::SetFlex { id: None, value, .. } if *value == 2.0));
    assert!(matches!(&writes[4], QueuedWrite::SetEyeTarget { target: Some(t), .. } if t.z == 3.0));
    assert!(matches!(
        &writes[5],
        QueuedWrite::SetEyeTarget { target: None, .. }
    ));
}

#[test]
fn say_and_npc_weapon_queue_writes() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             Player(1):Say('hello')
             local n = setmetatable({ __id = 5, __vars = {}, __tables = {}, __class_meta = __glua_classes.NPC }, __glua_classes.Entity)
             state.w = n:GetActiveWeapon():GetClass()
             n:Give('weapon_ar2')
             n:DropWeapon()
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let w: String = rt.actors()[0].eval("return state.w").unwrap();
    assert_eq!(w, "weapon_smg1");
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(matches!(&writes[0], QueuedWrite::PlayerSay { player: 1, text } if text == "hello"));
    assert!(
        matches!(&writes[1], QueuedWrite::NpcWeapon { ent: 5, class } if class == "weapon_ar2")
    );
    assert!(matches!(&writes[2], QueuedWrite::NpcWeapon { ent: 5, class } if class.is_empty()));
}

#[test]
fn vehicle_record_reads_the_server_table() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local v = setmetatable({ __id = 5, __vars = {}, __tables = {}, __class_meta = __glua_classes.Vehicle }, __glua_classes.Entity)
             local p = v:GetVehicleParams()
             state.r = { v:GetDriver():UserID(), v:GetWheelCount(), v:GetWheelBaseHeight(0), v:GetWheelTotalHeight(1), p.body.massOverride, p.wheels[2].springConstant, v:IsEngineEnabled() and 1 or 0 }
         end)",
    );
    let mut vehicles: std::collections::BTreeMap<u32, gm_server::Vehicle> =
        std::collections::BTreeMap::new();
    vehicles.insert(
        5,
        gm_server::Vehicle {
            key: "Jeep".into(),
            tuning: gm_server::VehicleTuning::default(),
            controls: gm_server::VehicleControls {
                throttle: 1.0,
                steer: 0.5,
                handbrake: false,
                boost: 0.0,
                engine_on: true,
                steer_angle: 9.0,
                speed: 300.0,
                wheels_in_contact: 4,
                gear: 1,
                contacts: Vec::new(),
            },
            class: "prop_vehicle_jeep".into(),
            model: "models/buggy.mdl".into(),
            health: 100,
            max_health: 100,
            driver: Some(1),
            wheels: vec![
                gm_server::Wheel {
                    hub: Vec3::new(40.0, 30.0, 10.0),
                    radius: 18.0,
                    spring: 100.0,
                    damping: 2.0,
                    damping_compression: 3.0,
                },
                gm_server::Wheel {
                    hub: Vec3::new(-40.0, 30.0, 10.0),
                    radius: 20.0,
                    spring: 120.0,
                    damping: 2.0,
                    damping_compression: 3.0,
                },
            ],
            add_gravity: 1.0,
            mass: 800.0,
        },
    );
    let mut snap = (*snapshot()).clone();
    snap.vehicles = gm_script::snapshot::VehiclesRef(&vehicles as *const _);
    let snap = Arc::new(snap);
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<f64> = rt.actors()[0].eval("return state.r").unwrap();
    assert_eq!(r, vec![1.0, 2.0, -8.0, 40.0, 800.0, 120.0, 1.0]);
}

#[test]
fn vehicle_driving_readbacks_and_writes() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local v = setmetatable({ __id = 5, __vars = {}, __tables = {}, __class_meta = __glua_classes.Vehicle }, __glua_classes.Entity)
             local o = v:GetOperatingParams()
             state.r = { v:GetThrottle(), v:GetSteering(), v:IsEngineStarted() and 1 or 0, o.speed, math.Round(o.RPM), o.gear,
                 o.steeringAngle, o.wheelsInContact, v:GetSteeringDegrees(), v:GetMaxSpeed(), v:GetRPM() == o.RPM and 1 or 0,
                 v:GetVehicleParams().engine.maxSpeed, v:GetVehicleParams().steering.degreesFast }
             v:SetThrottle(0.5)
             v:SetSteering(-1, 0)
             v:SetHandbrake(true)
             v:ReleaseHandbrake()
             v:StartEngine(false)
             v:EnableEngine(true)
         end)",
    );
    let mut vehicles: std::collections::BTreeMap<u32, gm_server::Vehicle> =
        std::collections::BTreeMap::new();
    vehicles.insert(
        5,
        gm_server::Vehicle {
            key: "Jeep".into(),
            tuning: gm_server::VehicleTuning::default(),
            class: "prop_vehicle_jeep".into(),
            model: "models/buggy.mdl".into(),
            health: 100,
            max_health: 100,
            driver: None,
            wheels: vec![],
            add_gravity: 0.5,
            mass: 1000.0,
            controls: gm_server::VehicleControls {
                throttle: 1.0,
                steer: 0.5,
                handbrake: false,
                boost: 0.0,
                engine_on: true,
                steer_angle: 9.0,
                speed: 300.0,
                wheels_in_contact: 4,
                gear: 1,
                contacts: Vec::new(),
            },
        },
    );
    let mut snap = (*snapshot()).clone();
    snap.vehicles = gm_script::snapshot::VehiclesRef(&vehicles as *const _);
    rt.actors_mut()[0]
        .dispatch(&Arc::new(snap), "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<f64> = rt.actors()[0].eval("return state.r").unwrap();
    // 300 u/s in second gear: 300 / (2 pi 20.3) * 60 * 2.4 * 4.56 = 1544 rpm; the cone at 300
    // u/s (between 14 and 20 mph) is 50 -> 18 at 51 %: 33.7
    assert_eq!(r[0..4], [1.0, 0.5, 1.0, 300.0]);
    assert_eq!(r[4], 1544.0);
    assert_eq!(r[5..8], [1.0, 9.0, 4.0]);
    assert!((r[8] - 33.66).abs() < 0.1, "cone {}", r[8]);
    assert_eq!(r[9], 616.0);
    assert_eq!(r[10], 1.0);
    assert_eq!(r[11], 35.0);
    assert_eq!(r[12], 18.0);
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(matches!(
        &writes[0],
        QueuedWrite::VehicleControls { ent: 5, throttle: Some(t), steer: None, handbrake: None } if *t == 0.5
    ));
    assert!(matches!(
        &writes[1],
        QueuedWrite::VehicleControls { ent: 5, throttle: None, steer: Some(s), handbrake: None } if *s == -1.0
    ));
    assert!(matches!(
        &writes[2],
        QueuedWrite::VehicleControls {
            ent: 5,
            throttle: None,
            steer: None,
            handbrake: Some(true)
        }
    ));
    assert!(matches!(
        &writes[3],
        QueuedWrite::VehicleControls {
            ent: 5,
            throttle: None,
            steer: None,
            handbrake: Some(false)
        }
    ));
    assert!(matches!(
        &writes[4],
        QueuedWrite::VehicleEngine { ent: 5, on: false }
    ));
    assert!(matches!(
        &writes[5],
        QueuedWrite::VehicleEngine { ent: 5, on: true }
    ));
}

#[test]
fn enemy_seen_tick_and_surface_reads() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local n = setmetatable({ __id = 5, __vars = {}, __tables = {}, __class_meta = __glua_classes.NPC }, __glua_classes.Entity)
             state.r = { math.Round(n:GetEnemyLastTimeSeen(), 3), Entity(5):GetFriction(), Entity(5):GetElasticity() }
         end)",
    );
    let mut models = std::collections::BTreeMap::new();
    models.insert(
        "models/props_c17/oildrum001.mdl".to_string(),
        gm_server::ModelScriptData {
            friction: 0.8,
            elasticity: 0.25,
            ..Default::default()
        },
    );
    let mut snap = (*snapshot()).clone();
    snap.models = gm_script::snapshot::ModelsRef(&models as *const _);
    let snap = Arc::new(snap);
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<f64> = rt.actors()[0].eval("return state.r").unwrap();
    assert_eq!(r, vec![3.0, 0.6000000238418579, 0.10000000149011612]);
}

#[test]
fn player_drop_notarget_colors_armor() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local p = Player(1)
             state.r = { p:GetNoTarget() and 1 or 0, p:GetPlayerColor().y, p:GetWeaponColor().x, p:GetMaxArmor() }
             p:DropWeapon()
             p:DropNamedWeapon('weapon_crowbar')
             p:DropObject()
             p:SetNoTarget(true)
             p:SetPlayerColor(Vector(1, 0.5, 0))
             p:SetMaxArmor(200)
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<f64> = rt.actors()[0].eval("return state.r").unwrap();
    assert_eq!(r, vec![1.0, 0.25, 1.0, 150.0]);
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(matches!(
        &writes[0],
        QueuedWrite::PlayerDropWeapon {
            player: 1,
            class: None
        }
    ));
    assert!(
        matches!(&writes[1], QueuedWrite::PlayerDropWeapon { class: Some(c), .. } if c == "weapon_crowbar")
    );
    assert!(matches!(
        &writes[2],
        QueuedWrite::PlayerDropObject { player: 1 }
    ));
    assert!(matches!(
        &writes[3],
        QueuedWrite::PlayerNoTarget { on: true, .. }
    ));
    assert!(
        matches!(&writes[4], QueuedWrite::PlayerColors { body: Some(b), weapon: None, .. } if b.y == 0.5)
    );
    assert!(matches!(
        &writes[5],
        QueuedWrite::PlayerMaxArmor { max: 200, .. }
    ));
}

#[test]
fn lock_and_surface_writes() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local p, e = Player(1), Entity(5)
             state.r = { p:IsFrozen() and 1 or 0, math.Round(e:GetFriction(), 3), math.Round(e:GetElasticity(), 3) }
             p:Lock()
             p:UnLock()
             e:SetFriction(1.5)
             e:SetElasticity(0.9)
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<f64> = rt.actors()[0].eval("return state.r").unwrap();
    assert_eq!(r, vec![1.0, 0.6, 0.1]);
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(matches!(
        &writes[0],
        QueuedWrite::PlayerLock {
            player: 1,
            locked: true
        }
    ));
    assert!(matches!(
        &writes[1],
        QueuedWrite::PlayerLock {
            player: 1,
            locked: false
        }
    ));
    assert!(
        matches!(&writes[2], QueuedWrite::SetSurface { ent: 5, friction: Some(f), elasticity: None } if *f == 1.5)
    );
    assert!(
        matches!(&writes[3], QueuedWrite::SetSurface { ent: 5, friction: None, elasticity: Some(e) } if *e == 0.9)
    );
}

#[test]
fn connected_time_hitgroup_fire_and_drop() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local p, e = Player(1), Entity(5)
             state.r = { math.Round(p:TimeConnected(), 3), p:LastHitGroup(), e:IsOnFire() and 1 or 0 }
             e:ForcePlayerDrop()
             e:PhysWake()
             e:SetBodyGroups('12')
         end)",
    );
    let mut snap = snapshot();
    Arc::get_mut(&mut snap).unwrap().tick = 100;
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 100)
        .unwrap();
    let r: Vec<f64> = rt.actors()[0].eval("return state.r").unwrap();
    assert_eq!(r, vec![0.9, 1.0, 1.0]);
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(matches!(&writes[0], QueuedWrite::ForceDrop { ent: 5 }));
    assert!(matches!(
        &writes[1],
        QueuedWrite::SetAsleep {
            ent: 5,
            asleep: false
        }
    ));
    assert_eq!(writes.len(), 4);
}

#[test]
fn player_delta_hooks_fire_on_landing_jump_and_ammo() {
    let mut rt = runtime(
        "state.log = {}
         hook.Add('OnPlayerHitGround', 't', function(p, w, f, s) table.insert(state.log, 'ground:' .. s) end)
         hook.Add('OnPlayerJump', 't', function(p, s) table.insert(state.log, 'jump') end)
         hook.Add('PlayerAmmoChanged', 't', function(p, id, old, new) table.insert(state.log, 'ammo:' .. tostring(id) .. ':' .. old .. ':' .. new) end)",
    );
    let mut s1 = snapshot();
    {
        let p = Arc::get_mut(&mut s1).unwrap().players.get_mut(&1).unwrap();
        p.on_ground = false;
        p.velocity = Vec3::new(0.0, 0.0, -300.0);
        p.buttons = 0;
        p.buttons_last = 0;
    }
    rt.actors_mut()[0]
        .dispatch(&s1, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let mut s2 = snapshot();
    {
        let p = Arc::get_mut(&mut s2).unwrap().players.get_mut(&1).unwrap();
        p.on_ground = true;
        p.buttons = 0;
        p.buttons_last = 0;
        p.ammo.push(("Pistol".into(), 99));
    }
    rt.actors_mut()[0]
        .dispatch(&s2, "tick.pre", &Payload::default(), 2)
        .unwrap();
    let mut s3 = snapshot();
    {
        let p = Arc::get_mut(&mut s3).unwrap().players.get_mut(&1).unwrap();
        p.on_ground = true;
        p.buttons = 2; // IN_JUMP (glua enum)
        p.buttons_last = 0;
        p.ammo.push(("Pistol".into(), 99));
    }
    rt.actors_mut()[0]
        .dispatch(&s3, "tick.pre", &Payload::default(), 3)
        .unwrap();
    let log: Vec<String> = rt.actors()[0].eval("return state.log").unwrap();
    let joined = log.join(" ");
    assert!(joined.contains("ground:300"), "{joined}");
    assert!(joined.contains(":99"), "{joined}");
    assert!(log.last().is_some_and(|l| l == "jump"), "{joined}");
}

#[test]
fn default_reload_and_drop_if_held() {
    let mut rt = runtime(
        "weapons.Register({ Primary = { ClipSize = 30, Ammo = 'Pistol' } }, 'weapon_pistol')
         events.on('tick.pre', function()
             local w = Player(1):GetWeapon('weapon_pistol')
             local before = w:Clip1()
             state.r = { w:DefaultReload(ACT_VM_RELOAD) and 1 or 0, w:GetMaxClip1(), before,
                         w:DefaultReload(ACT_VM_RELOAD) and 1 or 0 }
             DropEntityIfHeld(Entity(5))
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<f64> = rt.actors()[0].eval("return state.r").unwrap();
    assert_eq!((r[0], r[1]), (1.0, 30.0));
    let take = (30.0 - r[2]).min(20.0) as i32;
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    let dbg = format!("{writes:?}");
    assert!(
        dbg.contains(&format!("count: {}", r[2] as i32 + take)),
        "{dbg}"
    );
    assert!(dbg.contains(&format!("count: -{take}")), "{dbg}");
    assert!(
        matches!(writes.last(), Some(QueuedWrite::ForceDrop { ent: 5 })),
        "{dbg}"
    );
}

#[test]
fn hitboxes_child_bones_timer_and_sha256() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local e = Entity(5)
             local mins, maxs = e:GetHitBoxBounds(0, 0)
             local t = util.Timer(1)
             local later = t:Elapsed() and 1 or 0
             t:Start(0)
             state.r = { e:GetHitBoxBone(0, 0), e:GetHitBoxHitGroup(0, 0), mins.x, maxs.z,
                         #e:GetChildBones(0), e:GetHitBoxBone(9, 0), later, t:Elapsed() and 1 or 0 }
             state.h = util.SHA256('abc')
         end)",
    );
    let models = oildrum_models();
    let mut snap = (*snapshot()).clone();
    snap.models = gm_script::snapshot::ModelsRef(&models as *const _);
    let snap = Arc::new(snap);
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<f64> = rt.actors()[0].eval("return state.r").unwrap();
    let h: String = rt.actors()[0].eval("return state.h").unwrap();
    assert_eq!(&r[..4], &[0.0, 1.0, -4.0, 4.0]);
    assert!(r[4] >= 1.0, "{r:?}");
    assert_eq!(&r[5..], &[-1.0, 0.0, 1.0]);
    assert_eq!(
        h,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn previous_weapon_effects_and_eflags() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local e = Entity(5)
             e:AddEffects(EF_NODRAW)
             e:AddEFlags(EFL_DORMANT)
             local a, b = e:IsEffectActive(EF_NODRAW) and 1 or 0, e:GetEffects()
             e:RemoveEffects(EF_NODRAW)
             state.r = { a, b, e:GetEffects(), e:IsEFlagSet(EFL_DORMANT) and 1 or 0, e:IsEFlagSet(EFL_KILLME) and 1 or 0 }
             state.w = Player(1):GetPreviousWeapon():GetClass()
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<f64> = rt.actors()[0].eval("return state.r").unwrap();
    let w: String = rt.actors()[0].eval("return state.w").unwrap();
    assert_eq!(r, vec![1.0, 32.0, 0.0, 1.0, 0.0]);
    assert_eq!(w, "weapon_smg1");
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert_eq!(writes.len(), 2, "{writes:?}");
}

#[test]
fn bind_pose_bone_positions() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local e = Entity(5)
             local p0 = e:GetBonePosition(0)
             local p1, a1 = e:GetBonePosition(1)
             local m = e:GetBoneMatrix(1)
             state.r = { p0:Distance(e:GetPos()), p0:Distance(p1), m:GetTranslation():Distance(p1),
                         e:GetBonePosition(99) == nil and 1 or 0 }
         end)",
    );
    let models = oildrum_models();
    let mut snap = (*snapshot()).clone();
    snap.models = gm_script::snapshot::ModelsRef(&models as *const _);
    let snap = Arc::new(snap);
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<f64> = rt.actors()[0].eval("return state.r").unwrap();
    assert!(
        (r[0] - 10.0).abs() < 1e-3 && (r[1] - 5.000000).abs() < 1e-3 && r[2] < 1e-3,
        "{r:?}"
    );
    assert_eq!(r[3], 1.0);
}

#[test]
fn team_spawn_points_pick_the_players_spawn() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             team.SetUp(1, 'Red', Color(255, 0, 0))
             Player(1):SetTeam(1)
             team.SetSpawnPoint(1, Entity(5):GetClass())
             local a = hook.Run('PlayerSelectSpawn', Player(1), false)
             hook.Add('IsSpawnpointSuitable', 't', function() return false end)
             local b = hook.Run('PlayerSelectSpawn', Player(1), false)
             hook.Remove('IsSpawnpointSuitable', 't')
             team.SetSpawnPoint(1, 'no_such_class')
             local c = hook.Run('PlayerSelectSpawn', Player(1), false)
             state.r = { tostring(a), tostring(b == NULL), tostring(c == NULL) }
         end)",
    );
    let snap = snapshot();
    let a = &mut rt.actors_mut()[0];
    a.dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<String> = a.eval("return state.r").unwrap();
    assert!(
        r[0].starts_with("Entity [") && r[0].ends_with("[prop_physics]"),
        "one of the team's spawn entities: {r:?}"
    );
    assert_eq!(&r[1..], ["true", "true"], "{r:?}");
}

#[test]
fn run_string_and_compile_string_compile_chunks_in_the_sandbox() {
    let rt = runtime(
        "state = {}\n\
         state.ok = RunString('state.x = 5', 'one')\n\
         state.cs = CompileString('return 7 * 6', 'two')()\n\
         state.bad = RunString('x = = 1', 'three', false)\n\
         state.badc = CompileString('return (', 'four', false)\n\
         state.runtime_err = RunString('error(\"boom\")', 'five', false)\n\
         state.no_io = RunString('state.io = io', 'six')\n",
    );
    let a = &rt.actors()[0];
    let (ok, x, cs): (Option<String>, i64, i64) =
        a.eval("return state.ok, state.x, state.cs").unwrap();
    assert_eq!((ok, x, cs), (None, 5, 42));
    let (bad, badc, rt_err, io): (String, String, String, Option<bool>) = a
        .eval("return state.bad, state.badc, state.runtime_err, state.io ~= nil")
        .unwrap();
    assert!(
        bad.contains("three"),
        "the identifier names the chunk: {bad}"
    );
    assert!(!badc.is_empty());
    assert!(rt_err.contains("boom"));
    assert_eq!(
        io,
        Some(false),
        "the sandbox globals apply to compiled chunks"
    );
}

#[test]
fn path_follower_segments_and_cursor_follow_the_polyline() {
    let rt = runtime(
        "state = {}\n\
         local p = Path('Follow')\n\
         state.empty = p:IsValid()\n\
         p:__load({ Vector(0, 0, 0), Vector(100, 0, 0), Vector(100, 50, 0) })\n\
         state.valid = p:IsValid()\n\
         state.len = p:GetLength()\n\
         state.segs = #p:GetAllSegments()\n\
         state.first = tostring(p:FirstSegment().pos)\n\
         state.last = tostring(p:LastSegment().pos)\n\
         p:MoveCursorTo(120)\n\
         state.cur = tostring(p:GetCursorData().pos)\n\
         state.goal = tostring(p:GetCurrentGoal().pos)\n\
         state.on = tostring(p:GetPositionOnPath(50))\n\
         state.close = tostring(p:GetClosestPosition(Vector(50, 30, 0)))\n\
         p:MoveCursorToClosestPosition(Vector(100, 25, 0))\n\
         state.ccur = p:GetCursorPosition()\n\
         p:MoveCursor(1000)\n\
         state.clamped = p:GetCursorPosition()\n\
         state.d2 = p:FirstSegment().distanceFromStart .. ' ' .. p:LastSegment().distanceFromStart\n",
    );
    let a = &rt.actors()[0];
    let (empty, valid, len, segs): (bool, bool, f64, i64) = a
        .eval("return state.empty, state.valid, state.len, state.segs")
        .unwrap();
    assert_eq!((empty, valid, len, segs), (false, true, 150.0, 3));
    let (first, last, cur, goal): (String, String, String, String) = a
        .eval("return state.first, state.last, state.cur, state.goal")
        .unwrap();
    assert_eq!(first, "0.000000 0.000000 0.000000");
    assert_eq!(last, "100.000000 50.000000 0.000000");
    assert_eq!(cur, "100.000000 20.000000 0.000000");
    assert_eq!(goal, last, "the goal past the cursor is the last segment");
    let (on, close, ccur, clamped, d2): (String, String, f64, f64, String) = a
        .eval("return state.on, state.close, state.ccur, state.clamped, state.d2")
        .unwrap();
    assert_eq!(on, "50.000000 0.000000 0.000000");
    assert_eq!(close, "50.000000 0.000000 0.000000");
    assert_eq!((ccur, clamped), (125.0, 150.0));
    assert_eq!(d2, "0 150");
}

#[test]
fn held_props_and_water_level_come_from_the_player_snapshot() {
    let mut rt = runtime(
        "state = {}
         events.on('tick.pre', function()
             state.wl = Player(1):WaterLevel()
             state.conn = Player(1):Ping() .. ' ' .. Player(1):PacketLoss() .. ' ' .. Player(1):GetTimeoutSeconds() .. ' ' .. tostring(Player(1):IsTimingOut()) .. ' ' .. Player(1):IPAddress()
             state.held = Entity(777):IsPlayerHolding()
             state.other = Entity(778):IsPlayerHolding()
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let (wl, held, other): (i64, bool, bool) = rt.actors()[0]
        .eval("return state.wl, state.held, state.other")
        .unwrap();
    assert_eq!((wl, held, other), (2, true, false));
    let conn: String = rt.actors()[0].eval("return state.conn").unwrap();
    assert_eq!(conn, "30 2.5 0.5 false 127.0.0.1:27005");
}

#[test]
fn a_player_answers_every_entity_method() {
    let mut rt = runtime(
        "state = {}
         events.on('tick.pre', function()
             local p = Player(1)
             local missing = {}
             for k, v in pairs(__glua_classes.Entity) do
                 if type(v) == 'function' and p[k] == nil then missing[#missing + 1] = k end
             end
             table.sort(missing)
             state.missing = table.concat(missing, ' ')
             state.parent = IsValid(p:GetParent())
             state.children = #p:GetChildren()
             state.scale = p:GetModelScale()
             state.fire = p:IsOnFire()
             state.blood = p:GetBloodColor()
             state.holding = p:IsPlayerHolding()
             state.idx = p:EntIndex()
             state.cls = p:GetClass()
             state.eq = tostring(NULL == p) .. tostring(p == NULL) .. tostring(p == Player(1)) .. tostring(Entity(777) == p) .. tostring(p == Entity(777))
         end)",
    );
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let a = &rt.actors()[0];
    let missing: String = a.eval("return state.missing").unwrap();
    assert_eq!(missing, "", "Entity methods a Player cannot call");
    let (parent, children, scale, fire): (bool, i64, f64, bool) = a
        .eval("return state.parent, state.children, state.scale, state.fire")
        .unwrap();
    assert_eq!((parent, children, scale, fire), (false, 0, 1.0, false));
    let (blood, holding, idx, cls): (i64, bool, i64, String) = a
        .eval("return state.blood, state.holding, state.idx, state.cls")
        .unwrap();
    assert_eq!((blood, holding, idx, cls.as_str()), (0, false, 1, "player"));
    let eq: String = a.eval("return state.eq").unwrap();
    assert_eq!(
        eq, "falsefalsetruefalsefalse",
        "a player equals only itself"
    );
}

#[test]
fn is_mounted_reads_the_provider_labels_and_sound_duration_asks_the_host() {
    let mut rt = runtime(
        "state = {}
         events.on('tick.pre', function()
             state.m = tostring(IsMounted('hl2')) .. tostring(IsMounted('cstrike')) .. tostring(IsMounted('GarrysMod'))
             state.d = SoundDuration('ambient/alarms/klaxon1.wav')
         end)",
    );
    rt.set_sound_duration(std::sync::Arc::new(|n: &str| {
        (n == "ambient/alarms/klaxon1.wav").then_some(2.5)
    }));
    let mut rt2 = runtime("state = {}");
    let _ = &mut rt2;
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let (m, d): (String, f64) = rt.actors()[0].eval("return state.m, state.d").unwrap();
    assert_eq!(m, "truefalsetrue");
    assert_eq!(
        d, 0.0,
        "the lookup set after load does not reach the running actor"
    );
}

#[test]
fn sandbox_sent_giveswep_and_ragdoll_hooks() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"PlayerSpawnSENT\", \"t\", function(ply, class) state.sent = class; return false end)
            hook.Add(\"PlayerSpawnedSENT\", \"t\", function(ply, ent) state.spawned = ent:EntIndex() end)
            hook.Add(\"PlayerGiveSWEP\", \"t\", function(ply, class, tbl) state.give = class end)
            hook.Add(\"PlayerSpawnRagdoll\", \"t\", function(ply, model) state.ragdoll = model end)"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    let mut p = Payload::default();
    p.ints.push(("player", 1));
    p.strs.push(("kind", "sent".into()));
    p.strs.push(("name", "my_sent".into()));
    rt.actors_mut()[0]
        .dispatch(&snap, "sandbox.spawn_request", &p, 3)
        .unwrap();
    let mut q = Payload::default();
    q.ints.push(("player", 1));
    q.ints.push(("entity", 9));
    q.strs.push(("kind", "sent".into()));
    q.strs.push(("name", "my_sent".into()));
    rt.actors_mut()[0]
        .dispatch(&snap, "sandbox.spawned", &q, 4)
        .unwrap();
    let mut g = Payload::default();
    g.ints.push(("player", 1));
    g.strs.push(("kind", "giveswep".into()));
    g.strs.push(("name", "weapon_crowbar".into()));
    rt.actors_mut()[0]
        .dispatch(&snap, "sandbox.spawn_request", &g, 5)
        .unwrap();
    let mut r = Payload::default();
    r.ints.push(("player", 1));
    r.strs.push(("model", "models/alyx.mdl".into()));
    r.bools.push(("ragdoll", true));
    rt.actors_mut()[0]
        .dispatch(&snap, "prop.spawn_request", &r, 6)
        .unwrap();
    let (sent, spawned, give, ragdoll): (String, i32, String, String) = rt.actors()[0]
        .eval("return state.sent, state.spawned, state.give, state.ragdoll")
        .unwrap();
    assert_eq!(
        (sent.as_str(), spawned, give.as_str(), ragdoll.as_str()),
        ("my_sent", 9, "weapon_crowbar", "models/alyx.mdl")
    );
}

#[test]
fn step_sound_time_hook_returns_the_interval() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"PlayerStepSoundTime\", \"t\", function(ply, kind, walking) state.kind, state.walking = kind, walking; return 123 end)"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    let mut p = Payload::default();
    p.ints.push(("player", 1));
    p.ints.push(("kind", 3));
    p.bools.push(("walking", true));
    let reply = rt.actors_mut()[0]
        .dispatch_reply(&snap, "player.step_time", &p, 3)
        .unwrap();
    assert!(matches!(reply, gm_script::actor::Reply::Number(n) if n == 123.0));
    let (kind, walking): (i32, bool) = rt.actors()[0]
        .eval("return state.kind, state.walking")
        .unwrap();
    assert_eq!((kind, walking), (3, true));
}

#[test]
fn game_description_comes_from_the_hook() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"GetGameDescription\", \"t\", function() return \"My Mode\" end)".into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    let reply = rt.actors_mut()[0]
        .dispatch_reply(&snap, "gamemode.description", &Payload::default(), 3)
        .unwrap();
    assert!(matches!(reply, gm_script::actor::Reply::Text(t) if t == "My Mode"));
}

#[test]
fn accept_input_hook_sees_and_blocks_inputs() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"AcceptInput\", \"t\", function(ent, input, activator, caller, value)
                state.seen = (state.seen or 0) + 1; state.input, state.value = input, tostring(value)
                return input == \"Kill\" end)
            hook.Add(\"Think\", \"t\", function()
                local e = Entity(5)
                e:Fire(\"Open\", \"3\")
                e:Input(\"Kill\")
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
        .dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    let (seen, input, value): (i32, String, String) = rt.actors()[0]
        .eval("return state.seen, state.input, state.value")
        .unwrap();
    assert_eq!((seen, input.as_str(), value.as_str()), (2, "Kill", "nil"));
}

#[test]
fn weapon_tick_and_equip_ammo() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "weapons.Register({ Tick = function(self) state.ticks = (state.ticks or 0) + 1 end,
            EquipAmmo = function(self, ply) state.equip = ply:EntIndex() end }, 'weapon_t')"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    let mut p = Payload::default();
    p.ints.push(("player", 1));
    p.strs.push(("class", "weapon_t".into()));
    let a = &mut rt.actors_mut()[0];
    a.dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    a.dispatch(&snap, "player.weapon_pickup", &p, 3).unwrap();
    a.dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    a.dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    a.dispatch(&snap, "weapon.remove", &p, 3).unwrap();
    a.dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    a.dispatch(&snap, "player.weapon_equip_ammo", &p, 3)
        .unwrap();
    let (ticks, equip): (i32, i32) = a.eval("return state.ticks, state.equip").unwrap();
    assert_eq!((ticks, equip), (2, 1));
}

#[test]
fn sprint_and_ladder_writes_go_to_the_move_params() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"Think\", \"t\", function()
            local p = Player(1); p:SprintDisable(); p:SetLadderClimbSpeed(50); p:SetLaggedMovementValue(0.5)
            state.ladder, state.lag, state.sprint = p:GetLadderClimbSpeed(), p:GetLaggedMovementValue(), p:IsSprintEnabled() end)"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    let params: Vec<(u8, Option<f32>)> = writes
        .iter()
        .filter_map(|w| match w {
            QueuedWrite::PlayerMoveParam {
                player: 1,
                which,
                value,
            } => Some((*which, *value)),
            _ => None,
        })
        .collect();
    assert_eq!(
        params,
        vec![(15, Some(0.0)), (13, Some(50.0)), (14, Some(0.5))],
        "{writes:?}"
    );
    let (ladder, lag, sprint): (f32, f32, bool) = rt.actors()[0]
        .eval("return state.ladder, state.lag, state.sprint")
        .unwrap();
    assert_eq!((ladder, lag, sprint), (200.0, 1.0, true));
}

#[test]
fn rope_constraint_write_carries_its_kind() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"Think\", \"t\", function()
            local c = constraint.Rope(Entity(5), Entity(6), 0, 0, Vector(1, 2, 3), Vector(0, 0, 0), 100, 10, 0, 2, \"cable/rope\", false)
            constraint.Axis(Entity(5), Entity(6), 0, 0, Vector(0, 0, 0), Vector(0, 0, 0), 0, 0, 0, 1, Vector(0, 0, 1))
            state.kind = c and c.Type end)"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    let kinds: Vec<&gm_sandbox::ConstraintKind> = writes
        .iter()
        .filter_map(|w| match w {
            QueuedWrite::Constrain {
                ent: 5,
                ent2: 6,
                kind,
                ..
            } => Some(kind),
            _ => None,
        })
        .collect();
    assert_eq!(kinds.len(), 2, "{writes:?}");
    assert!(matches!(
        kinds[0],
        gm_sandbox::ConstraintKind::Rope { lpos1, length, addlength, rigid: false, .. }
            if *lpos1 == Vec3::new(1.0, 2.0, 3.0) && *length == 100.0 && *addlength == 10.0
    ));
    assert!(matches!(
        kinds[1],
        gm_sandbox::ConstraintKind::Axis { local_axis, nocollide: true, .. } if *local_axis == Vec3::Z
    ));
    let kind: String = rt.actors()[0].eval("return state.kind").unwrap();
    assert_eq!(kind, "Rope");
}

#[test]
fn hydraulic_constraint_write_builds_its_controller() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"Think\", \"t\", function()
            constraint.Hydraulic(Entity(5), Entity(6), 0, 0, Vector(), Vector(), 60, 100, 3, 37, false, 64, \"cable/rope\", true) end)"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    let Some(QueuedWrite::Constrain { kind, .. }) = writes
        .iter()
        .find(|w| matches!(w, QueuedWrite::Constrain { .. }))
    else {
        panic!("{writes:?}")
    };
    let gm_sandbox::ConstraintKind::Hydraulic {
        key,
        toggle,
        length2,
        controller,
        ..
    } = kind
    else {
        panic!("{kind:?}")
    };
    assert_eq!((*key, *toggle, *length2), (37, true, 100.0));
    assert_eq!(
        (controller.length, controller.max, controller.toggle),
        (60.0, Some(100.0), true)
    );
}

#[test]
fn pulley_and_motor_writes() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"Think\", \"t\", function()
            constraint.Pulley(Entity(5), Entity(6), 0, 0, Vector(), Vector(), Vector(0, 0, 100), Vector(50, 0, 100), 0, false, 2, \"cable/rope\")
            constraint.Motor(Entity(5), Entity(6), 0, 0, Vector(), Vector(), 0, 500, 0, false, true, nil, 0, 7, 8, 1, Vector(0, 0, 1)) end)"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(writes.iter().any(|w| matches!(w,
        QueuedWrite::Constrain { ent: 5, ent2: 6, kind: gm_sandbox::ConstraintKind::Pulley { wpos3, .. }, .. }
            if *wpos3 == Vec3::new(50.0, 0.0, 100.0))), "{writes:?}");
    assert!(writes.iter().any(|w| matches!(w,
        QueuedWrite::Motor { ent: 5, ent2: 6, axis_point, torque, toggle: true, key_f: 7, key_r: 8, .. }
            if *axis_point == Vec3::Z && *torque == 500.0)), "{writes:?}");
}

#[test]
fn nav_area_search_lists_follow_total_cost() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"Think\", \"t\", function()
            local A = __glua_classes.CNavArea
            local a, b, c = setmetatable({ __id = 1 }, A), setmetatable({ __id = 2 }, A), setmetatable({ __id = 3 }, A)
            a:ClearSearchLists()
            state.empty0 = a:IsOpenListEmpty()
            a:SetCostSoFar(4); a:SetTotalCost(10); a:AddToOpenList()
            b:SetTotalCost(5); b:AddToOpenList()
            c:SetTotalCost(7); c:AddToOpenList(); c:AddToOpenList()
            state.first = a:PopOpenList() == b
            state.bopen = b:IsOpen()
            c:SetTotalCost(20); c:UpdateOnOpenList()
            state.second = a:PopOpenList() == a
            state.third = a:PopOpenList() == c
            state.empty1 = a:IsOpenListEmpty() and a:PopOpenList() == nil
            a:AddToClosedList(); state.closed = a:IsClosed(); a:RemoveFromClosedList(); state.closed2 = a:IsClosed()
            a:SetParent(b, 3); state.parent = a:GetParent() == b; state.how = a:GetParentHow(); state.how2 = c:GetParentHow()
            state.cost = a:GetCostSoFar()
            a:ClearSearchLists(); state.cost2 = a:GetCostSoFar() end)"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    let a = &rt.actors()[0];
    let flags: Vec<bool> = a
        .eval("return { state.empty0, state.first, not state.bopen, state.second, state.third, state.empty1, state.closed, not state.closed2, state.parent }")
        .unwrap();
    assert_eq!(flags, vec![true; 9]);
    let (how, how2, cost, cost2): (i32, i32, f32, f32) = a
        .eval("return state.how, state.how2, state.cost, state.cost2")
        .unwrap();
    assert_eq!((how, how2, cost, cost2), (3, 9, 4.0, 0.0));
}

#[test]
fn enable_drag_write_and_readback() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"Think\", \"t\", function()
            local p = Entity(5):GetPhysicsObject(); p:EnableDrag(false); state.drag = p:IsDragEnabled() end)"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(
        writes
            .iter()
            .any(|w| matches!(w, QueuedWrite::EnableDrag { ent: 5, on: false })),
        "{writes:?}"
    );
    let drag: bool = rt.actors()[0].eval("return state.drag").unwrap();
    assert!(!drag);
}

#[test]
fn glua_stool_handles_unknown_tool_clicks() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "TOOL = ToolObj:Create()
            TOOL.Mode = \"mytool\"
            function TOOL:LeftClick(tr) state.z, state.owner, state.world = tr.HitPos.z, self:GetOwner():EntIndex(), tr.HitWorld; return true end
            function TOOL:RightClick(tr) return false end
            function TOOL:Reload(tr) state.reloaded = true; return true end"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    let a = &mut rt.actors_mut()[0];
    assert!(a.has_tool("mytool"));
    assert!(!a.has_tool("nosuch"));
    let mut p = Payload::default();
    p.ints.push(("player", 1));
    p.ints.push(("stage", 0));
    p.hit = Some((0, Vec3::new(0.0, 0.0, 7.0), Vec3::Z));
    let ops = a.dispatch_tool(&snap, "mytool", "left", &p).unwrap();
    assert_eq!(ops, vec![gm_script::api::ToolOp::Done]);
    let ops = a.dispatch_tool(&snap, "mytool", "right", &p).unwrap();
    assert!(ops.is_empty());
    let ops = a.dispatch_tool(&snap, "mytool", "reload", &p).unwrap();
    assert_eq!(ops, vec![gm_script::api::ToolOp::Done]);
    let (z, owner, world): (f32, i32, bool) =
        a.eval("return state.z, state.owner, state.world").unwrap();
    assert_eq!((z, owner, world), (7.0, 1, true));
}

#[test]
fn npc_arrival_distance_write() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"Think\", \"t\", function()
            local n = setmetatable({ __id = 5, __vars = {}, __tables = {}, __class_meta = __glua_classes.NPC }, __glua_classes.Entity)
            n:SetArrivalDistance(48) end)"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(
        writes
            .iter()
            .any(|w| matches!(w, QueuedWrite::NpcArrival { ent: 5, dist } if *dist == 48.0)),
        "{writes:?}"
    );
}

#[test]
fn entity_outputs_fire_through_inputs() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"AcceptInput\", \"t\", function(ent, input, activator, caller, value)
                state.n = (state.n or 0) + 1; state.input, state.value, state.caller = input, tostring(value), caller:EntIndex() end)
            hook.Add(\"Think\", \"t\", function()
                local e = Entity(5)
                e:StoreOutput(\"OnFoo\", \"!self,Bar,7,0,1\")
                state.kv = e:AddOutputFromKeyValue(\"OnBaz\", \"!activator,Q,,0,-1\")
                state.kv2 = e:AddOutputFromKeyValue(\"model\", \"x\")
                state.ai = e:AddOutputFromAcceptInput(\"AddOutput\", \"OnBaz !self,R,1,0,-1\")
                e:TriggerOutput(\"OnFoo\", e)
                e:TriggerOutput(\"OnFoo\", e)
                state.after_foo = state.n
                e:TriggerOutput(\"OnBaz\", Entity(6), \"d\")
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
        .dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    let (n, after_foo, input, value, caller): (i32, i32, String, String, i32) = rt.actors()[0]
        .eval("return state.n, state.after_foo, state.input, state.value, state.caller")
        .unwrap();
    assert_eq!(
        (after_foo, n),
        (1, 3),
        "once for OnFoo (times 1), twice for OnBaz"
    );
    // the list is walked from the newest entry down, so the last input seen is the first stored one
    assert_eq!((input.as_str(), value.as_str(), caller), ("Q", "d", 5));
    let flags: Vec<bool> = rt.actors()[0]
        .eval("return { state.kv, not state.kv2, state.ai }")
        .unwrap();
    assert_eq!(flags, vec![true; 3]);
}

#[test]
fn spawn_function_takes_over_the_sent_spawn() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "scripted_ents.Register({ Type = 'anim', Model = 'models/props_c17/oildrum001.mdl', SpawnFunction = function(tbl, ply, tr, class) state.called = class; state.ply = ply:EntIndex() end }, 'my_sent')
            scripted_ents.Register({ Type = 'anim', Model = 'models/props_c17/oildrum001.mdl' }, 'plain_sent')"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    let mut p = Payload::default();
    p.ints.push(("player", 1));
    p.strs.push(("kind", "sent".into()));
    p.strs.push(("name", "my_sent".into()));
    let reply = rt.actors_mut()[0]
        .dispatch_reply(&snap, "sandbox.spawn_request", &p, 3)
        .unwrap();
    assert!(
        matches!(reply, gm_script::actor::Reply::Cancel),
        "{reply:?}"
    );
    let (called, ply): (String, i32) = rt.actors()[0]
        .eval("return state.called, state.ply")
        .unwrap();
    assert_eq!((called.as_str(), ply), ("my_sent", 1));
    let mut p = Payload::default();
    p.ints.push(("player", 1));
    p.strs.push(("kind", "sent".into()));
    p.strs.push(("name", "plain_sent".into()));
    let reply = rt.actors_mut()[0]
        .dispatch_reply(&snap, "sandbox.spawn_request", &p, 3)
        .unwrap();
    assert!(
        !matches!(reply, gm_script::actor::Reply::Cancel),
        "{reply:?}"
    );
}

#[test]
fn npc_unreachable_memory_writes_an_ignore() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"Think\", \"t\", function()
            local n = setmetatable({ __id = 5, __vars = {}, __tables = {}, __class_meta = __glua_classes.NPC }, __glua_classes.Entity)
            n:RememberUnreachable(Player(1)); state.un = n:IsUnreachable(Player(1)) end)"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(
        writes
            .iter()
            .any(|w| format!("{w:?}").contains("NpcIgnore")),
        "{writes:?}"
    );
    let un: bool = rt.actors()[0].eval("return state.un").unwrap();
    assert!(!un, "nothing ignored in the test snapshot");
}

#[test]
fn phys_bones_translate_between_bones_and_solids() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local e = Entity(5)
             state.r = { e:GetPhysicsObjectCount(), e:TranslatePhysBoneToBone(0), e:TranslatePhysBoneToBone(1),
                 e:TranslateBoneToPhysBone(1), e:TranslateBoneToPhysBone(0), e:TranslateBoneToPhysBone(7) }
         end)",
    );
    let models = oildrum_models();
    let mut snap = (*snapshot()).clone();
    snap.models = gm_script::snapshot::ModelsRef(&models as *const _);
    let snap = Arc::new(snap);
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    let r: Vec<i32> = rt.actors()[0].eval("return state.r").unwrap();
    // one solid on the pelvis: the head (child of the pelvis) resolves to it through its parent
    assert_eq!(r, vec![1, 0, -1, 0, 0, -1]);
}

#[test]
fn npc_max_yaw_write() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"Think\", \"t\", function()
            local n = setmetatable({ __id = 5, __vars = {}, __tables = {}, __class_meta = __glua_classes.NPC }, __glua_classes.Entity)
            n:SetMaxYawSpeed(48) end)"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(
        writes
            .iter()
            .any(|w| matches!(w, QueuedWrite::NpcMaxYaw { ent: 5, rate } if *rate == 48.0)),
        "{writes:?}"
    );
}

#[test]
fn vehicle_tuning_queues_writes() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local v = setmetatable({ __id = 5, __vars = {}, __tables = {}, __class_meta = __glua_classes.Vehicle }, __glua_classes.Entity)
             v:SetMaxThrottle(0.5)
             v:SetSteeringDegrees(20)
             v:SetSpringLength(0, 9)
             v:SetVehicleParams({ engine = { maxSpeed = 30 }, axles = { { suspension = { springConstant = 50 }, wheels = { frictionScale = 0.8 } } } })
         end)",
    );
    rt.actors_mut()[0]
        .dispatch(&snapshot(), "tick.pre", &Payload::default(), 3)
        .unwrap();
    let d = format!("{:?}", rt.drain_writes());
    assert!(d.contains("max_throttle: Some(0.5)"), "{d}");
    assert!(
        d.contains("deg_slow: Some(20.0)") && d.contains("deg_fast: Some(20.0)"),
        "{d}"
    );
    assert!(d.contains("rest: Some(9.0)"), "{d}");
    assert!(
        d.contains("max_fwd: Some(528.0)") && d.contains("tyre_mu: Some(0.8)"),
        "{d}"
    );
    assert!(
        d.contains("VehicleWheel") && d.contains("spring: Some(50.0)"),
        "{d}"
    );
}

#[test]
fn phys_inertia_write_and_torque() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local p = Entity(5):GetPhysicsObject()
             p:SetInertia(Vector(1, 2, 3))
             local lin, ang = p:CalculateVelocityOffset(Vector(0, 0, 10), Entity(5):GetPos() + Vector(1, 0, 0))
             state.ang = ang
             p:ApplyTorqueCenter(Vector(10, 0, 0))
         end)",
    );
    rt.actors_mut()[0]
        .dispatch(&snapshot(), "tick.pre", &Payload::default(), 3)
        .unwrap();
    let d = format!("{:?}", rt.drain_writes());
    assert!(d.contains("SetInertia") && d.contains("2.0, 3.0"), "{d}");
    // unknown inertia divides by 1: the torque adds straight to the fixture's (1,2,3) angular velocity
    assert!(d.contains("angular: Vec3(11.0, 2.0, 3.0)"), "{d}");
    let ang: Vec<f32> = rt.actors()[0]
        .eval("return { state.ang.x, state.ang.y, state.ang.z }")
        .unwrap();
    assert_eq!(ang, vec![0.0, -10.0, 0.0]);
}

#[test]
fn transmit_with_parent_write_and_readback() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local e = Entity(5)
             state.before = e:GetTransmitWithParent()
             e:SetTransmitWithParent(true)
         end)",
    );
    rt.actors_mut()[0]
        .dispatch(&snapshot(), "tick.pre", &Payload::default(), 3)
        .unwrap();
    let before: bool = rt.actors()[0].eval("return state.before").unwrap();
    let d = format!("{:?}", rt.drain_writes());
    assert!(!before);
    assert!(d.contains("TransmitWithParent { ent: 5, on: true }"), "{d}");
}

#[test]
fn player_sprint_force_and_info() {
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local p = Player(1)
             p:StartSprinting()
             state.skin = p:GetInfo('cl_playerskin')
             state.num = p:GetInfoNum('cl_playerskin', 7)
             state.unknown = p:GetInfoNum('cl_nothing', 7)
             state.model = p:GetInfo('cl_playermodel')
         end)",
    );
    rt.actors_mut()[0]
        .dispatch(&snapshot(), "tick.pre", &Payload::default(), 3)
        .unwrap();
    let (skin, num, unknown, model): (String, f64, f64, String) = rt.actors()[0]
        .eval("return state.skin, state.num, state.unknown, state.model")
        .unwrap();
    assert_eq!((skin.as_str(), num, unknown), ("0", 0.0, 7.0));
    assert!(model.is_empty() || !model.contains('/'), "{model}");
    let d = format!("{:?}", rt.drain_writes());
    assert!(
        d.contains("PlayerMoveParam { player: 1, which: 16, value: Some(1.0) }"),
        "{d}"
    );
}

#[test]
fn npc_shoot_params_ask_the_swep() {
    let mut rt = runtime(
        "weapons.Register({ Base = 'weapon_base', Primary = { Ammo = 'SMG1', ClipSize = 30, DefaultClip = 30, Automatic = true }, Secondary = {},
             GetNPCBulletSpread = function(self, prof) state.prof = prof return 7.5 end,
             NPCShoot_Primary = function(self, pos, dir) state.dir = dir return false end }, 'my_swep')",
    );
    let mut p = Payload::default();
    p.ints.push(("npc", 5));
    p.strs.push(("weapon", "my_swep".into()));
    for (k, v) in [
        ("x", 0.0),
        ("y", 0.0),
        ("z", 64.0),
        ("dx", 1.0),
        ("dy", 0.0),
        ("dz", 0.0),
    ] {
        p.floats.push((k, v));
    }
    let reply = rt.actors_mut()[0]
        .dispatch_reply(&snapshot(), "npc.shoot_params", &p, 3)
        .unwrap();
    let text = format!("{reply:?}");
    assert!(text.contains("7.5") && !text.contains("fired"), "{text}");
    let (prof, dx): (f64, f64) = rt.actors()[0]
        .eval("return state.prof, state.dir.x")
        .unwrap();
    assert_eq!((prof, dx), (1.0, 1.0), "WEAPON_PROFICIENCY_AVERAGE is 1");
}

#[test]
fn move_final_fields_round_trip() {
    let mut rt = runtime(
        "hook.Add('Move', 'final', function(ply, mv)
            state.step, state.jump, state.ideal = mv:GetFinalStepHeight(), mv:GetFinalJumpVelocity(), mv:GetFinalIdealVelocity()
            mv:SetFinalStepHeight(30) mv:SetFinalJumpVelocity(300) mv:SetFinalIdealVelocity(400)
         end)",
    );
    let mut p = Payload::default();
    p.ints.push(("player", 1));
    p.ints.push(("buttons", 0));
    p.ints.push(("old_buttons", 0));
    for k in [
        "ox", "oy", "oz", "vx", "vy", "vz", "fx", "fy", "fz", "ax", "ay", "az",
    ] {
        p.floats.push((k, 0.0));
    }
    p.floats.push(("maxspeed", 200.0));
    p.floats.push(("step", 18.0));
    p.floats.push(("jump", 160.0));
    p.floats.push(("ideal", 200.0));
    let reply = rt.actors_mut()[0]
        .dispatch_reply(&snapshot(), "player.move", &p, 3)
        .unwrap();
    let text = format!("{reply:?}");
    assert!(text.ends_with("200 30 300 400\")"), "{text}");
    let seen: (f64, f64, f64) = rt.actors()[0]
        .eval("return state.step, state.jump, state.ideal")
        .unwrap();
    assert_eq!(seen, (18.0, 160.0, 200.0));
}

#[test]
fn npc_move_interval_crouch_pause_writes() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"Think\", \"t\", function()
            local n = setmetatable({ __id = 5, __vars = {}, __tables = {}, __class_meta = __glua_classes.NPC }, __glua_classes.Entity)
            n:SetMoveInterval(0.5) n:SetForceCrouch(true) n:SetForceCrouch(nil) n:MovePause() n:MoveStart() end)"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let snap = snapshot();
    rt.actors_mut()[0]
        .dispatch(&snap, "tick.pre", &Payload::default(), 3)
        .unwrap();
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    let text = format!("{writes:?}");
    for want in [
        "NpcMoveInterval { ent: 5, seconds: 0.5 }",
        "NpcForceCrouch { ent: 5, on: Some(true) }",
        "NpcForceCrouch { ent: 5, on: None }",
        "NpcMovePause { ent: 5, paused: true }",
        "NpcMovePause { ent: 5, paused: false }",
    ] {
        assert!(text.contains(want), "{want} in {text}");
    }
}

#[test]
fn npc_ideal_yaw_writes_and_readback() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"Think\", \"t\", function()
            local n = setmetatable({ __id = 5, __vars = {}, __tables = {}, __class_meta = __glua_classes.NPC }, __glua_classes.Entity)
            n:SetIdealYaw(90) n:UpdateYaw() n:SetIdealYawAndUpdate(-45) n:SetMoveYawLocked(true)
            state.yaw, state.locked = n:GetIdealYaw(), n:IsMoveYawLocked() end)"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let mut s = (*snapshot()).clone();
    if let Some(e) = s.entities.get_mut(&5) {
        e.npc_ideal_yaw = Some(135.0);
        e.npc_move_yaw_locked = true;
    }
    rt.actors_mut()[0]
        .dispatch(&Arc::new(s), "tick.pre", &Payload::default(), 3)
        .unwrap();
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    let text = format!("{writes:?}");
    for want in [
        "NpcIdealYaw { ent: 5, yaw: Some(90.0), update: false }",
        "NpcIdealYaw { ent: 5, yaw: None, update: true }",
        "NpcIdealYaw { ent: 5, yaw: Some(-45.0), update: true }",
        "NpcMoveYawLocked { ent: 5, on: true }",
    ] {
        assert!(text.contains(want), "{want} in {text}");
    }
    let (yaw, locked): (f64, bool) = rt.actors()[0]
        .eval("return state.yaw, state.locked")
        .unwrap();
    assert_eq!((yaw, locked), (135.0, true));
}

#[test]
fn npc_view_offset_write_and_readback() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"Think\", \"t\", function()
            local n = setmetatable({ __id = 5, __vars = {}, __tables = {}, __class_meta = __glua_classes.NPC }, __glua_classes.Entity)
            n:SetViewOffset(Vector(0, 0, 50)) n:SetViewOffset(nil)
            state.z = n:GetViewOffset().z end)"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let mut s = (*snapshot()).clone();
    if let Some(e) = s.entities.get_mut(&5) {
        e.npc_view_offset = Some(glam::Vec3::new(0.0, 0.0, 50.0));
    }
    rt.actors_mut()[0]
        .dispatch(&Arc::new(s), "tick.pre", &Payload::default(), 3)
        .unwrap();
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    let text = format!("{writes:?}");
    assert!(
        text.contains("NpcViewOffset { ent: 5, offset: Some(Vec3(0.0, 0.0, 50.0)) }"),
        "{text}"
    );
    assert!(
        text.contains("NpcViewOffset { ent: 5, offset: None }"),
        "{text}"
    );
    let z: f64 = rt.actors()[0].eval("return state.z").unwrap();
    assert_eq!(z, 50.0);
}

#[test]
fn npc_relationship_write_and_disposition() {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "gm".into(),
        priority: 0,
        source: "hook.Add(\"Think\", \"t\", function()
            local n = setmetatable({ __id = 5, __vars = {}, __tables = {}, __class_meta = __glua_classes.NPC }, __glua_classes.Entity)
            n:AddRelationship(\"player D_HT 99\") n:AddRelationship(\"npc_zombie D_LI 99\") n:AddEntityRelationship(Player(1), D_LI, 99)
            state.disp = n:Disposition(Player(1)) end)"
            .into(),
        config: ActorConfig {
            cpu_budget: std::time::Duration::from_millis(50),
            ..Default::default()
        },
    })
    .unwrap();
    let mut s = (*snapshot()).clone();
    if let Some(e) = s.entities.get_mut(&5) {
        e.npc_relation = Some(3);
    }
    rt.actors_mut()[0]
        .dispatch(&Arc::new(s), "tick.pre", &Payload::default(), 3)
        .unwrap();
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    let text = format!("{writes:?}");
    // one relationship row per call; a player row is the "player" class row
    assert_eq!(text.matches("NpcAiRelationship").count(), 3, "{text}");
    assert!(
        text.contains(
            "NpcAiRelationship { ent: 5, other: None, class: Some(\"player\"), disp: 1, prio: 99 }"
        ),
        "{text}"
    );
    assert!(
        text.contains("NpcAiRelationship { ent: 5, other: None, class: Some(\"npc_zombie\"), disp: 3, prio: 99 }"),
        "{text}"
    );
    assert!(
        text.contains(
            "NpcAiRelationship { ent: 5, other: None, class: Some(\"player\"), disp: 3, prio: 99 }"
        ),
        "{text}"
    );
    let disp: f64 = rt.actors()[0].eval("return state.disp").unwrap();
    assert_eq!(disp, 3.0);
}

#[test]
fn physenv_setters_drive_the_cvars() {
    let mut rt = runtime(
        "hook.Add('Think', 't', function()
            physenv.SetGravity(Vector(0, 0, -300)) physenv.SetTimeScale(0.5) physenv.SetPhysicsPaused(true)
            state.g, state.ts, state.p = physenv.GetGravity().z, physenv.GetTimeScale(), physenv.GetPhysicsPaused()
         end)",
    );
    let mut s = (*snapshot()).clone();
    for (k, v) in [
        ("sv_gravity", "600"),
        ("phys_timescale", "2"),
        ("gm_phys_paused", "1"),
    ] {
        s.cvars.insert(k.into(), v.into());
    }
    rt.actors_mut()[0]
        .dispatch(&Arc::new(s), "tick.pre", &Payload::default(), 3)
        .unwrap();
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    let text = format!("{writes:?}");
    for want in [
        "SetCvar { name: \"sv_gravity\", value: \"300\" }",
        "SetCvar { name: \"phys_timescale\", value: \"0.5\" }",
        "SetCvar { name: \"gm_phys_paused\", value: \"1\" }",
    ] {
        assert!(text.contains(want), "{want} in {text}");
    }
    let (g, ts, p): (f64, f64, bool) = rt.actors()[0]
        .eval("return state.g, state.ts, state.p")
        .unwrap();
    assert_eq!((g, ts, p), (-600.0, 2.0, true));
}

#[test]
fn phys_collide_box_trace_and_npc_class() {
    let mut rt = runtime(
        "hook.Add('Think', 't', function()
            local c = CreatePhysCollideBox(Vector(-8, -8, -8), Vector(8, 8, 8))
            local p, n, f = c:TraceBox(Vector(0, 0, 0), Angle(0, 0, 0), Vector(0, 0, -50), Vector(0, 0, 50), Vector(-1, -1, -1), Vector(1, 1, 1))
            state.f, state.z, state.nz = f, p.z, n.z
            local _, _, f2 = c:TraceBox(Vector(0, 0, 0), Angle(0, 0, 0), Vector(100, 0, -50), Vector(100, 0, 50), Vector(), Vector())
            state.f2 = f2
            local _, _, f3 = c:TraceBox(Vector(100, 0, 0), Angle(0, 0, 0), Vector(100, 0, -50), Vector(100, 0, 50), Vector(), Vector())
            state.f3 = f3
            c:Destroy() state.valid = c:IsValid()
            state.cls, state.cls2 = Add_NPC_Class('CLASS_MY'), Add_NPC_Class('CLASS_OTHER')
            state.again = Add_NPC_Class('CLASS_MY')
         end)",
    );
    rt.actors_mut()[0]
        .dispatch(&snapshot(), "tick.pre", &Payload::default(), 3)
        .unwrap();
    let (f, z, nz, f2, f3): (f64, f64, f64, f64, f64) = rt.actors()[0]
        .eval("return state.f, state.z, state.nz, state.f2, state.f3")
        .unwrap();
    assert!(
        (f - 0.41).abs() < 0.01 && (z + 9.0).abs() < 0.5 && nz == -1.0,
        "{f} {z} {nz}"
    );
    assert_eq!(f2, 1.0);
    assert!((f3 - 0.42).abs() < 0.01, "{f3}");
    let (valid, cls, cls2, again): (bool, f64, f64, f64) = rt.actors()[0]
        .eval("return state.valid, state.cls, state.cls2, state.again")
        .unwrap();
    assert_eq!((valid, cls, cls2, again), (false, 26.0, 27.0, 26.0));
}

#[test]
fn client_console_line_runs_a_script_command() {
    let mut rt = runtime(
        "concommand.Add('my_cmd', function(ply, cmd, args, str) state.got = cmd .. '|' .. (args[2] or '') .. '|' .. str .. '|' .. tostring(ply:IsValid()) end, nil, 'does things')
         state.help = select(3, concommand.GetTable()).my_cmd",
    );
    let mut p = Payload::default();
    p.ints.push(("player", 1));
    p.strs.push(("cmd", "my_cmd".into()));
    p.strs.push(("args", "a b".into()));
    let reply = rt.actors_mut()[0]
        .dispatch_reply(&snapshot(), "player.command", &p, 3)
        .unwrap();
    assert!(format!("{reply:?}").contains("\"1\""), "{reply:?}");
    let (got, help): (String, String) =
        rt.actors()[0].eval("return state.got, state.help").unwrap();
    assert_eq!(
        (got.as_str(), help.as_str()),
        ("my_cmd|b|a b|true", "does things")
    );
    p.strs.retain(|(k, _)| *k != "cmd");
    p.strs.push(("cmd", "unknown_cmd".into()));
    let reply = rt.actors_mut()[0]
        .dispatch_reply(&snapshot(), "player.command", &p, 4)
        .unwrap();
    assert!(!format!("{reply:?}").contains("\"1\""), "{reply:?}");
}

#[test]
fn swep_freeze_movement_pins_the_owner() {
    let mut rt = runtime(
        "state.freeze = true
         weapons.Register({ Base = 'weapon_base', Primary = { Ammo = 'SMG1', ClipSize = 30, DefaultClip = 30 }, Secondary = {},
             FreezeMovement = function(self) return state.freeze end,
             GetCapabilities = function(self) return 0 end }, 'cap_swep')
         local w = weapons.__ensure('cap_swep', 1)
         rawset(w, 'Owner', Player(1))
         weapons.__live(w, true)",
    );
    rt.actors_mut()[0]
        .dispatch(&snapshot(), "tick.pre", &Payload::default(), 3)
        .unwrap();
    let text = format!(
        "{:?}",
        rt.drain_writes()
            .into_iter()
            .map(|w| w.3)
            .collect::<Vec<_>>()
    );
    for which in [0, 1, 2, 4] {
        let want = format!("PlayerMoveParam {{ player: 1, which: {which}, value: Some(0.0) }}");
        assert!(text.contains(&want), "{want} in {text}");
    }
    rt.actors_mut()[0]
        .dispatch(&snapshot(), "tick.pre", &Payload::default(), 4)
        .unwrap();
    assert!(!format!("{:?}", rt.drain_writes()).contains("PlayerMoveParam"));
    rt.actors()[0].eval::<()>("state.freeze = false").unwrap();
    rt.actors_mut()[0]
        .dispatch(&snapshot(), "tick.pre", &Payload::default(), 5)
        .unwrap();
    let text = format!(
        "{:?}",
        rt.drain_writes()
            .into_iter()
            .map(|w| w.3)
            .collect::<Vec<_>>()
    );
    assert!(
        text.contains("PlayerMoveParam { player: 1, which: 0, value: None }"),
        "{text}"
    );
    let mut p = Payload::default();
    p.ints.push(("npc", 5));
    p.strs.push(("weapon", "cap_swep".into()));
    for (k, v) in [
        ("x", 0.0),
        ("y", 0.0),
        ("z", 64.0),
        ("dx", 1.0),
        ("dy", 0.0),
        ("dz", 0.0),
    ] {
        p.floats.push((k, v));
    }
    let reply = rt.actors_mut()[0]
        .dispatch_reply(&snapshot(), "npc.shoot_params", &p, 6)
        .unwrap();
    assert!(format!("{reply:?}").contains("fired"), "{reply:?}");
}

#[test]
fn nav_area_block_writes() {
    let mut rt = runtime(
        "hook.Add('Think', 't', function()
            local a = setmetatable({ __id = 7, __center = Vector() }, __glua_classes.CNavArea)
            a:MarkAsBlocked() a:MarkAsUnblocked()
            state.blocked, state.n = a:IsBlocked(), #navmesh.GetBlockedAreas()
         end)",
    );
    rt.actors_mut()[0]
        .dispatch(&snapshot(), "tick.pre", &Payload::default(), 3)
        .unwrap();
    let text = format!(
        "{:?}",
        rt.drain_writes()
            .into_iter()
            .map(|w| w.3)
            .collect::<Vec<_>>()
    );
    assert!(
        text.contains("NavBlock { poly: 7, on: true }")
            && text.contains("NavBlock { poly: 7, on: false }"),
        "{text}"
    );
    let (blocked, n): (bool, f64) = rt.actors()[0]
        .eval("return state.blocked, state.n")
        .unwrap();
    assert_eq!((blocked, n), (false, 0.0));
}

#[test]
fn nav_area_visibility_and_water() {
    let mut g = gm_nav::Geometry::default();
    let sq = [
        Vec3::new(-1024.0, -1024.0, 0.0),
        Vec3::new(1024.0, -1024.0, 0.0),
        Vec3::new(1024.0, 1024.0, 0.0),
        Vec3::new(-1024.0, 1024.0, 0.0),
    ];
    g.push_polygon(&sq);
    let mut rev = sq;
    rev.reverse();
    g.push_polygon(&rev);
    g.mins = Vec3::new(-1024.0, -1024.0, -8.0);
    g.maxs = Vec3::new(1024.0, 1024.0, 8.0);
    let mesh = gm_nav::NavMesh::from_geometry(&g, &gm_nav::NavParams::default()).unwrap();
    // a pool over the whole x < 0 half; a boxes world has no PVS data, so everything is visible
    let world = gm_collision::World::from_boxes_contents(&[(
        Vec3::new(-3000.0, -3000.0, -64.0),
        Vec3::new(0.0, 3000.0, 64.0),
        0x20,
    )]);
    let mut rt = runtime(
        "events.on('tick.pre', function()
             local all = navmesh.GetAllNavAreas()
             local wet_ok, n = true, 0
             for _, a in ipairs(all) do
                 if a:IsUnderwater() ~= (a:GetCenter().x < 0) then wet_ok = false end
                 if a:IsUnderwater() then n = n + 1 end
             end
             local a, b = all[1], all[#all]
             state.r = { #all >= 2, wet_ok, n > 0, a:IsPotentiallyVisible(b), a:IsPartiallyVisible(b:GetCenter()),
                         a:IsCompletelyVisible(b:GetCenter()), #a:GetVisibleAreas() == #all }
         end)",
    );
    let mut snap = (*snapshot()).clone();
    snap.nav = gm_script::snapshot::NavRef(&mesh as *const _);
    snap.world = gm_script::snapshot::WorldRef(&world as *const _);
    let snap = Arc::new(snap);
    let a = &mut rt.actors_mut()[0];
    a.dispatch(&snap, "tick.pre", &Payload::default(), 1)
        .unwrap();
    let r: Vec<bool> = a.eval("return state.r").unwrap();
    assert_eq!(r, vec![true; 7], "{r:?}");
    let mut dry = (*snapshot()).clone();
    dry.nav = gm_script::snapshot::NavRef(&mesh as *const _);
    a.dispatch(&Arc::new(dry), "tick.pre", &Payload::default(), 2)
        .unwrap();
    let r: Vec<bool> = a.eval("return state.r").unwrap();
    assert_eq!((r[2], r[3], r[6]), (false, false, false), "{r:?}");
}

#[test]
fn saverestore_round_trips_global_hooks_and_runs_the_gm_hooks() {
    let mut rt = runtime(
        "state.saved, state.restored, state.got = 0, 0, nil
         hook.Add('Saved', 't', function() state.saved = state.saved + 1 end)
         hook.Add('Restored', 't', function() state.restored = state.restored + 1 end)
         saverestore.AddSaveHook('MyAddon', function(save)
             saverestore.WriteVar(42, save)
             saverestore.WriteTable({ name = 'x', list = { 1, 2 } }, save)
         end)
         saverestore.AddRestoreHook('MyAddon', function(save)
             state.got = { saverestore.ReadVar(save), saverestore.ReadTable(save) }
         end)",
    );
    let a = &mut rt.actors_mut()[0];
    let mut p = Payload::default();
    p.strs.push(("ents", "7,8".to_string()));
    let reply = a.dispatch_reply(&snapshot(), "game.save", &p, 1).unwrap();
    let text = match reply {
        gm_script::actor::Reply::Text(t) => t,
        other => panic!("{other:?}"),
    };
    assert!(
        text.starts_with("G\tMyAddon\t") && !text.contains("E\t"),
        "{text}"
    );
    let mut r = Payload::default();
    r.strs.push(("data", format!("{text}\n")));
    a.dispatch(&snapshot(), "game.restore", &r, 2).unwrap();
    let (saved, restored, v, name, n): (f64, f64, f64, String, f64) = a
        .eval("return state.saved, state.restored, state.got[1], state.got[2].name, #state.got[2].list")
        .unwrap();
    assert_eq!(
        (saved, restored, v, name.as_str(), n),
        (1.0, 1.0, 42.0, "x", 2.0)
    );
}

#[test]
fn armor_reduction_hook_answers_the_split_only_when_it_changes_it() {
    let mut rt = runtime(
        "hook.Add('HandlePlayerArmorReduction', 't', function(ply, dmginfo)
             if state.off then return end
             local a, dmg = ply:Armor(), dmginfo:GetDamage()
             local toArmor = math.min(a, dmg * 0.5)
             ply:SetArmor(a - toArmor)
             dmginfo:SetDamage(dmg - toArmor)
         end)",
    );
    let a = &mut rt.actors_mut()[0];
    let mut p = Payload::default();
    p.ints.push(("player", 1));
    p.floats.push(("amount", 40.0));
    p.ints.push(("type", 2));
    p.ints.push(("armor", 50));
    let reply = a
        .dispatch_reply(&snapshot(), "player.armor", &p, 1)
        .unwrap();
    assert_eq!(format!("{reply:?}"), "Text(\"20.0000,30\")");
    a.eval::<()>("state.off = true").unwrap();
    let reply = a
        .dispatch_reply(&snapshot(), "player.armor", &p, 2)
        .unwrap();
    assert_eq!(format!("{reply:?}"), "Pass");
    let (armor, set): (bool, bool) = a
        .eval("local p = Player(1) return rawget(p, 'Armor') == nil, rawget(p, 'SetArmor') == nil")
        .unwrap();
    assert!(armor && set);
}

#[test]
fn lua_error_and_load_save_hooks_reach_the_gamemode() {
    let mut rt = runtime(
        "state.err, state.load, state.failed = nil, nil, nil
         hook.Add('OnLuaError', 't', function(err, realm, stack, name, id) state.err = { err, realm, name, id } end)
         hook.Add('LoadGModSave', 't', function(data, map, ts) state.load = { data, map, ts } end)
         hook.Add('LoadGModSaveFailed', 't', function(err) state.failed = err end)",
    );
    let a = &mut rt.actors_mut()[0];
    let mut p = Payload::default();
    p.strs.push(("name", "my_addon".to_string()));
    p.strs.push(("error", "boom".to_string()));
    a.dispatch(&snapshot(), "lua.error", &p, 1).unwrap();
    let mut r = Payload::default();
    r.strs.push(("data", "G\tX\t[]\n".to_string()));
    r.strs.push(("map", "gm_construct".to_string()));
    r.ints.push(("timestamp", 1700000000));
    a.dispatch(&snapshot(), "game.restore", &r, 2).unwrap();
    let mut f = Payload::default();
    f.strs.push(("error", "no such file".to_string()));
    a.dispatch(&snapshot(), "game.restore_failed", &f, 3)
        .unwrap();
    let (err, realm, name, id): (String, String, String, f64) = a
        .eval("return state.err[1], state.err[2], state.err[3], state.err[4]")
        .unwrap();
    assert_eq!(
        (err.as_str(), realm.as_str(), name.as_str(), id),
        ("boom", "server", "my_addon", 0.0)
    );
    let (data, map, ts, failed): (String, String, f64, String) = a
        .eval("return state.load[1], state.load[2], state.load[3], state.failed")
        .unwrap();
    assert_eq!(
        (data.as_str(), map.as_str(), ts, failed.as_str()),
        ("G\tX\t[]\n", "gm_construct", 1700000000.0, "no such file")
    );
}

#[test]
fn compat_sql_debug_digests_and_materials() {
    let rt = runtime(
        "state = {}
         sql.Query('CREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY, name TEXT, score REAL)')
         sql.Query(\"INSERT INTO t (name, score) VALUES ('a', 1.5), ('b', 2), ('c', 10)\")
         sql.Query(\"UPDATE t SET score = score + 1 WHERE name = 'b'\")
         sql.Query(\"DELETE FROM t WHERE name = 'c'\")
         state.rows = sql.Query('SELECT name, score FROM t ORDER BY score DESC')
         state.count = sql.QueryValue('SELECT COUNT(*) FROM t')
         state.exists = sql.TableExists('t') and not sql.TableExists('nope')
         state.bad = sql.Query('SELEC nonsense')
         state.err = sql.LastError()
         state.sha1 = util.SHA1('abc')
         state.md5 = util.MD5('abc')
         local info = debug.getinfo(1, 'S')
         state.what = info and info.what
         local m = Material('models/debug/debugwhite', 'noclamp smooth')
         m:SetFloat('$alpha', 0.5)
         state.alpha = m:GetFloat('$alpha')
         state.flag = m:GetInt('$noclamp')
         state.ev = util.GetAnimEventIDByName('AE_NPC_LEFTFOOT')",
    );
    let dump: String = rt.actors()[0]
        .eval("local t = {} for k, v in pairs(state) do t[#t + 1] = k .. '=' .. tostring(v) end table.sort(t) return table.concat(t, ' ')")
        .unwrap();
    let (count, exists, err, sha1, md5): (String, bool, String, String, String) = rt.actors()[0]
        .eval("return state.count, state.exists, state.err, state.sha1, state.md5")
        .expect(&dump);
    assert_eq!(count, "2");
    assert!(exists);
    assert!(err.contains("syntax error"), "{err}");
    assert_eq!(sha1, "a9993e364706816aba3e25717850c26c9cd0d89d");
    assert_eq!(md5, "900150983cd24fb0d6963f7d28e17f72");
    let (first, score, bad): (String, String, bool) = rt.actors()[0]
        .eval("return state.rows[1].name, state.rows[1].score, state.bad")
        .unwrap();
    assert_eq!((first.as_str(), score.as_str(), bad), ("b", "3", false));
    let (what, alpha, flag, ev): (String, f64, i64, i64) = rt.actors()[0]
        .eval("return state.what, state.alpha, state.flag, state.ev")
        .unwrap();
    assert_eq!((what.as_str(), alpha, flag, ev), ("Lua", 0.5, 1, 2050));
}

#[test]
fn bone_positions_sub_materials_flags_and_drag_coefficients() {
    let mut rt = runtime(
        "state = {}
         local e = Entity(5)
         e:ManipulateBonePosition(1, Vector(0, 0, 3))
         e:SetSubMaterial(2, 'models/debug/debugwhite')
         e:SetSubMaterial()
         e:GetPhysicsObject():SetDragCoefficient(2.5)
         e:GetPhysicsObject():SetAngleDragCoefficient(0)
         e:AddFlags(FL_NOTARGET)
         state.flag = e:IsFlagSet(FL_NOTARGET)
         e:RemoveFlags(FL_NOTARGET)
         state.flag2 = e:IsFlagSet(FL_NOTARGET)
         e:SetSpawnEffect(true)
         state.fx = e:GetSpawnEffect()
         state.sub = e:GetSubMaterial(0)
         local p = e:GetPhysicsObject()
         p:AddGameFlag(FVPHYSICS_NO_IMPACT_DMG)
         state.gf = p:HasGameFlag(FVPHYSICS_NO_IMPACT_DMG)
         state.len = e:BoneLength(0)",
    );
    let (flag, flag2, fx, sub, gf): (bool, bool, bool, String, bool) = rt.actors()[0]
        .eval("return state.flag, state.flag2, state.fx, state.sub, state.gf")
        .unwrap();
    assert_eq!(
        (flag, flag2, fx, sub.as_str(), gf),
        (true, false, true, "", true)
    );
    let writes: Vec<QueuedWrite> = rt.drain_writes().into_iter().map(|w| w.3).collect();
    assert!(
        matches!(&writes[0], QueuedWrite::SetBoneManip { ent: 5, bone: 1, scale: None, angles: None, pos: Some(p) } if p.z == 3.0)
    );
    assert!(
        matches!(&writes[1], QueuedWrite::SetSubMaterial { ent: 5, index: Some(2), material: Some(m) } if m == "models/debug/debugwhite")
    );
    assert!(matches!(
        &writes[2],
        QueuedWrite::SetSubMaterial {
            ent: 5,
            index: None,
            material: None
        }
    ));
    assert!(
        matches!(&writes[3], QueuedWrite::SetDragCoefficient { ent: 5, linear: Some(l), angular: None } if *l == 2.5)
    );
    assert!(
        matches!(&writes[4], QueuedWrite::SetDragCoefficient { ent: 5, linear: None, angular: Some(a) } if *a == 0.0)
    );
}
