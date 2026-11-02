//! net, ConVar, concommand, file, CTakeDamageInfo and the GM hook bridge.
use glam::Vec3;
use gm_script::api::{Payload, QueuedWrite};
use gm_script::snapshot::PlayerSnap;
use gm_script::{ActorConfig, ActorSpec, Runtime, Snapshot};
use std::sync::Arc;

fn snapshot() -> Arc<Snapshot> {
    let mut s = Snapshot::default();
    s.cvars.insert("sv_gravity".into(), "600".into());
    s.entities.insert(
        5,
        gm_script::snapshot::EntitySnap {
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
            asleep: false,
            parent: 0,
            color: [10, 20, 30, 40],
            material: "models/debug/debugwhite".into(),
            render_mode: 1,
            render_fx: 0,
            sequence: "idle".into(),
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
        6,
        gm_script::snapshot::EntitySnap {
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
            asleep: false,
            parent: 5,
            color: [10, 20, 30, 40],
            material: "models/debug/debugwhite".into(),
            render_mode: 1,
            render_fx: 0,
            sequence: "idle".into(),
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
    s.players.insert(
        1,
        PlayerSnap {
            id: 1,
            origin: Vec3::ZERO,
            velocity: Vec3::ZERO,
            view_angles: Vec3::ZERO,
            held: None,
            alive: true,
            health: 100,
            armor: 0,
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
        matches!(&writes[0], QueuedWrite::SetBoneManip { ent: 5, bone: 1, scale: Some(s), angles: None } if s.x == 3.0)
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
