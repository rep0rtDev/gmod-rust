//! Playable client: a window with the live world render, keyboard and mouse input, and an
//! in-process server reached through the loopback transport (the same protocol as over UDP).
//!
//! `GMOD_DIR=... cargo run --release -p gm-client -- [map] [yaw] [pitch]`
//! WASD move, mouse look, space jump, ctrl duck, shift run, LMB/RMB fire, 1/2/3 weapon
//! (none, physgun, tool gun), Q spawn a prop, Z undo, Esc release the mouse / quit,
//! ` (grave) console: `help`, `spawn [model]`, `undo`, `cleanup`, `clear`, `quit`, server
//! console lines (`status`, `cvar`, `save`, `load`, ...).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Result};
use glam::Vec3;
use gm_move::buttons;
use gm_net::msg::PlayerSnap;
use gm_net::{ClientNet, Command, Loopback, LoopbackEndpoint, NetCmd, ServerNet};
use gm_render::ui::{UiItem, UiRenderer};
use gm_render::world::{DrawItem, ModelTarget};
use gm_render::{Camera, Gpu, WorldRenderer};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

const SPAWN_MODEL: &str = "models/props_c17/oildrum001.mdl";

struct Sim {
    server: gm_server::Server<'static>,
    net: ServerNet,
    lo: Loopback,
    s_ep: LoopbackEndpoint,
    c_ep: LoopbackEndpoint,
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
        self.client.tick(cmd, &mut self.c_ep);
        self.lo.tick();
        self.net.receive(&mut self.server, &mut self.s_ep);
        self.net.tick(&mut self.server);
        self.net.send(&self.server, &mut self.s_ep);
        self.client.receive(&mut self.c_ep);
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
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    gpu: Gpu,
    world: WorldRenderer,
    depth: wgpu::TextureView,
    models: std::collections::HashMap<String, Option<gm_render::world::DynModel>>,
    player_box: gm_render::world::DynModel,
    beam: gm_render::world::DynModel,
    ui: UiRenderer,
}

const CONSOLE_LOG_LINES: usize = 500;

/// Prop folders the spawn menu lists (GMod's sandbox spawn lists cover these HL2 props).
const SPAWN_DIRS: [&str; 7] = [
    "models/props_c17",
    "models/props_junk",
    "models/props_borealis",
    "models/props_lab",
    "models/props_wasteland",
    "models/props_interior",
    "models/props_combine",
];
const SPAWN_MENU_ROWS_MAX: usize = 600;

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

struct SpawnMenu {
    scroll: usize,
}

