//! Playable client: a window with the live world render, keyboard and mouse input, and an
//! in-process server reached through the loopback transport (the same protocol as over UDP).
//!
//! `GMOD_DIR=... cargo run --release -p gm-client -- [map] [yaw] [pitch]`
//! WASD move, mouse look, space jump, ctrl duck, shift run, LMB/RMB fire, 1/2/3 weapon
//! (none, physgun, tool gun), Q spawn a prop, Z undo, Esc release the mouse / quit,
//! ` (grave) console: `help`, `spawn [model]`, `undo`, `cleanup`, `clear`, `quit`, server
//! console lines (`status`, `cvar`, `save`, `load`, ...).

mod derma;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, bail, Context, Result};
use glam::Vec3;
use gm_move::buttons;
use gm_net::msg::PlayerSnap;
use gm_net::{ClientNet, Command, Loopback, LoopbackEndpoint, NetCmd, ServerNet, Transport};
use gm_render::ui::{UiItem, UiRenderer};
use gm_render::world::{DrawItem, ModelTarget};
use gm_render::{Camera, Gpu, WorldRenderer};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

/// `cl_weaponcolor` default "0.30 1.80 2.10" clamped to the display range: the beam and
/// glow tint of the physgun.
const PHYSGUN_BEAM_COLOR: [f32; 3] = [0.3, 1.0, 1.0];

const SPAWN_MODEL: &str = "models/props_c17/oildrum001.mdl";

struct Sim {
    server: gm_server::Server<'static>,
    net: ServerNet,
    lo: Loopback,
    s_ep: LoopbackEndpoint,
    /// The client's side: the loopback to the embedded server, or UDP to a `--connect` server.
    c_ep: Box<dyn Transport>,
    /// `--connect`: the embedded server stays idle, every packet goes to the remote one.
    remote: bool,
    client: ClientNet,
    collision: &'static gm_collision::World,
    /// The local player runs every usercmd at once (`gm_net::Prediction`) and is reconciled
    /// with the server's state when a snapshot acknowledges the usercmd.
    pred: Option<gm_net::Prediction<'static>>,
    cmd_tick: u32,
    interp: gm_net::Interpolator,
    /// Other players, interpolated like props (origin, yaw as the angles).
    player_interp: gm_net::Interpolator,
    prev_origin: Vec3,
    origin: Vec3,
    ducked: bool,
    /// Client realm of the addon runtime: the server's client-realm packages are downloaded
    /// over the loopback like from a dedicated server, verified and loaded here.
    script: gm_script::Runtime,
    addons_seen: BTreeSet<[u8; 32]>,
    cache: std::path::PathBuf,
    last_snapshot_tick: u32,
    /// `draw.*` of the client actors' last `render.hud`, drawn over the HUD.
    hud_draws: Vec<gm_script::DrawCmd>,
}

impl Sim {
    /// The server's client-realm addons: a cached package by sha loads at once, a missing one is
    /// requested and loaded when the download verifies (the same path as `gm-bot --cache`).
    fn sync_addons(&mut self) {
        for a in self.client.server_addons.clone() {
            if !self.addons_seen.insert(a.sha) {
                continue;
            }
            let path = self
                .cache
                .join(format!("{}.gmr", gm_net::msg::sha_hex(&a.sha)));
            let cached = std::fs::read(&path)
                .ok()
                .and_then(|b| gm_script::Package::read(&b).ok())
                .is_some_and(|p| p.sha256 == a.sha);
            if cached {
                load_client_package(&mut self.script, &path, "cached");
            } else {
                self.client.want_addon(a.sha);
            }
        }
        for (sha, bytes) in std::mem::take(&mut self.client.downloaded) {
            if !gm_script::Package::read(&bytes).is_ok_and(|p| p.sha256 == sha) {
                eprintln!(
                    "addon download {} failed verification",
                    gm_net::msg::sha_hex(&sha)
                );
                continue;
            }
            let path = self
                .cache
                .join(format!("{}.gmr", gm_net::msg::sha_hex(&sha)));
            if std::fs::create_dir_all(&self.cache)
                .and_then(|_| std::fs::write(&path, &bytes))
                .is_err()
            {
                eprintln!("addon cache {}: cannot write", path.display());
                continue;
            }
            load_client_package(&mut self.script, &path, "downloaded");
        }
    }

    fn tick(&mut self, cmd: NetCmd) {
        self.cmd_tick += 1;
        if let Some(p) = &mut self.pred {
            p.predict(self.cmd_tick, &cmd);
        }
        let seen = self.client.world.tick;
        self.client.tick(cmd, &mut *self.c_ep);
        if !self.remote {
            self.lo.tick();
            self.net.receive(&mut self.server, &mut self.s_ep);
            self.net.tick(&mut self.server);
            self.net.send(&self.server, &mut self.s_ep);
        }
        self.client.receive(&mut *self.c_ep);
        self.sync_addons();
        let fresh = self.client.server_tick != self.last_snapshot_tick;
        self.last_snapshot_tick = self.client.server_tick;
        self.script.client_tick(&self.client.world, fresh);
        let inbox = std::mem::take(&mut self.client.addon_msgs);
        self.script.client_net(&self.client.world, inbox);
        for (_, name, data) in self.script.drain_net() {
            self.client.send_addon(&name, &data);
        }
        let w = &self.client.world;
        if w.tick != seen {
            self.interp.push(w.tick, &w.entities);
            let players = w
                .players
                .iter()
                .map(|p| {
                    (
                        p.id,
                        gm_net::msg::EntityState {
                            id: p.id,
                            model: String::new(),
                            model_id: 0,
                            pos: p.origin,
                            angles: Vec3::new(0.0, p.view_angles.y, 0.0),
                            vel: p.velocity,
                            angvel: Vec3::ZERO,
                            frozen: false,
                            skin: 0,
                            body: String::new(),
                            owner: 0,
                        },
                    )
                })
                .collect();
            self.player_interp.push(w.tick, &players);
        }
        if let Some(l) = &w.local {
            let p = self
                .pred
                .get_or_insert_with(|| gm_net::Prediction::new(self.collision, l.state));
            p.reconcile(w.last_cmd_tick, &l.state);
        }
        if let Some(p) = &self.pred {
            self.prev_origin = self.origin;
            self.origin = p.state.origin;
            self.ducked = p.state.ducked;
        }
    }
}

struct Gfx {
    /// Spawn-menu icon textures, kept alive for their UI bind groups.
    icon_textures: Vec<wgpu::Texture>,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    gpu: Gpu,
    world: WorldRenderer,
    depth: wgpu::TextureView,
    models: std::collections::HashMap<String, Option<gm_render::world::DynModel>>,
    player_box: gm_render::world::DynModel,
    /// Physgun beam materials: `sprites/physbeam` along the beam, `physg_glow1` at its end,
    /// `physg_glow2` at the muzzle (the game's materials, additive sprites).
    beam_mat: Option<wgpu::BindGroup>,
    /// `sprites/physbeama`: the brighter beam while something is held.
    beam_active_mat: Option<wgpu::BindGroup>,
    glow1: Option<wgpu::BindGroup>,
    glow2: Option<wgpu::BindGroup>,
    ui: UiRenderer,
}

const CONSOLE_LOG_LINES: usize = 500;

/// World model held by another player, by `gm_server::LOADOUT` index (`PlayerSnap::active`);
/// it shares `ValveBiped.Bip01_R_Hand` with the player skeleton and is bone-merged onto it.
fn world_model(active: u8) -> Option<&'static str> {
    Some(match gm_server::LOADOUT.get(active as usize)? {
        &"gmod_tool" => "models/weapons/w_toolgun.mdl",
        &"weapon_357" => "models/weapons/w_357.mdl",
        &"weapon_ar2" => "models/weapons/w_irifle.mdl",
        &"weapon_crossbow" => "models/weapons/w_crossbow.mdl",
        &"weapon_crowbar" => "models/weapons/w_crowbar.mdl",
        &"weapon_frag" => "models/weapons/w_grenade.mdl",
        &"weapon_physcannon" | &"weapon_physgun" => "models/weapons/w_physics.mdl",
        &"weapon_pistol" => "models/weapons/w_pistol.mdl",
        &"weapon_rpg" => "models/weapons/w_rocket_launcher.mdl",
        &"weapon_shotgun" => "models/weapons/w_shotgun.mdl",
        &"weapon_smg1" => "models/weapons/w_smg1.mdl",
        _ => return None,
    })
}

/// Stools the server implements in Rust (`Sandbox::knows_tool`).
const TOOLS: [&str; 5] = ["weld", "rope", "axis", "nocollide", "duplicator"];

/// What the last laid-out menu frame left for input and icon rendering.
#[derive(Default)]
struct MenuCache {
    /// Models whose spawn icons are on screen.
    visible: Vec<String>,
    max_scroll: f32,
    /// The tool's setting rows as laid out.
    rows: Vec<derma::ControlRow>,
}

struct SpawnMenu {
    st: derma::MenuState,
}

/// The context menu of a prop (hold C, right-click it): the entries the server offered, drawn
/// at the click.
struct ContextMenu {
    names: Vec<String>,
    x: f32,
    y: f32,
}

/// The tool settings panel inside the spawn menu: (x, y, width, first row y, row height).
/// The value a slider at fraction `t` of its track shows: whole numbers when the range is wider
/// than 20, else two decimals.
fn slider_value(s: &gm_server::ToolSetting, t: f32) -> String {
    let (lo, hi) = (s.min.max(-1.0e6), s.max.min(1.0e6));
    let v = lo + (hi - lo) * t;
    let v = if (hi - lo) > 20.0 {
        v.round()
    } else {
        (v * 100.0).round() / 100.0
    };
    v.to_string()
}

/// The `Gfx::models` key of a player's model with its skin and body groups.
fn player_model_key(p: &PlayerSnap) -> String {
    format!(
        "{}#{}#{}",
        gm_server::player_models::path_of_index(p.model),
        p.skin,
        p.body
    )
}

/// GMod's default `viewmodel_fov`.
const VIEWMODEL_FOV: f32 = 54.0;
/// Hands drawn on the `c_` viewmodels' arm bones when the local player's model is not known
/// yet (the game's default, kleiner's citizen hands).
const HANDS_MODEL: &str = "models/weapons/c_arms_citizen.mdl";

/// A weapon slot of the client: the server class, its `c_` viewmodel and the viewmodel's
/// idle / draw / attack sequences, plus the sequence while the physgun holds a prop.
struct Viewmodel {
    class: &'static str,
    name: &'static str,
    path: &'static str,
    idle: &'static str,
    draw: &'static str,
    fire: &'static str,
    hold: Option<&'static str>,
    reload: Option<&'static str>,
    /// Secondary fire sequence; `None` for weapons without one (the right click does nothing).
    alt: Option<&'static str>,
    /// Pump after a shot (shotgun) and dry fire on an empty clip: played on the server's
    /// viewmodel events (`PlayerSnap::vm_act`), not guessed from the buttons.
    pump: Option<&'static str>,
    dry: Option<&'static str>,
    /// Shell-by-shell reload: one shell going in and the end (shotgun `reload2` / `reload3`;
    /// `reload` is its start).
    reload_shell: Option<&'static str>,
    reload_end: Option<&'static str>,
}

const fn vm(
    class: &'static str,
    name: &'static str,
    model: &'static str,
    idle: &'static str,
    draw: &'static str,
    fire: &'static str,
) -> Viewmodel {
    Viewmodel {
        class,
        name,
        path: model,
        idle,
        draw,
        fire,
        hold: None,
        reload: None,
        alt: None,
        pump: None,
        dry: None,
        reload_shell: None,
        reload_end: None,
    }
}

/// Slots on the number keys 1..9, 0; the physgun is slot 0 (key 1).
const WEAPONS: [Viewmodel; 13] = [
    Viewmodel {
        hold: Some("hold_idle"),
        ..vm(
            "weapon_physgun",
            "physgun",
            "models/weapons/c_superphyscannon.mdl",
            "idle",
            "draw",
            "fire",
        )
    },
    vm(
        "gmod_tool",
        "toolgun",
        "models/weapons/c_toolgun.mdl",
        "idle01",
        "draw",
        "fire01",
    ),
    vm(
        "weapon_crowbar",
        "crowbar",
        "models/weapons/c_crowbar.mdl",
        "idle01",
        "draw",
        "misscenter1",
    ),
    Viewmodel {
        reload: Some("reload"),
        dry: Some("fireempty"),
        ..vm(
            "weapon_pistol",
            "pistol",
            "models/weapons/c_pistol.mdl",
            "idle01",
            "draw",
            "fire",
        )
    },
    Viewmodel {
        reload: Some("reload"),
        ..vm(
            "weapon_357",
            ".357",
            "models/weapons/c_357.mdl",
            "idle01",
            "draw",
            "fire",
        )
    },
    Viewmodel {
        alt: Some("altfire"),
        reload: Some("reload"),
        dry: Some("dryfire"),
        ..vm(
            "weapon_smg1",
            "smg",
            "models/weapons/c_smg1.mdl",
            "idle01",
            "draw",
            "fire01",
        )
    },
    Viewmodel {
        alt: Some("IR_fire2"),
        ..vm(
            "weapon_ar2",
            "ar2",
            "models/weapons/c_irifle.mdl",
            "IR_idle",
            "IR_draw",
            "IR_fire",
        )
    },
    Viewmodel {
        alt: Some("altfire"),
        reload: Some("reload1"),
        reload_shell: Some("reload2"),
        reload_end: Some("reload3"),
        pump: Some("pump"),
        dry: Some("dryfire"),
        ..vm(
            "weapon_shotgun",
            "shotgun",
            "models/weapons/c_shotgun.mdl",
            "idle01",
            "draw",
            "fire01",
        )
    },
    Viewmodel {
        reload: Some("reload"),
        ..vm(
            "weapon_crossbow",
            "crossbow",
            "models/weapons/c_crossbow.mdl",
            "idle",
            "draw",
            "fire",
        )
    },
    Viewmodel {
        reload: Some("reload"),
        ..vm(
            "weapon_rpg",
            "rpg",
            "models/weapons/c_rpg.mdl",
            "idle1",
            "draw",
            "fire",
        )
    },
    Viewmodel {
        hold: Some("hold_idle"),
        alt: Some("altfire"),
        ..vm(
            "weapon_physcannon",
            "physcannon",
            "models/weapons/c_physcannon.mdl",
            "idle",
            "draw",
            "fire",
        )
    },
    Viewmodel {
        alt: Some("lob"),
        ..vm(
            "weapon_frag",
            "frag",
            "models/weapons/c_grenade.mdl",
            "idle01",
            "draw",
            "throw",
        )
    },
    // the game's camera has no weapon model: nothing is drawn (the load of "" fails once)
    vm("gmod_camera", "camera", "", "idle", "draw", "fire"),
];

/// `NetCmd::weapon`: 1 while the physgun is out (the server's physgun acts on attack), 2 for
/// the toolgun, 0 for everything else.
fn net_weapon(weapon: u8) -> u8 {
    match weapon {
        0 => 1,
        1 => 2,
        _ => 0,
    }
}

/// GMod's walk / run split for the player animation (`animations.lua`): below this ground
/// speed the walk grid plays, above it the run grid.
const PLAYER_RUN_SPEED: f32 = 150.0;

/// Sequence of the shared player animation model and its pose parameters for a player's
/// state: idle / walk / run grids with `move_x` / `move_y` from the velocity relative to the
/// facing (Source's `move_x = cos`, `move_y = -sin` of the movement yaw), crouch variants.
/// The hold type of the active weapon: the suffix of the m_anm sequences (`idle_pistol`,
/// `run_smg1`, ...), as the game's weapon hold types; `fist` when nothing is held.
/// The m_anm hold type of the active weapon: the game's holdtype from the weapon table (the
/// toolgun uses the revolver hold), `fist` with nothing out; the SMG's sequences are `*_smg1`.
fn hold_type(active: u8) -> &'static str {
    match gm_server::LOADOUT
        .get(active as usize)
        .and_then(|c| gm_server::weapons::def(c))
        .map(|d| d.hold_type)
    {
        Some("smg") => "smg1",
        Some(h) => h,
        None => "fist",
    }
}

fn player_sequence(p: &PlayerSnap) -> (String, Vec<(&'static str, f32)>) {
    let speed = p.velocity.truncate().length();
    let hold = hold_type(p.active);
    // the aim layers of every sequence follow the eye pitch; the body faces the eye yaw
    let mut params = vec![("aim_pitch", p.view_angles.x), ("aim_yaw", 0.0)];
    if !p.on_ground {
        return (format!("jump_{hold}"), params);
    }
    if speed < 0.5 {
        let seq = match (p.ducked, hold) {
            (true, "fist") => "cidle_all".to_string(),
            (true, _) => format!("cidle_{hold}"),
            (false, "fist") => "idle_all_01".to_string(),
            (false, _) => format!("idle_{hold}"),
        };
        return (seq, params);
    }
    let yaw = p.velocity.y.atan2(p.velocity.x) - p.view_angles.y.to_radians();
    params.extend([("move_x", yaw.cos()), ("move_y", -yaw.sin())]);
    let seq = match (p.ducked, speed > PLAYER_RUN_SPEED, hold) {
        (true, _, "fist") => "cwalk_all".to_string(),
        (true, _, _) => format!("cwalk_{hold}"),
        (false, true, "fist") => "run_all_01".to_string(),
        (false, true, _) => format!("run_{hold}"),
        (false, false, "fist") => "walk_all".to_string(),
        (false, false, _) => format!("walk_{hold}"),
    };
    (seq, params)
}

/// The m_anm gesture another player plays for a viewmodel event (`PlayerSnap::vm_act`), by hold
/// type: `range_*` on a shot, `reload_*` where the animation model has one.
fn gesture_for(act: u8, hold: &str) -> Option<&'static str> {
    Some(match (act, hold) {
        (6, "pistol") => "range_pistol",
        (6, "smg1") => "range_smg1",
        (6, "ar2") => "range_ar2",
        (6, "shotgun") => "range_shotgun",
        (6, "revolver") => "range_revolver",
        (6, "crossbow") => "range_crossbow",
        (6, "rpg") => "range_rpg",
        (6, "grenade") => "range_grenade",
        (6, "melee") => "range_melee",
        (6, "physgun") => "range_gravgun",
        (6, "fist") => "range_fists_r",
        (1, "pistol") => "reload_pistol",
        (1, "smg1") => "reload_smg1",
        (1, "ar2") => "reload_ar2",
        (1, "shotgun") => "reload_shotgun",
        (1, "revolver") => "reload_revolver",
        _ => return None,
    })
}

/// Soundscript entry for a server viewmodel event (`VmAct`) on a loadout weapon, as the
/// game's weapons emit them: `.Single` on a shot (`.Launch` for the gravity gun), `.Reload`
/// when a one-go reload starts, the shotgun's `.Reload` per shell and `.Special1` (cock) at
/// its end and on the pump, `.Empty` on a dry fire. The shotgun's start stage is silent.
fn weapon_sound(act: u8, active: u8) -> Option<String> {
    let class = gm_server::LOADOUT.get(active as usize)?;
    let prefix = match *class {
        "weapon_357" => "Weapon_357",
        "weapon_ar2" => "Weapon_AR2",
        "weapon_crossbow" => "Weapon_Crossbow",
        "weapon_crowbar" => "Weapon_Crowbar",
        "weapon_physcannon" => "Weapon_PhysCannon",
        "weapon_pistol" => "Weapon_Pistol",
        "weapon_rpg" => "Weapon_RPG",
        "weapon_shotgun" => "Weapon_Shotgun",
        "weapon_smg1" => "Weapon_SMG1",
        "gmod_tool" => "Toolgun",
        _ => return None,
    };
    let suffix = match (act, prefix) {
        (6, "Weapon_PhysCannon") => "Launch",
        (_, "Toolgun") if act != 6 => return None,
        (6, _) => "Single",
        (1, "Weapon_Shotgun") => return None,
        (1, _) => "Reload",
        (4, _) => "Reload",
        (2, _) | (5, _) => "Special1",
        (3, _) => "Empty",
        _ => return None,
    };
    Some(format!("{prefix}.{suffix}"))
}

fn viewmodel(weapon: u8) -> Option<&'static Viewmodel> {
    WEAPONS.get(weapon as usize)
}

/// Drop-down console: the log of our lines and the server's replies, an input
/// line with history, and a scroll offset in lines from the bottom.
struct Console {
    open: bool,
    input: String,
    lines: VecDeque<String>,
    history: Vec<String>,
    history_at: Option<usize>,
    scroll: usize,
    opened_at: Instant,
}

impl Console {
    fn new() -> Self {
        Console {
            open: false,
            input: String::new(),
            lines: VecDeque::new(),
            history: Vec::new(),
            history_at: None,
            scroll: 0,
            opened_at: Instant::now(),
        }
    }

    fn log(&mut self, line: impl Into<String>) {
        if self.lines.len() == CONSOLE_LOG_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line.into());
    }

    fn history_step(&mut self, back: bool) {
        if self.history.is_empty() {
            return;
        }
        let last = self.history.len() - 1;
        self.history_at = match (self.history_at, back) {
            (None, true) => Some(last),
            (Some(i), true) => Some(i.saturating_sub(1)),
            (Some(i), false) if i < last => Some(i + 1),
            _ => None,
        };
        self.input = self
            .history_at
            .map(|i| self.history[i].clone())
            .unwrap_or_default();
    }
}