/// Layout of the spawn menu panel for a window of `w` x `h`: (x, y, width, height, first row y).
fn menu_layout(w: f32, h: f32, lh: f32) -> (f32, f32, f32, f32, f32) {
    let (pw, ph) = ((w * 0.7).round(), (h * 0.7).round());
    let (px, py) = (((w - pw) / 2.0).round(), ((h - ph) / 2.0).round());
    (px, py, pw, ph, py + lh * 2.0)
}
/// GMod's default `viewmodel_fov`.
const VIEWMODEL_FOV: f32 = 54.0;
/// Hands drawn on the `c_` viewmodels' arm bones (GMod's default player hands).
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
const WEAPONS: [Viewmodel; 10] = [
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
    /// Rows of the spawn menu: `-- category --` headers of the addons' lists, then their models,
    /// then the content's; a header row is not spawnable.
    spawn_models: Vec<String>,
    spawn_lists_seen: usize,
    cursor: (f32, f32),
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
        let beam = world.solid_box(&gpu, [90, 200, 255, 255], true);
        let ui = UiRenderer::new(&gpu, &self.mount, format)?;
        self.gfx = Some(Gfx {
            window,
            surface,
            config,
            gpu,
            world,
            depth,
            models: Default::default(),
            player_box,
            beam,
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
        let lists = &self.sim.client.world.spawn_lists;
        let seen = lists.iter().map(|(_, m)| m.len() + 1).sum::<usize>();
        if self.spawn_models.is_empty() || seen != self.spawn_lists_seen {
            self.spawn_lists_seen = seen;
            self.spawn_models.clear();
            for (category, models) in lists {
                self.spawn_models.push(format!("-- {category} --"));
                self.spawn_models.extend(models.iter().cloned());
            }
            for dir in SPAWN_DIRS {
                self.spawn_models
                    .extend(self.mount.find(&format!("{dir}/*.mdl")));
                if self.spawn_models.len() >= SPAWN_MENU_ROWS_MAX {
                    break;
                }
            }
            self.spawn_models.truncate(SPAWN_MENU_ROWS_MAX);
        }
        self.menu = Some(SpawnMenu { scroll: 0 });
        self.mouse.clear();
        self.set_grab(false);
    }

    /// The row under the cursor of the open spawn menu, if any.
    fn menu_row(&self) -> Option<usize> {
        let (menu, g) = (self.menu.as_ref()?, self.gfx.as_ref()?);
        let (w, h) = (g.config.width as f32, g.config.height as f32);
        let lh = g.ui.line_height();
        let (px, py, pw, ph, rows_y) = menu_layout(w, h, lh);
        let (cx, cy) = self.cursor;
        if cx < px || cx > px + pw || cy < rows_y || cy > py + ph - 8.0 {
            return None;
        }
        let row = ((cy - rows_y) / lh) as usize + menu.scroll;
        (row < self.spawn_models.len()).then_some(row)
    }

    fn menu_click(&mut self) {
        if let Some(row) = self.menu_row() {
            let model = self.spawn_models[row].clone();
            if !model.ends_with(".mdl") {
                return;
            }
            self.console.log(format!("spawn {model}"));
            self.sim.client.command(Command::Spawn { model });
        }
    }

    fn select_weapon(&mut self, weapon: u8) {
        let Some(vm) = viewmodel(weapon) else { return };
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
            self.acc -= dt;
            if !self.weapon_synced && self.sim.client.world.local.is_some() {
                self.weapon_synced = true;
                self.select_weapon(self.weapon);
            }
            for e in self.sim.client.events.drain(..) {
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
        let alpha = self.acc / gm_core::tick::DEFAULT_TICK_INTERVAL;
        let vm = viewmodel(self.weapon);
        let vm_seq = vm.as_ref().map(|vm| self.vm_sequence(vm));
        let ghost_tool = (self.weapon == 2).then(|| (self.tool.clone(), self.tool_settings_list()));
        let Some(g) = &mut self.gfx else { return };
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
        let camera = Camera {
            position: o + Vec3::Z * eye,
            angles: self.view,
            fov_x: 90.0,
            near: 3.0,
            far: 28000.0,
        };
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
        let others = self.sim.client.world.players.len() > 1;
        let me = self.sim.client.player_id;
        let held_models = self
            .sim
            .client
            .world
            .players
            .iter()
            .filter(|p| Some(p.id) != me)
            .filter_map(|p| world_model(p.active));
        for model in ents
            .values()
            .map(|e| e.model.as_str())
            .chain(others.then_some(gm_server::PLAYER_MODEL))
            .chain(held_models)
            .chain(ghost_models)
        {
            if !g.models.contains_key(model) {
                let m = g.world.load_model(&g.gpu, &self.mount, model);
                g.models.insert(model.to_string(), m);
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
        type PlayerPose = (
            Vec3,
            f32,
            Option<Vec<glam::Mat4>>,
            Option<(&'static str, Vec<glam::Mat4>)>,
        );
        let mut player_poses: Vec<PlayerPose> = Vec::new();
        let mut items: Vec<_> = ents
            .values()
            .filter_map(|e| {
                let (pos, ang) = match self.sim.interp.sample(e.id, rt) {
                    Some(p) => (p.pos, p.angles),
                    None => (e.pos, e.angles),
                };
                Some(DrawItem {
                    model: g.models.get(&e.model)?.as_ref()?,
                    transform: gm_render::world::model_transform(pos, ang),
                    bones: None,
                })
            })
            .collect();
        let player_model = g
            .models
            .get(gm_server::PLAYER_MODEL)
            .and_then(|m| m.as_ref());
        for p in &self.sim.client.world.players {
            if Some(p.id) == me || !p.alive {
                continue;
            }
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
                        let path = world_model(p.active)?;
                        let wm = g.models.get(path)?.as_ref()?;
                        Some((path, wm.pose_merged("idle", 0.0, m, w)?))
                    });
                    let (origin, yaw) = match self.sim.player_interp.sample(p.id, rt) {
                        Some(s) => (s.pos, s.angles.y),
                        None => (p.origin, p.view_angles.y),
                    };
                    player_poses.push((origin, yaw, pose, held));
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
        if let Some(m) = player_model {
            for (origin, yaw, pose, held) in &player_poses {
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
        // physgun beam: from just below and right of the eye to the held prop, or along the aim
        if self.weapon == 0 && self.mouse.contains(&0) {
            let fwd = camera.forward();
            let right = fwd.cross(Vec3::Z).normalize_or_zero();
            let start = camera.position + right * 6.0 - Vec3::Z * 6.0 + fwd * 12.0;
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
                let rot = glam::Quat::from_rotation_arc(Vec3::X, d / len);
                items.push(DrawItem {
                    model: &g.beam,
                    transform: glam::Mat4::from_scale_rotation_translation(
                        Vec3::new(len, 1.0, 1.0),
                        rot,
                        start + d * 0.5,
                    ),
                    bones: None,
                });
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
            if !g.models.contains_key(HANDS_MODEL) {
                let m = g.world.load_model(&g.gpu, &self.mount, HANDS_MODEL);
                g.models.insert(HANDS_MODEL.to_string(), m);
            }
            if let Some(m) = g.models.get(path).and_then(|m| m.as_ref()) {
                let world = m.bone_world(seq, t, None, &[]);
                let pose = world.as_ref().map(|w| m.skin_matrices(w));
                let hands = g.models.get(HANDS_MODEL).and_then(|h| h.as_ref());
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
        {
            let g = self.gfx.as_ref().unwrap();
            let (w, h) = (g.config.width, g.config.height);
            self.sim.hud_draws = self.sim.script.client_render(&self.sim.client.world, w, h);
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
                viewmodel(self.weapon).map_or("none", |v| v.name)
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
        if let Some(menu) = &self.menu {
            let (px, py, pw, ph, rows_y) = menu_layout(w, h, lh);
            items.push(UiItem::Rect {
                x: px,
                y: py,
                w: pw,
                h: ph,
                color: [0.05, 0.05, 0.07, 0.9],
            });
            items.push(UiItem::Text {
                x: px + 12.0,
                y: py + 6.0,
                text: format!(
                    "Spawn menu  ({} props, wheel scrolls, click spawns)",
                    self.spawn_models.len()
                ),
                color: white,
            });
            let visible = ((py + ph - 8.0 - rows_y) / lh).max(0.0) as usize;
            let hover = self.menu_row();
            for (i, model) in self
                .spawn_models
                .iter()
                .enumerate()
                .skip(menu.scroll)
                .take(visible)
            {
                let y = rows_y + (i - menu.scroll) as f32 * lh;
                if hover == Some(i) {
                    items.push(UiItem::Rect {
                        x: px + 4.0,
                        y,
                        w: pw - 8.0,
                        h: lh,
                        color: [1.0, 1.0, 1.0, 0.15],
                    });
                }
                items.push(UiItem::Text {
                    x: px + 12.0,
                    y,
                    text: model.trim_start_matches("models/").to_string(),
                    color: [0.85, 0.85, 0.85, 1.0],
                });
            }
            return items;
        }
        if !self.console.open {
            let hud = format!(
                "props {}  weapon {}{}  pos {:.0} {:.0} {:.0}",
                self.sim.client.world.entities.len(),
                viewmodel(self.weapon).map_or("none", |v| v.name),
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
                x: 10.0,
                y: 7.0,
                text: hud,
                color: white,
            });
            // client addons' `render.hud`
            for d in &self.sim.hud_draws {
                items.push(match d {
                    // a world draw from a HUD handler: nothing on the 2D layer
                    gm_script::DrawCmd::Model { .. } => continue,
                    gm_script::DrawCmd::Rect { x, y, w, h, color } => UiItem::Rect {
                        x: *x,
                        y: *y,
                        w: *w,
                        h: *h,
                        color: *color,
                    },
                    gm_script::DrawCmd::Text { x, y, text, color } => UiItem::Text {
                        x: *x,
                        y: *y,
                        text: text.clone(),
                        color: *color,
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
                let box_h = lh * 2.0 + 16.0;
                let box_w = 150.0;
                let y = h - box_h - 20.0;
                let mut x = 20.0;
                let boxes: [(&str, i16, bool); 2] =
                    [("Health", p.health, true), ("Suit", p.armor, p.armor > 0)];
                for (label, value, shown) in boxes {
                    if !shown {
                        continue;
                    }
                    items.push(UiItem::Rect {
                        x,
                        y,
                        w: box_w,
                        h: box_h,
                        color: [0.0, 0.0, 0.0, 0.5],
                    });
                    items.push(UiItem::Text {
                        x: x + 12.0,
                        y: y + 6.0,
                        text: label.to_string(),
                        color: [0.85, 0.85, 0.85, 0.9],
                    });
                    let bar_w = box_w - 24.0;
                    let frac = (value.max(0) as f32 / 100.0).min(1.0);
                    items.push(UiItem::Rect {
                        x: x + 12.0,
                        y: y + lh + 10.0,
                        w: bar_w,
                        h: lh - 2.0,
                        color: [1.0, 1.0, 1.0, 0.12],
                    });
                    items.push(UiItem::Rect {
                        x: x + 12.0,
                        y: y + lh + 10.0,
                        w: bar_w * frac,
                        h: lh - 2.0,
                        color: [0.6, 0.6, 0.6, 0.6],
                    });
                    let text = value.to_string();
                    items.push(UiItem::Text {
                        x: x + box_w - 12.0 - g.ui.text_width(&text),
                        y: y + 6.0,
                        text,
                        color: white,
                    });
                    x += box_w + 10.0;
                }
                // ammo box at the bottom right: clip (big) and reserve, as the sandbox HUD
                if p.clip >= 0 || p.reserve > 0 {
                    let aw = 170.0;
                    let ax = w - aw - 20.0;
                    items.push(UiItem::Rect {
                        x: ax,
                        y,
                        w: aw,
                        h: box_h,
                        color: [0.0, 0.0, 0.0, 0.5],
                    });
                    items.push(UiItem::Text {
                        x: ax + 12.0,
                        y: y + 6.0,
                        text: "Ammo".to_string(),
                        color: [0.85, 0.85, 0.85, 0.9],
                    });
                    let clip = if p.clip >= 0 {
                        p.clip.to_string()
                    } else {
                        p.reserve.to_string()
                    };
                    items.push(UiItem::Text {
                        x: ax + 12.0,
                        y: y + lh + 8.0,
                        text: clip,
                        color: white,
                    });
                    if p.clip >= 0 {
                        let r = p.reserve.to_string();
                        items.push(UiItem::Text {
                            x: ax + aw - 12.0 - g.ui.text_width(&r),
                            y: y + lh + 8.0,
                            text: r,
                            color: [0.85, 0.85, 0.85, 0.9],
                        });
                    }
                    // secondary ammo (AR2 balls, SMG grenades) in its own box to the left
                    if p.alt >= 0 {
                        let bw = 70.0;
                        let bx = ax - bw - 10.0;
                        items.push(UiItem::Rect {
                            x: bx,
                            y,
                            w: bw,
                            h: box_h,
                            color: [0.0, 0.0, 0.0, 0.5],
                        });
                        items.push(UiItem::Text {
                            x: bx + 12.0,
                            y: y + 6.0,
                            text: "Alt".to_string(),
                            color: [0.85, 0.85, 0.85, 0.9],
                        });
                        items.push(UiItem::Text {
                            x: bx + 12.0,
                            y: y + lh + 8.0,
                            text: p.alt.to_string(),
                            color: white,
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
            "say" => self.sim.client.command(Command::Chat {
                text: rest.join(" "),
            }),
            "noclip" => {
                self.noclip = !self.noclip;
                self.sim.client.command(Command::Noclip { on: self.noclip });
            }
            "help" => {
                self.console
                    .log("client: spawn [model] | tool [name] | bot | say <text> | noclip | undo | cleanup | clear | quit; server:");
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
                let Some(code) = code else { return };
                if down && !event.repeat {
                    match code {
                        KeyCode::Escape if self.grabbed => self.set_grab(false),
                        KeyCode::Escape => el.exit(),
                        KeyCode::Digit1 => self.select_weapon(0),
                        KeyCode::Digit2 => self.select_weapon(1),
                        KeyCode::Digit3 => self.select_weapon(2),
                        KeyCode::Digit4 => self.select_weapon(3),
                        KeyCode::Digit5 => self.select_weapon(4),
                        KeyCode::Digit6 => self.select_weapon(5),
                        KeyCode::Digit7 => self.select_weapon(6),
                        KeyCode::Digit8 => self.select_weapon(7),
                        KeyCode::Digit9 => self.select_weapon(8),
                        KeyCode::Digit0 => self.select_weapon(9),
                        KeyCode::KeyQ => self.open_menu(),
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
                        self.set_grab(true);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(menu) = &mut self.menu {
                    let lines = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                        winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32 / 16.0,
                    };
                    menu.scroll = (menu.scroll as f32 - lines * 3.0).max(0.0) as usize;
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if self.console.open {
                    return;
                }
                if self.menu.is_some() {
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        self.menu_click();
                    }
                    return;
                }
                if !self.grabbed && state == ElementState::Pressed {
                    self.set_grab(true);
                    return;
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
                        if let Some(vm) = viewmodel(self.weapon) {
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
    let map = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "gm_construct".into());
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
    let sim = Sim {
        server,
        net,
        s_ep,
        c_ep,
        lo,
        client: ClientNet::new(1, "player"),
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
        spawn_models: Vec::new(),
        spawn_lists_seen: usize::MAX,
        cursor: (0.0, 0.0),
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