struct App {
    /// HL2 weapon selection (`hud_fastswitch 0`): the highlighted weapon as an index into the
    /// server's class list while the bucket menu is open.
    wsel: Option<u8>,
    epoch: Instant,
    /// `GM:AddHint` timers: hint name and when it fires; `hints_done` is `ProcessedHints`.
    hints: Vec<(&'static str, Instant)>,
    hints_done: Vec<&'static str>,
    /// `notification.AddLegacy` panels (sandbox `AddNotify`).
    notices: std::cell::RefCell<Notices>,
    /// Pickup history (`cl_hudpickup.lua`) and the weapon bits already announced.
    pickups: std::cell::RefCell<PickupHistory>,
    prev_weapons: Option<u64>,
    /// Kill feed (`cl_deathnotice.lua`): attacker, victim, inflictor class, when.
    deaths: Vec<DeathNotice>,
    mount: gm_content::Mount,
    bsp_data: Vec<u8>,
    sim: Sim,
    gfx: Option<Gfx>,
    keys: BTreeSet<KeyCode>,
    mouse: BTreeSet<u8>,
    view: Vec3,
    weapon: u8,
    tool: String,
    /// `tool <name> <key> <value>`: sent with every click of that tool (`ctx.settings`).
    tool_settings: BTreeMap<String, BTreeMap<String, String>>,
    noclip: bool,
    /// One-shot viewmodel sequence playing (draw, fire) and when it started.
    vm_anim: Option<(&'static str, Instant)>,
    /// `PlayerSnap::vm_serial` of the last server viewmodel event played.
    vm_serial_seen: u8,
    /// The server was told our starting weapon once we were in the world.
    weapon_synced: bool,
    /// When each airborne player left the ground (their jump sequence runs from there).
    air_since: std::collections::HashMap<u32, Instant>,
    /// Per remote player: animation time advanced by the game's playback rate (speed over the
    /// sequence's ground speed, at most 2).
    anim_time: std::collections::HashMap<u32, f32>,
    /// Other players' running gesture (m_anm sequence, start, the `vm_serial` that started it).
    gestures: std::collections::HashMap<u32, (&'static str, Instant, u8)>,
    /// Weapon sounds from the scripts, mixed on the default output device.
    sounds: gm_audio::Sounds,
    last_render: Instant,
    /// Spawn menu while Q is held: the prop models to pick from and the scroll offset in rows.
    menu: Option<SpawnMenu>,
    spawnlists: Vec<derma::SpawnList>,
    /// `tool.properties`: tool and setting labels.
    lang: BTreeMap<String, String>,
    /// From the last laid-out menu frame: visible spawn icons, the ControlPanel rect, max scroll.
    menu_cache: std::cell::RefCell<MenuCache>,
    /// C held: the cursor is free and a right-click asks a prop for its properties.
    context_held: bool,
    context: Option<ContextMenu>,
    /// Scripted weapons as last seen (`ClientWorld::weapon_defs`) and their viewmodel / world
    /// model entries after the stock table (leaked once per change: the table is `'static`).
    scripted_defs: Vec<gm_server::ScriptedWeapon>,
    scripted_vms: Vec<&'static Viewmodel>,
    scripted_world: Vec<&'static str>,
    /// Key / mouse button edges since the last frame, for the client actors (`input.key`).
    pending_input: Vec<(String, bool)>,
    spawn_lists_seen: usize,
    cursor: (f32, f32),
    /// `cl_playerskin` / `cl_playerbodygroups` as typed, sent together.
    player_skin: u8,
    player_body: String,
    /// A slider row being dragged (left button held since a press on it).
    panel_drag: Option<usize>,
    /// A text row with the keyboard focus and the text typed so far.
    panel_text: Option<(usize, String)>,
    grabbed: bool,
    last: Instant,
    acc: f32,
    frames: u32,
    fps_at: Instant,
    error: Option<anyhow::Error>,
    console: Console,
}

fn depth_view(gpu: &Gpu, w: u32, h: u32) -> wgpu::TextureView {
    let (_, depth) = gm_render::world::offscreen_targets(gpu, w, h);
    depth.create_view(&Default::default())
}

impl App {
    fn init_gfx(&mut self, el: &ActiveEventLoop) -> Result<()> {
        let window = Arc::new(
            el.create_window(
                Window::default_attributes()
                    .with_title("gmod-rust")
                    .with_inner_size(winit::dpi::LogicalSize::new(1280, 720)),
            )?,
        );
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance.create_surface(window.clone())?;
        let gpu = Gpu::for_surface(instance, &surface)?;
        eprintln!("adapter: {}", gpu.adapter_name());
        let size = window.inner_size();
        let caps = surface.get_capabilities(&gpu.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let mut config = surface
            .get_default_config(&gpu.adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| anyhow!("surface not supported by the adapter"))?;
        config.format = format;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&gpu.device, &config);
        let bsp = gm_formats::bsp::Bsp::parse(&self.bsp_data)?;
        let t0 = Instant::now();
        let world = WorldRenderer::new(&gpu, &self.mount, &bsp, format)?;
        eprintln!(
            "world: {} faces, {} props, built in {:.1}s",
            world.stats.faces,
            world.stats.props,
            t0.elapsed().as_secs_f32()
        );
        let depth = depth_view(&gpu, config.width, config.height);
        let player_box = world.solid_box(&gpu, [200, 120, 60, 255], false);
        let beam_mat = world.sprite_material(&gpu, &self.mount, "sprites/physbeam");
        let beam_active_mat = world.sprite_material(&gpu, &self.mount, "sprites/physbeama");
        let glow1 = world.sprite_material(&gpu, &self.mount, "sprites/physg_glow1");
        let glow2 = world.sprite_material(&gpu, &self.mount, "sprites/physg_glow2");
        let ui = UiRenderer::new(&gpu, &self.mount, format)?;
        let mut ui = ui;
        {
            // the HL2 HUD fonts of resource/ClientScheme.res, scaled from the 480-line layout
            let fs = config.height as f32 / 480.0;
            ui.register_font(&gpu, &self.mount, "HudNumbers", "HalfLife2", 32.0 * fs);
            ui.register_font(&gpu, &self.mount, "HudNumbersSmall", "HalfLife2", 16.0 * fs);
            if !ui.register_font(&gpu, &self.mount, "HudDefault", "Verdana", 9.0 * fs) {
                ui.register_font(&gpu, &self.mount, "HudDefault", "Roboto-Bold", 9.0 * fs);
            }
            // TargetID / TargetIDSmall: Verdana 12 / 11 bold (the 480-599 line variants)
            ui.register_font(&gpu, &self.mount, "HL2MPTypeDeath", "csd", 64.0 * fs);
            // DermaDefaultBold: Tahoma 13 weight 800 (derma/init.lua), not scaled by yres
            if !ui.register_font(&gpu, &self.mount, "DermaDefaultBold", "Tahoma", 13.0) {
                ui.register_font(&gpu, &self.mount, "DermaDefaultBold", "Roboto-Bold", 13.0);
            }
            ui.load_image(&gpu, &self.mount, "gui/corner8");
            // GModNotify: Arial max(12, ceil(ScreenScaleH(9))) weight 500
            let text_h = (9.0 * config.height as f32 / 480.0).ceil().max(12.0);
            if !ui.register_font(&gpu, &self.mount, "GModNotify", "Arial", text_h) {
                ui.register_font(&gpu, &self.mount, "GModNotify", "Roboto-Regular", text_h);
            }
            for n in ["generic", "error", "undo", "hint", "cleanup"] {
                ui.load_image(&gpu, &self.mount, &format!("vgui/notices/{n}"));
            }
            // HL2 weapon selection and crosshair: halflife2.ttf glyph fonts, Verdana labels
            ui.register_font(&gpu, &self.mount, "WeaponIcons", "HalfLife2", 64.0 * fs);
            ui.register_font(&gpu, &self.mount, "Crosshairs", "HalfLife2", 40.0 * fs);
            for (name, tall) in [("HudSelectionNumbers", 11.0), ("HudSelectionText", 8.0)] {
                if !ui.register_font(&gpu, &self.mount, name, "Verdana", tall * fs) {
                    ui.register_font(&gpu, &self.mount, name, "Roboto-Bold", tall * fs);
                }
            }
            ui.load_image(&gpu, &self.mount, "weapons/swep");
            ui.load_image(&gpu, &self.mount, "gui/speech_lid");
            // Derma: DermaDefault = Tahoma 13 (500 / 800); Roboto when the system font is missing
            if !ui.register_font(&gpu, &self.mount, derma::FONT, "Tahoma", 13.0) {
                ui.register_font(&gpu, &self.mount, derma::FONT, "Roboto-Medium", 13.0);
            }
            if !ui.register_font(&gpu, &self.mount, derma::FONT_BOLD, "Tahoma-Bold", 13.0) {
                ui.register_font(&gpu, &self.mount, derma::FONT_BOLD, "Roboto-Bold", 13.0);
            }
            ui.load_image(&gpu, &self.mount, derma::ATLAS);
            ui.load_image(&gpu, &self.mount, derma::HOVER_MAT);
            ui.load_image(&gpu, &self.mount, derma::GENERATING_MAT);
            for name in derma::IMAGES {
                ui.load_image(&gpu, &self.mount, name);
            }
            for (name, tall) in [
                ("TargetID", 12.0),
                ("TargetIDSmall", 11.0),
                ("ChatFont", 12.0),
            ] {
                if !ui.register_font(&gpu, &self.mount, name, "Verdana", tall * fs) {
                    ui.register_font(&gpu, &self.mount, name, "Roboto-Bold", tall * fs);
                }
            }
        }
        for (_, _, icon, _) in derma::PROPERTIES {
            if !icon.is_empty() {
                ui.load_image(&gpu, &self.mount, icon);
            }
        }
        {
            let m = ui.font_metrics(None);
            gm_script::set_font_metrics(
                "",
                gm_script::TextMetrics {
                    line_height: m.line_height,
                    advances: m.advances,
                    fallback: m.fallback,
                },
            );
        }
        self.gfx = Some(Gfx {
            icon_textures: Vec::new(),
            window,
            surface,
            config,
            gpu,
            world,
            depth,
            models: Default::default(),
            player_box,
            beam_mat,
            beam_active_mat,
            glow1,
            glow2,
            ui,
        });
        self.set_grab(true);
        Ok(())
    }

    fn set_grab(&mut self, on: bool) {
        let Some(g) = &self.gfx else { return };
        let ok = if on {
            g.window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| g.window.set_cursor_grab(CursorGrabMode::Confined))
                .is_ok()
        } else {
            g.window.set_cursor_grab(CursorGrabMode::None).is_ok()
        };
        g.window.set_cursor_visible(!on);
        if on && !ok {
            eprintln!("cursor grab unavailable: mouse look needs the pointer inside the window");
        }
        // without a grab (some X servers) input still works, only the pointer is not held
        self.grabbed = on;
    }

    fn local_snap(&self) -> Option<&PlayerSnap> {
        let me = self.sim.client.player_id;
        self.sim
            .client
            .world
            .players
            .iter()
            .find(|p| Some(p.id) == me)
    }

    fn open_menu(&mut self) {
        if self.menu.is_some() || self.console.open {
            return;
        }
        // cl_spawnmenu.lua OnSpawnMenuOpen
        self.add_hint("OpeningContext", 20);
        self.add_hint("EditingSpawnlists", 5);
        if self.spawnlists.is_empty() {
            self.spawnlists = derma::load_spawnlists(&self.mount);
            self.lang =
                derma::load_properties(&self.mount, "resource/localization/en/tool.properties");
            self.lang.extend(derma::load_properties(
                &self.mount,
                "resource/localization/en/context.properties",
            ));
        }
        // the server's addon spawnlists follow the client's defaults
        let lists = &self.sim.client.world.spawn_lists;
        let seen = lists.iter().map(|(_, m)| m.len() + 1).sum::<usize>();
        if seen != self.spawn_lists_seen {
            self.spawn_lists_seen = seen;
            self.spawnlists.retain(|l| !l.addon);
            for (category, models) in lists {
                self.spawnlists.push(derma::SpawnList {
                    name: category.clone(),
                    icon: "icon16/folder_database.png".to_string(),
                    models: models.clone(),
                    addon: true,
                });
            }
        }
        if let Some(g) = &mut self.gfx {
            for l in &self.spawnlists {
                g.ui.load_image(&g.gpu, &self.mount, &l.icon);
            }
        }
        self.menu = Some(SpawnMenu {
            st: derma::MenuState::default(),
        });
        self.mouse.clear();
        self.set_grab(false);
    }

    /// The row under the cursor of the open spawn menu, if any.
    /// The context-menu entry under the cursor.
    fn context_row(&self) -> Option<String> {
        let (ctx, g) = (self.context.as_ref()?, self.gfx.as_ref()?);
        let font = g.ui.font_metrics(Some(derma::FONT));
        let labels: Vec<String> = ctx
            .names
            .iter()
            .map(|n| derma::property_label(&self.lang, n))
            .collect();
        let row = derma::dmenu_row(&font, ctx.x, ctx.y, &labels, self.cursor)?;
        ctx.names.get(row).cloned()
    }

    /// The selected stool's setting schema while the tool gun is out (the panel's content).
    /// The tool's ControlPanel: a scripted stool's schema from the server, else the built-in
    /// stool's `BuildCPanel` controls.
    fn panel_schema(&self) -> Option<Vec<gm_server::ToolSetting>> {
        Some(
            self.tool_schema(&self.tool)
                .map_or_else(|| gm_server::builtin_tool_schema(&self.tool), <[_]>::to_vec),
        )
    }

    /// The panel row under the cursor and the cursor's 0..1 position across its control.
    fn panel_hit(&self) -> Option<(usize, f32)> {
        if self.menu.is_none() && !self.context_held {
            return None;
        }
        let cache = self.menu_cache.borrow();
        let row = cache.rows.iter().position(|r| r.contains(self.cursor))?;
        Some((row, cache.rows[row].fraction(self.cursor)))
    }

    /// A press on the panel: a slider takes the cursor's position and follows it until the
    /// button is released, a checkbox flips, a text row takes the keyboard (typed chars edit
    /// it, Enter or Escape leave it).
    fn panel_click(&mut self) -> bool {
        let Some(schema) = self.panel_schema() else {
            return false;
        };
        let Some((row, t)) = self.panel_hit() else {
            self.panel_text = None;
            return false;
        };
        let Some(s) = schema.get(row) else {
            self.panel_text = None;
            return true;
        };
        self.panel_text = None;
        let cur = self
            .tool_settings
            .get(&self.tool)
            .and_then(|m| m.get(&s.name))
            .cloned()
            .unwrap_or_else(|| s.default.clone());
        let value = match s.kind {
            gm_server::ToolSettingKind::Number => {
                self.panel_drag = Some(row);
                slider_value(s, t)
            }
            gm_server::ToolSettingKind::Bool => {
                if matches!(cur.trim(), "1" | "true" | "on") {
                    "0".into()
                } else {
                    "1".into()
                }
            }
            gm_server::ToolSettingKind::Text => {
                self.panel_text = Some((row, cur));
                return true;
            }
        };
        self.set_tool_setting(&s.name, value);
        true
    }

    /// Spawn-menu icons: with the menu open, up to two visible rows without an icon get their
    /// model rendered once into a 64 px square (the game's spawnicon pose: from the front-right,
    /// a little above, framed on the model's bounds) and registered as a UI image.
    fn render_spawn_icons(spawn_models: &[String], mount: &gm_content::Mount, g: &mut Gfx) {
        let mut budget = 2;
        for model in spawn_models {
            if budget == 0 {
                break;
            }
            let key = format!("icon:{model}");
            if g.ui.has_image(&key) {
                continue;
            }
            if !g.models.contains_key(model.as_str()) {
                let m = g.world.load_model(&g.gpu, mount, model);
                g.models.insert(model.clone(), m);
            }
            budget -= 1;
            let Some(m) = g.models.get(model.as_str()).and_then(|m| m.as_ref()) else {
                // no drawable mesh: remember the miss so the row is not retried every frame
                g.ui.register_missing_image(&key);
                continue;
            };
            let (lo, hi) = m.bounds();
            let center = (lo + hi) * 0.5;
            let radius = ((hi - lo).length() * 0.5).max(1.0);
            let angles = glam::Vec3::new(20.0, 135.0, 0.0);
            let dir = Camera {
                position: glam::Vec3::ZERO,
                angles,
                fov_x: 50.0,
                near: 1.0,
                far: 1.0,
            }
            .forward();
            let dist = radius / (25f32.to_radians().tan()) * 1.05;
            let camera = Camera {
                position: center - dir * dist,
                angles,
                fov_x: 50.0,
                near: (dist - radius).max(0.5),
                far: dist + radius * 2.0,
            };
            let (color, depth) = gm_render::world::icon_targets(&g.gpu, 64, g.config.format);
            let color_view = color.create_view(&Default::default());
            let depth_view = depth.create_view(&Default::default());
            let target = gm_render::world::ModelTarget {
                color: &color_view,
                depth: &depth_view,
                aspect: 1.0,
                clear_depth: true,
                alpha: 1.0,
            };
            let item = DrawItem {
                model: m,
                transform: glam::Mat4::IDENTITY,
                bones: None,
            };
            g.world.draw_models(&g.gpu, &camera, &target, &[item]);
            g.ui.register_image_view(&g.gpu, &key, &color_view);
            g.icon_textures.push(color);
        }
    }

    fn send_player_skin(&mut self) {
        self.sim.client.command(Command::PlayerSkin {
            skin: self.player_skin,
            body: self.player_body.clone(),
        });
    }

    fn set_tool_setting(&mut self, key: &str, value: String) {
        self.console
            .log(format!("tool {}: {key} = {value}", self.tool));
        self.tool_settings
            .entry(self.tool.clone())
            .or_default()
            .insert(key.to_string(), value);
    }

    /// The cursor moved with a slider held: the value follows it (clamped to the row's range,
    /// the cursor may leave the panel).
    fn panel_drag_move(&mut self) {
        let Some(row) = self.panel_drag else { return };
        if self.menu.is_none() && !self.context_held {
            self.panel_drag = None;
            return;
        }
        let Some(schema) = self.panel_schema() else {
            return;
        };
        let Some(s) = schema.get(row).cloned() else {
            return;
        };
        let Some(t) = self
            .menu_cache
            .borrow()
            .rows
            .get(row)
            .map(|r| r.fraction(self.cursor))
        else {
            return;
        };
        let value = slider_value(&s, t);
        let same = self
            .tool_settings
            .get(&self.tool)
            .and_then(|m| m.get(&s.name))
            .is_some_and(|v| *v == value);
        if !same {
            self.tool_settings
                .entry(self.tool.clone())
                .or_default()
                .insert(s.name.clone(), value);
        }
    }

    /// A key while a text row has the focus; true when the key was taken.
    fn panel_key(&mut self, code: Option<KeyCode>, text: Option<&str>) -> bool {
        let Some((row, mut value)) = self.panel_text.take() else {
            return false;
        };
        match code {
            Some(KeyCode::Enter | KeyCode::NumpadEnter) => {
                if let Some(s) = self.panel_schema().and_then(|sc| sc.get(row).cloned()) {
                    self.set_tool_setting(&s.name, value);
                }
            }
            Some(KeyCode::Escape) => {}
            Some(KeyCode::Backspace) => {
                value.pop();
                self.panel_text = Some((row, value));
            }
            _ => {
                if let Some(t) = text {
                    value.extend(t.chars().filter(|c| !c.is_control()));
                    value.truncate(255);
                }
                self.panel_text = Some((row, value));
            }
        }
        true
    }

    fn menu_click(&mut self) {
        if self.panel_click() {
            return;
        }
        let Some((menu, g)) = self.menu.as_mut().zip(self.gfx.as_ref()) else {
            return;
        };
        let (w, h) = (g.config.width as f32, g.config.height as f32);
        let font = g.ui.font_metrics(Some(derma::FONT));
        let bold = g.ui.font_metrics(Some(derma::FONT_BOLD));
        let has_image = |name: &str| g.ui.has_image(name);
        let ctx = derma::Ctx {
            w,
            h,
            cursor: self.cursor,
            font: &font,
            bold: &bold,
            lists: &self.spawnlists,
            tool: &self.tool,
            has_image: &has_image,
        };
        let action = derma::build(&ctx, &menu.st).hit(self.cursor).cloned();
        match action {
            Some(derma::Action::Spawn(model)) => {
                self.console.log(format!("spawn {model}"));
                self.sim.client.command(Command::Spawn { model });
            }
            Some(derma::Action::CreationTab(i)) => menu.st.creation_tab = i,
            Some(derma::Action::ToolTab(i)) => menu.st.tool_tab = i,
            Some(derma::Action::Node(i)) => {
                menu.st.node = i;
                menu.st.scroll = 0.0;
            }
            Some(derma::Action::Browse) => menu.st.browse_open = !menu.st.browse_open,
            Some(derma::Action::Category(i)) => menu.st.collapsed[i] = !menu.st.collapsed[i],
            Some(derma::Action::Tool(t)) => {
                self.tool = t.to_string();
                self.console.log(format!("tool {t}"));
            }
            Some(derma::Action::Property(_) | derma::Action::Nothing) | None => {}
        }
    }

    /// Stock viewmodel table, then the scripted weapons' entries (indices after `LOADOUT`).
    fn vm_of(&self, weapon: u8) -> Option<&'static Viewmodel> {
        viewmodel(weapon).or_else(|| {
            self.scripted_vms
                .get((weapon as usize).checked_sub(gm_server::LOADOUT.len())?)
                .copied()
        })
    }

    /// Rebuilds the scripted weapon entries when the server's definitions changed.
    fn sync_scripted_weapons(&mut self) {
        let defs = &self.sim.client.world.weapon_defs;
        if *defs == self.scripted_defs {
            return;
        }
        self.scripted_defs = defs.clone();
        let leak = |s: &str| -> &'static str { Box::leak(s.to_owned().into_boxed_str()) };
        self.scripted_vms = defs
            .iter()
            .map(|d| {
                let vm = Viewmodel {
                    reload: Some("reload"),
                    ..vm(
                        leak(&d.class),
                        leak(&d.print_name),
                        leak(&d.view_model),
                        "idle01",
                        "draw",
                        "fire",
                    )
                };
                &*Box::leak(Box::new(vm))
            })
            .collect();
        self.scripted_world = defs.iter().map(|d| leak(&d.world_model)).collect();
    }

    /// A digit key: the game's slot categories (measured `gm_server::SLOTS` plus the scripted
    /// definitions' `slot` / `slot_pos`). The first owned weapon of the slot, or the one after
    /// the current when it is in the slot (wrapping); weapons without a viewmodel here are
    /// skipped. Applied at once (the game's selection menu / `hud_fastswitch` is not modelled).
    /// Local weapon index (`WEAPONS`, then the scripted definitions) of an index into the
    /// server's class list.
    fn to_local(&self, i: u8) -> Option<u8> {
        let defs = &self.sim.client.world.weapon_defs;
        let classes = gm_net::msg::weapon_classes(defs);
        let class = classes.get(i as usize)?;
        if let Some(w) = WEAPONS.iter().position(|w| w.class == class) {
            return Some(w as u8);
        }
        defs.iter()
            .position(|d| d.class == *class)
            .map(|p| (gm_server::LOADOUT.len() + p) as u8)
    }

    /// The owned weapons of every bucket in the game's menu order (class indices).
    fn buckets(&self) -> Vec<Vec<u8>> {
        let Some(snap) = self.local_snap() else {
            return Vec::new();
        };
        let defs = &self.sim.client.world.weapon_defs;
        let classes = gm_net::msg::weapon_classes(defs);
        (0..6u8)
            .map(|slot| {
                gm_server::slot_order(&classes, snap.weapons, slot, defs)
                    .into_iter()
                    .filter(|&i| self.to_local(i).is_some())
                    .collect()
            })
            .collect()
    }

    /// Slot key with `hud_fastswitch 0`: the bucket menu opens on (or moves to) the first
    /// weapon of the bucket, or the one after the highlighted/active weapon when it is in it.
    fn wsel_slot(&mut self, slot: u8) {
        let Some(active) = self.local_snap().map(|p| p.active) else {
            return;
        };
        let order = self.buckets().swap_remove(slot as usize);
        if order.is_empty() {
            self.sounds
                .emit(&self.mount, "Player.DenyWeaponSelection", None);
            return;
        }
        let cur = self.wsel.unwrap_or(active);
        let next = match order.iter().position(|&i| i == cur) {
            Some(k) => order[(k + 1) % order.len()],
            None => order[0],
        };
        self.wsel = Some(next);
        self.sounds
            .emit(&self.mount, "Player.WeaponSelectionMoveSlot", None);
    }

    /// `invnext` / `invprev`: the next or previous owned weapon across the buckets.
    fn wsel_step(&mut self, dir: i32) {
        let Some(active) = self.local_snap().map(|p| p.active) else {
            return;
        };
        let all: Vec<u8> = self.buckets().into_iter().flatten().collect();
        if all.is_empty() {
            return;
        }
        let cur = self.wsel.unwrap_or(active);
        let k = all
            .iter()
            .position(|&i| i == cur)
            .map(|k| k as i32)
            .unwrap_or(-dir.max(0));
        let n = all.len() as i32;
        self.wsel = Some(all[((k + dir).rem_euclid(n)) as usize]);
        self.sounds
            .emit(&self.mount, "Player.WeaponSelectionMoveSlot", None);
    }

    /// `GM:AddHint`: one timer per hint name, once per session.
    fn add_hint(&mut self, name: &'static str, delay: u64) {
        if self.hints_done.contains(&name) {
            return;
        }
        self.hints_done.push(name);
        self.hints
            .push((name, Instant::now() + std::time::Duration::from_secs(delay)));
    }

    /// Fires due hints: `Hint_<name>` with `%+bind%` replaced by the key, NOTIFY_HINT for
    /// 20 s and a water drip.
    fn tick_hints(&mut self) {
        let now = Instant::now();
        let mut due = Vec::new();
        self.hints.retain(|(name, at)| {
            if *at <= now {
                due.push(*name);
                false
            } else {
                true
            }
        });
        for name in due {
            let text = match name {
                "OpeningMenu" => "Press and hold 'Q' to open the SPAWN MENU",
                "OpeningContext" => "Press and hold 'C' to open the CONTEXT MENU",
                "ContextClick" => {
                    "You can also use TOOLS by clicking on the screen with the CONTEXT MENU open"
                }
                "Annoy1" => "You can turn these hints off in the CONTEXT MENU",
                "Annoy2" => {
                    "Hold 'C', click on DRAWING at the top of the screen, and deselect HINTS"
                }
                "PhysgunFreeze" => "Right click to freeze an object or a ragdoll's limb",
                "PhysgunUse" => "You can rotate objects with the PHYSGUN by holding down 'E'",
                "EditingSpawnlists" => {
                    "You can edit your spawnlists by drag and dropping icons around and in between spawnlists!"
                }
                _ => continue,
            };
            self.notify(text.to_string(), NotifyKind::Hint, 20.0);
            let n = 1 + (now.elapsed().subsec_nanos() % 4);
            let wave = format!("ambient/water/drip{n}.wav");
            self.sounds.play_wave(&self.mount, &wave, 1.0);
        }
    }

    /// `GM:AddNotify` -> `notification.AddLegacy`.
    fn notify(&mut self, text: String, kind: NotifyKind, length: f32) {
        let fm = self
            .gfx
            .as_ref()
            .map(|g| g.ui.font_metrics(Some("GModNotify")));
        let (w, h) = self
            .gfx
            .as_ref()
            .map(|g| (g.config.width as f32, g.config.height as f32))
            .unwrap_or((640.0, 480.0));
        self.notices
            .borrow_mut()
            .add(text, kind, length, fm.as_ref(), w, h);
    }

    /// HudWeaponSelection (hudlayout.res, bucket layout): the open bucket as a column of
    /// 112x80 boxes with the WeaponIcons glyph and the highlighted weapon's name at 64, the
    /// other buckets as 32x32 boxes, numbers at 4,4, gap 8, all on the 640x480 layout scaled
    /// by the screen height; scripted weapons draw `weapons/swep` and the info bubble.
    fn draw_weapon_select(&self, items: &mut Vec<UiItem>, _w: f32, h: f32, ui: &UiRenderer) {
        let Some(sel) = self.wsel else { return };
        let Some(snap) = self.local_snap() else {
            return;
        };
        if !snap.alive {
            return;
        }
        let s = h / 480.0;
        let classes = gm_net::msg::weapon_classes(&self.sim.client.world.weapon_defs);
        let buckets = self.buckets();
        let fg = [1.0, 220.0 / 255.0, 0.0, 1.0];
        let dim = [1.0, 220.0 / 255.0, 0.0, 0.5];
        let box_bg = [0.0, 0.0, 0.0, 80.0 / 255.0];
        let (small, large_w, large_h, gap) = (32.0 * s, 112.0 * s, 80.0 * s, 8.0 * s);
        let icons = ui.font_metrics(Some("WeaponIcons"));
        let numbers = ui.font_metrics(Some("HudSelectionNumbers"));
        let text_m = ui.font_metrics(Some("HudSelectionText"));
        let mut x = 0.0;
        let y0 = 16.0 * s;
        let t = self.epoch.elapsed().as_secs_f32();
        for (slot, list) in buckets.iter().enumerate() {
            if list.is_empty() {
                continue;
            }
            let open = list.contains(&sel);
            let wide = if open { large_w } else { small };
            let mut y = y0;
            let rows = if open { list.len() } else { 1 };
            for (row, &i) in list.iter().enumerate().take(rows) {
                let tall = if open { large_h } else { small };
                items.push(UiItem::Rect {
                    x,
                    y,
                    w: wide,
                    h: tall,
                    color: box_bg,
                });
                if row == 0 {
                    items.push(UiItem::Text {
                        font: Some("HudSelectionNumbers".to_string()),
                        x: x + 4.0 * s,
                        y: y + 4.0 * s,
                        text: (slot + 1).to_string(),
                        color: fg,
                    });
                }
                if open {
                    let class = classes.get(i as usize).map(String::as_str).unwrap_or("");
                    let color = if i == sel { fg } else { dim };
                    if let Some(glyph) = weapon_icon(class) {
                        let gw = icons.text_width(glyph);
                        items.push(UiItem::Text {
                            font: Some("WeaponIcons".to_string()),
                            x: x + (wide - gw) / 2.0,
                            y: y + (64.0 * s - icons.line_height) / 2.0,
                            text: glyph.to_string(),
                            color,
                        });
                    } else {
                        // weapon_base DrawWeaponSelection: WepSelectIcon inset 10, bouncing
                        let fsin = (t * 10.0).sin() * 5.0;
                        let iw = wide - 20.0;
                        items.push(UiItem::Image {
                            x: x + 10.0 + fsin,
                            y: y + 10.0 - fsin,
                            w: iw - fsin * 2.0,
                            h: iw / 2.0 + fsin,
                            material: "weapons/swep".to_string(),
                            color: [1.0, 1.0, 1.0, color[3]],
                            rot: 0,
                        });
                        if i == sel {
                            // PrintWeaponInfo: speech bubble lid and the Author line
                            let (bx, by) = (x + iw + 20.0 + 10.0, y + tall * 0.95);
                            let grey = [60.0 / 255.0, 60.0 / 255.0, 60.0 / 255.0, 1.0];
                            items.push(UiItem::Image {
                                x: bx,
                                y: by - 64.0 - 5.0,
                                w: 128.0,
                                h: 64.0,
                                material: "gui/speech_lid".to_string(),
                                color: grey,
                                rot: 0,
                            });
                            items.push(UiItem::Rect {
                                x: bx - 5.0,
                                y: by - 5.0,
                                w: 260.0,
                                h: text_m.line_height + 18.0,
                                color: grey,
                            });
                            items.push(UiItem::Text {
                                font: Some("HudSelectionText".to_string()),
                                x: bx + 5.0,
                                y: by + 5.0,
                                text: "Author".to_string(),
                                color: [230.0 / 255.0, 230.0 / 255.0, 230.0 / 255.0, 1.0],
                            });
                            items.push(UiItem::Text {
                                font: Some("HudSelectionText".to_string()),
                                x: bx + 5.0 + text_m.text_width("Author\t"),
                                y: by + 5.0,
                                text: "Facepunch".to_string(),
                                color: [150.0 / 255.0, 150.0 / 255.0, 150.0 / 255.0, 1.0],
                            });
                        }
                    }
                    if i == sel {
                        let name =
                            weapon_select_name(class, self.to_local(i).and_then(|l| self.vm_of(l)));
                        let tw = text_m.text_width(&name);
                        items.push(UiItem::Text {
                            font: Some("HudSelectionText".to_string()),
                            x: x + (wide - tw) / 2.0,
                            y: y + 64.0 * s,
                            text: name,
                            color: fg,
                        });
                    }
                }
                y += tall + gap;
            }
            x += wide + gap;
        }
        let _ = numbers;
    }

    fn select_weapon(&mut self, weapon: u8) {
        let Some(vm) = self.vm_of(weapon) else { return };
        if self.weapon != weapon {
            self.weapon = weapon;
            self.vm_anim = Some((vm.draw, Instant::now()));
        }
        self.sim.client.command(Command::SelectWeapon {
            class: vm.class.to_string(),
        });
    }

    /// Sequence and time for the viewmodel this frame: the one-shot sequence while it lasts,
    /// then the hold sequence while a prop is held, else idle.
    fn vm_sequence(&mut self, vm: &Viewmodel) -> (&'static str, f32) {
        // server-decided events: reload (manual, empty clip, automatic), pump, dry fire
        if let Some((act, serial, active)) =
            self.local_snap().map(|p| (p.vm_act, p.vm_serial, p.active))
        {
            if serial != self.vm_serial_seen {
                self.vm_serial_seen = serial;
                let seq = match act {
                    1 => vm.reload,
                    2 => vm.pump,
                    3 => vm.dry,
                    4 => vm.reload_shell,
                    5 => vm.reload_end,
                    6 => Some(vm.fire),
                    _ => None,
                };
                if let Some(seq) = seq {
                    self.vm_anim = Some((seq, Instant::now()));
                }
                if let Some(name) = weapon_sound(act, active) {
                    self.sounds.emit(&self.mount, &name, None);
                }
            }
        }
        let model = self
            .gfx
            .as_ref()
            .and_then(|g| g.models.get(vm.path))
            .and_then(|m| m.as_ref());
        if let Some((seq, at)) = self.vm_anim {
            let t = at.elapsed().as_secs_f32();
            match model.map(|m| m.sequence_length(seq)) {
                // model not loaded yet: keep the request
                None => return (seq, t),
                Some(Some((len, false))) if t < len => return (seq, t),
                // unknown sequence or finished: fall through to the loops
                _ => self.vm_anim = None,
            }
        }
        let me = self.sim.client.player_id;
        let held = self
            .sim
            .client
            .world
            .players
            .iter()
            .any(|p| Some(p.id) == me && p.held.is_some());
        let t = self.fps_at.elapsed().as_secs_f32();
        match vm.hold {
            Some(hold) if held => (hold, t),
            _ => (vm.idle, t),
        }
    }

    fn cmd(&self) -> NetCmd {
        let k = |c: KeyCode| self.keys.contains(&c);
        let mut b = 0;
        let run = k(KeyCode::ShiftLeft);
        let speed = if run { 400.0 } else { 200.0 };
        let mut fwd = 0.0;
        let mut side = 0.0;
        if k(KeyCode::KeyW) {
            fwd += speed;
            b |= buttons::FORWARD;
        }
        if k(KeyCode::KeyS) {
            fwd -= speed;
            b |= buttons::BACK;
        }
        if k(KeyCode::KeyD) {
            side += speed;
            b |= buttons::MOVERIGHT;
        }
        if k(KeyCode::KeyA) {
            side -= speed;
            b |= buttons::MOVELEFT;
        }
        if k(KeyCode::Space) {
            b |= buttons::JUMP;
        }
        if k(KeyCode::ControlLeft) {
            b |= buttons::DUCK;
        }
        if run {
            b |= buttons::SPEED;
        }
        if k(KeyCode::KeyE) {
            b |= buttons::USE;
        }
        if k(KeyCode::KeyR) && self.weapon != 1 {
            b |= buttons::RELOAD;
        }
        if self.mouse.contains(&0) {
            b |= buttons::ATTACK;
        }
        if self.mouse.contains(&1) {
            b |= buttons::ATTACK2;
        }
        NetCmd {
            buttons: b,
            view_angles: self.view,
            forward_move: fwd,
            side_move: side,
            weapon: net_weapon(self.weapon),
            ..Default::default()
        }
    }

    fn step(&mut self) {
        let now = Instant::now();
        self.acc += (now - self.last).as_secs_f32().min(0.25);
        self.last = now;
        let dt = gm_core::tick::DEFAULT_TICK_INTERVAL;
        while self.acc >= dt {
            let cmd = self.cmd();
            self.sim.tick(cmd);
            let keys = std::mem::take(&mut self.pending_input);
            self.sim.script.client_input(&self.sim.client.world, &keys);
            self.acc -= dt;
            if !self.weapon_synced && self.sim.client.world.local.is_some() {
                self.weapon_synced = true;
                self.select_weapon(self.weapon);
            }
            {
                // HUDWeaponPickedUp: every weapon bit that appeared while alive
                self.tick_hints();
                let me = self.sim.client.player_id;
                if let Some(p) = self
                    .sim
                    .client
                    .world
                    .players
                    .iter()
                    .find(|p| Some(p.id) == me)
                {
                    let prev = self.prev_weapons.unwrap_or(0);
                    if p.alive {
                        let mut gained = p.weapons & !prev;
                        while gained != 0 {
                            let bit = gained.trailing_zeros();
                            gained &= gained - 1;
                            if let Some(vm) = self.vm_of(bit as u8) {
                                let m = self
                                    .gfx
                                    .as_ref()
                                    .map(|g| g.ui.font_metrics(Some("DermaDefaultBold")));
                                self.pickups.borrow_mut().add(
                                    vm.name,
                                    None,
                                    [1.0, 200.0 / 255.0, 50.0 / 255.0],
                                    m.as_ref(),
                                );
                            }
                        }
                    }
                    self.prev_weapons = Some(p.weapons);
                }
            }
            let events = std::mem::take(&mut self.sim.client.events);
            for e in events {
                if let Some(d) = DeathNotice::parse(&e, &self.sim.client.world.players) {
                    self.deaths.push(d);
                }
                if let Some((text, kind, length)) = Notices::legacy(&e) {
                    self.notify(text, kind, length);
                }
                println!("{e}");
                self.console.log(e);
            }
            for s in std::mem::take(&mut self.sim.client.sounds) {
                self.sounds
                    .emit_scaled(&self.mount, &s.name, Some(s.pos), s.volume);
            }
        }
    }

    fn render(&mut self) {
        self.sync_scripted_weapons();
        let alpha = self.acc / gm_core::tick::DEFAULT_TICK_INTERVAL;
        let vm = self.vm_of(self.weapon);
        let vm_seq = vm.as_ref().map(|vm| self.vm_sequence(vm));
        let ghost_tool = (self.weapon == 1).then(|| (self.tool.clone(), self.tool_settings_list()));
        // the hands follow the player model (`player_manager`'s hands table: model, skin, body
        // groups), kleiner's citizen hands until the snapshot names one
        let hands_of = self
            .local_snap()
            .map(|p| gm_server::player_models::path_of_index(p.model))
            .and_then(gm_server::player_models::name_of)
            .map(gm_server::player_models::hands)
            .unwrap_or(gm_server::player_models::Hands {
                model: HANDS_MODEL,
                skin: 0,
                body: "",
            });
        if self.context_held {
            if let Some((ent, names)) = self.sim.client.properties.take() {
                if ent != 0 && !names.is_empty() {
                    let mut names = names;
                    derma::sort_properties(&mut names);
                    self.context = Some(ContextMenu {
                        names,
                        x: self.cursor.0,
                        y: self.cursor.1,
                    });
                }
            }
        }
        let scripted_world = self.scripted_world.clone();
        let world_model_of = |active: u8| -> Option<&'static str> {
            world_model(active).or_else(|| {
                scripted_world
                    .get((active as usize).checked_sub(gm_server::LOADOUT.len())?)
                    .copied()
            })
        };
        let Some(g) = &mut self.gfx else { return };
        if self.menu.is_some() {
            let visible = self.menu_cache.borrow().visible.clone();
            Self::render_spawn_icons(&visible, &self.mount, g);
        }
        let frame = match g.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            _ => {
                g.surface.configure(&g.gpu.device, &g.config);
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let o = self
            .sim
            .prev_origin
            .lerp(self.sim.origin, alpha.clamp(0.0, 1.0));
        let eye = if self.sim.ducked { 28.0 } else { 64.0 };
        // the view punch (landing roll, `move_roll_*`) and the `sv_rollangle` strafe roll
        let punch = self.sim.pred.as_ref().map_or(Vec3::ZERO, |p| {
            p.state.punch
                + Vec3::Z * gm_move::view_roll(&gm_move::MoveConfig::default(), &p.state, self.view)
        });
        let mut camera = Camera {
            position: o + Vec3::Z * eye,
            angles: self.view + punch,
            fov_x: 90.0,
            near: 3.0,
            far: 28000.0,
        };
        // driving a prop: a chase camera on it (the game's observer mode on the entity; the
        // distance is not measured)
        let me = self.sim.client.player_id;
        if let Some(ent) = self
            .sim
            .client
            .world
            .players
            .iter()
            .find(|p| Some(p.id) == me)
            .and_then(|p| p.driving)
            .and_then(|id| self.sim.client.world.entities.get(&id))
        {
            let (p, y) = (self.view.x.to_radians(), self.view.y.to_radians());
            let fwd = Vec3::new(p.cos() * y.cos(), p.cos() * y.sin(), -p.sin());
            camera.position = ent.pos - fwd * 160.0 + Vec3::Z * 24.0;
        }
        let ghost_draws = match &ghost_tool {
            Some((tool, settings)) => {
                let (p, y) = (self.view.x.to_radians(), self.view.y.to_radians());
                let fwd = Vec3::new(p.cos() * y.cos(), p.cos() * y.sin(), -p.sin());
                self.sim.script.client_ghost(
                    &self.sim.client.world,
                    tool,
                    settings,
                    camera.position,
                    fwd,
                )
            }
            None => Vec::new(),
        };
        let ghost_models = ghost_draws.iter().filter_map(|d| match d {
            gm_script::DrawCmd::Model { model, .. } => Some(model.as_str()),
            _ => None,
        });
        let yaw = self.view.y.to_radians();
        self.sounds.mixer.set_listener(gm_audio::Listener {
            position: camera.position,
            right: Vec3::new(yaw.sin(), -yaw.cos(), 0.0),
        });
        let aspect = g.config.width as f32 / g.config.height as f32;
        g.world.draw(&g.gpu, &camera, &view, &g.depth, aspect);
        let ents = &self.sim.client.world.entities;
        let me = self.sim.client.player_id;
        let held_models = self
            .sim
            .client
            .world
            .players
            .iter()
            .filter(|p| Some(p.id) != me)
            .filter_map(|p| world_model_of(p.active));
        for model in ents
            .values()
            .map(|e| e.model.as_str())
            .chain(held_models)
            .chain(ghost_models)
        {
            if !g.models.contains_key(model) {
                let m = g.world.load_model(&g.gpu, &self.mount, model);
                g.models.insert(model.to_string(), m);
            }
        }
        // props with a skin or body groups set load as a variant under `model#skin#body`
        for e in self.sim.client.world.entities.values() {
            if e.skin == 0 && e.body.is_empty() {
                continue;
            }
            let key = format!("{}#{}#{}", e.model, e.skin, e.body);
            if !g.models.contains_key(&key) {
                let m = g
                    .world
                    .load_model_variant(&g.gpu, &self.mount, &e.model, e.skin, &e.body);
                g.models.insert(key, m);
            }
        }
        // player models by model, skin and body groups (`cl_playerskin` / `cl_playerbodygroups`
        // of each player, worn from their spawn)
        for p in &self.sim.client.world.players {
            let key = player_model_key(p);
            if !g.models.contains_key(&key) {
                let m = g.world.load_model_variant(
                    &g.gpu,
                    &self.mount,
                    gm_server::player_models::path_of_index(p.model),
                    p.skin,
                    &p.body,
                );
                g.models.insert(key, m);
            }
        }
        let rt = self.sim.interp.render_tick(self.sim.client.world.tick) + alpha.clamp(0.0, 1.0);
        let dt = self.last_render.elapsed().as_secs_f32().min(0.1);
        self.last_render = Instant::now();
        let air_since = &mut self.air_since;
        let anim_time = &mut self.anim_time;
        let gestures = &mut self.gestures;
        let sounds = &mut self.sounds;
        let mount = &self.mount;
        type PlayerPose<'a> = (
            Vec3,
            f32,
            Option<Vec<glam::Mat4>>,
            Option<(&'static str, Vec<glam::Mat4>)>,
            &'a gm_render::world::DynModel,
        );
        let mut player_poses: Vec<PlayerPose<'_>> = Vec::new();
        let mut items: Vec<_> = ents
            .values()
            .filter_map(|e| {
                let (pos, ang) = match self.sim.interp.sample(e.id, rt) {
                    Some(p) => (p.pos, p.angles),
                    None => (e.pos, e.angles),
                };
                Some(DrawItem {
                    model: if e.skin == 0 && e.body.is_empty() {
                        g.models.get(&e.model)?.as_ref()?
                    } else {
                        g.models
                            .get(&format!("{}#{}#{}", e.model, e.skin, e.body))?
                            .as_ref()?
                    },
                    transform: gm_render::world::model_transform(pos, ang),
                    bones: None,
                })
            })
            .collect();
        for p in &self.sim.client.world.players {
            if Some(p.id) == me || !p.alive {
                continue;
            }
            let player_model = g.models.get(&player_model_key(p)).and_then(|m| m.as_ref());
            // the player model animated from the snapshot, the hull box when it is missing
            match player_model {
                Some(m) => {
                    let (seq, params) = player_sequence(p);
                    // a one-shot (jump) runs from the moment the player left the ground; on
                    // the ground the cycle advances at speed / ground speed (at most 2x)
                    let t = if p.on_ground {
                        air_since.remove(&p.id);
                        let len = p.velocity.length();
                        let rate = match m.ground_speed(&seq, &params) {
                            Some(g) if len > 0.2 && g > 0.0 => (len / g).min(2.0),
                            _ => 1.0,
                        };
                        let at = anim_time.entry(p.id).or_insert(0.0);
                        *at += dt * rate;
                        *at
                    } else {
                        air_since
                            .entry(p.id)
                            .or_insert(Instant::now())
                            .elapsed()
                            .as_secs_f32()
                    };
                    // server-decided one-shots (shot, reload) as a gesture on top of the pose
                    if let Some(g) = gesture_for(p.vm_act, hold_type(p.active)) {
                        match gestures.get(&p.id) {
                            Some((_, _, serial)) if *serial == p.vm_serial => {}
                            _ => {
                                gestures.insert(p.id, (g, Instant::now(), p.vm_serial));
                                if let Some(name) = weapon_sound(p.vm_act, p.active) {
                                    let eye = if p.ducked { 28.0 } else { 64.0 };
                                    sounds.emit(mount, &name, Some(p.origin + Vec3::Z * eye));
                                }
                            }
                        }
                    }
                    let gesture = gestures.get(&p.id).and_then(|(g, since, _)| {
                        let t = since.elapsed().as_secs_f32();
                        let (len, _) = m.sequence_length(g)?;
                        (t < len).then_some((*g, t))
                    });
                    let world = m.bone_world_gesture(&seq, t, None, &params, gesture);
                    let pose = world.as_ref().map(|w| m.skin_matrices(w));
                    // the weapon's world model rides the right hand (bone merge by name)
                    let held = world.as_ref().and_then(|w| {
                        let path = world_model_of(p.active)?;
                        let wm = g.models.get(path)?.as_ref()?;
                        Some((path, wm.pose_merged("idle", 0.0, m, w)?))
                    });
                    let (origin, yaw) = match self.sim.player_interp.sample(p.id, rt) {
                        Some(s) => (s.pos, s.angles.y),
                        None => (p.origin, p.view_angles.y),
                    };
                    player_poses.push((origin, yaw, pose, held, m));
                }
                None => {
                    let h = if p.ducked { 36.0 } else { 72.0 };
                    items.push(DrawItem {
                        model: &g.player_box,
                        transform: glam::Mat4::from_translation(p.origin + Vec3::Z * (h / 2.0))
                            * glam::Mat4::from_rotation_z(p.view_angles.y.to_radians())
                            * glam::Mat4::from_scale(Vec3::new(32.0, 32.0, h)),
                        bones: None,
                    });
                }
            }
        }
        {
            for (origin, yaw, pose, held, m) in &player_poses {
                let transform =
                    gm_render::world::model_transform(*origin, Vec3::new(0.0, *yaw, 0.0));
                items.push(DrawItem {
                    model: m,
                    transform,
                    bones: pose.as_deref(),
                });
                if let Some((path, bones)) = held {
                    if let Some(wm) = g.models.get(*path).and_then(|w| w.as_ref()) {
                        items.push(DrawItem {
                            model: wm,
                            transform,
                            bones: Some(bones),
                        });
                    }
                }
            }
        }
        // physgun beam: from the viewmodel's `muzzle` attachment to the held prop, or along the
        // aim; leaves along the barrel and bends to the target (the game's curved beam), drawn
        // additive with the game's `sprites/physbeam`, a glow at each end
        let mut additive: Vec<(&wgpu::BindGroup, Vec<gm_render::world::Vertex>, Vec<u32>)> =
            Vec::new();
        if self.weapon == 0 && self.mouse.contains(&0) {
            let fwd = camera.forward();
            let right = fwd.cross(Vec3::Z).normalize_or_zero();
            let muzzle = match (vm, vm_seq) {
                (Some(vm), Some((seq, t))) => g
                    .models
                    .get(vm.path)
                    .and_then(|m| m.as_ref())
                    .and_then(|m| {
                        let w = m.bone_world(seq, t, None, &[])?;
                        let a = m.attachment("muzzle", &w)?;
                        Some(
                            gm_render::world::model_transform(camera.position, self.view)
                                .transform_point3(a.w_axis.truncate()),
                        )
                    }),
                _ => None,
            };
            let start =
                muzzle.unwrap_or(camera.position + right * 6.0 - Vec3::Z * 6.0 + fwd * 12.0);
            let held = self
                .sim
                .client
                .world
                .players
                .iter()
                .find(|p| Some(p.id) == me)
                .and_then(|p| p.held)
                .and_then(|id| ents.get(&id))
                .map(|e| e.pos);
            let end = held.unwrap_or_else(|| {
                self.sim
                    .collision
                    .trace_ray(
                        camera.position,
                        camera.position + fwd * 4096.0,
                        gm_collision::MASK_SOLID,
                    )
                    .end
            });
            let d = end - start;
            let len = d.length();
            if len > 1.0 {
                let color = PHYSGUN_BEAM_COLOR;
                // cubic bezier: out along the barrel for half the distance, into the end point
                let (p0, p1, p2, p3) = (start, start + fwd * (len * 0.5), end, end);
                let n = 24;
                let points: Vec<Vec3> = (0..=n)
                    .map(|i| {
                        let t = i as f32 / n as f32;
                        let u = 1.0 - t;
                        p0 * (u * u * u)
                            + p1 * (3.0 * u * u * t)
                            + p2 * (3.0 * u * t * t)
                            + p3 * (t * t * t)
                    })
                    .collect();
                let scroll = -self.fps_at.elapsed().as_secs_f32() * 2.0;
                let mat = if held.is_some() {
                    g.beam_active_mat.as_ref().or(g.beam_mat.as_ref())
                } else {
                    g.beam_mat.as_ref()
                };
                if let Some(bm) = mat {
                    let (v, i) = g.world.beam_strip(
                        &points,
                        6.0,
                        color,
                        camera.position,
                        scroll,
                        1.0 / 128.0,
                    );
                    additive.push((bm, v, i));
                }
                if let Some(gl) = &g.glow1 {
                    let (v, i) = g.world.sprite_quad(end, 12.0, color, &camera);
                    additive.push((gl, v, i));
                }
                if let Some(gl) = &g.glow2 {
                    let (v, i) = g.world.sprite_quad(start, 6.0, color, &camera);
                    additive.push((gl, v, i));
                }
            }
        }
        let target = ModelTarget {
            color: &view,
            depth: &g.depth,
            aspect,
            clear_depth: false,
            alpha: 1.0,
        };
        g.world.draw_models(&g.gpu, &camera, &target, &items);
        if !additive.is_empty() {
            g.world.draw_additive(&g.gpu, &camera, &target, &additive);
        }
        for d in &ghost_draws {
            let gm_script::DrawCmd::Model {
                model,
                pos,
                angles,
                alpha,
            } = d
            else {
                continue;
            };
            if let Some(m) = g.models.get(model.as_str()).and_then(|m| m.as_ref()) {
                let ghost = [DrawItem {
                    model: m,
                    transform: gm_render::world::model_transform(*pos, *angles),
                    bones: None,
                }];
                let target = ModelTarget {
                    alpha: *alpha,
                    ..target
                };
                g.world.draw_models(&g.gpu, &camera, &target, &ghost);
            }
        }
        // viewmodel: own projection (narrower fov, near plane at 1 u) over a cleared depth buffer
        if let (Some(vm), Some((seq, t))) = (vm, vm_seq) {
            let path = vm.path;
            if !g.models.contains_key(path) {
                let m = g.world.load_model(&g.gpu, &self.mount, path);
                g.models.insert(path.to_string(), m);
            }
            let hands_key = format!("{}#{}#{}", hands_of.model, hands_of.skin, hands_of.body);
            if !g.models.contains_key(&hands_key) {
                let m = g.world.load_model_variant(
                    &g.gpu,
                    &self.mount,
                    hands_of.model,
                    hands_of.skin,
                    hands_of.body,
                );
                g.models.insert(hands_key.clone(), m);
            }
            if let Some(m) = g.models.get(path).and_then(|m| m.as_ref()) {
                let world = m.bone_world(seq, t, None, &[]);
                let pose = world.as_ref().map(|w| m.skin_matrices(w));
                let hands = g.models.get(&hands_key).and_then(|h| h.as_ref());
                let hands_pose = match (hands, &world) {
                    (Some(h), Some(w)) => h.pose_merged("idle", 0.0, m, w),
                    _ => None,
                };
                // Source fovs are given for 4:3; wider screens widen the horizontal fov
                let fov_x = 2.0
                    * ((VIEWMODEL_FOV.to_radians() * 0.5).tan() * aspect * 0.75)
                        .atan()
                        .to_degrees();
                let vm_camera = Camera {
                    fov_x,
                    near: 1.0,
                    far: 512.0,
                    ..camera
                };
                let transform = gm_render::world::model_transform(camera.position, self.view);
                let mut vm_items = vec![DrawItem {
                    model: m,
                    transform,
                    bones: pose.as_deref(),
                }];
                if let (Some(h), Some(hp)) = (hands, &hands_pose) {
                    vm_items.push(DrawItem {
                        model: h,
                        transform,
                        bones: Some(hp),
                    });
                }
                let target = ModelTarget {
                    clear_depth: true,
                    ..target
                };
                g.world.draw_models(&g.gpu, &vm_camera, &target, &vm_items);
            }
        }
        // 3D2D planes of the previous frame's HUD handlers, on the world's depth
        {
            let g = self.gfx.as_ref().unwrap();
            let planes = hud_planes(&self.sim.hud_draws, &self.mount, g);
            if !planes.is_empty() {
                let aspect = g.config.width as f32 / g.config.height as f32;
                g.ui.draw_world(&g.gpu, &view, &g.depth, camera.view_proj(aspect), &planes);
            }
        }
        {
            let g = self.gfx.as_ref().unwrap();
            let (w, h) = (g.config.width, g.config.height);
            self.sim.hud_draws = self.sim.script.client_render(&self.sim.client.world, w, h);
            if let Some(gm) = self.gfx.as_mut() {
                // `draw.font` builds its atlas now and reports the metrics to the runtime;
                // `draw.texture` decodes its VTF once
                for d in &self.sim.hud_draws {
                    match d {
                        gm_script::DrawCmd::Font { name, family, size } => {
                            if gm
                                .ui
                                .register_font(&gm.gpu, &self.mount, name, family, *size)
                            {
                                let m = gm.ui.font_metrics(Some(name));
                                gm_script::set_font_metrics(
                                    name,
                                    gm_script::TextMetrics {
                                        line_height: m.line_height,
                                        advances: m.advances,
                                        fallback: m.fallback,
                                    },
                                );
                            }
                        }
                        gm_script::DrawCmd::Image { material, .. } => {
                            gm.ui.load_image(&gm.gpu, &self.mount, material);
                        }
                        _ => {}
                    }
                }
            }
        }
        let ui_items = self.ui_items(o);
        let g = self.gfx.as_ref().unwrap();
        g.ui.draw(&g.gpu, &view, g.config.width, g.config.height, &ui_items);
        g.gpu.queue.present(frame);
        self.frames += 1;
        let el = self.fps_at.elapsed().as_secs_f32();
        if el >= 1.0 {
            let w = &self.sim.client.world;
            g.window.set_title(&format!(
                "gmod-rust  {:.0} fps  pos ({:.0} {:.0} {:.0})  props {}  weapon {}",
                self.frames as f32 / el,
                o.x,
                o.y,
                o.z,
                w.entities.len(),
                self.vm_of(self.weapon).map_or("none", |v| v.name)
            ));
            self.frames = 0;
            self.fps_at = Instant::now();
        }
    }
}

impl App {
    fn tool_settings_list(&self) -> Vec<(String, String)> {
        let schema = self.tool_schema(&self.tool);
        let empty = BTreeMap::new();
        let set = self.tool_settings.get(&self.tool).unwrap_or(&empty);
        gm_net::tool_click_settings(schema, set)
    }

    fn tool_schema(&self, tool: &str) -> Option<&[gm_server::ToolSetting]> {
        self.sim
            .client
            .world
            .tool_schemas
            .iter()
            .find(|(t, _)| t == tool)
            .map(|(_, s)| s.as_slice())
    }

    fn ui_items(&self, o: Vec3) -> Vec<UiItem> {
        let Some(g) = &self.gfx else {
            return Vec::new();
        };
        let (w, h) = (g.config.width as f32, g.config.height as f32);
        let lh = g.ui.line_height();
        let mut items = Vec::new();
        let white = [0.92, 0.92, 0.92, 1.0];
        if self.context_held || self.context.is_some() {
            // contextmenu.lua: the tool's ControlPanel moves into a 320 wide canvas bottom-right
            let font = g.ui.font_metrics(Some(derma::FONT));
            let bold = g.ui.font_metrics(Some(derma::FONT_BOLD));
            let has_image = |name: &str| g.ui.has_image(name);
            let dctx = derma::Ctx {
                w,
                h,
                cursor: self.cursor,
                font: &font,
                bold: &bold,
                lists: &self.spawnlists,
                tool: &self.tool,
                has_image: &has_image,
            };
            let mut layout = derma::Layout::default();
            if let (Some(schema), true) = (self.panel_schema(), self.menu.is_none()) {
                let rect = derma::context_canvas(&mut layout, &dctx, &schema);
                let set = self.tool_settings.get(&self.tool);
                let value_of = |name: &str| {
                    set.and_then(|m| m.get(name)).cloned().unwrap_or_else(|| {
                        schema
                            .iter()
                            .find(|s| s.name == name)
                            .map(|s| s.default.clone())
                            .unwrap_or_default()
                    })
                };
                let label_of = |name: &str| derma::setting_label(&self.lang, &self.tool, name);
                let focused = self.panel_text.as_ref().map(|(r, v)| (*r, v.as_str()));
                let rows = derma::controls(
                    &mut layout,
                    &dctx,
                    rect,
                    &schema,
                    &value_of,
                    focused,
                    &label_of,
                );
                self.menu_cache.borrow_mut().rows = rows;
            }
            if let Some(ctx) = &self.context {
                let labels: Vec<String> = ctx
                    .names
                    .iter()
                    .map(|n| derma::property_label(&self.lang, n))
                    .collect();
                let icons: Vec<&str> = ctx
                    .names
                    .iter()
                    .map(|n| {
                        derma::PROPERTIES
                            .iter()
                            .find(|p| p.0 == n)
                            .map_or("", |p| p.2)
                    })
                    .collect();
                derma::dmenu(
                    &mut layout,
                    &dctx,
                    ctx.x,
                    ctx.y,
                    &ctx.names,
                    &labels,
                    &icons,
                );
            }
            items.extend(layout.items);
        }
        if let Some(menu) = &self.menu {
            let font = g.ui.font_metrics(Some(derma::FONT));
            let bold = g.ui.font_metrics(Some(derma::FONT_BOLD));
            let has_image = |name: &str| g.ui.has_image(name);
            let ctx = derma::Ctx {
                w,
                h,
                cursor: self.cursor,
                font: &font,
                bold: &bold,
                lists: &self.spawnlists,
                tool: &self.tool,
                has_image: &has_image,
            };
            let mut layout = derma::build(&ctx, &menu.st);
            let rows = match self.panel_schema() {
                Some(schema) => {
                    let set = self.tool_settings.get(&self.tool);
                    let value_of = |name: &str| {
                        set.and_then(|m| m.get(name)).cloned().unwrap_or_else(|| {
                            schema
                                .iter()
                                .find(|s| s.name == name)
                                .map(|s| s.default.clone())
                                .unwrap_or_default()
                        })
                    };
                    let label_of = |name: &str| derma::setting_label(&self.lang, &self.tool, name);
                    let focused = self.panel_text.as_ref().map(|(r, v)| (*r, v.as_str()));
                    let control = layout.control;
                    derma::controls(
                        &mut layout,
                        &ctx,
                        control,
                        &schema,
                        &value_of,
                        focused,
                        &label_of,
                    )
                }
                None => Vec::new(),
            };
            *self.menu_cache.borrow_mut() = MenuCache {
                visible: layout.visible.clone(),
                max_scroll: layout.max_scroll,
                rows,
            };
            items.extend(layout.items);
            return items;
        }
        if !self.console.open {
            let hud = format!(
                "props {}  weapon {}{}  pos {:.0} {:.0} {:.0}",
                self.sim.client.world.entities.len(),
                self.vm_of(self.weapon).map_or("none", |v| v.name),
                if self.weapon == 1 {
                    format!(" ({})", self.tool)
                } else {
                    String::new()
                },
                o.x,
                o.y,
                o.z
            );
            items.push(UiItem::Rect {
                x: 4.0,
                y: 4.0,
                w: g.ui.text_width(&hud) + 12.0,
                h: lh + 6.0,
                color: [0.0, 0.0, 0.0, 0.45],
            });
            items.push(UiItem::Text {
                font: None,
                x: 10.0,
                y: 7.0,
                text: hud,
                color: white,
            });
            // client addons' `render.hud`
            // 3D2D blocks were drawn in the world before the HUD; the rest is screen space
            let mut in_plane = false;
            for d in &self.sim.hud_draws {
                match d {
                    gm_script::DrawCmd::Begin3D2D { .. } => {
                        in_plane = true;
                        continue;
                    }
                    gm_script::DrawCmd::End3D2D => {
                        in_plane = false;
                        continue;
                    }
                    _ if in_plane => continue,
                    _ => {}
                }
                items.push(match d {
                    // a world draw from a HUD handler: nothing on the 2D layer
                    gm_script::DrawCmd::Model { .. } => continue,
                    gm_script::DrawCmd::Begin3D2D { .. } | gm_script::DrawCmd::End3D2D => continue,
                    // fonts and images were prepared before the frame
                    gm_script::DrawCmd::Font { .. } => continue,
                    gm_script::DrawCmd::Rect { x, y, w, h, color } => UiItem::Rect {
                        x: *x,
                        y: *y,
                        w: *w,
                        h: *h,
                        color: *color,
                    },
                    gm_script::DrawCmd::Text {
                        x,
                        y,
                        text,
                        color,
                        font,
                    } => UiItem::Text {
                        x: *x,
                        y: *y,
                        text: text.clone(),
                        color: *color,
                        font: font.clone(),
                    },
                    gm_script::DrawCmd::Image {
                        x,
                        y,
                        w,
                        h,
                        material,
                        color,
                    } => UiItem::Image {
                        x: *x,
                        y: *y,
                        w: *w,
                        h: *h,
                        material: material.clone(),
                        color: *color,
                        rot: 0,
                    },
                });
            }
            // sandbox HUD: health and suit boxes at the bottom left
            let me = self.sim.client.player_id;
            if let Some(p) = self
                .sim
                .client
                .world
                .players
                .iter()
                .find(|p| Some(p.id) == me)
            {
                // the game's HL2 HUD: scripts/hudlayout.res positions on a 640x480 layout
                // scaled by the screen height, ClientScheme.res fonts and colours
                let s = h / 480.0;
                let fg = [1.0, 235.0 / 255.0, 20.0 / 255.0, 1.0];
                let bg = [0.0, 0.0, 0.0, 76.0 / 255.0];
                let numbers = Some("HudNumbers".to_string());
                let small = Some("HudNumbersSmall".to_string());
                let label_font = Some("HudDefault".to_string());
                let y = 432.0 * s;
                let tall = 36.0 * s;
                let panel = |items: &mut Vec<UiItem>, x: f32, wide: f32, label: &str| {
                    items.push(UiItem::Rect {
                        x,
                        y,
                        w: wide,
                        h: tall,
                        color: bg,
                    });
                    items.push(UiItem::Text {
                        font: label_font.clone(),
                        x: x + 8.0 * s,
                        y: y + 20.0 * s,
                        text: label.to_string(),
                        color: fg,
                    });
                };
                // HudCrosshair: the weapon script's Crosshairs glyph (Q for every weapon the
                // loadout has) centred, ClientScheme Normal 255 208 64
                if p.alive && self.menu.is_none() && !self.context_held {
                    let cm = g.ui.font_metrics(Some("Crosshairs"));
                    items.push(UiItem::Text {
                        font: Some("Crosshairs".to_string()),
                        x: w / 2.0 - cm.text_width("Q") / 2.0,
                        y: h / 2.0 - cm.line_height / 2.0,
                        text: "Q".to_string(),
                        color: [1.0, 208.0 / 255.0, 64.0 / 255.0, 1.0],
                    });
                }
                panel(&mut items, 16.0 * s, 102.0 * s, "HEALTH");
                items.push(UiItem::Text {
                    font: numbers.clone(),
                    x: 16.0 * s + 50.0 * s,
                    y: y + 2.0 * s,
                    text: p.health.max(0).to_string(),
                    color: fg,
                });
                if p.armor > 0 {
                    panel(&mut items, 140.0 * s, 102.0 * s, "SUIT");
                    items.push(UiItem::Text {
                        font: numbers.clone(),
                        x: 140.0 * s + 50.0 * s,
                        y: y + 2.0 * s,
                        text: p.armor.to_string(),
                        color: fg,
                    });
                }
                // PaintNotes -> notification Think: spring the panels to the anchor at 80 %
                // height, charge left 4 % at 0.7 s left, leave right at 0.2 s
                self.notices.borrow_mut().draw(&mut items, w, h, &g.ui);
                self.draw_weapon_select(&mut items, w, h, &g.ui);
                // HUDDrawPickupHistory: notices slide in from the right edge with their alpha,
                // corner8 border in the pickup colour, grey body, DermaDefaultBold text
                {
                    let mut ph = self.pickups.borrow_mut();
                    let fm = g.ui.font_metrics(Some("DermaDefaultBold"));
                    ph.draw(&mut items, w, h, &fm);
                }
                // DrawDeathNotice(0.85, 0.04): kill icon glyph (HL2MPTypeDeath, 255 80 0),
                // attacker right-aligned 16 px left of it, victim 16 px right, ChatFont,
                // fading out over the last second of hud_deathnotice_time 6
                {
                    let icon_m = g.ui.font_metrics(Some("HL2MPTypeDeath"));
                    let chat_m = g.ui.font_metrics(Some("ChatFont"));
                    let x = (0.85 * w).floor();
                    let mut y = (0.04 * h).floor();
                    let fh = icon_m.line_height * 0.75;
                    for d in &self.deaths {
                        let left = 6.0 - d.at.elapsed().as_secs_f32();
                        if left <= 0.0 {
                            continue;
                        }
                        let a = (left * 255.0).clamp(0.0, 255.0) / 255.0;
                        let (glyph, height_scale) = kill_icon(&d.inflictor);
                        let gw = icon_m.text_width(glyph);
                        let gh = icon_m.line_height * height_scale;
                        let (iw, ih) = (gw * (fh / gh), fh);
                        items.push(UiItem::Text {
                            font: Some("HL2MPTypeDeath".to_string()),
                            x: x - iw / 2.0,
                            y: y - ih * 0.1,
                            text: glyph.to_string(),
                            color: [1.0, 80.0 / 255.0, 0.0, a],
                        });
                        let ty = y + ih / 2.0 - chat_m.line_height / 2.0;
                        if let Some(l) = &d.left {
                            items.push(UiItem::Text {
                                font: Some("ChatFont".to_string()),
                                x: x - iw / 2.0 - 16.0 - chat_m.text_width(l),
                                y: ty,
                                text: l.clone(),
                                color: [1.0, 1.0, 100.0 / 255.0, a],
                            });
                        }
                        items.push(UiItem::Text {
                            font: Some("ChatFont".to_string()),
                            x: x + iw / 2.0 + 16.0,
                            y: ty,
                            text: d.right.clone(),
                            color: [1.0, 1.0, 100.0 / 255.0, a],
                        });
                        y = (y + ih * 0.75).ceil();
                    }
                }
                // HUDDrawTargetID: the aimed player's name and health under the crosshair
                // (TargetID at centre + 30, two shadows, the team colour 255 255 100)
                {
                    let (pt, yw) = (self.view.x.to_radians(), self.view.y.to_radians());
                    let fwd = Vec3::new(pt.cos() * yw.cos(), pt.cos() * yw.sin(), -pt.sin());
                    let world_d = (self
                        .sim
                        .collision
                        .trace_ray(o, o + fwd * 8192.0, gm_collision::MASK_SOLID)
                        .end
                        - o)
                        .length();
                    let mut best: Option<(f32, &gm_net::msg::PlayerSnap)> = None;
                    for q in &self.sim.client.world.players {
                        if q.id == p.id || !q.alive {
                            continue;
                        }
                        let top = if q.ducked { 36.0 } else { 72.0 };
                        let (mins, maxs) = (
                            q.origin + Vec3::new(-16.0, -16.0, 0.0),
                            q.origin + Vec3::new(16.0, 16.0, top),
                        );
                        let (mut t0, mut t1) = (0.0f32, world_d);
                        for i in 0..3 {
                            let (d, s0) = (fwd[i], o[i]);
                            if d.abs() < 1e-6 {
                                if s0 < mins[i] || s0 > maxs[i] {
                                    t0 = f32::INFINITY;
                                }
                                continue;
                            }
                            let (ta, tb) = ((mins[i] - s0) / d, (maxs[i] - s0) / d);
                            t0 = t0.max(ta.min(tb));
                            t1 = t1.min(ta.max(tb));
                        }
                        if t0 <= t1 && best.is_none_or(|(bt, _)| t0 < bt) {
                            best = Some((t0, q));
                        }
                    }
                    if let Some((_, q)) = best {
                        let team = [1.0, 1.0, 100.0 / 255.0, 1.0];
                        let mut yy = h / 2.0 + 30.0;
                        for (font, text) in [
                            ("TargetID", q.name.clone()),
                            ("TargetIDSmall", format!("{}%", q.health.max(0))),
                        ] {
                            let m = g.ui.font_metrics(Some(font));
                            let x = w / 2.0 - m.text_width(&text) / 2.0;
                            for (dx, a) in [(1.0, 120.0 / 255.0), (2.0, 50.0 / 255.0)] {
                                items.push(UiItem::Text {
                                    font: Some(font.to_string()),
                                    x: x + dx,
                                    y: yy + dx,
                                    text: text.clone(),
                                    color: [0.0, 0.0, 0.0, a],
                                });
                            }
                            items.push(UiItem::Text {
                                font: Some(font.to_string()),
                                x,
                                y: yy,
                                text,
                                color: team,
                            });
                            yy += m.line_height + 5.0;
                        }
                    }
                }
                if p.clip >= 0 || p.reserve > 0 {
                    let ax = w - 150.0 * s;
                    panel(&mut items, ax, 136.0 * s, "AMMO");
                    let clip = if p.clip >= 0 { p.clip } else { p.reserve };
                    items.push(UiItem::Text {
                        font: numbers.clone(),
                        x: ax + 40.0 * s,
                        y: y + 2.0 * s,
                        text: clip.to_string(),
                        color: fg,
                    });
                    if p.clip >= 0 {
                        items.push(UiItem::Text {
                            font: small.clone(),
                            x: ax + 102.0 * s,
                            y: y + 16.0 * s,
                            text: p.reserve.to_string(),
                            color: fg,
                        });
                    }
                    // HudSecondaryAmmo: r150 wide 60 to the left of the ammo panel
                    if p.alt >= 0 {
                        let bx = ax - 60.0 * s - 6.0 * s;
                        panel(&mut items, bx, 60.0 * s, "ALT");
                        items.push(UiItem::Text {
                            font: numbers.clone(),
                            x: bx + 8.0 * s,
                            y: y + 2.0 * s,
                            text: p.alt.to_string(),
                            color: fg,
                        });
                    }
                }
            }
            return items;
        }
        let panel = (h * 0.5).round();
        items.push(UiItem::Rect {
            x: 0.0,
            y: 0.0,
            w,
            h: panel,
            color: [0.05, 0.05, 0.07, 0.88],
        });
        items.push(UiItem::Rect {
            x: 0.0,
            y: panel - lh - 8.0,
            w,
            h: 1.0,
            color: [0.4, 0.4, 0.45, 1.0],
        });
        let visible = ((panel - lh - 12.0) / lh).max(0.0) as usize;
        let total = self.console.lines.len();
        let scroll = self.console.scroll.min(total.saturating_sub(visible));
        let end = total - scroll;
        let start = end.saturating_sub(visible);
        let top = panel - lh - 12.0 - (end - start) as f32 * lh;
        for (i, line) in self.console.lines.range(start..end).enumerate() {
            let n = g.ui.fit(line, w - 16.0);
            let color = if line.starts_with("] ") {
                [1.0, 0.85, 0.5, 1.0]
            } else {
                white
            };
            items.push(UiItem::Text {
                font: None,
                x: 8.0,
                y: top + i as f32 * lh,
                text: line.chars().take(n).collect(),
                color,
            });
        }
        let blink = (self.console.opened_at.elapsed().as_millis() / 500).is_multiple_of(2);
        let shown: String = {
            let full = format!("] {}", self.console.input);
            let max = g.ui.fit(&full, w - 24.0);
            let skip = full.chars().count().saturating_sub(max);
            full.chars().skip(skip).collect()
        };
        items.push(UiItem::Text {
            font: None,
            x: 8.0,
            y: panel - lh - 4.0,
            text: format!("{shown}{}", if blink { "_" } else { "" }),
            color: white,
        });
        items
    }

    fn toggle_console(&mut self) {
        self.console.open = !self.console.open;
        self.console.opened_at = Instant::now();
        self.keys.clear();
        self.mouse.clear();
        let open = self.console.open;
        self.set_grab(!open);
    }

    fn console_key(&mut self, el: &ActiveEventLoop, code: Option<KeyCode>, text: Option<&str>) {
        match code {
            Some(KeyCode::Enter | KeyCode::NumpadEnter) => {
                let line = std::mem::take(&mut self.console.input);
                self.console.history_at = None;
                self.run_console(el, &line);
            }
            Some(KeyCode::Backspace) => {
                self.console.input.pop();
            }
            Some(KeyCode::Escape) => self.toggle_console(),
            Some(KeyCode::ArrowUp) => self.console.history_step(true),
            Some(KeyCode::ArrowDown) => self.console.history_step(false),
            Some(KeyCode::PageUp) => self.console.scroll += 5,
            Some(KeyCode::PageDown) => self.console.scroll = self.console.scroll.saturating_sub(5),
            _ => {
                if let Some(t) = text {
                    self.console
                        .input
                        .extend(t.chars().filter(|c| !c.is_control()));
                }
            }
        }
    }

    fn run_console(&mut self, el: &ActiveEventLoop, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        self.console.log(format!("] {line}"));
        if self.console.history.last().map(String::as_str) != Some(line) {
            self.console.history.push(line.to_string());
        }
        self.console.scroll = 0;
        let mut words = line.split_whitespace();
        let verb = words.next().unwrap_or("");
        let rest: Vec<&str> = words.collect();
        match verb {
            "quit" | "exit" => el.exit(),
            "clear" => self.console.lines.clear(),
            "spawn" => self.sim.client.command(Command::Spawn {
                model: rest.first().unwrap_or(&SPAWN_MODEL).to_string(),
            }),
            "undo" => self.sim.client.command(Command::Undo),
            "tool" => match rest.first() {
                Some(t) if rest.len() >= 3 => {
                    let value = rest[2..].join(" ");
                    self.tool_settings
                        .entry(t.to_string())
                        .or_default()
                        .insert(rest[1].to_string(), value.clone());
                    self.console.log(format!("tool {t}: {} = {value}", rest[1]));
                }
                Some(t) if TOOLS.contains(t) => {
                    self.tool = t.to_string();
                    self.console.log(format!("tool {t}"));
                }
                Some(t) => {
                    // a stool of an addon: the server decides whether it exists
                    self.tool = t.to_string();
                    match self.tool_schema(t).map(<[_]>::to_vec) {
                        Some(schema) => {
                            self.console.log(format!("tool {t} (scripted), settings:"));
                            for s in schema {
                                let cur = self
                                    .tool_settings
                                    .get(*t)
                                    .and_then(|m| m.get(&s.name))
                                    .cloned()
                                    .unwrap_or_else(|| s.default.clone());
                                let range = match s.kind {
                                    gm_server::ToolSettingKind::Number => {
                                        format!(" number {}..{}", s.min, s.max)
                                    }
                                    gm_server::ToolSettingKind::Bool => " bool".into(),
                                    gm_server::ToolSettingKind::Text => " text".into(),
                                };
                                self.console.log(format!(
                                    "  {} = {cur}{range} (default {})",
                                    s.name, s.default
                                ));
                            }
                        }
                        None => self.console.log(format!(
                            "tool {t} (scripted; built in: {})",
                            TOOLS.join(" ")
                        )),
                    }
                }
                None => self
                    .console
                    .log(format!("tool {} ({})", self.tool, TOOLS.join(" "))),
            },
            "bot" => {
                let id = 1000 + self.sim.server.players.len() as u32;
                let yaw = self.view.y.to_radians();
                let fwd = Vec3::new(yaw.cos(), yaw.sin(), 0.0);
                let pitch = rest.first().and_then(|a| a.parse().ok()).unwrap_or(0.0);
                self.sim.server.add_player(
                    id,
                    self.sim.origin + fwd * 96.0,
                    Vec3::new(pitch, self.view.y + 180.0, 0.0),
                );
                self.console.log(format!("bot {id} added"));
            }
            "cleanup" => self.sim.client.command(Command::Cleanup),
            "property" if !rest.is_empty() => self.sim.client.command(Command::Property {
                name: rest.join(" "),
            }),
            "playermodel" if !rest.is_empty() => {
                self.sim.client.command(Command::PlayerModel {
                    name: rest.join(" "),
                });
            }
            // `cl_playerskin` / `cl_playerbodygroups`: both ride in one command, so each keeps
            // the other's current value
            "playerskin" if !rest.is_empty() => {
                self.player_skin = rest[0].parse().unwrap_or(0);
                self.send_player_skin();
            }
            "playerbodygroups" => {
                self.player_body = rest.join("");
                self.send_player_skin();
            }
            "kill" => self.sim.client.command(Command::Kill),
            // a weapon by class, scripted ones included (their index follows the stock table)
            "weapon" if !rest.is_empty() => {
                let class = rest.join(" ");
                let classes = gm_net::msg::weapon_classes(&self.sim.client.world.weapon_defs);
                match classes.iter().position(|c| *c == class) {
                    Some(i) if i < 255 => self.select_weapon(i as u8),
                    _ => self.console.log(format!("unknown weapon {class}")),
                }
            }
            "say" => self.sim.client.command(Command::Chat {
                text: rest.join(" "),
            }),
            "noclip" => {
                self.noclip = !self.noclip;
                self.sim.client.command(Command::Noclip { on: self.noclip });
            }
            "help" => {
                self.console
                    .log("client: spawn [model] | tool [name] | bot | say <text> | noclip | kill | playermodel <name> | playerskin <n> | playerbodygroups <digits> | undo | cleanup | clear | quit; server:");
                self.sim.client.command(Command::Console {
                    line: line.to_string(),
                });
            }
            _ => self.sim.client.command(Command::Console {
                line: line.to_string(),
            }),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_none() {
            if let Err(e) = self.init_gfx(el) {
                self.error = Some(e);
                el.exit();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => {
                if let Some(g) = &mut self.gfx {
                    g.config.width = s.width.max(1);
                    g.config.height = s.height.max(1);
                    g.surface.configure(&g.gpu.device, &g.config);
                    g.depth = depth_view(&g.gpu, g.config.width, g.config.height);
                }
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                // focus changes replay held keys as synthetic presses / releases; they must not
                // close the spawn menu or type into the console
                if is_synthetic {
                    return;
                }
                let code = match event.physical_key {
                    PhysicalKey::Code(code) => Some(code),
                    _ => None,
                };
                let down = event.state == ElementState::Pressed;
                if down && !event.repeat && code == Some(KeyCode::Backquote) {
                    self.toggle_console();
                    return;
                }
                if self.console.open {
                    if down {
                        self.console_key(el, code, event.text.as_deref());
                    }
                    return;
                }
                // the held menu key keeps its meaning (and its repeats stay out of the field)
                if self.menu.is_some() && self.panel_text.is_some() && code != Some(KeyCode::KeyQ) {
                    if down {
                        self.panel_key(code, event.text.as_deref());
                    }
                    return;
                }
                let Some(code) = code else { return };
                if !event.repeat {
                    self.pending_input.push((format!("{code:?}"), down));
                }
                if down && !event.repeat {
                    match code {
                        KeyCode::Escape if self.grabbed => self.set_grab(false),
                        KeyCode::Escape => el.exit(),
                        KeyCode::Digit1 => self.wsel_slot(0),
                        KeyCode::Digit2 => self.wsel_slot(1),
                        KeyCode::Digit3 => self.wsel_slot(2),
                        KeyCode::Digit4 => self.wsel_slot(3),
                        KeyCode::Digit5 => self.wsel_slot(4),
                        KeyCode::Digit6 => self.wsel_slot(5),
                        KeyCode::KeyQ => self.open_menu(),
                        KeyCode::KeyC => {
                            if self.menu.is_none() && !self.context_held {
                                self.context_held = true;
                                self.set_grab(false);
                                // cl_spawnmenu.lua OnContextMenuOpen
                                self.add_hint("ContextClick", 20);
                            }
                        }
                        KeyCode::KeyZ => self.sim.client.command(Command::Undo),
                        KeyCode::KeyV => {
                            self.noclip = !self.noclip;
                            self.sim.client.command(Command::Noclip { on: self.noclip });
                        }
                        KeyCode::KeyR if self.weapon == 1 => {
                            self.sim.client.command(Command::ToolClick {
                                tool: self.tool.clone(),
                                right: false,
                                reload: true,
                                settings: self.tool_settings_list(),
                            })
                        }
                        // the reload animation when the clip can take rounds from the reserve
                        _ => {}
                    }
                }
                if down {
                    self.keys.insert(code);
                } else {
                    self.keys.remove(&code);
                    if code == KeyCode::KeyQ && self.menu.is_some() {
                        self.menu = None;
                        self.panel_drag = None;
                        self.panel_text = None;
                        self.set_grab(true);
                    }
                    if code == KeyCode::KeyC && self.context_held {
                        self.context_held = false;
                        self.context = None;
                        if self.menu.is_none() {
                            self.set_grab(true);
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                self.panel_drag_move();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32 / 16.0,
                };
                if let Some(menu) = &mut self.menu {
                    let max = self.menu_cache.borrow().max_scroll;
                    menu.st.scroll = (menu.st.scroll - lines * 64.0).clamp(0.0, max);
                } else if !self.console.open && !self.context_held && self.grabbed {
                    // invprev / invnext walk the buckets in order
                    if lines > 0.0 {
                        self.wsel_step(-1);
                    } else if lines < 0.0 {
                        self.wsel_step(1);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if self.console.open {
                    return;
                }
                let name = match button {
                    MouseButton::Left => "MouseLeft",
                    MouseButton::Right => "MouseRight",
                    MouseButton::Middle => "MouseMiddle",
                    MouseButton::Back => "MouseBack",
                    MouseButton::Forward => "MouseForward",
                    MouseButton::Other(_) => "MouseOther",
                };
                self.pending_input
                    .push((name.to_string(), state == ElementState::Pressed));
                if button == MouseButton::Left && state == ElementState::Released {
                    self.panel_drag = None;
                }
                if self.menu.is_some() {
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        self.menu_click();
                    }
                    return;
                }
                if self.context_held {
                    if state == ElementState::Pressed {
                        match button {
                            MouseButton::Right => {
                                self.context = None;
                                self.sim.client.properties = None;
                                self.sim.client.command(Command::PropertyList);
                            }
                            MouseButton::Left => {
                                if self.panel_click() {
                                    return;
                                }
                                if let Some(name) = self.context_row() {
                                    self.sim.client.command(Command::Property { name });
                                    self.context = None;
                                }
                            }
                            _ => {}
                        }
                    }
                    return;
                }
                if !self.grabbed && state == ElementState::Pressed {
                    self.set_grab(true);
                    return;
                }
                if let Some(i) = self.wsel {
                    // +attack confirms the highlighted weapon and is swallowed
                    if button == MouseButton::Left {
                        if state == ElementState::Pressed {
                            self.wsel = None;
                            if let Some(l) = self.to_local(i) {
                                self.select_weapon(l);
                            }
                            self.sounds.emit(&self.mount, "Player.WeaponSelected", None);
                        }
                        return;
                    }
                }
                let b = match button {
                    MouseButton::Left => 0,
                    MouseButton::Right => 1,
                    _ => return,
                };
                if state == ElementState::Pressed {
                    if self.mouse.insert(b) {
                        if self.weapon == 1 {
                            self.sim.client.command(Command::ToolClick {
                                tool: self.tool.clone(),
                                right: b == 1,
                                reload: false,
                                settings: self.tool_settings_list(),
                            });
                        }
                        if let Some(vm) = self.vm_of(self.weapon) {
                            // left: the shot comes back as the server's fire event (so held
                            // automatic fire and dry fire play right); right: the secondary
                            // fire where the weapon has one (the physgun's right click only
                            // acts while holding)
                            if b == 1 {
                                if let Some(alt) = vm.alt {
                                    self.vm_anim = Some((alt, Instant::now()));
                                }
                            }
                        }
                    }
                } else {
                    self.mouse.remove(&b);
                }
            }
            WindowEvent::Focused(false) => {
                self.keys.clear();
                self.mouse.clear();
            }
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if self.menu.is_some() {
            return;
        }
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.grabbed {
                // m_yaw / m_pitch 0.022 at sensitivity 3
                self.view.y -= delta.0 as f32 * 0.066;
                self.view.x = (self.view.x + delta.1 as f32 * 0.066).clamp(-89.0, 89.0);
                self.view.y = self.view.y.rem_euclid(360.0);
            }
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        self.step();
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }
}

fn main() -> Result<()> {
    let arg_f = |i: usize| std::env::args().nth(i).and_then(|v| v.parse::<f32>().ok());
    let argv: Vec<String> = std::env::args().collect();
    let connect = argv
        .iter()
        .position(|a| a == "--connect")
        .and_then(|i| argv.get(i + 1).cloned());
    let positional = argv
        .iter()
        .enumerate()
        .skip(1)
        .find(|(i, a)| !a.starts_with("--") && !argv[i - 1].starts_with("--"))
        .map(|(_, a)| a.clone());
    // `--connect host:port`: the server names the map in its Welcome, so ask it first
    let (remote_client, remote_ep, map) = match connect {
        Some(addr) => {
            let mut udp = gm_net::Udp::client(addr.parse().context("--connect address")?)?;
            let mut client = ClientNet::new(1, "player");
            let start = Instant::now();
            while client.map.is_empty() {
                if start.elapsed() > std::time::Duration::from_secs(5) {
                    bail!("no answer from {addr} in 5 s");
                }
                client.tick(NetCmd::default(), &mut udp);
                client.receive(&mut udp);
                std::thread::sleep(std::time::Duration::from_secs_f32(
                    gm_core::tick::DEFAULT_TICK_INTERVAL,
                ));
            }
            let map = client.map.clone();
            eprintln!("connected to {addr}: map {map}");
            (Some(client), Some(Box::new(udp) as Box<dyn Transport>), map)
        }
        None => (
            None,
            None,
            positional.unwrap_or_else(|| "gm_construct".into()),
        ),
    };
    let inst =
        gm_content::find_install().ok_or_else(|| anyhow!("no GMod install (set GMOD_DIR)"))?;
    let mount = gm_content::Mount::for_install(&inst)?;
    let sounds = gm_audio::Sounds::new(&mount);
    eprintln!(
        "audio: {} ({} script entries)",
        sounds.mixer.device,
        sounds.scripts.len()
    );
    let bsp_data = mount.read(&format!("maps/{map}.bsp"))?;
    let (spawn, yaw) = {
        let bsp = gm_formats::bsp::Bsp::parse(&bsp_data)?;
        let e = bsp
            .entities
            .iter()
            .find(|e| e.classname() == "info_player_start")
            .ok_or_else(|| anyhow!("no info_player_start"))?;
        let o = e.origin().unwrap_or([0.0; 3]);
        (Vec3::from(o), e.angles().unwrap_or([0.0; 3])[1])
    };
    let gm_dedicated::LoadedWorld {
        collision,
        physics,
        world_body,
    } = gm_dedicated::load_world(&mount, &map)?;
    let collision: &'static gm_collision::World = Box::leak(Box::new(collision));
    let mut server = gm_server::Server::new(collision, physics, world_body);
    let models = gm_content::Mount::for_install(&inst)?;
    server.model_loader = Some(Box::new(move |m: &str| {
        gm_dedicated::model_info(&models, m)
            .map_err(|e| eprintln!("model {m}: {e}"))
            .ok()
    }));
    let (steps_mount, table) = (
        gm_content::Mount::for_install(&inst)?,
        gm_dedicated::surface_table(&mount)?,
    );
    server.step_loader = Some(Box::new(move |m: Option<&str>| {
        gm_dedicated::step_sounds(&steps_mount, &table, m)
    }));
    let mut net = ServerNet::new(&map, spawn);
    net.admins.extend(1..=16);
    // `--addon <dir|file.gmr>` (repeatable): addons of the in-process server; their client realm
    // reaches the client realm below over the loopback like from a dedicated server
    let mut rt = gm_script::Runtime::new();
    let argv: Vec<String> = std::env::args().collect();
    for (i, a) in argv.iter().enumerate() {
        if a == "--addon" {
            if let Some(path) = argv.get(i + 1) {
                match rt.load_addon(std::path::Path::new(path)) {
                    Ok(m) => println!("addon {} {} loaded", m.id, m.version),
                    Err(e) => eprintln!("addon {path}: {e}"),
                }
            }
        }
    }
    println!("{} server actors", rt.actors().len());
    gm_script::attach(rt, &mut server, &mut net.hooks);
    let lo = Loopback::new(0, 0.0, 1);
    let (s_ep, c_ep) = (lo.endpoint(0), lo.endpoint(1));
    let remote = remote_ep.is_some();
    let sim = Sim {
        server,
        net,
        s_ep,
        c_ep: remote_ep.unwrap_or_else(|| Box::new(c_ep)),
        remote,
        lo,
        client: remote_client.unwrap_or_else(|| ClientNet::new(1, "player")),
        collision,
        pred: None,
        cmd_tick: 0,
        interp: gm_net::Interpolator::new(4.0),
        player_interp: gm_net::Interpolator::new(4.0),
        prev_origin: spawn,
        origin: spawn,
        ducked: false,
        script: {
            let mut rt = gm_script::Runtime::client();
            let hulls = gm_content::Mount::for_install(&inst)?;
            rt.set_client_world(
                collision,
                Box::new(move |m: &str| {
                    gm_dedicated::model_info(&hulls, m)
                        .ok()
                        .map(|i| i.hull_planes)
                }),
            );
            rt
        },
        addons_seen: BTreeSet::new(),
        cache: std::env::var_os("GMOD_RUST_CACHE")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("cache")),
        last_snapshot_tick: 0,
        hud_draws: Vec::new(),
    };
    let mut app = App {
        notices: std::cell::RefCell::new(Notices::default()),
        wsel: None,
        epoch: Instant::now(),
        // cl_hints.lua: GM:AddHint( "OpeningMenu", 30 ), Annoy1 5, Annoy2 7
        hints: vec![
            (
                "OpeningMenu",
                Instant::now() + std::time::Duration::from_secs(30),
            ),
            ("Annoy1", Instant::now() + std::time::Duration::from_secs(5)),
            ("Annoy2", Instant::now() + std::time::Duration::from_secs(7)),
        ],
        hints_done: vec!["OpeningMenu", "Annoy1", "Annoy2"],
        pickups: std::cell::RefCell::new(PickupHistory::default()),
        prev_weapons: None,
        deaths: Vec::new(),
        mount,
        bsp_data,
        sim,
        gfx: None,
        keys: BTreeSet::new(),
        mouse: BTreeSet::new(),
        view: Vec3::new(arg_f(3).unwrap_or(0.0), arg_f(2).unwrap_or(yaw), 0.0),
        weapon: 0,
        tool: "weld".to_string(),
        tool_settings: BTreeMap::new(),
        noclip: false,
        vm_anim: None,
        vm_serial_seen: 0,
        weapon_synced: false,
        air_since: Default::default(),
        anim_time: Default::default(),
        gestures: Default::default(),
        sounds,
        last_render: Instant::now(),
        menu: None,
        spawnlists: Vec::new(),
        lang: BTreeMap::new(),
        menu_cache: Default::default(),
        context_held: false,
        context: None,
        scripted_defs: Vec::new(),
        scripted_vms: Vec::new(),
        scripted_world: Vec::new(),
        pending_input: Vec::new(),
        spawn_lists_seen: usize::MAX,
        cursor: (0.0, 0.0),
        panel_drag: None,
        player_skin: 0,
        player_body: String::new(),
        panel_text: None,
        grabbed: false,
        last: Instant::now(),
        acc: 0.0,
        frames: 0,
        fps_at: Instant::now(),
        error: None,
        console: Console::new(),
    };
    let el = EventLoop::new()?;
    el.set_control_flow(ControlFlow::Poll);
    el.run_app(&mut app)?;
    match app.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Loads a verified client-realm package; one of an addon already running is a hot reload.
fn load_client_package(script: &mut gm_script::Runtime, path: &std::path::Path, how: &str) {
    let known = gm_script::Package::open(path)
        .ok()
        .and_then(|p| p.load_realm("client").ok())
        .is_some_and(|(m, _)| script.addon_paths.contains_key(&m.id));
    let r = if known {
        script.reload_addon(path)
    } else {
        script.load_addon(path)
    };
    match r {
        Ok(m) => println!(
            "{how} client addon {} {} {}",
            m.id,
            m.version,
            if known { "reloaded" } else { "loaded" }
        ),
        Err(e) => eprintln!("{how} client addon {}: {e}", path.display()),
    }
}

/// The 3D2D blocks of a frame's draws: (plane matrix, the 2D items between `start3d2d` and
/// `end3d2d`); `Model` and `Font` commands inside a block are ignored, images were loaded with
/// the frame's preparation.
fn hud_planes(
    draws: &[gm_script::DrawCmd],
    _mount: &gm_content::Mount,
    _g: &Gfx,
) -> Vec<(glam::Mat4, Vec<UiItem>)> {
    let mut planes = Vec::new();
    let mut current: Option<(glam::Mat4, Vec<UiItem>)> = None;
    for d in draws {
        match d {
            gm_script::DrawCmd::Begin3D2D { pos, angles, scale } => {
                if let Some(p) = current.take() {
                    planes.push(p);
                }
                current = Some((
                    gm_render::ui::plane_transform(*pos, *angles, *scale),
                    Vec::new(),
                ));
            }
            gm_script::DrawCmd::End3D2D => {
                if let Some(p) = current.take() {
                    planes.push(p);
                }
            }
            _ => {
                let Some((_, items)) = current.as_mut() else {
                    continue;
                };
                match d {
                    gm_script::DrawCmd::Rect { x, y, w, h, color } => items.push(UiItem::Rect {
                        x: *x,
                        y: *y,
                        w: *w,
                        h: *h,
                        color: *color,
                    }),
                    gm_script::DrawCmd::Text {
                        x,
                        y,
                        text,
                        color,
                        font,
                    } => items.push(UiItem::Text {
                        x: *x,
                        y: *y,
                        text: text.clone(),
                        color: *color,
                        font: font.clone(),
                    }),
                    gm_script::DrawCmd::Image {
                        x,
                        y,
                        w,
                        h,
                        material,
                        color,
                    } => items.push(UiItem::Image {
                        x: *x,
                        y: *y,
                        w: *w,
                        h: *h,
                        material: material.clone(),
                        color: *color,
                        rot: 0,
                    }),
                    _ => {}
                }
            }
        }
    }
    if let Some(p) = current.take() {
        planes.push(p);
    }
    planes
}

/// One line of the kill feed (`Deaths[]` of cl_deathnotice.lua).
struct DeathNotice {
    /// Attacker's name; none when the world killed.
    left: Option<String>,
    right: String,
    inflictor: String,
    at: Instant,
}

impl DeathNotice {
    /// `Death <victim> <attacker> <inflictor>` from the server.
    fn parse(text: &str, players: &[gm_net::msg::PlayerSnap]) -> Option<Self> {
        let mut it = text.split(' ');
        if it.next()? != "Death" {
            return None;
        }
        let victim: u32 = it.next()?.parse().ok()?;
        let attacker: u32 = it.next()?.parse().ok()?;
        let inflictor = it.next()?.to_string();
        let name = |id: u32| {
            players
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| format!("Player {id}"))
        };
        Some(Self {
            left: (attacker != 0 && attacker != victim).then(|| name(attacker)),
            right: name(victim),
            inflictor,
            at: Instant::now(),
        })
    }
}

/// `killicon.AddFont` glyphs of the base gamemode (HL2MPTypeDeath) with their height scales.
fn kill_icon(class: &str) -> (&'static str, f32) {
    match class {
        "prop_physics" | "prop_ragdoll" | "func_physbox" => ("9", 0.52),
        "weapon_smg1" => ("/", 0.55),
        "weapon_357" => (".", 0.55),
        "weapon_ar2" => ("2", 0.6),
        "crossbow_bolt" | "weapon_crossbow" => ("1", 0.5),
        "weapon_shotgun" => ("0", 0.45),
        "rpg_missile" | "weapon_rpg" => ("3", 0.35),
        "npc_grenade_frag" | "weapon_frag" | "gmod_dynamite" => ("4", 0.56),
        "weapon_pistol" => ("-", 0.52),
        "prop_combine_ball" => ("8", 0.5),
        "grenade_ar2" => ("7", 0.35),
        "weapon_stunstick" => ("!", 0.6),
        "npc_satchel" | "weapon_slam" => ("*", 0.53),
        "weapon_crowbar" => ("6", 0.45),
        "weapon_physcannon" | "weapon_physgun" => (",", 0.55),
        _ => ("9", 0.52),
    }
}

/// One pickup notice of `cl_hudpickup.lua`.
struct Pickup {
    name: String,
    amount: Option<String>,
    color: [f32; 3],
    at: Instant,
    /// Smoothed row centre; none until first drawn.
    y: Option<f32>,
    width: f32,
    height: f32,
}

/// `GM.PickupHistory*`: notices, the smoothed top and width of the column.
struct PickupHistory {
    list: Vec<Pickup>,
    top: Option<f32>,
    wide: f32,
}

impl Default for PickupHistory {
    fn default() -> Self {
        Self {
            list: Vec::new(),
            top: None,
            wide: 300.0,
        }
    }
}

impl PickupHistory {
    const HOLD: f32 = 5.0;
    const FADEIN: f32 = 0.04;
    const FADEOUT: f32 = 0.3;

    fn add(
        &mut self,
        name: &str,
        amount: Option<i32>,
        color: [f32; 3],
        fm: Option<&gm_render::ui::FontMetrics>,
    ) {
        let (mut width, height) = fm
            .map(|m| (m.text_width(name), m.line_height))
            .unwrap_or((8.0 * name.len() as f32, 13.0));
        if let Some(a) = amount {
            width += fm.map(|m| m.text_width(&a.to_string())).unwrap_or(16.0) + 16.0;
        }
        self.list.push(Pickup {
            name: name.to_string(),
            amount: amount.map(|a| a.to_string()),
            color,
            at: Instant::now(),
            y: None,
            width,
            height,
        });
    }

    fn draw(&mut self, items: &mut Vec<UiItem>, sw: f32, sh: f32, fm: &gm_render::ui::FontMetrics) {
        let top = *self.top.get_or_insert(sh / 2.0);
        let x = sw - self.wide - 20.0;
        let mut y = top;
        let (mut tall, mut wide) = (0.0f32, 0.0f32);
        let font = Some("DermaDefaultBold".to_string());
        let border = 8.0;
        let corner = "gui/corner8".to_string();
        let pickup_wide = self.wide;
        self.list.retain_mut(|v| {
            let vy = v.y.get_or_insert(y);
            *vy = (*vy * 5.0 + y) / 6.0;
            let delta = (Self::HOLD - v.at.elapsed().as_secs_f32()) / Self::HOLD;
            let colordelta = delta.clamp(0.6, 0.7);
            let alpha = if delta > 1.0 - Self::FADEIN {
                ((1.0 - delta) * (255.0 / Self::FADEIN)).clamp(0.0, 255.0)
            } else if delta < Self::FADEOUT {
                (delta * (255.0 / Self::FADEOUT)).clamp(0.0, 255.0)
            } else {
                255.0
            };
            let a = alpha / 255.0;
            let vx = x + pickup_wide - pickup_wide * a;
            let (rx, ry, rw, rh) = (
                (vx - 4.0).round(),
                (*vy - v.height / 2.0 - 4.0).round(),
                (pickup_wide + 9.0).round(),
                (v.height + 8.0).round(),
            );
            let col = [v.color[0], v.color[1], v.color[2], a];
            let img = |x: f32, y: f32, rot: u8, color: [f32; 4]| UiItem::Image {
                x: x - border / 2.0,
                y: y - border / 2.0,
                w: border,
                h: border,
                material: corner.clone(),
                color,
                rot,
            };
            // coloured left part: two corners, the edge and a square of the row height
            items.push(img(rx + border / 2.0, ry + border / 2.0, 0, col));
            items.push(img(rx + border / 2.0, ry + rh - border / 2.0, 1, col));
            items.push(UiItem::Rect {
                x: rx,
                y: ry + border,
                w: border,
                h: rh - border * 2.0,
                color: col,
            });
            items.push(UiItem::Rect {
                x: rx + border,
                y: ry,
                w: v.height - 4.0,
                h: rh,
                color: col,
            });
            let g = 230.0 * colordelta / 255.0;
            let grey = [g, g, g, a];
            items.push(img(rx + rw - border / 2.0, ry + rh - border / 2.0, 2, grey));
            items.push(img(rx + rw - border / 2.0, ry + border / 2.0, 3, grey));
            items.push(UiItem::Rect {
                x: rx + rw - border,
                y: ry + border,
                w: border,
                h: rh - border * 2.0,
                color: grey,
            });
            items.push(UiItem::Rect {
                x: rx + border + v.height - 4.0,
                y: ry,
                w: rw - (v.height - 4.0) - border * 2.0,
                h: rh,
                color: grey,
            });
            let ty = ry + rh / 2.0 - fm.line_height / 2.0;
            for (dx, c) in [(1.0, [0.0, 0.0, 0.0, a * 0.5]), (0.0, [1.0, 1.0, 1.0, a])] {
                items.push(UiItem::Text {
                    font: font.clone(),
                    x: vx + v.height + 8.0 + dx,
                    y: ty + dx,
                    text: v.name.clone(),
                    color: c,
                });
                if let Some(am) = &v.amount {
                    items.push(UiItem::Text {
                        font: font.clone(),
                        x: vx + pickup_wide - fm.text_width(am) + dx,
                        y: ty + dx,
                        text: am.clone(),
                        color: c,
                    });
                }
            }
            y += v.height + 16.0;
            tall += v.height + 18.0;
            wide = wide.max(v.width + v.height + 24.0);
            alpha > 0.0
        });
        self.top = Some((top * 5.0 + (sh * 0.75 - tall) / 2.0) / 6.0);
        self.wide = (self.wide * 5.0 + wide) / 6.0;
    }
}

/// `NOTIFY_*` of notification.lua.
#[derive(Clone, Copy)]
enum NotifyKind {
    Error,
    Hint,
    Undo,
    Cleanup,
}

impl NotifyKind {
    fn material(self) -> &'static str {
        match self {
            Self::Error => "vgui/notices/error",
            Self::Hint => "vgui/notices/hint",
            Self::Undo => "vgui/notices/undo",
            Self::Cleanup => "vgui/notices/cleanup",
        }
    }
}

/// One `NoticePanel`: text, icon, spring state (`fx/fy`, `VelX/VelY`), start and length.
struct Notice {
    text: String,
    kind: NotifyKind,
    start: Instant,
    length: f32,
    x: f32,
    y: f32,
    vel_x: f32,
    vel_y: f32,
    w: f32,
    h: f32,
}

#[derive(Default)]
struct Notices {
    list: Vec<Notice>,
    last: Option<Instant>,
}

impl Notices {
    const START_X: f32 = 200.0;
    const CHARGE_X: f32 = 0.04;
    const GAP_X: f32 = 0.015;
    const ANCHOR_Y: f32 = 0.8;

    fn text_h(sh: f32) -> f32 {
        (9.0 * sh / 480.0).ceil().max(12.0)
    }

    /// Sandbox `LimitHit` / `OnUndo` / `OnCleanup` texts (hint.hitXlimit, hint.undoneX,
    /// hint.cleanedX) with their types and lengths.
    fn legacy(event: &str) -> Option<(String, NotifyKind, f32)> {
        let (head, rest) = event.split_once(' ')?;
        Some(match head {
            "LimitHit" => (
                format!("You have hit the \"{rest}\" limit!"),
                NotifyKind::Error,
                6.0,
            ),
            "Undone" => (format!("Undone \"{rest}\""), NotifyKind::Undo, 2.0),
            "Cleanup" => (
                format!("Cleaned up all \"{rest}\""),
                NotifyKind::Cleanup,
                5.0,
            ),
            _ => return None,
        })
    }

    fn add(
        &mut self,
        text: String,
        kind: NotifyKind,
        length: f32,
        fm: Option<&gm_render::ui::FontMetrics>,
        sw: f32,
        sh: f32,
    ) {
        let text_h = Self::text_h(sh);
        let icon_h = (text_h * 1.25).ceil();
        let (lw, lh) = fm
            .map(|m| (m.text_width(&text), m.line_height))
            .unwrap_or((text_h * 0.6 * text.len() as f32, text_h));
        // SizeToContents: label + padding, the icon docked left with its gap, DockPadding 3
        let gap = (icon_h * 0.1).ceil();
        let w = lw + (text_h * 0.75).ceil() + icon_h + gap * 2.0 + 6.0;
        let h = (lh + (text_h * 0.25).ceil()).max(icon_h + gap * 2.0) + 6.0;
        self.list.push(Notice {
            text,
            kind,
            start: Instant::now(),
            length: length.max(0.0),
            x: sw + Self::START_X,
            y: sh,
            vel_x: -5.0,
            vel_y: 0.0,
            w,
            h,
        });
    }

    fn draw(&mut self, items: &mut Vec<UiItem>, sw: f32, sh: f32, ui: &UiRenderer) {
        let now = Instant::now();
        let dt = self
            .last
            .map(|l| now.duration_since(l).as_secs_f32().min(0.1))
            .unwrap_or(0.0);
        self.last = Some(now);
        let text_h = Self::text_h(sh);
        let icon_h = (text_h * 1.25).ceil();
        let fm = ui.font_metrics(Some("GModNotify"));
        let mut total_h = 0.0;
        let spd = dt * 15.0;
        let damp = 0.95 - dt * 8.0;
        self.list.retain_mut(|n| {
            let age = now.duration_since(n.start).as_secs_f32();
            if age > n.length {
                return false;
            }
            let h = n.h + (text_h * 0.2).ceil();
            let ideal_y = sh * Self::ANCHOR_Y - h - total_h;
            let mut ideal_x = sw - n.w - sw * Self::GAP_X;
            let timeleft = n.length - age;
            if timeleft < 0.7 {
                ideal_x -= sw * Self::CHARGE_X;
            }
            if timeleft < 0.2 {
                ideal_x += n.w * 2.0;
            }
            n.y += n.vel_y * spd;
            n.x += n.vel_x * spd;
            let dist = ideal_y - n.y;
            n.vel_y += dist * spd;
            if dist.abs() < 2.0 && n.vel_y.abs() < 0.1 {
                n.vel_y = 0.0;
            }
            let dist = ideal_x - n.x;
            n.vel_x += dist * spd;
            if dist.abs() < 2.0 && n.vel_x.abs() < 0.1 {
                n.vel_x = 0.0;
            }
            n.vel_x *= damp;
            n.vel_y *= damp;
            total_h += h;
            // DPanel background, the icon docked left, the label centred in the rest
            items.push(UiItem::Rect {
                x: n.x,
                y: n.y,
                w: n.w,
                h: n.h,
                color: [20.0 / 255.0, 20.0 / 255.0, 20.0 / 255.0, 0.6],
            });
            let gap = (icon_h * 0.1).ceil();
            let inner_h = n.h - 6.0;
            items.push(UiItem::Image {
                x: n.x + 3.0 + gap,
                y: n.y + 3.0 + (inner_h - icon_h) / 2.0,
                w: icon_h,
                h: icon_h,
                material: n.kind.material().to_string(),
                color: [1.0, 1.0, 1.0, 1.0],
                rot: 0,
            });
            let label_x = n.x + 3.0 + icon_h + gap * 2.0;
            let label_w = n.x + n.w - 3.0 - label_x;
            let tx = label_x + (label_w - fm.text_width(&n.text)) / 2.0;
            let ty = n.y + 3.0 + (inner_h - fm.line_height) / 2.0;
            for (d, c) in [
                (1.0, [0.0, 0.0, 0.0, 200.0 / 255.0]),
                (0.0, [1.0, 1.0, 1.0, 1.0]),
            ] {
                items.push(UiItem::Text {
                    font: Some("GModNotify".to_string()),
                    x: tx + d,
                    y: ty + d,
                    text: n.text.clone(),
                    color: c,
                });
            }
            true
        });
    }
}

/// WeaponIcons glyph of a loadout weapon (scripts/weapon_*.txt TextureData "weapon").
fn weapon_icon(class: &str) -> Option<&'static str> {
    Some(match class {
        "weapon_crowbar" => "c",
        "weapon_physcannon" | "weapon_physgun" => "m",
        "weapon_pistol" => "d",
        "weapon_357" => "e",
        "weapon_smg1" => "a",
        "weapon_ar2" => "l",
        "weapon_shotgun" => "b",
        "weapon_crossbow" => "g",
        "weapon_frag" => "k",
        "weapon_rpg" => "i",
        _ => return None,
    })
}

/// The selection label: the weapon script's printname through the game's hl2_english /
/// garrysmod localisation, else the scripted weapon's PrintName.
fn weapon_select_name(class: &str, vm: Option<&Viewmodel>) -> String {
    match class {
        "weapon_crowbar" => "CROWBAR",
        "weapon_physcannon" => "GRAVITY GUN",
        "weapon_physgun" => "Physics Gun",
        "weapon_pistol" => "9MM PISTOL",
        "weapon_357" => ".357 MAGNUM",
        "weapon_smg1" => "SMG",
        "weapon_ar2" => "PULSE-RIFLE",
        "weapon_shotgun" => "SHOTGUN",
        "weapon_crossbow" => "CROSSBOW",
        "weapon_frag" => "GRENADE",
        "weapon_rpg" => "RPG",
        _ => return vm.map(|v| v.name.to_string()).unwrap_or_default(),
    }
    .to_string()
}
