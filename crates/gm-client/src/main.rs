//! Playable client: a window with the live world render, keyboard and mouse input, and an
//! in-process server reached through the loopback transport (the same protocol as over UDP).
//!
//! `GMOD_DIR=... cargo run --release -p gm-client -- [map] [yaw] [pitch]`
//! WASD move, mouse look, space jump, ctrl duck, shift run, LMB/RMB fire, 1/2/3 weapon
//! (none, physgun, tool gun), Q spawn a prop, Z undo, Esc release the mouse / quit,
//! ` (grave) console: `help`, `spawn [model]`, `undo`, `cleanup`, `clear`, `quit`, server
//! console lines (`status`, `cvar`, `save`, `load`, ...).

mod addons;
mod derma;
mod newgame;
mod options;
mod particles;
mod presets;
mod servers;
mod settings;
mod ugc;
mod voice;

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
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

/// `weapon_physgun` in `WEAPONS`: the weapon a sandbox spawn leaves out (`gm_server::SPAWN_WEAPON`).
const PHYSGUN_SLOT: u8 = 0;
/// Slots in `WEAPONS` of the weapons with their own effects.
/// `gmod_suit` (default 0): whether players wear the HEV suit (the AUX POWER panel).
const GMOD_SUIT: bool = false;
/// Decals never expire with time in the game; `r_decals` (2048) is a ring that drops the
/// oldest (clientfx.md §4, gap-report #128).
const DECAL_RING: usize = 2048;
const DECAL_LIFE: f32 = f32::INFINITY;
const GRAVGUN_SLOT: u8 = 11;
/// `gmod_camera` in `WEAPONS`.
const CAMERA_SLOT: u8 = 13;

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
                            material: String::new(),
                            color: [255; 4],
                            render_mode: 0,
                            render_fx: 0,
                            sub_materials: Vec::new(),
                            model_scale: 1.0,
                            bones: Vec::new(),
                            manips: Vec::new(),
                            flex: None,
                            eye_target: None,
                            sequence: String::new(),
                            cycle: 0.0,
                            playback_rate: 1.0,
                            sequence_finished: false,
                            gesture: p.gesture.clone(),
                            pose: Vec::new(),
                            mins: glam::Vec3::ZERO,
                            maxs: glam::Vec3::ZERO,
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
    /// Multisampled scene colour (`mat_antialias` > 1), resolved into the frame / scene.
    msaa_color: Option<wgpu::TextureView>,
    /// Bloom post-process (scene target + passes) when the map renders in HDR.
    post: gm_render::post::Post,
    models: std::collections::HashMap<String, Option<gm_render::world::DynModel>>,
    player_box: gm_render::world::DynModel,
    /// Physgun beam materials: `sprites/physbeam` along the beam, `physg_glow1` at its end,
    /// `physg_glow2` at the muzzle (the game's materials, additive sprites).
    beam_mat: Option<wgpu::BindGroup>,
    /// Materials of the constraint ropes seen so far (`cable/cable`, `cable/rope`, ...).
    rope_mats: std::collections::HashMap<String, Option<wgpu::BindGroup>>,
    /// When each thruster effect started (`OnStart`), for its 0.2 s scale-in.
    thruster_on: std::collections::HashMap<u32, std::time::Instant>,
    /// Live particles of the magic / rings / smoke thruster effects.
    thruster_fx: Vec<ThrusterParticle>,
    particles: particles::Particles,
    particles_tick: Option<std::time::Instant>,
    /// RPG missiles' last drawn origin: the `RocketTrail` puffs are spread over each frame's
    /// movement (clientfx.md 7.2, gap-report #132).
    missile_trails: HashMap<u32, Vec3>,
    /// Next emission time per thruster (the effects' 0.01 / 0.015 s timers).
    thruster_emit: std::collections::HashMap<u32, std::time::Instant>,
    fx_rng: u32,
    /// `sprites/physbeama`: the brighter beam while something is held.
    beam_active_mat: Option<wgpu::BindGroup>,
    glow1: Option<wgpu::BindGroup>,
    glow2: Option<wgpu::BindGroup>,
    /// Gravity gun: `sprites/orangecore1` at the muzzle, `glow04_noz` on the fork tips,
    /// `orangelight1` beams from the tips to the held prop (the game's physcannon sprites).
    gg_core: Option<wgpu::BindGroup>,
    gg_glow: Option<wgpu::BindGroup>,
    /// `orangecore2_gmod` (launch blast), `orangeflare1_gmod` (fork end caps while holding),
    /// `orangelight1_noz` (first-person hold beams).
    gg_core2: Option<wgpu::BindGroup>,
    gg_endcap: Option<wgpu::BindGroup>,
    gg_beam_noz: Option<wgpu::BindGroup>,
    gg_beam: Option<wgpu::BindGroup>,
    /// Shot effects: `effects/muzzleflash1` (muzzle flash, impact and explosion flashes),
    /// `effects/spark` (tracer streak), `decals/<group>/shot1..5` (bullet holes, by the
    /// surface's game material, loaded on first use).
    fx_muzzle: Option<wgpu::BindGroup>,
    /// `C_BaseExplosionEffect` sprites: the smoke puffs (`particle/particle_smokegrenade`),
    /// the embers (`effects/fire_embers1`/`2`), the fire clouds (`effects/fire_cloud2`) and the
    /// cement flecks (`effects/fleck_cement1`/`2`).
    fx_explo_smoke: Option<wgpu::BindGroup>,
    fx_explo_embers: Vec<wgpu::BindGroup>,
    fx_explo_fire: Option<wgpu::BindGroup>,
    fx_explo_flecks: Vec<wgpu::BindGroup>,
    /// Impact effects (clientfx.md 3): `fleck_wood1/2`, `particle_noisesphere` dust,
    /// `yellowflare` spark flare; `muzzleflash1..4` for the 8-sprite muzzle flash (2.1).
    fx_wood_flecks: Vec<wgpu::BindGroup>,
    fx_noise: Option<wgpu::BindGroup>,
    fx_flare: Option<wgpu::BindGroup>,
    fx_muzzles: Vec<wgpu::BindGroup>,
    /// Tool gun click effects (toolgun.md F30-F32): `effects/tool_tracer`, `select_dot`,
    /// `select_ring`.
    fx_tool_tracer: Option<wgpu::BindGroup>,
    fx_select_dot: Option<wgpu::BindGroup>,
    fx_select_ring: Option<wgpu::BindGroup>,
    /// `sprites/lgtning`: the combine ball explosion's rings.
    fx_lgtning: Option<wgpu::BindGroup>,
    fx_spark: Option<wgpu::BindGroup>,
    /// `AR2Tracer` (`effects/gunshiptracer`, width 5) and the `AR2Impact` flash
    /// (`effects/combinemuzzle2`) of the pulse rifle (hl2weapons.md, gap-report #103).
    fx_ar2_tracer: Option<wgpu::BindGroup>,
    fx_ar2_impact: Option<wgpu::BindGroup>,
    /// Combine ball motion-blur (`effects/ar2_altfire1b`) and flicker (`effects/combinemuzzle1`)
    /// halos (clientfx.md 8.2, gap-report #134), and each ball's last origin for the blur.
    fx_cball_blur: Option<wgpu::BindGroup>,
    fx_cball_flicker: Option<wgpu::BindGroup>,
    ball_last: HashMap<u32, Vec3>,
    /// The crossbow's charger sprite `sprites/blueflare1` (hl2weapons.md, gap-report #105).
    fx_blueflare: Option<wgpu::BindGroup>,
    fx_decal_mats: std::collections::HashMap<String, Option<gm_render::world::Decal>>,
    /// `sprites/redglow1`: the RPG's laser dot.
    fx_laserdot: Option<wgpu::BindGroup>,
    /// `effects/laser1_noz`: the RPG's 128 u laser beam out of the muzzle (gap-report #110).
    fx_laserbeam: Option<wgpu::BindGroup>,
    /// `sprites/sent_ball`: the bouncy ball's look (sent_ball.lua `Draw`, a sprite of its size).
    fx_ball: Option<wgpu::BindGroup>,
    /// `env_sprite` materials by name with their texel width.
    fx_sprite_mats: std::collections::HashMap<String, Option<(wgpu::BindGroup, f32, bool)>>,
    /// The `env_sun` glow sprite (`sprites/light_glow02_add_noz` unless the map names another).
    fx_sun: Option<wgpu::BindGroup>,
    /// `effects/splash2`: the water splash.
    fx_splash: Option<wgpu::BindGroup>,
    ui: UiRenderer,
}

const CONSOLE_LOG_LINES: usize = 500;

/// World model held by another player, by `gm_server::WIRE_WEAPONS` index (`PlayerSnap::active`);
/// it shares `ValveBiped.Bip01_R_Hand` with the player skeleton and is bone-merged onto it.
fn world_model(active: u8) -> Option<&'static str> {
    Some(match gm_server::WIRE_WEAPONS.get(active as usize)? {
        &"gmod_tool" => "models/weapons/w_toolgun.mdl",
        &"weapon_357" => "models/weapons/w_357.mdl",
        &"weapon_ar2" => "models/weapons/w_irifle.mdl",
        &"weapon_crossbow" => "models/weapons/w_crossbow.mdl",
        &"weapon_crowbar" => "models/weapons/w_crowbar.mdl",
        &"weapon_stunstick" => "models/weapons/w_stunstick.mdl",
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
    /// Options dialog slider bars `(cvar, x0, x1)` of the last frame.
    opt_bars: options::SliderBars,
    /// New Game icon grid max scroll of the last frame.
    ng_max_scroll: f32,
    ugc_per_page: usize,
    /// Icons per page of the spawn menu's Dupes / Saves tab.
    qm_per_page: usize,
    /// The hovered spawn icon's tooltip and when the hover began.
    tip: Option<(String, std::time::Instant)>,
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
    /// The entity the properties are for and the open submenu path.
    ent: u32,
    open: Vec<usize>,
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
/// The fonts of the spawn menu's Dupes / Saves tab (the Saves page's), owned so a
/// `ugc::Fonts` can borrow them next to the derma context.
struct UgcFontSet {
    f11: gm_render::ui::FontMetrics,
    f13: gm_render::ui::FontMetrics,
    f14b: gm_render::ui::FontMetrics,
    f32: gm_render::ui::FontMetrics,
    f48b: gm_render::ui::FontMetrics,
    opt: gm_render::ui::FontMetrics,
    opt_font: &'static str,
}

impl UgcFontSet {
    fn fonts(&self) -> ugc::Fonts<'_> {
        ugc::Fonts {
            f11: &self.f11,
            f13: &self.f13,
            f14b: &self.f14b,
            f32: &self.f32,
            f48b: &self.f48b,
            opt: &self.opt,
            opt_font: self.opt_font,
        }
    }
}

fn ugc_font_set(g: &Gfx) -> UgcFontSet {
    let f = |n: &str| g.ui.font_metrics(Some(n));
    let opt_font = if g.config.width as f32 > 1280.0 {
        derma::MENU_FONT_23
    } else {
        derma::MENU_FONT_16
    };
    UgcFontSet {
        f11: f(newgame::F11),
        f13: f(newgame::F13),
        f14b: f(ugc::F14B),
        f32: f(newgame::F32),
        f48b: f(ugc::F48B),
        opt: f(opt_font),
        opt_font,
    }
}

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
/// World-space bone matrices of a prop_ragdoll from its networked physics objects: every bone
/// follows the object of its `physics_bone` (MDL `TranslateBoneToPhysBone`) by its offset from
/// that object's own bone in the model's `ragdoll` pose (bind pose without one); bones no object
/// drives ride the entity. `ManipulateBoneAngles` / `ManipulateBoneScale` apply in bone space.
fn ragdoll_bone_matrices(
    m: &gm_render::world::DynModel,
    parts: &[gm_net::msg::BonePose],
    pos: Vec3,
    angles: Vec3,
    manips: &[gm_net::msg::BoneManip],
) -> Option<Vec<glam::Mat4>> {
    let bones = m.skeleton_bones()?;
    let pose_local: Vec<glam::Mat4> = match m.bone_world("ragdoll", 0.0, None, &[]) {
        Some(w) if w.len() == bones.len() => w,
        _ => {
            let mut w = Vec::with_capacity(bones.len());
            for (i, b) in bones.iter().enumerate() {
                let local = glam::Mat4::from_rotation_translation(
                    glam::Quat::from_array(b.quat),
                    Vec3::from(b.pos),
                );
                let parent = usize::try_from(b.parent)
                    .ok()
                    .filter(|k| *k < i)
                    .map_or(glam::Mat4::IDENTITY, |k| w[k]);
                w.push(parent * local);
            }
            w
        }
    };
    // the bone an object is named after: the first (parent-most) bone it drives
    let mut solid_bone: Vec<Option<usize>> = vec![None; parts.len()];
    for (i, b) in bones.iter().enumerate() {
        if let Some(slot) = usize::try_from(b.physics_bone)
            .ok()
            .and_then(|k| solid_bone.get_mut(k))
        {
            if slot.is_none() {
                *slot = Some(i);
            }
        }
    }
    let entity = gm_render::world::model_transform(pos, angles);
    let mut world: Vec<glam::Mat4> = bones
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let driven = usize::try_from(b.physics_bone)
                .ok()
                .and_then(|k| Some((parts.get(k)?, (*solid_bone.get(k)?)?)));
            match driven {
                Some((part, sb)) => {
                    gm_render::world::model_transform(part.pos, part.angles)
                        * pose_local[sb].inverse()
                        * pose_local[i]
                }
                None => entity * pose_local[i],
            }
        })
        .collect();
    for mp in manips {
        if let Some(w) = world.get_mut(mp.bone as usize) {
            *w = *w
                * gm_render::world::model_transform(mp.pos, mp.angles)
                * glam::Mat4::from_scale(mp.scale);
        }
    }
    Some(world)
}

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
    /// The FOV the viewmodel is drawn with: `viewmodel_fov` 54 for HL2 weapons,
    /// `SWEP.ViewModelFOV` (62) for scripted ones (render.md §2.1, gap-report #144).
    fov: f32,
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
        fov: VIEWMODEL_FOV,
    }
}

/// Slots on the number keys 1..9, 0; the physgun is slot 0 (key 1).
/// `GRAVGUN_SLOT` / `CAMERA_SLOT` index this table (`slot_constants_match_the_table`).
const WEAPONS: [Viewmodel; 14] = [
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
    vm(
        "weapon_stunstick",
        "stunstick",
        "models/weapons/c_stunstick.mdl",
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
        // the pulse rifle's reload is `IR_reload` on c_irifle (the stock `vm()` has none)
        reload: Some("IR_reload"),
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
/// Lifetime of a world burst sprite: explosions (over 32 u) hold about as long as the game's
/// 20-frame fireball, small flashes and splashes a third of a second.
/// `ScaleFOVByWidthRatio`: a Source fov (fov_desired, a weapon's zoom, the viewmodel's) is
/// the horizontal fov of a 4:3 screen; a wider screen widens it, keeping the vertical fov.
fn scale_fov_by_width(fov: f32, aspect: f32) -> f32 {
    2.0 * ((fov.to_radians() * 0.5).tan() * aspect * 0.75)
        .atan()
        .to_degrees()
}

fn burst_life(size: f32) -> f32 {
    if size > 32.0 {
        0.6
    } else {
        0.35
    }
}

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
    match gm_server::WIRE_WEAPONS
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

/// `AngleVectors`: forward, right, up of (pitch, yaw, roll) in degrees.
fn angle_vectors(angles: Vec3) -> (Vec3, Vec3, Vec3) {
    let m = gm_render::props::angle_matrix(angles.to_array());
    (m * Vec3::X, -(m * Vec3::Y), m * Vec3::Z)
}

/// The viewmodel's bob and sway state: `CBaseHLCombatWeapon::CalcViewmodelBob`'s bob time
/// (advances with the player's speed) and `CBaseViewModel::CalcViewModelLag`'s last facing.
struct VmView {
    bob_time: f32,
    at: Instant,
    last_facing: Vec3,
}

impl VmView {
    fn new() -> Self {
        Self {
            bob_time: 0.0,
            at: Instant::now(),
            last_facing: Vec3::X,
        }
    }

    /// `CBaseViewModel::CalcViewModelView`: the viewmodel's origin and angles from the eye
    /// position and view angles - the HL2 weapon bob (`AddViewmodelBob`), then the sway behind
    /// the facing and the pitch offset (`CalcViewModelLag`, `cl_wpn_sway_interp` 0.1,
    /// `cl_wpn_sway_scale` 1). `speed` is the player's 2D speed.
    fn view(&mut self, speed: f32, eye: Vec3, angles: Vec3) -> (Vec3, Vec3) {
        use std::f32::consts::PI;
        let now = Instant::now();
        let dt = now.duration_since(self.at).as_secs_f32().min(0.1);
        self.at = now;
        let (mut origin, mut ang) = (eye, angles);
        // HL2_BOB_CYCLE_MAX 0.45, HL2_BOB_UP 0.5: the vertical cycle, the lateral one twice as long
        let speed = speed.clamp(0.0, 320.0);
        self.bob_time += dt * speed / 320.0;
        let bob_time = self.bob_time;
        let cycle = |period: f32| {
            let c = (bob_time - (bob_time / period).trunc() * period) / period;
            if c < 0.5 {
                PI * c / 0.5
            } else {
                PI + PI * (c - 0.5) / 0.5
            }
        };
        let bob = |c: f32| {
            let b = speed * 0.005;
            (b * 0.3 + b * 0.7 * c.sin()).clamp(-7.0, 4.0)
        };
        let vertical = bob(cycle(0.45));
        let lateral = bob(cycle(0.9));
        let (fwd, right, _) = angle_vectors(ang);
        origin += fwd * (vertical * 0.1);
        origin.z += vertical * 0.1;
        ang.z += vertical * 0.5;
        ang.x -= vertical * 0.4;
        ang.y -= lateral * 0.3;
        origin += right * (lateral * 0.8);
        // the lag: the facing catches up at 5/s (faster once more than a unit behind)
        let (facing, _, _) = angle_vectors(ang);
        if dt > 0.0 {
            let diff = facing - self.last_facing;
            let d = diff.length();
            let rate = if d > 1.0 { 5.0 * d } else { 5.0 };
            self.last_facing = (self.last_facing + diff * (rate * dt)).normalize_or_zero();
            origin -= diff * 5.0;
        }
        let (fwd, right, up) = angle_vectors(angles);
        let mut pitch = angles.x;
        if pitch > 180.0 {
            pitch -= 360.0;
        } else if pitch < -180.0 {
            pitch += 360.0;
        }
        origin += fwd * (-pitch * 0.035) + right * (-pitch * 0.03) + up * (-pitch * 0.02);
        (origin, ang)
    }
}

/// The m_anm gesture another player plays for a viewmodel event (`PlayerSnap::vm_act`), by hold
/// Interns a model-resolved sequence name so it can sit in the `&'static str` viewmodel
/// animation slot (a handful of names per process).
fn static_seq_name(name: &str) -> &'static str {
    static NAMES: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut names = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(n) = names.iter().find(|n| **n == name) {
        return n;
    }
    let n: &'static str = Box::leak(name.to_owned().into_boxed_str());
    names.push(n);
    n
}

/// The sandbox stools whose click repeats every tick while held (the game's
/// `TOOL.LeftClickAutomatic` / `RightClickAutomatic`).
const AUTOMATIC_STOOLS: &[&str] = &["leafblower", "inflator", "paint"];

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

/// Toolgun screen cache: the `screen_bg` pixels, the font and the rasterized name of the
/// current tool (mode, width, height, coverage, baseline).
#[derive(Default)]
struct ToolScreen {
    bg: Option<gm_render::material::Rgba>,
    font: Option<Vec<u8>>,
    text: Option<(String, u32, u32, Vec<u8>, f32)>,
}

/// The toolgun's screen as the game's `cl_viewscreen.lua` paints its render target: the
/// `screen_bg` texture with `#tool.<mode>.name` in a 60 px heavy font (Helvetica there;
/// the first of Helvetica / Arial / Roboto-Bold here) scrolling left at 250 px/s, wrapping
/// every width + 64 px, a 3 px black shadow under white, centred on y = 104.
fn paint_tool_screen(
    ts: &mut ToolScreen,
    mount: &gm_content::Mount,
    tool: &str,
    lang: Option<&std::collections::BTreeMap<String, String>>,
    elapsed: f32,
    g: &Gfx,
) {
    let Some(Some(model)) = g.models.get(WEAPONS[1].path) else {
        return;
    };
    if ts.bg.is_none() {
        ts.bg = gm_render::material::load_texture(mount, "models/weapons/v_toolgun/screen_bg", 256)
            .ok()
            .filter(|t| t.pixels.len() == (t.width * t.height * 4) as usize);
    }
    let Some(bg) = &ts.bg else {
        return;
    };
    if ts.font.is_none() {
        ts.font = ["Helvetica", "Arial", "Roboto-Bold", "Verdana"]
            .iter()
            .find_map(|f| gm_render::ui::font_file(mount, f));
    }
    let Some(font) = &ts.font else {
        return;
    };
    if ts.text.as_ref().is_none_or(|t| t.0 != tool) {
        let key = format!("tool.{}.name", tool);
        let name = lang
            .and_then(|l| l.get(&key).cloned())
            .unwrap_or_else(|| tool.to_string());
        let px = 60.0 * bg.height as f32 / 256.0;
        ts.text = gm_render::world::WorldRenderer::raster_text(font, px, &name)
            .map(|(w, h, cov, base)| (tool.to_string(), w, h, cov, base));
    }
    let Some((_, tw, th, cov, _)) = &ts.text else {
        return;
    };
    let (w, h) = (bg.width, bg.height);
    let scale = w as f32 / 256.0;
    let mut px = bg.pixels.clone();
    let period = *tw as f32 + 64.0 * scale;
    let mut x = -(elapsed * 250.0 * scale) % period;
    let y = (104.0 * scale - *th as f32 / 2.0).round() as i32;
    let blit = |px: &mut Vec<u8>, ox: i32, oy: i32, rgb: [u8; 3]| {
        for ty in 0..*th {
            let dy = oy + ty as i32;
            if dy < 0 || dy >= h as i32 {
                continue;
            }
            for tx in 0..*tw {
                let dx = ox + tx as i32;
                if dx < 0 || dx >= w as i32 {
                    continue;
                }
                let a = cov[(ty * *tw + tx) as usize] as u32;
                if a == 0 {
                    continue;
                }
                let i = ((dy as u32 * w + dx as u32) * 4) as usize;
                for c in 0..3 {
                    let d = px[i + c] as u32;
                    px[i + c] = ((rgb[c] as u32 * a + d * (255 - a)) / 255) as u8;
                }
            }
        }
    };
    while x < w as f32 {
        let ox = x.round() as i32;
        let sh = (3.0 * scale).round() as i32;
        blit(&mut px, ox + sh, y + sh, [0, 0, 0]);
        blit(&mut px, ox, y, [255, 255, 255]);
        x += period;
    }
    model.write_part_texture(&g.gpu, "v_toolgun/screen", &px, w, h);
}

/// Loadout weapons whose shot shows a muzzle flash (the game's `MuzzleFlash` effect).
fn fires_bullets(active: u8) -> bool {
    matches!(
        gm_server::WIRE_WEAPONS.get(active as usize),
        Some(&("weapon_pistol" | "weapon_357" | "weapon_smg1" | "weapon_ar2" | "weapon_shotgun"))
    )
}

/// Soundscript entry for a server viewmodel event (`VmAct`) on a loadout weapon, as the
/// game's weapons emit them: `.Single` on a shot (`.Launch` for the gravity gun), `.Reload`
/// when a one-go reload starts, the shotgun's `.Reload` per shell and `.Special1` (cock) at
/// its end and on the pump, `.Empty` on a dry fire. The shotgun's start stage is silent.
fn weapon_sound(act: u8, active: u8) -> Option<String> {
    let class = gm_server::WIRE_WEAPONS.get(active as usize)?;
    let prefix = match *class {
        "weapon_357" => "Weapon_357",
        "weapon_ar2" => "Weapon_AR2",
        "weapon_crossbow" => "Weapon_Crossbow",
        "weapon_crowbar" => "Weapon_Crowbar",
        "weapon_stunstick" => "Weapon_StunStick",
        "weapon_physcannon" => "Weapon_PhysCannon",
        "weapon_pistol" => "Weapon_Pistol",
        "weapon_rpg" => "Weapon_RPG",
        "weapon_shotgun" => "Weapon_Shotgun",
        "weapon_smg1" => "Weapon_SMG1",
        "gmod_tool" => "Toolgun",
        _ => return None,
    };
    let suffix = match (act, prefix) {
        // the server plays the physcannon's Launch / DryFire / Pickup / Drop itself
        (_, "Weapon_PhysCannon") => return None,
        (_, "Toolgun") if act != 6 => return None,
        (6, _) => "Single",
        (1, "Weapon_Shotgun") => return None,
        (1, _) => "Reload",
        (4, _) => "Reload",
        (2, _) | (5, _) => "Special1",
        // the 357 / AR2 / RPG scripts borrow their empty click (hl2weapons.md, #99)
        // secondary fires: SMG1 grenade `Double`, AR2 ball `Weapon_IRifle.Single` (#102, #104)
        (8, "Weapon_SMG1") => "Double",
        (9, "Weapon_AR2") => return Some("Weapon_CombineGuard.Special1".into()),
        // the stunstick's deploy is `Activate` (hl2weapons.md #113), the RPG's `LaserOn` (#110)
        (7, "Weapon_StunStick") => "Activate",
        (7, "Weapon_RPG") => "LaserOn",
        (9, _) | (10, _) | (11, _) | (12, _) | (13, _) => return None,
        (8, "Weapon_AR2") => return Some("Weapon_IRifle.Single".into()),
        (3, "Weapon_357") => return Some("Weapon_Pistol.Empty".into()),
        (3, "Weapon_AR2") => return Some("Weapon_IRifle.Empty".into()),
        (3, "Weapon_RPG") => return Some("Weapon_SMG1.Empty".into()),
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

/// A prop hull for the acoustic occluders (`None`: the model has no usable `.phy`).
type OccluderHull = Option<(std::sync::Arc<str>, std::sync::Arc<gm_audio::Geometry>)>;

/// `menu/background.lua`: out of game a random image of `backgrounds/` fills the screen
/// behind the main menu, zooming by 0.3 / 30 per second, replaced after 30 s while the old one
/// fades out over a second. The game also turns the image 5 / 30 deg/s; image items only
/// turn by quarters, so that is left out.
#[derive(Default)]
struct Backgrounds {
    images: Option<Vec<String>>,
    active: Option<Slide>,
    outgoing: Option<Slide>,
    seed: u64,
}

struct Slide {
    name: String,
    born: Instant,
    fading_since: Option<Instant>,
}

impl Backgrounds {
    /// Advances the slideshow; the image to load when a new one was chosen.
    fn tick(&mut self, mount: &gm_content::Mount) -> Option<String> {
        let images = self.images.get_or_insert_with(|| {
            let mut v: Vec<String> = mount
                .find("backgrounds/*.*")
                .into_iter()
                .filter(|p| {
                    let l = p.to_ascii_lowercase();
                    l.ends_with(".jpg") || l.ends_with(".jpeg") || l.ends_with(".png")
                })
                .collect();
            v.sort();
            v
        });
        if images.is_empty() {
            return None;
        }
        if self.outgoing.as_ref().is_some_and(|o| {
            o.fading_since
                .is_some_and(|t| t.elapsed().as_secs_f32() >= 1.0)
        }) {
            self.outgoing = None;
        }
        let due = self
            .active
            .as_ref()
            .is_none_or(|a| a.born.elapsed().as_secs_f32() >= 30.0);
        if !due {
            return None;
        }
        if self.seed == 0 {
            self.seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0x9E37_79B9_7F4A_7C15, |d| d.as_nanos() as u64)
                | 1;
        }
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        let mut i = (self.seed % images.len() as u64) as usize;
        if images.len() > 1 && self.active.as_ref().is_some_and(|a| a.name == images[i]) {
            i = (i + 1) % images.len();
        }
        let name = images[i].clone();
        self.outgoing = self.active.take().map(|mut a| {
            a.fading_since = Some(Instant::now());
            a
        });
        self.active = Some(Slide {
            name: name.clone(),
            born: Instant::now(),
            fading_since: None,
        });
        Some(name)
    }
    /// The slides over a black screen: `Size` starts at 1 (more when the image is narrower
    /// than the screen so it still covers it) and grows as `Size += SizeVel / Size` per
    /// second, drawn centred at `ScrH() * Size` tall.
    fn items(
        &self,
        items: &mut Vec<UiItem>,
        w: f32,
        h: f32,
        size_of: impl Fn(&str) -> Option<(u32, u32)>,
    ) {
        items.push(UiItem::Rect {
            x: 0.0,
            y: 0.0,
            w,
            h,
            color: [0.0, 0.0, 0.0, 1.0],
        });
        for slide in self.outgoing.iter().chain(self.active.iter()) {
            let Some((iw, ih)) = size_of(&slide.name) else {
                continue;
            };
            let ratio = iw as f32 / ih.max(1) as f32;
            let size0 = 1.0 + (w / h - ratio).max(0.0);
            let t = slide.born.elapsed().as_secs_f32();
            let size = (size0 * size0 + 2.0 * (0.3 / 30.0) * t).sqrt();
            let sh = h * size;
            let sw = sh * ratio;
            let alpha = slide
                .fading_since
                .map_or(1.0, |f| (1.0 - f.elapsed().as_secs_f32()).clamp(0.0, 1.0));
            items.push(UiItem::Image {
                x: (w - sw) / 2.0,
                y: (h - sh) / 2.0,
                w: sw,
                h: sh,
                material: slide.name.clone(),
                color: [1.0, 1.0, 1.0, alpha],
                rot: 0,
            });
        }
    }
}

struct App {
    /// HL2 weapon selection (`hud_fastswitch 0`): the highlighted weapon as an index into the
    /// server's class list while the bucket menu is open.
    wsel: Option<u8>,
    /// When the bucket menu last moved (`m_flSelectionTime`): it fades after 0.5 s idle.
    wsel_at: Option<Instant>,
    /// HUD_PRINTCENTER text (CHudMessage) and when it arrived.
    center_print: Option<(String, Instant)>,
    /// Lua `DynamicLight`s by key: the light, its `Decay` (units/s), when it dies, when it
    /// was last sent (the decay runs from there), and `nomodel`/`noworld`.
    lua_dlights: HashMap<i32, (gm_render::world::DLight, f32, Instant, Instant, bool, bool)>,
    /// The Utilities tab's values by convar: the server's replicated ones as last seen,
    /// the client-side ones as set here.
    utility_values: BTreeMap<String, String>,
    /// The spawnmenu's state while it is closed (restored on the next `+menu`).
    saved_menu: Option<derma::MenuState>,
    /// CHudSuitPower: when the last active device went off (SuitAuxPowerMax 0.4 s fade).
    aux_off_at: std::cell::RefCell<Option<Instant>>,
    /// CHudZoom: when the FOV dropped below fov_desired / came back (0.4 s fades).
    zoom_at: std::cell::RefCell<(Option<Instant>, Option<Instant>)>,
    /// When the bucket menu was opened: OpenWeaponSelectionMenu fades Alpha in over 0.1 s.
    wsel_opened: Option<Instant>,
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
    /// HudAnimations.txt state of the health and suit panels
    hud_health_anim: std::cell::RefCell<HudPanelAnim>,
    hud_suit_anim: std::cell::RefCell<HudPanelAnim>,
    hud_ammo_anim: std::cell::RefCell<HudAmmoAnim>,
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
    /// `impulse N` fired by a bind, sent with the next command.
    impulse: u8,
    /// Mouse counts since the last command while rotating a held prop (physgun + E): the view
    /// stays locked (`DIROVERRIDE`) and the counts go to the server as `mouse_x` / `mouse_y`.
    rotate_delta: (f32, f32),
    /// Wheel lines since the last command while holding with the physgun (hold distance).
    wheel_delta: f32,
    /// One-shot viewmodel sequence playing (draw, fire) and when it started.
    vm_anim: Option<(&'static str, Instant)>,
    /// Last frame's viewmodel (sequence, seconds into it) and the sequence left before it with
    /// the time it was left at and when (`CSequenceTransitioner` fade-out).
    vm_last: Option<(&'static str, f32)>,
    /// The viewmodel sequence and time its animation events were last run up to.
    vm_event_t: Option<(&'static str, f32)>,
    /// A 6001 event this frame: the brass kind to eject from attachment 2.
    fx_brass_pending: Option<u8>,
    /// Ejected shells (`CTempEnts::EjectBrass`): a second of flight, bouncing off the world.
    fx_shells: Vec<Shell>,
    fx_shells_at: Instant,
    /// The viewmodel's bob and sway state (`CalcViewModelView`).
    vm_view: VmView,
    vm_prev: Option<(&'static str, f32, Instant)>,
    /// `PlayerSnap::vm_serial` of the last server viewmodel event played.
    vm_serial_seen: u8,
    /// Tracers in flight: shooter, end, birth (a 128 u streak at 5000 u/s from the muzzle).
    fx_tracers: Vec<(u32, Vec3, Instant, bool)>,
    /// `AR2Impact` flashes: position and birth, 0.1 s.
    fx_ar2_impacts: Vec<(Vec3, Instant)>,
    /// When the crossbow's loaded bolt started charging (clip went to 1 with the crossbow out):
    /// START_CHARGE 0.5 s then READY over 1 s (hl2weapons.md charger states, #105).
    xbow_charge_since: Option<Instant>,
    /// Explosion dynamic lights (GMod's, clientfx.md §1 / gap-report #117): radius 350 u,
    /// colour (255, 145, 30), gone after 0.3 s with the radius shrinking at 1166.67 u/s.
    fx_explosion_lights: Vec<(Vec3, Instant)>,
    /// Bullet decals: position, normal, metal, birth (the newest 256).
    fx_decals: std::collections::VecDeque<(Vec3, Vec3, String, Instant)>,
    /// Bullet decals on props: entity, position and normal in its frame, decal material.
    fx_prop_decals: std::collections::VecDeque<(u32, Vec3, Vec3, String, std::time::Instant)>,
    /// Flashes: position, size, birth (impact sparks, explosions).
    fx_bursts: Vec<(Vec3, f32, Instant, bool)>,
    /// `PhyscannonImpact` effects: the punting player, the punt point and when.
    fx_cannon: Vec<(u32, Vec3, Instant)>,
    /// Combine ball explosions: where and when (three growing `lgtning` rings).
    fx_rings: Vec<(Vec3, Instant)>,
    /// Tool gun click effects: player, hit, normal, when, the six rings' speeds (0.5..1.5).
    fx_tool: Vec<(u32, Vec3, Vec3, Instant, [f32; 6])>,
    /// `gmod_camera`: its `Zoom` (fov, degrees) and `Roll`, and the last mouse step time.
    cam_zoom: f32,
    cam_roll: f32,
    cam_at: Instant,
    /// Gravity gun claw pose parameter (`active`: 0 closed .. 1 open), when it was last stepped,
    /// and the core sprite's current opacity (gravgun.md 7.3 interpolations).
    claw_pose: f32,
    claw_at: Instant,
    gg_core_alpha: f32,
    /// The gravity gun's networked effect state as the client sees it (gravgun.md 3.1):
    /// 0 NONE, 1 CLOSED, 2 READY, 3 HOLDING.
    gg_effect: u8,
    /// The toolgun's predicted click (sound + fire animation played locally; the server's
    /// matching fire event within the window is skipped).
    tool_predicted: Option<Instant>,
    /// `gmod_camera`: last photo time (0.4 s rate limit) and a pending frame capture.
    cam_photo_at: Option<Instant>,
    cam_photo_pending: bool,
    /// When the room around the listener was last measured for `dsp_room`.
    room_at: Instant,
    /// Explosion particles (`C_BaseExplosionEffect`), simulated with the shells.
    fx_sprites: Vec<FxSprite>,
    fx_seed: u32,
    /// Sounds due later (the crossbow's `BoltElectrify` as the reload seats the bolt).
    fx_sound_queue: Vec<(Instant, String)>,
    /// Own muzzle flash shows until then.
    fx_muzzle_until: Option<Instant>,
    /// The own muzzle flash's 8 sprites (position, half-size, roll, material, born), drawn in
    /// the viewmodel pass; `fx_muzzle_spawned` is the `fx_muzzle_until` they were made for.
    fx_vm_muzzle: Vec<(Vec3, f32, f32, usize, Instant, f32)>,
    /// The muzzle event's entity light (clientfx.md 2.1.4, #122): radius randint(32, 64), born.
    fx_muzzle_light: Option<(f32, Instant)>,
    /// Other players' weapon world-model `muzzle` attachment in world space from the last
    /// drawn pose (clientfx.md 2.1, gap-report #123).
    other_muzzles: HashMap<u32, Vec3>,
    fx_muzzle_spawned: Option<Instant>,
    /// Surface property -> game material letter (`scripts/surfaceproperties*.txt`).
    gamematerials: HashMap<String, char>,
    tool_screen: ToolScreen,
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
    voice: voice::VoiceChat,
    soundscapes: gm_audio::Soundscapes,
    /// Prop collision hulls for the acoustic occluders, by model path (`None`: no `.phy`).
    occluder_hulls: std::collections::HashMap<String, OccluderHull>,
    last_render: Instant,
    /// Spawn menu while Q is held: the prop models to pick from and the scroll offset in rows.
    menu: Option<SpawnMenu>,
    spawnlists: Vec<derma::SpawnList>,
    shift_held: bool,
    /// `tool.properties`: tool and setting labels.
    lang: BTreeMap<String, String>,
    /// From the last laid-out menu frame: visible spawn icons, the ControlPanel rect, max scroll.
    menu_cache: std::cell::RefCell<MenuCache>,
    /// C held: the cursor is free and a right-click asks a prop for its properties.
    context_held: bool,
    /// The context menu's Player Model window (`editor_player.lua`) is open.
    player_editor: bool,
    /// `+showscores` held (TAB): the sandbox scoreboard.
    scoreboard: bool,
    /// When each remote player's last voice packet arrived (the voice panels).
    voice_seen: HashMap<u32, Instant>,
    /// When the local player died (first dead snapshot) and the view angles at that moment:
    /// the death camera keeps them while it pulls back.
    death_at: Option<Instant>,
    death_view: Vec3,
    /// CHudDamageIndicator (ui.md §9, gap-report #162-#163): when the left / right bar
    /// animation started and whether it is the "High" (> 25 damage) variant.
    dmg_side: [Option<(Instant, bool)>; 2],
    /// HudTakeDamageHighBehind: the full-screen flash start.
    dmg_full: Option<Instant>,
    context: Option<ContextMenu>,
    /// The editor entity whose "Edit Properties" sheet is open in the context menu.
    edit_sheet: Option<u32>,
    /// Scripted weapons as last seen (`ClientWorld::weapon_defs`) and their viewmodel / world
    /// model entries after the stock table (leaked once per change: the table is `'static`).
    scripted_defs: Vec<gm_server::ScriptedWeapon>,
    scripted_vms: Vec<&'static Viewmodel>,
    scripted_world: Vec<&'static str>,
    /// Key / mouse button edges since the last frame, for the client actors (`input.key`).
    pending_input: Vec<(String, bool)>,
    spawn_lists_seen: usize,
    /// Weapons / Entities tab categories built from the server's content lists.
    content: [Vec<derma::ContentList>; 5],
    /// Post Process entries whose `pp_*` convar is on (the tab's checkboxes).
    pp_enabled: std::collections::BTreeSet<String>,
    content_seen: usize,
    content_lang: Option<std::collections::BTreeMap<String, String>>,
    /// Spawn menu search: every mounted model (lazily listed) and the current results.
    search_models: Option<Vec<String>>,
    search_results: derma::SearchResults,
    cursor: (f32, f32),
    /// `cl_playerskin` / `cl_playerbodygroups` as typed, sent together.
    player_skin: u8,
    player_body: String,
    /// A slider row being dragged (left button held since a press on it).
    panel_drag: Option<usize>,
    /// A text row with the keyboard focus and the text typed so far.
    panel_text: Option<(usize, String)>,
    grabbed: bool,
    /// `gui.EnableScreenClicker` from a script: the cursor stays visible over the game.
    screen_clicker: bool,
    /// Fonts the scripts created (`surface.CreateFont`), rebuilt with the HUD fonts on resize.
    script_fonts: Vec<(String, String, f32)>,
    /// Script audio channels (`sound.PlayFile`) by the script's id: the mixer voice behind each.
    audio_channels: std::collections::BTreeMap<u32, AudioChan>,
    /// Script server browser requests (`serverlist`) still being answered on a thread.
    script_servers: Vec<(u32, std::sync::mpsc::Receiver<Vec<servers::ServerEntry>>)>,
    /// Options dialog cvars and binds (`cfg/config.cfg`).
    settings: settings::Settings,
    /// The camera FOV being blended toward the server's `Player:SetFOV` target (value, last frame).
    /// `(start fov, target fov, when the target changed, current)`: the game's SetFOV blend
    /// (render.md §1.1, gap-report #141) runs from the FOV at the call with
    /// `start + (target - start) * (3t^2 - 2t^3)`, t = elapsed / rate.
    fov_blend: Option<(f32, f32, std::time::Instant, f32)>,
    /// The own flashlight's `distMod` pull-back from a near wall (render.md 3.3 step 6, #137).
    flashlight_dist_mod: f32,
    /// `Player:SendHint` tokens waiting for their delay.
    hint_queue: Vec<(Instant, String)>,
    /// `Player:ScreenFade`: the running full-screen fade.
    screen_fade: Option<ScreenFade>,
    /// The mixer is silenced while the main menu sits over a game not yet started.
    menu_muted: bool,
    /// A weapon to put out on the next tick (the crowbar after a respawn).
    pending_select: Option<u8>,
    /// `env_shake`s on the view.
    shakes: Vec<Shake>,
    shake_seed: u32,
    /// The Options dialog while open.
    options: Option<options::OptionsState>,
    /// The main menu's New Game page while open (`#/newgame/`).
    newgame: Option<newgame::NewGameState>,
    /// The Servers page while open (`#/servers/`).
    servers: Option<servers::ServersState>,
    /// The Saves / Dupes / Demos page while open.
    ugc: Option<ugc::UgcState>,
    /// The spawn menu's Dupes / Saves tabs (local files), and a countdown of frames until
    /// they are re-read after a save command.
    qm_dupes: ugc::UgcState,
    qm_saves: ugc::UgcState,
    qm_ugc_dirty: u8,
    /// The Addons page while open.
    addons: Option<addons::AddonsState>,
    /// The map this client runs.
    map_name: String,
    /// Slider being dragged in the Options dialog.
    opt_drag: Option<&'static str>,
    /// Monitor modes for the Video page.
    opt_modes: Vec<(u32, u32)>,
    /// Keyboard tab rows (`scripts/kb_act.lst`).
    kb_rows: Vec<settings::KbRow>,
    /// `resource/*_english.txt` tokens (GameUI_*, Valve_*, GarrysMod_*), lower-cased keys.
    gameui: BTreeMap<String, String>,
    /// Last presented frame, for `fps_max`.
    last_frame: Instant,
    /// Last HDR auto-exposure probe.
    exposure_at: Instant,
    /// The game's main menu over the game (Escape): `menu.html` with the in-game NavBar.
    main_menu: bool,
    /// A game has been started (a map named on the command line, a server joined, or New
    /// Game pressed): before that the main menu shows over the background slideshow.
    started: bool,
    /// `sp_paused` last frame (the mixer is told on the change).
    was_paused: bool,
    backgrounds: Backgrounds,
    last: Instant,
    acc: f32,
    frames: u32,
    fps_at: Instant,
    /// Effect time base (the toolgun screen scroll, beam noise / scroll): never reset,
    /// unlike `fps_at` which restarts every second.
    fx_epoch: Instant,
    error: Option<anyhow::Error>,
    console: Console,
}

fn depth_view(
    gpu: &Gpu,
    w: u32,
    h: u32,
    format: wgpu::TextureFormat,
    msaa: u32,
) -> wgpu::TextureView {
    let (_, depth) = gm_render::world::offscreen_targets(gpu, w, h, format, msaa);
    depth.create_view(&Default::default())
}

/// The multisampled colour target the scene draws into with `mat_antialias` > 1.
fn msaa_view(
    gpu: &Gpu,
    w: u32,
    h: u32,
    format: wgpu::TextureFormat,
    msaa: u32,
) -> Option<wgpu::TextureView> {
    (msaa > 1).then(|| {
        gm_render::world::offscreen_targets(gpu, w, h, format, msaa)
            .0
            .create_view(&Default::default())
    })
}

/// `mat_antialias` clamped to a sample count the device supports for the colour and depth
/// formats (8 -> 4 -> 2 -> 1): without `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` on the device
/// only 1 and 4 are guaranteed whatever the adapter reports.
fn pick_msaa(gpu: &gm_render::Gpu, format: wgpu::TextureFormat, want: u32) -> u32 {
    let specific = gpu
        .device
        .features()
        .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES);
    let color = gpu.adapter.get_texture_format_features(format).flags;
    let depth = gpu
        .adapter
        .get_texture_format_features(wgpu::TextureFormat::Depth32Float)
        .flags;
    let ok = |n: u32| {
        if specific {
            color.sample_count_supported(n) && depth.sample_count_supported(n)
        } else {
            n == 1 || n == 4
        }
    };
    let mut n = want.clamp(1, 8).next_power_of_two();
    while n > 1 && !ok(n) {
        n /= 2;
    }
    eprintln!("msaa: {n} samples (mat_antialias {want})");
    n
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
        eprintln!(
            "adapter: {} ({:?})",
            gpu.adapter_name(),
            gpu.adapter.get_info().backend
        );
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
        // the HDR auto-exposure probe reads the frame back
        config.usage |= wgpu::TextureUsages::COPY_SRC;
        surface.configure(&gpu.device, &config);
        let bsp = gm_formats::bsp::Bsp::parse(&self.bsp_data)?;
        let t0 = Instant::now();
        // mat_hdr_level >= 1 takes the HDR lumps when the map has them (applied on map load)
        let hdr = self.settings.mat_hdr_level >= 1 && bsp.has_hdr();
        let quality = gm_render::world::Quality {
            hdr,
            picmip: self.settings.mat_picmip,
            trilinear: self.settings.mat_trilinear,
            aniso: self.settings.mat_forceaniso,
            root_lod: self.settings.r_rootlod.max(0) as u32,
            msaa: pick_msaa(&gpu, format, self.settings.mat_antialias),
        };
        let world = WorldRenderer::new(&gpu, &self.mount, &bsp, format, quality)?;
        if hdr {
            println!("hdr lighting: exposure {:.2}", world.exposure);
        }
        {
            let t = Instant::now();
            let mount = &self.mount;
            let geometry = gm_audio::Geometry::from_bsp(&bsp, |texture| {
                gm_dedicated::material_surfaceprop(mount, texture)
            });
            let triangles = geometry.triangles.len();
            match self.sounds.mixer.set_scene(geometry) {
                Ok(()) => println!(
                    "acoustics: {triangles} triangles in {:.0} ms",
                    t.elapsed().as_secs_f32() * 1000.0
                ),
                Err(e) => println!("acoustics: {e}"),
            }
            self.soundscapes.clear(&mut self.sounds);
            self.soundscapes = gm_audio::Soundscapes::new(
                gm_audio::SoundscapeDefinitions::from_mount(&self.mount),
                gm_audio::SoundscapeEnv::from_bsp(&bsp),
            );
            println!(
                "soundscapes: {} defined, {} env_soundscape in the map",
                self.soundscapes.defs.len(),
                self.soundscapes.envs.len()
            );
        }
        eprintln!(
            "world: {} faces, {} props, built in {:.1}s",
            world.stats.faces,
            world.stats.props,
            t0.elapsed().as_secs_f32()
        );
        let depth = depth_view(&gpu, config.width, config.height, format, quality.msaa);
        let msaa_color = msaa_view(&gpu, config.width, config.height, format, quality.msaa);
        let post = gm_render::post::Post::new(&gpu, format, config.width, config.height);
        let player_box = world.solid_box(&gpu, [200, 120, 60, 255], false);
        let beam_mat = world.sprite_material(&gpu, &self.mount, "sprites/physbeam");
        let beam_active_mat = world.sprite_material(&gpu, &self.mount, "sprites/physbeama");
        let glow1 = world.sprite_material(&gpu, &self.mount, "sprites/physg_glow1");
        eprintln!(
            "physgun beam materials: beam {} active {} glow {}",
            if beam_mat.is_some() { "ok" } else { "MISSING" },
            if beam_active_mat.is_some() {
                "ok"
            } else {
                "MISSING"
            },
            if glow1.is_some() { "ok" } else { "MISSING" }
        );
        let glow2 = world.sprite_material(&gpu, &self.mount, "sprites/physg_glow2");
        let gg_core = world.sprite_material(&gpu, &self.mount, "sprites/orangecore1_gmod");
        let gg_glow = world.sprite_material(&gpu, &self.mount, "sprites/glow04_noz_gmod");
        let gg_beam = world.sprite_material(&gpu, &self.mount, "sprites/orangelight1_gmod");
        let gg_core2 = world.sprite_material(&gpu, &self.mount, "sprites/orangecore2_gmod");
        let gg_endcap = world.sprite_material(&gpu, &self.mount, "sprites/orangeflare1_gmod");
        let gg_beam_noz = world.sprite_material(&gpu, &self.mount, "sprites/orangelight1_noz");
        let fx_muzzle = world.sprite_material(&gpu, &self.mount, "effects/muzzleflash1");
        let fx_explo_smoke =
            world.sprite_material(&gpu, &self.mount, "particle/particle_smokegrenade");
        let fx_explo_embers = ["effects/fire_embers1", "effects/fire_embers2"]
            .iter()
            .filter_map(|m| world.sprite_material(&gpu, &self.mount, m))
            .collect();
        let fx_explo_fire = world.sprite_material(&gpu, &self.mount, "effects/fire_cloud2");
        let fx_explo_flecks = ["effects/fleck_cement1", "effects/fleck_cement2"]
            .iter()
            .filter_map(|m| world.sprite_material(&gpu, &self.mount, m))
            .collect();
        let fx_spark = world.sprite_material(&gpu, &self.mount, "effects/spark");
        let fx_ar2_tracer = world.sprite_material(&gpu, &self.mount, "effects/gunshiptracer");
        let fx_ar2_impact = world.sprite_material(&gpu, &self.mount, "effects/combinemuzzle2");
        let fx_cball_blur = world.sprite_material(&gpu, &self.mount, "effects/ar2_altfire1b");
        let fx_cball_flicker = world.sprite_material(&gpu, &self.mount, "effects/combinemuzzle1");
        let fx_blueflare = world.sprite_material(&gpu, &self.mount, "sprites/blueflare1");
        let fx_wood_flecks = ["effects/fleck_wood1", "effects/fleck_wood2"]
            .iter()
            .filter_map(|m| world.sprite_material(&gpu, &self.mount, m))
            .collect();
        let fx_noise = world.sprite_material(&gpu, &self.mount, "particle/particle_noisesphere");
        let fx_flare = world.sprite_material(&gpu, &self.mount, "effects/yellowflare");
        let fx_muzzles = (1..=4)
            .filter_map(|i| {
                world.sprite_material(&gpu, &self.mount, &format!("effects/muzzleflash{i}"))
            })
            .collect();
        let fx_tool_tracer = world.sprite_material(&gpu, &self.mount, "effects/tool_tracer");
        let fx_select_dot = world.sprite_material(&gpu, &self.mount, "effects/select_dot");
        let fx_select_ring = world.sprite_material(&gpu, &self.mount, "effects/select_ring");
        let fx_lgtning = world.sprite_material(&gpu, &self.mount, "sprites/lgtning");
        let fx_laserdot = world.sprite_material(&gpu, &self.mount, "sprites/redglow1");
        let fx_laserbeam = world.sprite_material(&gpu, &self.mount, "effects/laser1_noz");
        let fx_ball = world.sprite_material(&gpu, &self.mount, "sprites/sent_ball");
        let fx_sun = world
            .env_sun
            .as_ref()
            .and_then(|s| world.sprite_material(&gpu, &self.mount, &s.material));
        let fx_splash = world.sprite_material(&gpu, &self.mount, "effects/splash2");
        let ui = UiRenderer::new(&gpu, &self.mount, format, quality.msaa)?;
        let mut ui = ui;
        register_ui_fonts(&self.mount, &mut ui, &gpu, config.height);
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
            msaa_color,
            post,
            models: Default::default(),
            player_box,
            beam_mat,
            rope_mats: Default::default(),
            thruster_on: Default::default(),
            thruster_fx: Vec::new(),
            particles: Default::default(),
            particles_tick: None,
            missile_trails: Default::default(),
            thruster_emit: Default::default(),
            fx_rng: 0x9E37_79B9,
            gg_core,
            gg_glow,
            gg_beam,
            gg_core2,
            gg_endcap,
            gg_beam_noz,
            fx_muzzle,
            fx_explo_smoke,
            fx_explo_embers,
            fx_explo_fire,
            fx_explo_flecks,
            fx_wood_flecks,
            fx_noise,
            fx_flare,
            fx_muzzles,
            fx_tool_tracer,
            fx_select_dot,
            fx_select_ring,
            fx_lgtning,
            fx_spark,
            fx_ar2_tracer,
            fx_ar2_impact,
            fx_cball_blur,
            fx_cball_flicker,
            ball_last: HashMap::new(),
            fx_blueflare,
            fx_decal_mats: Default::default(),
            fx_laserdot,
            fx_laserbeam,
            fx_ball,
            fx_sprite_mats: Default::default(),
            fx_sun,
            fx_splash,
            beam_active_mat,
            glow1,
            glow2,
            ui,
        });
        if let Some(g) = &mut self.gfx {
            for idx in gm_render::world::movable_brush_models(&bsp) {
                let m = g.world.brush_model_dyn(&g.gpu, &self.mount, &bsp, idx);
                g.models.insert(format!("*{idx}"), m);
            }
        }
        // in the main menu at launch the pointer stays free
        self.set_grab(!self.main_menu);
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

    /// The camera FOV: the server's `Player:SetFOV` target blended over its time, else the own
    /// `fov_desired`.
    fn camera_fov(&mut self) -> f32 {
        let own = self.settings.fov_desired;
        let (target, secs) = self
            .local_snap()
            .filter(|p| p.fov > 0.0)
            .map_or((own, 0.0), |p| (p.fov, p.fov_time));
        let now = std::time::Instant::now();
        let (start, tgt, since, cur) = self.fov_blend.unwrap_or((target, target, now, target));
        let (start, since) = if (tgt - target).abs() > 0.01 {
            (cur, now)
        } else {
            (start, since)
        };
        let next = if secs > 0.0 {
            let t = (now.duration_since(since).as_secs_f32() / secs).clamp(0.0, 1.0);
            start + (target - start) * (3.0 * t * t - 2.0 * t * t * t)
        } else {
            target
        };
        self.fov_blend = Some((start, target, since, next));
        next
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

    /// `CHudDamageIndicator::CalcDamageDirection` (ui.md §9.3): the attack direction's
    /// angle against the view yaw, 0 = in front: 45..135 left, 225..315 right, 135..225 behind
    /// (both bars, and the full-screen flash for > 25 damage); a frontal hit shows nothing. A
    /// zero origin (no direction) is "behind".
    fn damage_from(&mut self, from: Vec3, origin: Vec3, amount: f32) {
        let high = amount > 25.0;
        let now = Instant::now();
        let behind = |me: &mut Self| {
            me.dmg_side = [Some((now, high)); 2];
            if high {
                me.dmg_full = Some(now);
            }
        };
        if from == Vec3::ZERO {
            behind(self);
            return;
        }
        let d = (from - origin).normalize_or_zero();
        let y = self.view.y.to_radians();
        let fwd = Vec3::new(y.cos(), y.sin(), 0.0);
        let left = Vec3::new(-y.sin(), y.cos(), 0.0);
        let angle = d.dot(left).atan2(d.dot(fwd)).to_degrees().rem_euclid(360.0);
        if (45.0..135.0).contains(&angle) {
            self.dmg_side[0] = Some((now, high));
        } else if (225.0..315.0).contains(&angle) {
            self.dmg_side[1] = Some((now, high));
        } else if (135.0..225.0).contains(&angle) {
            behind(self);
        }
    }

    /// HudAnimations HudTakeDamageLeft/Right (High): DmgColor 255 88 0 200 (255 for High)
    /// linear to 255 0 0 200 over 0.3 s (0.4), then Deaccel to alpha 0 over 0.5 s (2.4).
    fn dmg_side_color(&self, side: usize) -> Option<[f32; 4]> {
        let (at, high) = self.dmg_side[side]?;
        let t = at.elapsed().as_secs_f32();
        let (t1, t2, a0) = if high {
            (0.4, 2.4, 1.0)
        } else {
            (0.3, 0.5, 200.0 / 255.0)
        };
        if t < t1 {
            let k = t / t1;
            Some([
                1.0,
                88.0 / 255.0 * (1.0 - k),
                0.0,
                a0 + (200.0 / 255.0 - a0) * k,
            ])
        } else if t < t1 + t2 {
            let k = ((t - t1) / t2).sqrt();
            Some([1.0, 0.0, 0.0, 200.0 / 255.0 * (1.0 - k)])
        } else {
            None
        }
    }

    /// HudTakeDamageHighFullscreen: alpha 255 held 0.4 s, then Deaccel to 0 over 2.4 s.
    fn dmg_full_alpha(&self) -> Option<f32> {
        let t = self.dmg_full?.elapsed().as_secs_f32();
        if t < 0.4 {
            Some(1.0)
        } else if t < 2.8 {
            Some(1.0 - ((t - 0.4) / 2.4).sqrt())
        } else {
            None
        }
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
        }
        // the server's addon spawnlists follow the client's defaults
        let lists = &self.sim.client.world.spawn_lists;
        let seen = lists.iter().map(|(_, m)| m.len() + 1).sum::<usize>();
        if seen != self.spawn_lists_seen {
            self.spawn_lists_seen = seen;
            self.spawnlists.retain(|l| !l.addon);
            for (category, models) in lists {
                self.spawnlists.push(derma::SpawnList {
                    id: 0,
                    name: category.clone(),
                    icon: "icon16/folder_database.png".to_string(),
                    models: models.clone(),
                    addon: true,
                    parent: 0,
                });
            }
        }
        self.sync_content_lists();
        if let Some(g) = &mut self.gfx {
            for l in &self.spawnlists {
                g.ui.load_image(&g.gpu, &self.mount, &l.icon);
            }
            for name in [
                "gui/ContentIcon-normal.png",
                "gui/ContentIcon-hovered.png",
                "gui/noicon.png",
                "icon64/playermodel.png",
            ]
            .into_iter()
            .chain(derma::POST_PROCESS.iter().map(|r| r.2))
            {
                g.ui.load_image(&g.gpu, &self.mount, name);
            }
            for l in self.content[..4].iter().flatten() {
                g.ui.load_image(&g.gpu, &self.mount, &l.icon);
                for (class, _) in &l.items {
                    g.ui.load_image(&g.gpu, &self.mount, &format!("entities/{class}.png"));
                }
            }
        }
        // the spawnmenu keeps its tabs / node / scroll between openings (the game's panel is
        // only hidden); the search box and the right-click menu start closed
        let mut st = self.saved_menu.take().unwrap_or_default();
        st.search_focus = false;
        st.ctx_menu = None;
        self.menu = Some(SpawnMenu { st });
        self.mouse.clear();
        self.set_grab(false);
    }

    /// The Weapons / Entities tabs from the server's content lists: grouped by category in
    /// order of appearance, names localised through `entities.properties` /
    /// `spawnmenu.properties`, `ContentCategoryIcons` for the game categories.
    fn sync_content_lists(&mut self) {
        let world = &self.sim.client.world;
        let seen = world.content_weapons.len() * 4096 * 4096
            + world.content_npcs.len() * 4096
            + world.content_vehicles.len() * 64
            + world.content_entities.len()
            + 1;
        if seen == self.content_seen {
            return;
        }
        self.content_seen = seen;
        let lang = self.content_lang.get_or_insert_with(|| {
            let mut m =
                derma::load_properties(&self.mount, "resource/localization/en/entities.properties");
            m.extend(derma::load_properties(
                &self.mount,
                "resource/localization/en/spawnmenu.properties",
            ));
            for f in ["tool", "postprocessing", "context"] {
                m.extend(derma::load_properties(
                    &self.mount,
                    &format!("resource/localization/en/{f}.properties"),
                ));
            }
            m
        });
        let localize = |s: &str| -> String {
            match s.strip_prefix('#') {
                Some(key) => lang.get(key).cloned().unwrap_or_else(|| key.to_string()),
                None => s.to_string(),
            }
        };
        let category_icon = |category: &str, default: &str| -> String {
            match category {
                "Half-Life 2" => "games/16/hl2.png".into(),
                "Half-Life: Source" => "games/16/hl1.png".into(),
                "Portal" => "games/16/portal.png".into(),
                _ => default.into(),
            }
        };
        // `content.lua`: items grouped by category, within it by `SubCategory` (phrase; the
        // empty one first), a `ContentHeader` before each named sub-category.
        let build = |entries: &[gm_server::ContentEntry],
                     default_icon: &str,
                     sub: &dyn Fn(&str) -> &'static str| {
            // (sub-category, localised name, class)
            type SubItem = (String, String, String);
            let mut lists: Vec<(derma::ContentList, Vec<SubItem>)> = Vec::new();
            for e in entries {
                let category = localize(&e.category);
                let sub_name = match sub(&e.class) {
                    "" => String::new(),
                    phrase => localize(phrase),
                };
                let item = (sub_name, localize(&e.name), e.class.clone());
                match lists.iter_mut().find(|(l, _)| l.category == category) {
                    Some((_, items)) => items.push(item),
                    None => lists.push((
                        derma::ContentList {
                            icon: category_icon(&e.category, default_icon),
                            category,
                            items: Vec::new(),
                            headers: Vec::new(),
                        },
                        vec![item],
                    )),
                }
            }
            let mut out: Vec<derma::ContentList> = Vec::new();
            for (mut l, mut items) in lists {
                items.sort();
                let mut last = String::new();
                for (i, (sub_name, name, class)) in items.into_iter().enumerate() {
                    if !sub_name.is_empty() && sub_name != last {
                        l.headers.push((i, sub_name.clone()));
                    }
                    last = sub_name;
                    l.items.push((class, name));
                }
                out.push(l);
            }
            out.sort_by(|a, b| a.category.cmp(&b.category));
            out
        };
        self.content = [
            build(&world.content_weapons, "icon16/gun.png", &|_| ""),
            build(&world.content_entities, "icon16/bricks.png", &|class| {
                if gm_server::hl2_item_pickup(class).is_some() {
                    "#spawnmenu.category.ammo_items"
                } else {
                    ""
                }
            }),
            build(&world.content_npcs, "icon16/monkey.png", &|_| ""),
            build(&world.content_vehicles, "icon16/car.png", &|_| ""),
            {
                let mut lists: Vec<derma::ContentList> = Vec::new();
                for (key, name, _, cat, _) in derma::POST_PROCESS {
                    let category = localize(cat);
                    let item = (key.to_string(), localize(name));
                    match lists.iter_mut().find(|l| l.category == category) {
                        Some(l) => l.items.push(item),
                        None => lists.push(derma::ContentList {
                            category,
                            icon: "icon16/picture.png".to_string(),
                            items: vec![item],
                            headers: Vec::new(),
                        }),
                    }
                }
                lists.sort_by(|a, b| a.category.cmp(&b.category));
                lists
            },
        ];
    }

    /// `search.GetResults` on Enter: models by substring of the path without `models/` /
    /// `.mdl` (up to `sbox_search_maxresults` / 2 = 512), Weapons / Entities content by
    /// localised name or class (up to 256).
    fn refresh_search(&mut self) {
        let Some(menu) = &self.menu else { return };
        let query = menu.st.search.trim().to_ascii_lowercase();
        self.search_results = derma::SearchResults::default();
        if query.is_empty() {
            return;
        }
        let models = self.search_models.get_or_insert_with(|| {
            self.mount
                .find("models/**/*.mdl")
                .into_iter()
                .map(|m| m.to_ascii_lowercase())
                .collect()
        });
        let strip = !query.ends_with(".mdl");
        for m in models.iter() {
            let hay = if strip {
                m.strip_prefix("models/")
                    .unwrap_or(m)
                    .strip_suffix(".mdl")
                    .unwrap_or(m)
            } else {
                m.as_str()
            };
            if hay.contains(&query) {
                self.search_results.models.push(m.clone());
                if self.search_results.models.len() >= 512 {
                    break;
                }
            }
        }
        'content: for (tab, lists) in self.content.iter().enumerate() {
            for (class, name) in lists.iter().flat_map(|l| l.items.iter()) {
                if name.to_ascii_lowercase().contains(&query) || class.contains(&query) {
                    self.search_results
                        .content
                        .push((tab, class.clone(), name.clone()));
                    if self.search_results.content.len() >= 256 {
                        break 'content;
                    }
                }
            }
        }
    }

    /// The row under the cursor of the open spawn menu, if any.
    /// The context-menu entry under the cursor.
    fn context_row(&self) -> Option<String> {
        let (ctx, g) = (self.context.as_ref()?, self.gfx.as_ref()?);
        let font = g.ui.font_metrics(Some(derma::FONT));
        let entries = self.context_entries(ctx);
        derma::menu_leaf(&font, ctx.x, ctx.y, &entries, &ctx.open, self.cursor)
    }

    /// The context menu's entries for its entity (skin / body groups checked at its state).
    fn context_entries(&self, ctx: &ContextMenu) -> Vec<derma::MenuEntry> {
        let ent = self.sim.client.world.entities.get(&ctx.ent);
        let skin = ent.map_or(0, |e| e.skin);
        let body = ent.map_or("", |e| e.body.as_str());
        // `Entity:GetBodyGroups`: part and submodel names from the entity's MDL
        let groups: Vec<(String, Vec<String>)> = ent
            .and_then(|e| self.gfx.as_ref()?.models.get(&e.model)?.as_ref())
            .map(|m| m.bodygroups().to_vec())
            .unwrap_or_default();
        derma::menu_entries(&self.lang, &ctx.names, skin, body, &groups)
    }

    /// Hovering the context menu opens / closes submenus.
    fn context_hover(&mut self) {
        let Some(ctx) = self.context.take() else {
            return;
        };
        let mut ctx = ctx;
        if let Some(g) = &self.gfx {
            let font = g.ui.font_metrics(Some(derma::FONT));
            let entries = self.context_entries(&ctx);
            derma::menu_hover(&font, ctx.x, ctx.y, &entries, &mut ctx.open, self.cursor);
        }
        self.context = Some(ctx);
    }

    /// The selected stool's setting schema while the tool gun is out (the panel's content).
    /// The tool's ControlPanel: a scripted stool's schema from the server, else the built-in
    /// stool's `BuildCPanel` controls.
    fn panel_schema(&self) -> Option<Vec<gm_server::ToolSetting>> {
        if let Some(ent) = self.edit_sheet.filter(|_| self.menu.is_none()) {
            return self.edit_class(ent).and_then(gm_server::editor_schema);
        }
        if let Some(key) = self.utility_key() {
            return Some(utility_schema(key));
        }
        Some(
            self.tool_schema(&self.tool)
                .map_or_else(|| gm_server::builtin_tool_schema(&self.tool), <[_]>::to_vec),
        )
    }

    /// The Utilities option whose panel is open (the spawnmenu on its Utilities tab).
    fn utility_key(&self) -> Option<&'static str> {
        let st = &self.menu.as_ref()?.st;
        (st.tool_tab == 1).then(|| derma::UTILITIES[st.utility.min(derma::UTILITIES.len() - 1)].1)
    }

    /// The map the control panel's rows read: the Utilities values or the tool's settings.
    fn settings_map(&self) -> Option<&BTreeMap<String, String>> {
        if self.utility_key().is_some() {
            Some(&self.utility_values)
        } else {
            self.tool_settings.get(&self.tool)
        }
    }

    /// The server's replicated convars into the Utilities values (a client-side value set
    /// here stays).
    fn refresh_utility_values(&mut self) {
        for (name, value) in &self.sim.client.world.cvars {
            self.utility_values.insert(name.clone(), value.clone());
        }
    }

    /// The class of entity `ent` as the server told it (`EntityClasses`).
    fn edit_class(&self, ent: u32) -> Option<&str> {
        self.sim
            .client
            .world
            .classes
            .iter()
            .find(|(id, _)| *id == ent)
            .map(|(_, c)| c.as_str())
    }

    /// The editor entity's vars (`EntityEdits`), key -> value.
    fn edit_values(&self, ent: u32) -> BTreeMap<String, String> {
        edit_values_of(&self.sim.client.world.edits, ent)
    }

    /// The panel row's current value: the open editor sheet's var, else the tool's setting.
    fn panel_value(&self, name: &str) -> Option<String> {
        if let Some(ent) = self.edit_sheet.filter(|_| self.menu.is_none()) {
            return self.edit_values(ent).get(name).cloned();
        }
        self.tool_settings
            .get(&self.tool)
            .and_then(|m| m.get(name))
            .cloned()
    }

    /// The game's player model table, sorted by name: (name, model path).
    fn player_model_list() -> Vec<(&'static str, &'static str)> {
        let mut v: Vec<(&str, &str)> = gm_server::player_models::PLAYER_MODELS
            .iter()
            .map(|m| (m.0, m.1))
            .collect();
        v.sort_unstable();
        v.dedup_by(|a, b| a.1 == b.1);
        v
    }

    /// The local player's model name (`cl_playermodel`), from the snapshot.
    fn current_player_model(&self) -> String {
        let me = self.sim.client.player_id;
        self.sim
            .client
            .world
            .players
            .iter()
            .find(|p| Some(p.id) == me)
            .map(|p| gm_server::player_models::path_of_index(p.model))
            .and_then(gm_server::player_models::name_of)
            .unwrap_or("kleiner")
            .to_string()
    }

    /// A left click in the context menu on the desktop icons or inside the Player Model
    /// window: true when it was taken.
    fn player_editor_click(&mut self) -> bool {
        let (ix, iy, iw, ih) = derma::desktop_icon_rect(0);
        let (cx, cy) = self.cursor;
        if !self.player_editor && cx >= ix && cx < ix + iw && cy >= iy && cy < iy + ih {
            self.player_editor = true;
            return true;
        }
        if !self.player_editor {
            return false;
        }
        let Some(g) = self.gfx.as_ref() else {
            return false;
        };
        let (w, h) = (g.config.width as f32, g.config.height as f32);
        let font = g.ui.font_metrics(Some(derma::FONT));
        let bold = g.ui.font_metrics(Some(derma::FONT_BOLD));
        let header = g.ui.font_metrics(Some(derma::HEADER_FONT));
        let has_image = |name: &str| g.ui.has_image(name);
        let pp_on = |_: &str| false;
        let ctx = derma::Ctx {
            w,
            h,
            cursor: self.cursor,
            font: &font,
            bold: &bold,
            header: &header,
            lists: &self.spawnlists,
            content: [&[], &[], &[], &[], &[]],
            results: &self.search_results,
            tool: &self.tool,
            has_image: &has_image,
            pp_on: &pp_on,
            lang: &self.lang,
            ugc: None,
        };
        let mut layout = derma::Layout::default();
        let models = Self::player_model_list();
        derma::player_editor(&mut layout, &ctx, &models, &self.current_player_model());
        match layout.hit(self.cursor).cloned() {
            Some(derma::Action::PlayerModel(name)) => {
                self.console.log(format!("cl_playermodel {name}"));
                self.sim.client.command(Command::PlayerModel { name });
                true
            }
            Some(derma::Action::ClosePlayerEditor) => {
                self.player_editor = false;
                true
            }
            Some(_) => true,
            None => false,
        }
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
            .panel_value(&s.name)
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
            gm_server::ToolSettingKind::Help => return true,
            gm_server::ToolSettingKind::Key => {
                let v = cur.trim().parse::<i32>().unwrap_or(0);
                match v {
                    37..=45 => (v + 1).to_string(),
                    46 => "0".into(),
                    _ => "37".into(),
                }
            }
            gm_server::ToolSettingKind::Choice => {
                let n = s.options.len();
                if n == 0 {
                    return true;
                }
                let next = match s.options.iter().position(|(_, v)| v == cur.trim()) {
                    Some(i) if t < 0.5 => (i + n - 1) % n,
                    Some(i) => (i + 1) % n,
                    None => 0,
                };
                s.options[next].1.clone()
            }
            gm_server::ToolSettingKind::Color => {
                let Some(name) = self.color_channel(s, row) else {
                    return true;
                };
                self.panel_drag = Some(row);
                self.set_tool_setting(&name, format!("{:.0}", t * 255.0));
                return true;
            }
            gm_server::ToolSettingKind::Text => {
                self.panel_text = Some((row, cur));
                return true;
            }
            gm_server::ToolSettingKind::Button => {
                // `dupe_show`: the spawn menu's Dupes tab
                if s.name == "dupe_show" {
                    if let Some(m) = self.menu.as_mut() {
                        m.st.creation_tab = 6;
                        m.st.scroll = 0.0;
                    }
                }
                return true;
            }
            gm_server::ToolSettingKind::Presets => {
                let sel = self.preset_selected(&s.name);
                if t > 0.9 {
                    // the wrench: a new preset named in the text row (Enter saves); with shift
                    // held it removes the chosen preset instead (the game's Preset Editor)
                    if self.shift_held {
                        if !sel.is_empty() {
                            let mut g = presets::load(&s.name);
                            g.remove(&sel);
                            if let Err(e) = presets::save(&s.name, &g) {
                                eprintln!("presets: {e}");
                            }
                            self.set_preset_selected(&s.name, String::new());
                        }
                    } else {
                        self.panel_text = Some((row, String::new()));
                    }
                    return true;
                }
                // the combo box: the left half steps back, the right half forward
                let names: Vec<String> = presets::load(&s.name).into_keys().collect();
                let n = names.len() + 1;
                let i = if sel.is_empty() {
                    0
                } else {
                    names.iter().position(|p| *p == sel).map_or(0, |i| i + 1)
                };
                let next = if t < 0.5 {
                    (i + n - 1) % n
                } else {
                    (i + 1) % n
                };
                let chosen = if next == 0 {
                    None
                } else {
                    names.get(next - 1).cloned()
                };
                self.apply_preset(&s.name, chosen);
                return true;
            }
        };
        self.set_tool_setting(&s.name, value);
        true
    }

    /// The preset the tool's ControlPresets shows (empty: Default); kept with the tool's
    /// settings under a key no convar uses.
    fn preset_selected(&self, tool: &str) -> String {
        self.tool_settings
            .get(tool)
            .and_then(|m| m.get(&format!("__preset:{tool}")))
            .cloned()
            .unwrap_or_default()
    }

    fn set_preset_selected(&mut self, tool: &str, name: String) {
        self.tool_settings
            .entry(tool.to_string())
            .or_default()
            .insert(format!("__preset:{tool}"), name);
    }

    /// The tool's convars a preset covers: every control's convar (a colour's r, g and b).
    fn preset_convars(&self, tool: &str) -> Vec<(String, String)> {
        let schema = self
            .tool_schema(tool)
            .map_or_else(|| gm_server::builtin_tool_schema(tool), <[_]>::to_vec);
        let mut out = Vec::new();
        for s in &schema {
            if matches!(
                s.kind,
                gm_server::ToolSettingKind::Help | gm_server::ToolSettingKind::Presets
            ) {
                continue;
            }
            out.push((s.name.clone(), s.default.clone()));
            if s.kind == gm_server::ToolSettingKind::Color {
                for (_, name) in &s.options {
                    out.push((name.clone(), s.default.clone()));
                }
            }
        }
        out
    }

    /// `ControlPresets:OnSelect`: Default restores the controls' defaults, a preset sets its
    /// convars.
    fn apply_preset(&mut self, tool: &str, name: Option<String>) {
        match &name {
            None => {
                for (k, d) in self.preset_convars(tool) {
                    self.set_tool_setting(&k, d);
                }
            }
            Some(p) => {
                if let Some(vars) = presets::load(tool).remove(p) {
                    for (k, v) in vars {
                        self.set_tool_setting(&k, v);
                    }
                }
            }
        }
        self.set_preset_selected(tool, name.unwrap_or_default());
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
            if model.is_empty() || g.ui.has_image(&key) {
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
                gamma: 2.2,
            }
            .forward();
            let dist = radius / (25f32.to_radians().tan()) * 1.05;
            let camera = Camera {
                position: center - dir * dist,
                angles,
                fov_x: 50.0,
                near: (dist - radius).max(0.5),
                far: dist + radius * 2.0,
                gamma: 2.2,
            };
            // rendered at twice the 64 px SpawnIcon so the icon stays sharp when drawn
            let (color, depth) = gm_render::world::icon_targets(&g.gpu, 128, g.config.format);
            let color_view = color.create_view(&Default::default());
            let msaa = g.world.quality.msaa;
            // the pipelines are multisampled: draw into MSAA targets and resolve into the icon
            let ms = (msaa > 1).then(|| {
                gm_render::world::offscreen_targets(&g.gpu, 128, 128, g.config.format, msaa)
            });
            let ms_color = ms.as_ref().map(|(c, _)| c.create_view(&Default::default()));
            let depth_view = ms
                .as_ref()
                .map(|(_, d)| d)
                .unwrap_or(&depth)
                .create_view(&Default::default());
            let target = gm_render::world::ModelTarget {
                color: ms_color.as_ref().unwrap_or(&color_view),
                resolve: ms_color.as_ref().map(|_| &color_view),
                depth: &depth_view,
                aspect: 1.0,
                clear_depth: true,
                alpha: 1.0,
            };
            let item = DrawItem {
                tint: Vec3::ONE,
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
        // a Utilities row is a convar: server ones go up as console lines
        if self.utility_key().is_some() {
            if self.utility_values.get(key) == Some(&value) {
                return;
            }
            self.utility_values.insert(key.to_string(), value.clone());
            self.sim.client.command(Command::Console {
                line: format!("{key} {value}"),
            });
            return;
        }
        // an open "Edit Properties" sheet: the row is the editor entity's var, not a convar
        if let Some(ent) = self.edit_sheet.filter(|_| self.menu.is_none()) {
            self.sim.client.command(Command::EditEntity {
                ent,
                key: key.to_string(),
                value: value.clone(),
            });
            if let Some((_, vars)) = self
                .sim
                .client
                .world
                .edits
                .iter_mut()
                .find(|(id, _)| *id == ent)
            {
                match vars.iter_mut().find(|(k, _)| k == key) {
                    Some(kv) => kv.1 = value,
                    None => vars.push((key.to_string(), value)),
                }
            }
            return;
        }
        self.console
            .log(format!("tool {}: {key} = {value}", self.tool));
        self.tool_settings
            .entry(self.tool.clone())
            .or_default()
            .insert(key.to_string(), value);
    }

    /// The cursor moved with a slider held: the value follows it (clamped to the row's range,
    /// the cursor may leave the panel).
    /// The convar of the colour picker's r/g/b bar under the cursor (the bars sit under a 16 px
    /// label, 16 px each).
    fn color_channel(&self, s: &gm_server::ToolSetting, row: usize) -> Option<String> {
        let y = self.menu_cache.borrow().rows.get(row)?.y;
        let k = ((self.cursor.1 - y - 16.0) / 16.0).floor().clamp(0.0, 2.0) as usize;
        match k {
            0 => Some(s.name.clone()),
            _ => s.options.get(k - 1).map(|(n, _)| n.clone()),
        }
    }

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
        if s.kind == gm_server::ToolSettingKind::Color {
            let t = self
                .menu_cache
                .borrow()
                .rows
                .get(row)
                .map(|r| r.fraction(self.cursor));
            if let (Some(t), Some(name)) = (t, self.color_channel(&s, row)) {
                self.set_tool_setting(&name, format!("{:.0}", t * 255.0));
            }
            return;
        }
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
                    if s.kind == gm_server::ToolSettingKind::Presets {
                        // `presets.Add`: the current settings under the typed name
                        let name = value.trim().to_string();
                        if !name.is_empty() {
                            let vars: std::collections::BTreeMap<String, String> = self
                                .preset_convars(&s.name)
                                .into_iter()
                                .map(|(k, d)| {
                                    let v = self
                                        .tool_settings
                                        .get(&s.name)
                                        .and_then(|m| m.get(&k))
                                        .cloned()
                                        .unwrap_or(d);
                                    (k, v)
                                })
                                .collect();
                            let mut g = presets::load(&s.name);
                            g.insert(name.clone(), vars);
                            if let Err(e) = presets::save(&s.name, &g) {
                                eprintln!("presets: {e}");
                            }
                            self.set_preset_selected(&s.name, name);
                        }
                    } else {
                        self.set_tool_setting(&s.name, value);
                    }
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

    /// The main menu laid out for this frame (fonts by the CSS width breakpoint).
    fn main_menu_layout(&self, g: &Gfx) -> derma::Layout {
        let (w, h) = (g.config.width as f32, g.config.height as f32);
        let font = g.ui.font_metrics(Some(derma::FONT));
        let bold = g.ui.font_metrics(Some(derma::FONT_BOLD));
        let header = g.ui.font_metrics(Some(derma::HEADER_FONT));
        let opt_font = if w > 1280.0 {
            derma::MENU_FONT_23
        } else {
            derma::MENU_FONT_16
        };
        let opt = g.ui.font_metrics(Some(opt_font));
        let nav = g.ui.font_metrics(Some(derma::NAV_FONT));
        let has_image = |name: &str| g.ui.has_image(name);
        let pp_set = self.pp_enabled.clone();
        let pp_on = |k: &str| pp_set.contains(k);
        let uf = ugc_font_set(g);
        let ufonts = uf.fonts();
        let ctx = derma::Ctx {
            w,
            h,
            cursor: self.cursor,
            font: &font,
            bold: &bold,
            header: &header,
            lists: &self.spawnlists,
            content: [
                &self.content[0],
                &self.content[1],
                &self.content[2],
                &self.content[3],
                &self.content[4],
            ],
            results: &self.search_results,
            tool: &self.tool,
            has_image: &has_image,
            pp_on: &pp_on,
            lang: &self.lang,
            ugc: Some((&self.qm_dupes, &self.qm_saves, &ufonts)),
        };
        let mut layout = derma::Layout::default();
        if !self.started {
            self.backgrounds
                .items(&mut layout.items, w, h, |n| g.ui.image_size(n));
        }
        derma::main_menu(
            &mut layout,
            &ctx,
            self.started,
            self.newgame.is_none()
                && self.servers.is_none()
                && self.ugc.is_none()
                && self.addons.is_none(),
            &self.lang,
            &opt,
            opt_font,
            &nav,
        );
        if let Some(ng) = &self.newgame {
            let f = |n: &str| g.ui.font_metrics(Some(n));
            let (f10, f11, f11b, f12, f13, f14, f20, f32) = (
                f(newgame::F10),
                f(newgame::F11),
                f(newgame::F11B),
                f(newgame::F12),
                f(newgame::F13),
                f(newgame::F14),
                f(newgame::F20),
                f(newgame::F32),
            );
            let fonts = newgame::Fonts {
                f10: &f10,
                f11: &f11,
                f11b: &f11b,
                f12: &f12,
                f13: &f13,
                f14: &f14,
                f20: &f20,
                f32: &f32,
            };
            let max = newgame::page(&mut layout, &ctx, ng, &self.lang, &fonts);
            self.menu_cache.borrow_mut().ng_max_scroll = max;
        }
        if let Some(sv) = &self.servers {
            let f = |n: &str| g.ui.font_metrics(Some(n));
            let (f11, f12, f13, f13b, f20b, f32) = (
                f(newgame::F11),
                f(newgame::F12),
                f(newgame::F13),
                f(servers::F13B),
                f(servers::F20B),
                f(newgame::F32),
            );
            let fonts = servers::Fonts {
                f11: &f11,
                f12: &f12,
                f13: &f13,
                f13b: &f13b,
                f20b: &f20b,
                f32: &f32,
                opt: &opt,
                opt_font,
            };
            servers::page(&mut layout, &ctx, sv, &self.lang, &fonts);
        }
        if let Some(u) = &self.ugc {
            let f = |n: &str| g.ui.font_metrics(Some(n));
            let (f11, f13, f14b, f32, f48b) = (
                f(newgame::F11),
                f(newgame::F13),
                f(ugc::F14B),
                f(newgame::F32),
                f(ugc::F48B),
            );
            let fonts = ugc::Fonts {
                f11: &f11,
                f13: &f13,
                f14b: &f14b,
                f32: &f32,
                f48b: &f48b,
                opt: &opt,
                opt_font,
            };
            let per_page = ugc::page(&mut layout, &ctx, u, &self.lang, &fonts);
            self.menu_cache.borrow_mut().ugc_per_page = per_page;
        }
        if let Some(a) = &self.addons {
            let f = |n: &str| g.ui.font_metrics(Some(n));
            let (f11, f12, f13, f14b, f32, f48b) = (
                f(newgame::F11),
                f(newgame::F12),
                f(newgame::F13),
                f(ugc::F14B),
                f(newgame::F32),
                f(ugc::F48B),
            );
            let fonts = addons::Fonts {
                f11: &f11,
                f12: &f12,
                f13: &f13,
                f14b: &f14b,
                f32: &f32,
                f48b: &f48b,
                opt: &opt,
                opt_font,
            };
            let per_page = addons::page(&mut layout, &ctx, a, &self.lang, &fonts);
            self.menu_cache.borrow_mut().ugc_per_page = per_page;
        }
        layout
    }

    fn addons_click(&mut self) {
        let Some(g) = self.gfx.as_ref() else { return };
        let action = self.main_menu_layout(g).hit(self.cursor).cloned();
        let per_page = self.menu_cache.borrow().ugc_per_page.max(1);
        let Some(a) = self.addons.as_mut() else {
            return;
        };
        a.per_page = per_page;
        a.focus = false;
        match action {
            Some(derma::Action::Menu("back")) => self.addons = None,
            Some(derma::Action::Menu("quit" | "disconnect")) => std::process::exit(0),
            Some(derma::Action::Menu("resume_game")) => {
                self.main_menu = false;
                self.addons = None;
                self.set_grab(true);
            }
            Some(derma::Action::Addons(act)) => {
                use addons::AdAction as A;
                match act {
                    A::Category(c) => {
                        a.category = c;
                        a.tag = "";
                        a.offset = 0;
                        if c != "subscribed" && a.entries.is_empty() {
                            a.fetch_registry();
                        }
                    }
                    A::Tag(t) => {
                        a.tag = if a.tag == t { "" } else { t };
                        a.offset = 0;
                    }
                    A::Search => a.focus = true,
                    A::Page(dir) => {
                        let total = a.items().len();
                        let next = a.offset as i64 + dir as i64 * per_page as i64;
                        if next >= 0 && (next as usize) < total.max(1) {
                            a.offset = next as usize;
                        }
                    }
                    A::GoToPage(p) => {
                        let off = (p.max(1) - 1) * per_page;
                        if off < a.items().len().max(1) {
                            a.offset = off;
                        }
                    }
                    A::Enable(id) => a.set_enabled(&id, true),
                    A::Disable(id) => a.set_enabled(&id, false),
                    A::Install(id) => a.install(&id),
                    A::Uninstall(id) => a.uninstall(&id),
                    A::OpenWorkshop => {
                        if let Some(r) = a.registry.clone() {
                            let _ = std::process::Command::new("xdg-open").arg(r).spawn();
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn addons_key(&mut self, code: Option<KeyCode>, text: Option<&str>) {
        let Some(a) = self.addons.as_mut() else {
            return;
        };
        match code {
            Some(KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Escape | KeyCode::Tab) => {
                a.focus = false
            }
            Some(KeyCode::Backspace) => {
                a.search.pop();
            }
            _ => {
                if let Some(t) = text {
                    a.search.extend(t.chars().filter(|c| !c.is_control()));
                    a.search.truncate(64);
                }
            }
        }
        a.offset = 0;
    }

    fn open_ugc(&mut self, kind: &'static str) {
        let mut st = ugc::UgcState::new(kind, &self.lang);
        st.per_page = self.menu_cache.borrow().ugc_per_page.max(1);
        if let Some(g) = self.gfx.as_mut() {
            for p in st.files.iter().filter_map(|f| f.preview.as_deref()) {
                g.ui.load_image(&g.gpu, &self.mount, p);
            }
        }
        self.ugc = Some(st);
    }

    fn ugc_click(&mut self) {
        let Some(g) = self.gfx.as_ref() else { return };
        let action = self.main_menu_layout(g).hit(self.cursor).cloned();
        let per_page = self.menu_cache.borrow().ugc_per_page.max(1);
        let Some(u) = self.ugc.as_mut() else { return };
        u.per_page = per_page;
        match action {
            Some(derma::Action::Menu("back")) => self.ugc = None,
            Some(derma::Action::Menu("quit" | "disconnect")) => std::process::exit(0),
            Some(derma::Action::Menu("resume_game")) => {
                self.main_menu = false;
                self.ugc = None;
                self.set_grab(true);
            }
            Some(derma::Action::Ugc(a)) => {
                use ugc::UgcAction as A;
                match a {
                    A::Category(c) => {
                        u.category = c;
                        u.offset = 0;
                    }
                    A::OpenFolder => {
                        let dir = u.folder();
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
                    }
                    A::Page(dir) => {
                        let total = u.total();
                        let next = u.offset as i64 + dir as i64 * per_page as i64;
                        if next >= 0 && (next as usize) < total.max(1) {
                            u.offset = next as usize;
                        }
                    }
                    A::GoToPage(p) => {
                        let off = (p.max(1) - 1) * per_page;
                        if off < u.total().max(1) {
                            u.offset = off;
                        }
                    }
                    A::Primary(name) => match u.kind {
                        // `gm_load`: the in-process server's console `load <name>`
                        "saves" | "dupes" => {
                            self.sim.client.command(Command::Console {
                                line: format!("load {name}"),
                            });
                            self.main_menu = false;
                            self.ugc = None;
                            self.set_grab(true);
                        }
                        _ => {}
                    },
                    A::Delete(name) => u.delete(&name, &self.lang),
                    A::Save => {}
                }
            }
            _ => {}
        }
    }

    /// Relaunches this client with the arguments rebuilt: the positional map replaced (or
    /// dropped) and `--connect` replaced.
    fn relaunch(&self, map: Option<&str>, connect: Option<&str>) {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let argv: Vec<String> = std::env::args().skip(1).collect();
        let mut args: Vec<String> = Vec::new();
        let mut i = 0;
        while i < argv.len() {
            let a = &argv[i];
            if a == "--connect" {
                i += 2;
                continue;
            }
            if a.starts_with("--") {
                args.push(a.clone());
                if let Some(v) = argv.get(i + 1).filter(|v| !v.starts_with("--")) {
                    args.push(v.clone());
                    i += 1;
                }
            }
            // positional (the old map) dropped
            i += 1;
        }
        if let Some(c) = connect {
            args.push("--connect".to_string());
            args.push(c.to_string());
        } else if let Some(m) = map {
            args.insert(0, m.to_string());
        }
        match std::process::Command::new(exe).args(&args).spawn() {
            Ok(_) => std::process::exit(0),
            Err(e) => eprintln!("relaunch: {e}"),
        }
    }

    fn servers_click(&mut self) {
        let Some(g) = self.gfx.as_ref() else { return };
        let action = self.main_menu_layout(g).hit(self.cursor).cloned();
        let Some(sv) = self.servers.as_mut() else {
            return;
        };
        sv.focus = false;
        match action {
            Some(derma::Action::Menu("back")) => self.servers = None,
            Some(derma::Action::Menu("quit" | "disconnect")) => std::process::exit(0),
            Some(derma::Action::Menu("resume_game")) => {
                self.main_menu = false;
                self.servers = None;
                self.set_grab(true);
            }
            Some(derma::Action::Servers(a)) => {
                use servers::SvAction as A;
                match a {
                    A::Type(k) => {
                        sv.kind = k;
                        sv.gamemode = None;
                        sv.selected = None;
                        sv.status.clear();
                        if sv.servers().is_empty() {
                            sv.refresh();
                        }
                    }
                    A::Refresh => {
                        if sv.refreshing.is_some() {
                            sv.stop_refresh();
                        } else {
                            sv.refresh();
                        }
                    }
                    A::Gamemode(gm) => sv.gamemode = gm,
                    A::FindText => sv.focus = true,
                    A::Find => sv.find(),
                    A::Select(addr) => {
                        if sv.selected.as_deref() == Some(addr.as_str()) {
                            // second click joins, like the double click
                            sv.record_join(&addr);
                            self.relaunch(None, Some(&addr));
                        } else {
                            sv.selected = Some(addr);
                        }
                    }
                    A::Join(addr) => {
                        sv.record_join(&addr);
                        self.relaunch(None, Some(&addr));
                    }
                    A::Fav(addr) => sv.toggle_favorite(&addr),
                }
            }
            _ => {}
        }
    }

    fn servers_key(&mut self, code: Option<KeyCode>, text: Option<&str>) {
        let Some(sv) = self.servers.as_mut() else {
            return;
        };
        match code {
            Some(KeyCode::Enter | KeyCode::NumpadEnter) => {
                sv.focus = false;
                sv.find();
            }
            Some(KeyCode::Escape | KeyCode::Tab) => sv.focus = false,
            Some(KeyCode::Backspace) => {
                sv.find_text.pop();
            }
            _ => {
                if let Some(t) = text {
                    sv.find_text
                        .extend(t.chars().filter(|c| !c.is_control() && !c.is_whitespace()));
                    sv.find_text.truncate(64);
                }
            }
        }
    }

    fn open_newgame(&mut self) {
        let hostname = "Garry's Mod".to_string();
        let st = newgame::NewGameState::new(&self.mount, &self.map_name, &hostname);
        if let Some(g) = &mut self.gfx {
            for (_, maps) in &st.categories {
                for m in maps {
                    g.ui.load_image(&g.gpu, &self.mount, &format!("maps/thumb/{m}.png"));
                }
            }
        }
        self.newgame = Some(st);
    }

    /// `StartGame`: the settings go to `cfg/server.cfg` for the client's own server, then
    /// the client relaunches on the map (the game disconnects and loads it the same way).
    fn start_game(&mut self) {
        let Some(ng) = &self.newgame else { return };
        let Some(map) = ng.selected.clone() else {
            return;
        };
        let cfg = settings::home_dir().join("cfg").join("server.cfg");
        if let Some(dir) = cfg.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&cfg, ng.server_cfg()) {
            eprintln!("server.cfg: {e}");
        }
        self.relaunch(Some(&map), None);
    }

    fn newgame_click(&mut self) {
        let Some(g) = self.gfx.as_ref() else { return };
        let action = self.main_menu_layout(g).hit(self.cursor).cloned();
        let Some(ng) = self.newgame.as_mut() else {
            return;
        };
        let was_open = std::mem::take(&mut ng.players_open);
        ng.focus = None;
        match action {
            Some(derma::Action::Menu("back")) => self.newgame = None,
            Some(derma::Action::Menu("quit" | "disconnect")) => std::process::exit(0),
            Some(derma::Action::Menu("resume_game")) => {
                self.main_menu = false;
                self.newgame = None;
                self.set_grab(true);
            }
            Some(derma::Action::NewGame(a)) => {
                use newgame::NgAction as A;
                match a {
                    A::Category(c) => {
                        ng.current = c;
                        ng.scroll = 0.0;
                    }
                    A::Map(m) => ng.selected = Some(m),
                    A::Fav(m) => ng.toggle_favourite(&self.mount, &m),
                    A::Search => ng.focus = Some("search".to_string()),
                    A::Players => ng.players_open = !was_open,
                    A::PlayersPick(i) => ng.maxplayers = i,
                    A::Check(name) => match name.as_str() {
                        "sv_lan" => ng.sv_lan = !ng.sv_lan,
                        "p2p_enabled" => ng.p2p_enabled = !ng.p2p_enabled,
                        "p2p_friendsonly" => ng.p2p_friendsonly = !ng.p2p_friendsonly,
                        n => {
                            if let Some(s) = ng.settings.iter_mut().find(|s| s.name == n) {
                                let on = !s.value.is_empty() && s.value != "0";
                                s.value = if on { "0" } else { "1" }.to_string();
                            }
                        }
                    },
                    A::Text(name) => ng.focus = Some(name),
                    A::Start => self.start_game(),
                }
            }
            _ => {}
        }
    }

    /// Typing into a New Game text field.
    fn newgame_key(&mut self, code: Option<KeyCode>, text: Option<&str>) {
        let Some(ng) = self.newgame.as_mut() else {
            return;
        };
        match code {
            Some(KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Escape | KeyCode::Tab) => {
                ng.focus = None;
            }
            Some(KeyCode::Backspace) => {
                if let Some(v) = ng.text_mut() {
                    v.pop();
                }
            }
            _ => {
                if let (Some(t), Some(v)) = (text, ng.text_mut()) {
                    v.extend(t.chars().filter(|c| !c.is_control()));
                    v.truncate(255);
                }
            }
        }
    }

    /// The Options dialog laid out for this frame (caches the slider bars for dragging).
    fn options_layout(&self, g: &Gfx, st: &options::OptionsState) -> derma::Layout {
        let (w, h) = (g.config.width as f32, g.config.height as f32);
        let font = g.ui.font_metrics(Some(derma::FONT));
        let bold = g.ui.font_metrics(Some(derma::FONT_BOLD));
        let header = g.ui.font_metrics(Some(derma::HEADER_FONT));
        let default = g.ui.font_metrics(Some(options::FONT));
        let sbold = g.ui.font_metrics(Some(options::FONT_BOLD));
        let large = g.ui.font_metrics(Some(options::FONT_LARGE));
        let has_image = |name: &str| g.ui.has_image(name);
        let pp_set = self.pp_enabled.clone();
        let pp_on = |k: &str| pp_set.contains(k);
        let uf = ugc_font_set(g);
        let ufonts = uf.fonts();
        let ctx = derma::Ctx {
            w,
            h,
            cursor: self.cursor,
            font: &font,
            bold: &bold,
            header: &header,
            lists: &self.spawnlists,
            content: [
                &self.content[0],
                &self.content[1],
                &self.content[2],
                &self.content[3],
                &self.content[4],
            ],
            results: &self.search_results,
            tool: &self.tool,
            has_image: &has_image,
            pp_on: &pp_on,
            lang: &self.lang,
            ugc: Some((&self.qm_dupes, &self.qm_saves, &ufonts)),
        };
        let fonts = options::Fonts {
            default: &default,
            bold: &sbold,
            large: &large,
        };
        let mut layout = derma::Layout::default();
        let bars = options::dialog(
            &mut layout,
            &ctx,
            st,
            &self.kb_rows,
            &self.gameui,
            &self.opt_modes,
            &fonts,
        );
        self.menu_cache.borrow_mut().opt_bars = bars;
        layout
    }

    fn open_options(&mut self) {
        if let Some(g) = &self.gfx {
            let mut modes: Vec<(u32, u32)> = g
                .window
                .current_monitor()
                .map(|m| {
                    m.video_modes()
                        .map(|v| (v.size().width, v.size().height))
                        .collect()
                })
                .unwrap_or_default();
            modes.push((g.config.width, g.config.height));
            modes.sort_unstable();
            modes.dedup();
            self.opt_modes = modes;
        }
        self.options = Some(options::OptionsState::new(&self.settings));
    }

    /// OK / Apply: the pending settings become the live ones, saved to `cfg/config.cfg`.
    fn apply_options(&mut self) {
        let Some(st) = &self.options else { return };
        let pending = st.pending.clone();
        let resize = pending.width != self.settings.width
            || pending.height != self.settings.height
            || pending.fullscreen != self.settings.fullscreen;
        self.settings = pending;
        if let Err(e) = self.settings.save() {
            eprintln!("config.cfg: {e}");
        }
        self.sounds.mixer.set_master(self.settings.volume);
        if let Some(g) = self.gfx.as_mut() {
            let mode = if self.settings.mat_vsync {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::AutoNoVsync
            };
            if g.config.present_mode != mode {
                g.config.present_mode = mode;
                g.surface.configure(&g.gpu.device, &g.config);
            }
        }
        if resize {
            if let Some(g) = &self.gfx {
                if self.settings.fullscreen {
                    g.window
                        .set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                } else {
                    g.window.set_fullscreen(None);
                    let _ = g.window.request_inner_size(winit::dpi::PhysicalSize::new(
                        self.settings.width,
                        self.settings.height,
                    ));
                }
            }
        }
    }

    fn options_click(&mut self) {
        let Some(g) = self.gfx.as_ref() else { return };
        let Some(st) = self.options.as_ref() else {
            return;
        };
        let action = self.options_layout(g, st).hit(self.cursor).cloned();
        let Some(st) = self.options.as_mut() else {
            return;
        };
        let was_combo = st.combo.take();
        if !matches!(
            action,
            Some(derma::Action::Opt(options::OptAction::Entry(_)))
        ) {
            st.commit_entry();
        }
        let Some(derma::Action::Opt(a)) = action else {
            return;
        };
        use options::OptAction as A;
        match a {
            A::Tab(i) => {
                st.tab = i;
                st.waiting_key = false;
            }
            A::Ok => {
                self.apply_options();
                self.options = None;
            }
            A::Apply => self.apply_options(),
            A::Cancel | A::Close => self.options = None,
            A::KbRow(i) => {
                st.kb_sel = Some(i);
                st.waiting_key = false;
            }
            A::EditKey => {
                if st.kb_sel.is_some() {
                    st.waiting_key = true;
                }
            }
            A::ClearKey => {
                if let Some(r) = st.kb_sel.and_then(|i| self.kb_rows.get(i)) {
                    st.pending.unbind_command(&r.action);
                }
            }
            A::Defaults => st.pending.binds = settings::Settings::default_binds(&self.mount),
            A::Slider(name) => {
                self.opt_drag = Some(name);
                self.opt_drag_move();
            }
            A::Check(name) => options::check_toggle(&mut st.pending, name),
            A::Combo(name) => {
                if was_combo != Some(name) {
                    st.combo = Some(name);
                }
            }
            A::ComboItem(name, i) => options::combo_pick(st, name, i, &self.opt_modes),
            A::Entry(name) => {
                st.entry_focus = Some(name);
                st.entry_text = format!("{}", options::slider_get(&st.pending, name));
            }
            A::Button(name @ ("VideoAdvanced" | "Gamma" | "KeyAdvanced")) => {
                st.sub = Some(name);
                st.sub_backup = Some(st.pending.clone());
            }
            A::Button(_) => {}
            A::SubOk => {
                st.sub = None;
                st.sub_backup = None;
            }
            A::SubCancel => {
                if let Some(b) = st.sub_backup.take() {
                    st.pending = b;
                }
                st.sub = None;
            }
        }
    }

    fn opt_drag_move(&mut self) {
        let Some(name) = self.opt_drag else { return };
        let bar = self
            .menu_cache
            .borrow()
            .opt_bars
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|&(_, x0, x1)| (x0, x1));
        let (Some((x0, x1)), Some(st)) = (bar, self.options.as_mut()) else {
            return;
        };
        let f = ((self.cursor.0 - x0) / (x1 - x0)).clamp(0.0, 1.0);
        let (lo, hi) = options::slider_range(name);
        options::slider_set(&mut st.pending, name, lo + f * (hi - lo));
    }

    /// A key / mouse button while the Keyboard page waits for one: bind it.
    fn options_bind(&mut self, key: &str) {
        let Some(st) = self.options.as_mut() else {
            return;
        };
        if let Some(r) = st.kb_sel.and_then(|i| self.kb_rows.get(i)) {
            st.pending.bind(key, &r.action);
        }
        st.waiting_key = false;
    }

    fn main_menu_click(&mut self) {
        if self.options.is_some() {
            self.options_click();
            return;
        }
        if self.newgame.is_some() {
            self.newgame_click();
            return;
        }
        if self.servers.is_some() {
            self.servers_click();
            return;
        }
        if self.ugc.is_some() {
            self.ugc_click();
            return;
        }
        if self.addons.is_some() {
            self.addons_click();
            return;
        }
        let Some(g) = self.gfx.as_ref() else { return };
        let action = self.main_menu_layout(g).hit(self.cursor).cloned();
        match action {
            Some(derma::Action::Menu("resume_game")) => {
                self.main_menu = false;
                self.set_grab(true);
            }
            Some(derma::Action::Menu("quit" | "disconnect")) => std::process::exit(0),
            Some(derma::Action::Menu("options")) => self.open_options(),
            Some(derma::Action::Menu("newgame")) => self.open_newgame(),
            Some(derma::Action::Menu(k @ ("saves" | "dupes" | "demos"))) => self.open_ugc(k),
            Some(derma::Action::Menu("addons")) => {
                let mut st = addons::AddonsState::new();
                st.per_page = self.menu_cache.borrow().ugc_per_page.max(1);
                self.addons = Some(st);
            }
            Some(derma::Action::Menu("servers")) => {
                let mut sv = servers::ServersState::new();
                sv.refresh();
                self.servers = Some(sv);
            }
            _ => {}
        }
    }

    /// Right click in the spawn menu: the icons' `DermaMenu` on a spawn icon of an editable
    /// (non-addon) spawnlist, over the shift-selection when the icon is part of it.
    fn menu_action_at(&self) -> Option<derma::Action> {
        let (menu, g) = self.menu.as_ref().zip(self.gfx.as_ref())?;
        let (w, h) = (g.config.width as f32, g.config.height as f32);
        let font = g.ui.font_metrics(Some(derma::FONT));
        let bold = g.ui.font_metrics(Some(derma::FONT_BOLD));
        let header = g.ui.font_metrics(Some(derma::HEADER_FONT));
        let has_image = |name: &str| g.ui.has_image(name);
        let pp_set = self.pp_enabled.clone();
        let pp_on = |k: &str| pp_set.contains(k);
        let uf = ugc_font_set(g);
        let ufonts = uf.fonts();
        let ctx = derma::Ctx {
            w,
            h,
            cursor: self.cursor,
            font: &font,
            bold: &bold,
            header: &header,
            lists: &self.spawnlists,
            content: [
                &self.content[0],
                &self.content[1],
                &self.content[2],
                &self.content[3],
                &self.content[4],
            ],
            results: &self.search_results,
            tool: &self.tool,
            has_image: &has_image,
            pp_on: &pp_on,
            lang: &self.lang,
            ugc: Some((&self.qm_dupes, &self.qm_saves, &ufonts)),
        };
        derma::build(&ctx, &menu.st).hit(self.cursor).cloned()
    }

    /// A left press on a spawn icon of a spawnlist starts a possible drag (contenticon.lua
    /// `Droppable( "SandboxContentPanel" )`).
    fn menu_drag_start(&mut self) {
        let action = self.menu_action_at();
        let lists = self.spawnlists.len();
        let Some(menu) = self.menu.as_mut() else {
            return;
        };
        menu.st.drag = None;
        if let Some(derma::Action::Spawn(model)) = action {
            if menu.st.creation_tab == 0
                && menu.st.node < lists
                && menu.st.ctx_menu.is_none()
                && menu.st.list_menu.is_none()
            {
                let models = if menu.st.selected.contains(&model) {
                    menu.st.selected.to_vec()
                } else {
                    vec![model]
                };
                menu.st.drag = Some((self.cursor, models));
            }
        }
    }

    /// Releasing a dragged icon over another tree list moves it there; from a read-only
    /// (addon) list it is copied (the game's Move / Copy drop menu, with Move as default).
    fn menu_drop(&mut self) {
        let Some((at, models)) = self.menu.as_mut().and_then(|m| m.st.drag.take()) else {
            return;
        };
        if (self.cursor.0 - at.0).abs() < 6.0 && (self.cursor.1 - at.1).abs() < 6.0 {
            return;
        }
        let Some(derma::Action::Node(j)) = self.menu_action_at() else {
            return;
        };
        let Some(src) = self.menu.as_ref().map(|m| m.st.node) else {
            return;
        };
        if src == j || !self.spawnlists.get(j).is_some_and(|l| !l.addon) {
            return;
        }
        if self.spawnlists.get(src).is_some_and(|l| !l.addon) {
            if let Some(l) = self.spawnlists.get_mut(src) {
                l.models.retain(|m| !models.contains(m));
            }
        }
        if let Some(l) = self.spawnlists.get_mut(j) {
            for m in models {
                if !l.models.contains(&m) {
                    l.models.push(m);
                }
            }
        }
        if let Some(menu) = self.menu.as_mut() {
            menu.st.selected.clear();
        }
        if let Err(e) = derma::save_spawnlists(&self.spawnlists) {
            eprintln!("spawnlist: {e}");
        }
    }

    fn menu_right_click(&mut self) {
        if self.panel_click() {
            return;
        }
        let pending = self.menu_right_click_inner();
        if let Some((event, kind)) = pending {
            let mut p = gm_script::api::Payload::default();
            p.strs.push(("kind", kind.to_string()));
            self.sim
                .script
                .client_event(&self.sim.client.world, event, &p);
        }
    }

    /// The right click's menu; the sandbox hook event it opens (SpawnmenuIconMenuOpen /
    /// SpawnlistOpenGenericMenu), if any.
    fn menu_right_click_inner(&mut self) -> Option<(&'static str, &'static str)> {
        let (menu, g) = self.menu.as_mut().zip(self.gfx.as_ref())?;
        let (w, h) = (g.config.width as f32, g.config.height as f32);
        let font = g.ui.font_metrics(Some(derma::FONT));
        let bold = g.ui.font_metrics(Some(derma::FONT_BOLD));
        let header = g.ui.font_metrics(Some(derma::HEADER_FONT));
        let has_image = |name: &str| g.ui.has_image(name);
        let pp_set = self.pp_enabled.clone();
        let pp_on = |k: &str| pp_set.contains(k);
        let uf = ugc_font_set(g);
        let ufonts = uf.fonts();
        let ctx = derma::Ctx {
            w,
            h,
            cursor: self.cursor,
            font: &font,
            bold: &bold,
            header: &header,
            lists: &self.spawnlists,
            content: [
                &self.content[0],
                &self.content[1],
                &self.content[2],
                &self.content[3],
                &self.content[4],
            ],
            results: &self.search_results,
            tool: &self.tool,
            has_image: &has_image,
            pp_on: &pp_on,
            lang: &self.lang,
            ugc: Some((&self.qm_dupes, &self.qm_saves, &ufonts)),
        };
        let action = derma::build(&ctx, &menu.st).hit(self.cursor).cloned();
        menu.st.ctx_menu = None;
        menu.st.list_menu = None;
        if let Some(derma::Action::Node(i)) = action {
            if menu.st.creation_tab == 0 && self.spawnlists.get(i).is_some_and(|l| !l.addon) {
                menu.st.list_menu = Some((self.cursor, i));
                return Some(("spawnmenu.generic_menu", ""));
            }
        }
        if let Some(derma::Action::Spawn(model)) = action {
            let listed = menu.st.creation_tab == 0 && menu.st.node < self.spawnlists.len();
            if listed {
                let models = if menu.st.selected.contains(&model) {
                    menu.st.selected.clone()
                } else {
                    vec![model]
                };
                menu.st.ctx_menu = Some((self.cursor, models));
                return Some(("spawnmenu.icon_menu", "model"));
            }
        }
        None
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
        let header = g.ui.font_metrics(Some(derma::HEADER_FONT));
        let has_image = |name: &str| g.ui.has_image(name);
        let pp_set = self.pp_enabled.clone();
        let pp_on = |k: &str| pp_set.contains(k);
        let uf = ugc_font_set(g);
        let ufonts = uf.fonts();
        let ctx = derma::Ctx {
            w,
            h,
            cursor: self.cursor,
            font: &font,
            bold: &bold,
            header: &header,
            lists: &self.spawnlists,
            content: [
                &self.content[0],
                &self.content[1],
                &self.content[2],
                &self.content[3],
                &self.content[4],
            ],
            results: &self.search_results,
            tool: &self.tool,
            has_image: &has_image,
            pp_on: &pp_on,
            lang: &self.lang,
            ugc: Some((&self.qm_dupes, &self.qm_saves, &ufonts)),
        };
        let action = derma::build(&ctx, &menu.st).hit(self.cursor).cloned();
        if let Some((_, i)) = menu.st.list_menu.take() {
            match action {
                Some(derma::Action::Menu("spawnlist_new")) => {
                    let at = self.spawnlists.iter().filter(|l| !l.addon).count();
                    let id = self.spawnlists.iter().map(|l| l.id).max().unwrap_or(0) + 1;
                    self.spawnlists.insert(
                        at,
                        derma::SpawnList {
                            id,
                            name: "New Spawnlist".into(),
                            icon: "icon16/page.png".into(),
                            models: Vec::new(),
                            addon: false,
                            parent: 0,
                        },
                    );
                    menu.st.node = at;
                }
                Some(derma::Action::Menu("spawnlist_sub")) => {
                    let Some(parent) = self.spawnlists.get(i).map(|l| l.id) else {
                        return;
                    };
                    let id = self.spawnlists.iter().map(|l| l.id).max().unwrap_or(0) + 1;
                    self.spawnlists.insert(
                        i + 1,
                        derma::SpawnList {
                            id,
                            name: "New Category".into(),
                            icon: "icon16/folder.png".into(),
                            models: Vec::new(),
                            addon: false,
                            parent,
                        },
                    );
                    menu.st.node = i + 1;
                }
                Some(derma::Action::Menu("spawnlist_remove")) => {
                    let editable = self.spawnlists.iter().filter(|l| !l.addon).count() > 1
                        && self.spawnlists.get(i).is_some_and(|l| !l.addon);
                    if editable {
                        // the list and its sub-categories
                        let mut gone = vec![self.spawnlists[i].id];
                        loop {
                            let more: Vec<u32> = self
                                .spawnlists
                                .iter()
                                .filter(|l| gone.contains(&l.parent) && !gone.contains(&l.id))
                                .map(|l| l.id)
                                .collect();
                            if more.is_empty() {
                                break;
                            }
                            gone.extend(more);
                        }
                        self.spawnlists.retain(|l| !gone.contains(&l.id));
                        menu.st.node = menu.st.node.min(self.spawnlists.len().saturating_sub(1));
                    }
                }
                Some(derma::Action::Menu("spawnlist_edit")) => {
                    if let Some(l) = self.spawnlists.get(i) {
                        menu.st.rename = Some((i, l.name.clone()));
                    }
                    return;
                }
                _ => return,
            }
            menu.st.selected.clear();
            menu.st.scroll = 0.0;
            if let Err(e) = derma::save_spawnlists(&self.spawnlists) {
                eprintln!("spawnlist: {e}");
            }
            return;
        }
        if let Some((_, models)) = menu.st.ctx_menu.take() {
            if matches!(action, Some(derma::Action::Menu("spawnlist_copy"))) {
                if let Some(t) = self.spawnlists.iter().position(|l| !l.addon) {
                    for model in &models {
                        if !self.spawnlists[t].models.contains(model) {
                            self.spawnlists[t].models.push(model.clone());
                        }
                    }
                    menu.st.selected.clear();
                    if let Err(e) = derma::save_spawnlists(&self.spawnlists) {
                        eprintln!("spawnlist: {e}");
                    }
                }
                return;
            }
            if matches!(action, Some(derma::Action::Menu("spawnlist_delete"))) {
                if let Some(l) = self.spawnlists.get_mut(menu.st.node) {
                    l.models.retain(|m| !models.contains(m));
                }
                menu.st.selected.clear();
                if let Err(e) = derma::save_spawnlists(&self.spawnlists) {
                    eprintln!("spawnlist: {e}");
                }
            }
            return;
        }
        if !matches!(action, Some(derma::Action::SearchFocus)) {
            menu.st.search_focus = false;
        }
        match action {
            Some(derma::Action::Spawn(model)) => {
                if self.shift_held {
                    // shift-click selects icons (the content container's selection canvas)
                    match menu.st.selected.iter().position(|m| *m == model) {
                        Some(i) => {
                            menu.st.selected.remove(i);
                        }
                        None => menu.st.selected.push(model),
                    }
                } else {
                    menu.st.selected.clear();
                    self.console.log(format!("spawn {model}"));
                    self.sim.client.command(Command::Spawn { model });
                }
            }
            Some(derma::Action::CreationTab(i)) => {
                menu.st.creation_tab = i;
                menu.st.scroll = 0.0;
                if i >= 6 {
                    self.qm_dupes.refresh(&self.lang);
                    self.qm_saves.refresh(&self.lang);
                }
            }
            Some(derma::Action::Ugc(a)) => {
                use ugc::UgcAction as A;
                let dupes = menu.st.creation_tab == 6;
                let per_page = self.menu_cache.borrow().qm_per_page.max(1);
                let u = if dupes {
                    &mut self.qm_dupes
                } else {
                    &mut self.qm_saves
                };
                match a {
                    A::Category(c) => {
                        u.category = c;
                        u.offset = 0;
                    }
                    A::OpenFolder => {
                        let dir = u.folder();
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
                    }
                    A::Page(dir) => {
                        let next = u.offset as i64 + dir as i64 * per_page as i64;
                        if next >= 0 && (next as usize) < u.total().max(1) {
                            u.offset = next as usize;
                        }
                    }
                    A::GoToPage(p) => {
                        let off = (p.max(1) - 1) * per_page;
                        if off < u.total().max(1) {
                            u.offset = off;
                        }
                    }
                    // a dupe is armed into the duplicator's clipboard (dupe_arm), a save
                    // loaded into the world (gm_load)
                    A::Primary(name) => {
                        let line = if dupes {
                            self.tool = "duplicator".to_string();
                            format!("dupe_arm {name}")
                        } else {
                            format!("load {name}")
                        };
                        self.console.log(line.clone());
                        self.sim.client.command(Command::Console { line });
                    }
                    A::Save => {
                        let stamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |d| d.as_secs());
                        let line = if dupes {
                            format!("dupe_save dupe_{stamp}")
                        } else {
                            format!("save save_{stamp}")
                        };
                        self.console.log(line.clone());
                        self.sim.client.command(Command::Console { line });
                        self.qm_ugc_dirty = 20;
                    }
                    A::Delete(name) => u.delete(&name, &self.lang),
                }
            }
            Some(derma::Action::SearchFocus) => menu.st.search_focus = true,
            Some(derma::Action::ContentNode(i)) => {
                menu.st.content_node[menu.st.creation_tab.saturating_sub(1).min(4)] = i;
                menu.st.scroll = 0.0;
            }
            Some(derma::Action::GiveWeapon(class)) => {
                self.console.log(format!("gm_giveswep {class}"));
                self.sim.client.command(Command::GiveWeapon { class });
            }
            Some(derma::Action::SpawnSent(class)) => {
                self.console.log(format!("gm_spawnsent {class}"));
                self.sim.client.command(Command::SpawnSent { class });
            }
            Some(derma::Action::SpawnVehicle(key)) => {
                self.console.log(format!("gmod_spawnvehicle {key}"));
                self.sim.client.command(Command::SpawnVehicle { key });
            }
            Some(derma::Action::TogglePostProcess(key)) => {
                let on = !self.pp_enabled.remove(&key);
                if on {
                    self.pp_enabled.insert(key.clone());
                }
                self.console.log(format!("pp_{key} {}", u8::from(on)));
            }
            Some(derma::Action::DesktopWindow(id)) => {
                if id == "PlayerEditor" {
                    self.player_editor = true;
                }
            }
            Some(derma::Action::PlayerModel(_) | derma::Action::ClosePlayerEditor) => {}
            Some(derma::Action::SpawnNpc(key)) => {
                self.console.log(format!("gmod_spawnnpc {key}"));
                self.sim.client.command(Command::SpawnNpc {
                    key,
                    weapon: String::new(),
                });
            }
            Some(derma::Action::ToolTab(i)) => {
                menu.st.tool_tab = i;
                self.panel_text = None;
                if i == 1 {
                    self.refresh_utility_values();
                }
                if i == 0 {
                    let p = gm_script::api::Payload::default();
                    self.sim
                        .script
                        .client_event(&self.sim.client.world, "spawnmenu.toolbox", &p);
                }
            }
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
            Some(derma::Action::Utility(i)) => {
                menu.st.utility = i;
                self.panel_text = None;
                self.refresh_utility_values();
            }
            Some(derma::Action::Command(line)) => {
                self.console.log(line.clone());
                self.sim.client.command(Command::Console { line });
            }
            Some(
                derma::Action::Property(_)
                | derma::Action::Menu(_)
                | derma::Action::Opt(_)
                | derma::Action::NewGame(_)
                | derma::Action::Servers(_)
                | derma::Action::Addons(_)
                | derma::Action::Nothing,
            )
            | None => {}
        }
    }

    /// Stock viewmodel table, then the scripted weapons' entries (indices after `LOADOUT`).
    fn vm_of(&self, weapon: u8) -> Option<&'static Viewmodel> {
        viewmodel(weapon).or_else(|| {
            self.scripted_vms
                .get((weapon as usize).checked_sub(WEAPONS.len())?)
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
                    fov: if d.view_model_fov > 0.0 {
                        d.view_model_fov
                    } else {
                        62.0
                    },
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
            .map(|p| (WEAPONS.len() + p) as u8)
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
        if self.settings.hud_fastswitch {
            // hud_fastswitch 1: the slot key switches straight away, no bucket menu
            self.wsel = None;
            self.select_weapon(next);
            self.sounds.emit(&self.mount, "Player.WeaponSelected", None);
            return;
        }
        if self.wsel.is_none() {
            // OpenWeaponSelectionMenu: Player.WeaponSelectionOpen (ui.md §8.1, gap-report #160)
            self.wsel_opened = Some(Instant::now());
            self.sounds
                .emit(&self.mount, "Player.WeaponSelectionOpen", None);
        }
        self.wsel = Some(next);
        self.wsel_at = Some(Instant::now());
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
        let next = all[((k + dir).rem_euclid(n)) as usize];
        if self.settings.hud_fastswitch {
            self.wsel = None;
            self.select_weapon(next);
            self.sounds.emit(&self.mount, "Player.WeaponSelected", None);
            return;
        }
        if self.wsel.is_none() {
            // OpenWeaponSelectionMenu: Player.WeaponSelectionOpen (ui.md §8.1, gap-report #160)
            self.wsel_opened = Some(Instant::now());
            self.sounds
                .emit(&self.mount, "Player.WeaponSelectionOpen", None);
        }
        self.wsel = Some(next);
        self.wsel_at = Some(Instant::now());
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
        if self.main_menu && !self.started {
            return;
        }
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
    fn draw_weapon_select(&self, items: &mut Vec<UiItem>, w: f32, h: f32, ui: &UiRenderer) {
        let Some(sel) = self.wsel else { return };
        let Some(snap) = self.local_snap() else {
            return;
        };
        if !snap.alive {
            return;
        }
        // SELECTION_TIMEOUT_THRESHOLD 0.5 s, then SELECTION_FADEOUT_TIME 0.75 s
        let idle = self.wsel_at.map_or(0.0, |t| t.elapsed().as_secs_f32());
        let opened = self
            .wsel_opened
            .map_or(1.0, |t| t.elapsed().as_secs_f32() / 0.1);
        let fade = (1.0 - (idle - 0.5) / 0.75).clamp(0.0, 1.0) * opened.clamp(0.0, 1.0);
        let s = h / 480.0;
        let classes = gm_net::msg::weapon_classes(&self.sim.client.world.weapon_defs);
        let buckets = self.buckets();
        let fg = [1.0, 220.0 / 255.0, 0.0, fade];
        let dim = [1.0, 220.0 / 255.0, 0.0, 0.5 * fade];
        // SelectionBoxBg 0 0 0 90, the selected one 0 0 0 150 (ui.md §8.1, gap-report #159)
        let box_bg = [0.0, 0.0, 0.0, 90.0 / 255.0 * fade];
        let sel_bg = [0.0, 0.0, 0.0, 150.0 / 255.0 * fade];
        let (small, large_w, large_h, gap) = (32.0 * s, 112.0 * s, 80.0 * s, 8.0 * s);
        let icons = ui.font_metrics(Some("WeaponIcons"));
        let numbers = ui.font_metrics(Some("HudSelectionNumbers"));
        let text_m = ui.font_metrics(Some("HudSelectionText"));
        // CHudWeaponSelection::Paint: the row of all slots (small boxes, the open one large)
        // centred on the panel's width; an empty slot keeps its place
        let width = (buckets.len().saturating_sub(1)) as f32 * (small + gap) + large_w;
        let mut x = ((w - width) / 2.0).round();
        let y0 = 16.0 * s;
        let t = self.epoch.elapsed().as_secs_f32();
        for (slot, list) in buckets.iter().enumerate() {
            if list.is_empty() {
                // an empty bucket is a 0 0 0 70 box without a number (ui.md §8.3)
                items.push(UiItem::Rect {
                    x,
                    y: y0,
                    w: small,
                    h: small,
                    color: [0.0, 0.0, 0.0, 70.0 / 255.0 * fade],
                });
                x += small + gap;
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
                    color: if open && i == sel { sel_bg } else { box_bg },
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
                        // the selected weapon's `weapon_s` sprite is the
                        // WeaponIconsSelected font (blur 5, scanlines 2; #161)
                        let font = if i == sel {
                            "WeaponIconsSelected"
                        } else {
                            "WeaponIcons"
                        };
                        items.push(UiItem::Text {
                            font: Some(font.to_string()),
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
            // gmod_camera `Equip`: the zoom starts from fov_desired (toolgun.md F38, #96)
            if weapon == CAMERA_SLOT && self.weapon != CAMERA_SLOT {
                self.cam_zoom = self.settings.fov_desired;
            }
            // the stunstick's holster is `Weapon_StunStick.Deactivate` (hl2weapons.md #113)
            if weapon != self.weapon {
                // holster sounds: the stunstick's `Deactivate` (#113), the RPG's `LaserOff` (#110)
                match viewmodel(self.weapon).map(|v| v.class) {
                    Some("weapon_stunstick") => {
                        self.sounds
                            .emit(&self.mount, "Weapon_StunStick.Deactivate", None);
                    }
                    Some("weapon_rpg") => {
                        self.sounds.emit(&self.mount, "Weapon_RPG.LaserOff", None);
                    }
                    _ => {}
                }
            }
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
        let (seq, t) = self.vm_sequence_raw(vm);
        if let Some((last, lt)) = self.vm_last {
            if last != seq {
                self.vm_prev = Some((last, lt, Instant::now()));
            }
        }
        self.vm_last = Some((seq, t));
        (seq, t)
    }

    /// `CSequenceTransitioner`: the sequence the viewmodel just left, frozen at the cycle it was
    /// at, with its weight easing out over that sequence's `fadeouttime`.
    fn vm_transition(
        prev: Option<(&'static str, f32, Instant)>,
        fade_of: impl Fn(&str) -> f32,
    ) -> Option<(&'static str, f32, f32)> {
        let (seq, t, at) = prev?;
        let x = (at.elapsed().as_secs_f32() / fade_of(seq).max(0.001)).clamp(0.0, 1.0);
        if x >= 1.0 {
            return None;
        }
        Some((seq, t, 1.0 - (3.0 * x * x - 2.0 * x * x * x)))
    }

    /// The toolgun's predicted trace: the game's tool trace from the eye along the view
    /// (56756 u); the world and the entities we see are enough to know the click lands.
    fn predict_tool_hit(&self) -> bool {
        let Some(p) = self.local_snap() else {
            return false;
        };
        let eye = p.origin + Vec3::Z * if p.ducked { 28.0 } else { 64.0 };
        let (pitch, yaw) = (p.view_angles.x.to_radians(), p.view_angles.y.to_radians());
        let fwd = Vec3::new(
            pitch.cos() * yaw.cos(),
            pitch.cos() * yaw.sin(),
            -pitch.sin(),
        );
        let tr = self
            .sim
            .collision
            .trace_ray(eye, eye + fwd * 56756.0, gm_collision::MASK_SOLID);
        tr.fraction < 1.0
    }

    fn vm_sequence_raw(&mut self, vm: &Viewmodel) -> (&'static str, f32) {
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
                    7 => Some(vm.draw),
                    // no named `alt` (the physgun): the model's own ACT_VM_SECONDARYATTACK
                    // sequence, as the game's SendWeaponAnim resolves it (physgun.md 70, #77)
                    8 => vm.alt.or_else(|| {
                        let name = self
                            .gfx
                            .as_ref()
                            .and_then(|g| g.models.get(vm.path))
                            .and_then(|m| m.as_ref())
                            .and_then(|m| m.sequence_for_activity("ACT_VM_SECONDARYATTACK"))?;
                        Some(static_seq_name(name))
                    }),
                    // the frag's pull-back / throw / roll and the AR2 charge resolve through
                    // the model's activity tags (hl2weapons.md, #100)
                    9..=13 => self
                        .gfx
                        .as_ref()
                        .and_then(|g| g.models.get(vm.path))
                        .and_then(|m| m.as_ref())
                        .and_then(|m| {
                            m.sequence_for_activity(match act {
                                9 => "ACT_VM_FIDGET",
                                10 => "ACT_VM_PULLBACK_HIGH",
                                11 => "ACT_VM_THROW",
                                12 => "ACT_VM_HAULBACK",
                                _ => "ACT_VM_PULLBACK_LOW",
                            })
                        })
                        .map(static_seq_name),
                    _ => None,
                };
                let predicted = act == 6
                    && gm_server::WIRE_WEAPONS.get(active as usize) == Some(&"gmod_tool")
                    && self
                        .tool_predicted
                        .take()
                        .is_some_and(|t| t.elapsed().as_secs_f32() < 0.3);
                if let (Some(seq), false) = (seq, predicted) {
                    self.vm_anim = Some((seq, Instant::now()));
                }
                if act == 6 && fires_bullets(active) {
                    self.fx_muzzle_until =
                        Some(Instant::now() + std::time::Duration::from_millis(60));
                }
                if let (Some(name), false) = (weapon_sound(act, active), predicted) {
                    self.sounds.emit(&self.mount, &name, None);
                }
                // the crossbow's load effect (`SPECIAL1`, Weapon_Crossbow.BoltElectrify) comes
                // as the reload seats the bolt, about a second into the sequence
                if act == 1
                    && gm_server::WIRE_WEAPONS.get(active as usize) == Some(&"weapon_crossbow")
                {
                    self.fx_sound_queue.push((
                        Instant::now() + std::time::Duration::from_millis(1000),
                        "Weapon_Crossbow.BoltElectrify".to_string(),
                    ));
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
        let t = self.fx_epoch.elapsed().as_secs_f32();
        match vm.hold {
            Some(hold) if held => (hold, t),
            _ => (vm.idle, t),
        }
    }

    /// A bound console command on key press (`down`) / release (`+` commands end).
    fn run_bind(&mut self, cmd: &str, down: bool) {
        if down {
            match cmd {
                "slot1" | "slot2" | "slot3" | "slot4" | "slot5" | "slot6" => {
                    let n = cmd.as_bytes()[4] - b'1';
                    self.wsel_slot(n);
                }
                "impulse 100" => self.impulse = 100,
                "invnext" => self.wsel_step(1),
                "invprev" => self.wsel_step(-1),
                "+menu" => self.open_menu(),
                "+showscores" => self.scoreboard = true,
                "+voicerecord" => self.voice.start(),
                "+menu_context" => {
                    if self.menu.is_none() && !self.context_held {
                        self.context_held = true;
                        self.set_grab(false);
                        // cl_spawnmenu.lua OnContextMenuOpen
                        self.add_hint("ContextClick", 20);
                    }
                }
                "gmod_undo" | "undo" => self.sim.client.command(Command::Undo),
                "gmod_cleanup" => self.sim.client.command(Command::Cleanup),
                "noclip" => {
                    self.noclip = !self.noclip;
                    self.sim.client.command(Command::Noclip { on: self.noclip });
                }
                // gmod_camera `Reload`: zoom back to fov_desired, roll 0 (toolgun.md F38)
                "+reload" if self.weapon == CAMERA_SLOT => {
                    if !self.down("+attack2") {
                        self.cam_zoom = self.settings.fov_desired;
                    }
                    self.cam_roll = 0.0;
                }
                "+reload" if self.weapon == 1 => self.sim.client.command(Command::ToolClick {
                    tool: self.tool.clone(),
                    right: false,
                    reload: true,
                    settings: self.tool_settings_list(),
                }),
                _ => {}
            }
        } else {
            match cmd {
                "+showscores" => self.scoreboard = false,
                "+menu" if self.menu.is_some() => {
                    self.saved_menu = self.menu.take().map(|m| m.st);
                    self.panel_drag = None;
                    self.panel_text = None;
                    self.set_grab(true);
                }
                "+menu_context" if self.context_held => {
                    self.context_held = false;
                    self.player_editor = false;
                    self.edit_sheet = None;
                    self.context = None;
                    if self.menu.is_none() {
                        self.set_grab(true);
                    }
                }
                "+voicerecord" => self.voice.stop(),
                _ => {}
            }
        }
    }

    /// Whether a key (Source name) is held: `MOUSEn` from the button set, the rest by code.
    fn key_down(&self, key: &str) -> bool {
        match settings::mouse_button(key) {
            Some(b) => self.mouse.contains(&b),
            None => settings::key_code(key).is_some_and(|c| self.keys.contains(&c)),
        }
    }

    /// Whether any key bound to `command` is held.
    fn down(&self, command: &str) -> bool {
        self.settings
            .binds
            .iter()
            .any(|(k, c)| c == command && self.key_down(k))
    }

    /// The command bound to a key code, if any.
    fn bound(&self, code: KeyCode) -> Option<String> {
        let name = settings::key_name(code)?;
        self.settings.binds.get(name).cloned()
    }

    fn cmd(&self) -> NetCmd {
        // gmod_camera `FreezeMovement` (toolgun.md F38, #96): no walking while +attack2 zooms
        let frozen = self.weapon == CAMERA_SLOT && self.down("+attack2");
        let k = |a: &str| !frozen && self.down(a);
        let mut b = 0;
        let run = k("+speed");
        let speed = if run { 400.0 } else { 200.0 };
        let mut fwd = 0.0;
        let mut side = 0.0;
        if k("+forward") {
            fwd += speed;
            b |= buttons::FORWARD;
        }
        if k("+back") {
            fwd -= speed;
            b |= buttons::BACK;
        }
        if k("+moveright") {
            side += speed;
            b |= buttons::MOVERIGHT;
        }
        if k("+moveleft") {
            side -= speed;
            b |= buttons::MOVELEFT;
        }
        if k("+jump") {
            b |= buttons::JUMP;
        }
        if k("+duck") {
            b |= buttons::DUCK;
        }
        if run {
            b |= buttons::SPEED;
        }
        if k("+use") {
            b |= buttons::USE;
        }
        if k("+reload") && self.weapon != 1 {
            b |= buttons::RELOAD;
        }
        if k("+attack") {
            b |= buttons::ATTACK;
        }
        if k("+attack2") {
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

    /// The game menu pauses a single-player game (the engine's `pause` while the server has
    /// one player and `sv_pausable` is implied in single player): the simulation, the
    /// interpolation clocks and every sound hold until the menu closes.
    fn sp_paused(&self) -> bool {
        self.main_menu && self.started && !self.sim.remote && self.sim.server.players.len() <= 1
    }

    fn step(&mut self) {
        let now = Instant::now();
        let frame = (now - self.last).as_secs_f32().min(0.25);
        self.last = now;
        let paused = self.sp_paused();
        if paused != self.was_paused {
            self.was_paused = paused;
            self.sounds.mixer.set_game_paused(paused);
        }
        if paused {
            self.acc = 0.0;
            return;
        }
        if self.qm_ugc_dirty > 0 {
            self.qm_ugc_dirty -= 1;
            if self.qm_ugc_dirty == 0 {
                self.qm_dupes.refresh(&self.lang);
                self.qm_saves.refresh(&self.lang);
            }
        }
        self.acc += frame;
        self.sim.interp.advance(frame);
        self.sim.player_interp.advance(frame);
        let dt = gm_core::tick::DEFAULT_TICK_INTERVAL;
        while self.acc >= dt {
            let mut cmd = self.cmd();
            cmd.impulse = std::mem::take(&mut self.impulse);
            let (mx, my) = std::mem::take(&mut self.rotate_delta);
            cmd.mouse_x = mx;
            cmd.mouse_y = my;
            cmd.wheel = std::mem::take(&mut self.wheel_delta);
            self.sim.tick(cmd);
            // automatic stools (`TOOL.LeftClickAutomatic` / `RightClickAutomatic`: leafblower,
            // inflator, paint in the game's sandbox) fire every tick while the button is held
            // (toolgun.md F9-F10, gap-report #90); the others only on the press edge
            if self.weapon == 1 && AUTOMATIC_STOOLS.contains(&self.tool.as_str()) {
                for (b, right) in [(0, false), (1, true)] {
                    if self.mouse.contains(&b) {
                        self.sim.client.command(Command::ToolClick {
                            tool: self.tool.clone(),
                            right,
                            reload: false,
                            settings: self.tool_settings_list(),
                        });
                    }
                }
            }
            let keys = std::mem::take(&mut self.pending_input);
            self.sim.script.client_input(&self.sim.client.world, &keys);
            self.acc -= dt;
            if !self.weapon_synced && self.sim.client.world.local.is_some() {
                self.weapon_synced = true;
                self.select_weapon(self.weapon);
            }
            if let Some(w) = self.pending_select.take() {
                self.select_weapon(w);
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
            let now = Instant::now();
            let due: Vec<String> = {
                let (ready, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.hint_queue)
                    .into_iter()
                    .partition(|(t, _)| *t <= now);
                self.hint_queue = later;
                ready.into_iter().map(|(_, n)| n).collect()
            };
            for name in due {
                let key = format!("hint_{name}").to_lowercase();
                let text = self
                    .gameui
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| format!("#Hint_{name}"));
                self.notify(text, NotifyKind::Hint, 6.0);
            }
            if self
                .screen_fade
                .as_ref()
                .is_some_and(|f| f.alpha(now).is_none())
            {
                self.screen_fade = None;
            }
            let events = std::mem::take(&mut self.sim.client.events);
            for e in events {
                if let Some(rest) = e.strip_prefix("Hurt ") {
                    let v: Vec<f32> = rest.split(' ').filter_map(|x| x.parse().ok()).collect();
                    if let (&[amount, x, y, z], Some(me)) = (v.as_slice(), self.local_snap()) {
                        if amount > 0.0 {
                            let o = me.origin;
                            self.damage_from(Vec3::new(x, y, z), o, amount);
                        }
                    }
                }
                if let Some(d) = DeathNotice::parse(&e, &self.sim.client.world.players) {
                    self.deaths.push(d);
                }
                // `Chat <player> <dead> <name>\x1f<text>`: the console gets `name: text`, the
                // chat box the Lua side (GM:OnPlayerChat -> chat.AddText)
                if let Some(rest) = e.strip_prefix("Print ") {
                    // TextMsg destinations (ui.md §12.4, §15; gap-report #164)
                    let (kind, text) = rest.split_once('\x1f').unwrap_or(("2", rest));
                    match kind {
                        "1" => self.notify(text.to_string(), NotifyKind::Hint, 5.0),
                        "3" | "5" => {
                            self.console.log(text.to_string());
                            let mut p = gm_script::api::Payload::default();
                            p.ints.push(("player", 0));
                            p.bools.push(("dead", false));
                            p.bools.push(("team", false));
                            p.strs.push(("name", String::new()));
                            p.strs.push(("text", text.to_string()));
                            self.sim
                                .script
                                .client_event(&self.sim.client.world, "chat.line", &p);
                        }
                        "4" => self.center_print = Some((text.to_string(), Instant::now())),
                        _ => self.console.log(text.to_string()),
                    }
                    continue;
                }
                if let Some(rest) = e.strip_prefix("Chat ") {
                    let mut it = rest.splitn(3, ' ');
                    let player: i64 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                    let dead = it.next() == Some("1");
                    let (name, text) = it
                        .next()
                        .and_then(|r| r.split_once('\x1f'))
                        .unwrap_or(("", rest));
                    self.console.log(format!("{name}: {text}"));
                    let mut p = gm_script::api::Payload::default();
                    p.ints.push(("player", player));
                    p.bools.push(("dead", dead));
                    p.bools.push(("team", false));
                    p.strs.push(("name", name.to_string()));
                    p.strs.push(("text", text.to_string()));
                    self.sim
                        .script
                        .client_event(&self.sim.client.world, "chat.line", &p);
                    continue;
                }
                // GM:OnUndo: hint.undoneX with `language.GetPhrase(name)` (prop_physics is
                // "Physics Prop", a `#class` its entities.properties name)
                let named = match e.strip_prefix("Undone ") {
                    Some(n) => {
                        let key = n.trim_start_matches('#');
                        let name = self
                            .lang
                            .get(key)
                            .cloned()
                            .unwrap_or_else(|| key.to_string());
                        format!("Undone {name}")
                    }
                    None => e.clone(),
                };
                if let Some((text, kind, length)) = Notices::legacy(&named) {
                    // sandbox cl_init `AddUndoNotify`: `buttons/button15.wav` with the notice
                    if matches!(kind, NotifyKind::Undo)
                        && !self
                            .sounds
                            .play_wave(&self.mount, "buttons/button15.wav", 1.0)
                    {
                        eprintln!("undo sound: buttons/button15.wav did not load");
                    }
                    self.notify(text, kind, length);
                }
                if let Some(rest) = e.strip_prefix("Hint ") {
                    let mut it = rest.split(' ');
                    let name = it.next().unwrap_or("").to_string();
                    let delay = it.next().and_then(|d| d.parse::<f32>().ok()).unwrap_or(0.0);
                    self.hint_queue.push((
                        Instant::now() + std::time::Duration::from_secs_f32(delay.max(0.0)),
                        name,
                    ));
                }
                if let Some(f) = e.strip_prefix("ScreenFade ").and_then(ScreenFade::parse) {
                    self.screen_fade = Some(f);
                }
                if let Some(rest) = e.strip_prefix("Shake ") {
                    let v: Vec<f32> = rest.split(' ').filter_map(|x| x.parse().ok()).collect();
                    if let [amplitude, duration, frequency] = v[..] {
                        self.shakes.push(Shake {
                            amplitude,
                            duration,
                            frequency: frequency.max(0.1),
                            start: Instant::now(),
                            next_at: 0.0,
                            offset: Vec3::ZERO,
                            roll: 0.0,
                        });
                    }
                }
                println!("{e}");
                self.console.log(e);
            }
            let now = Instant::now();
            for e in std::mem::take(&mut self.sim.client.effects) {
                match e {
                    gm_net::msg::EffectEvent::Impact {
                        pos,
                        normal,
                        surface,
                        ..
                    } => {
                        if self.fx_decals.len() >= DECAL_RING {
                            self.fx_decals.pop_front();
                        }
                        self.fx_decals.push_back((
                            pos,
                            normal,
                            impact_decal(
                                self.gamematerials
                                    .get(&surface.to_ascii_lowercase())
                                    .copied(),
                                &surface,
                                pos,
                            ),
                            now,
                        ));
                        // `AR2Impact` at every non-sky hit of a pulse-rifle shot (#103)
                        if self
                            .fx_tracers
                            .iter()
                            .any(|t| t.3 && t.1.distance(pos) < 8.0)
                        {
                            self.fx_ar2_impacts.push((pos + normal * 2.0, now));
                        }
                        self.spawn_impact(pos, normal, &surface, now);
                    }
                    gm_net::msg::EffectEvent::Tracer { player, end } => {
                        // the pulse rifle's shots draw `AR2Tracer` (#103)
                        let ar2 = if Some(player) == self.sim.client.player_id {
                            viewmodel(self.weapon).is_some_and(|v| v.class == "weapon_ar2")
                        } else {
                            let classes =
                                gm_net::msg::weapon_classes(&self.sim.client.world.weapon_defs);
                            self.sim
                                .client
                                .world
                                .players
                                .iter()
                                .find(|p| p.id == player)
                                .and_then(|p| classes.get(p.active as usize))
                                .is_some_and(|c| c == "weapon_ar2")
                        };
                        self.fx_tracers.push((player, end, now, ar2));
                        // `FX_MuzzleEffect` for every other shooter at their weapon's world
                        // model `muzzle` attachment (clientfx.md 2.1, gap-report #123); a
                        // point beside the eye when the model has none / was not drawn
                        let me = self.sim.client.player_id;
                        let shooter = self
                            .sim
                            .client
                            .world
                            .players
                            .iter()
                            .find(|p| p.id == player && Some(p.id) != me)
                            .map(|p| p.origin + Vec3::Z * 60.0);
                        if let Some(start) = shooter {
                            let fwd = (end - start).normalize_or_zero();
                            let right = fwd.cross(Vec3::Z).normalize_or_zero();
                            let muzzle = self
                                .other_muzzles
                                .get(&player)
                                .copied()
                                .unwrap_or(start + fwd * 16.0 + right * 6.0 - Vec3::Z * 4.0);
                            let fwd = (end - muzzle).normalize_or_zero();
                            for (pos, size, roll, variant) in
                                muzzle_sprites(&mut self.fx_seed, muzzle, fwd, 1.0)
                            {
                                self.fx_sprites.push(FxSprite {
                                    kind: FxKind::Muzzle,
                                    variant,
                                    pos,
                                    vel: Vec3::ZERO,
                                    born: now,
                                    life: 0.1,
                                    size0: size,
                                    size1: size,
                                    color: [1.0, 1.0, 1.0],
                                    roll,
                                    roll_d: 0.0,
                                    alpha: 1.0,
                                });
                            }
                        }
                    }
                    // the blast / beam at the gun are drawn from `fx_cannon`; `FX_GaussExplosion`
                    // at the punt point stands in as a spark burst
                    gm_net::msg::EffectEvent::PhyscannonImpact { player, pos } => {
                        self.fx_cannon.push((player, pos, now));
                        self.fx_bursts.push((pos, 5.0, now, false));
                    }
                    // `cball_bounce`: a short blue-white flare at the bounce
                    gm_net::msg::EffectEvent::BallBounce { pos } => {
                        self.fx_sprites.push(FxSprite {
                            kind: FxKind::Flare,
                            variant: 0,
                            pos,
                            vel: Vec3::ZERO,
                            born: now,
                            life: 0.15,
                            size0: 10.0,
                            size1: 10.0,
                            color: [0.6, 0.8, 1.0],
                            roll: 0.0,
                            roll_d: 0.0,
                            alpha: 1.0,
                        });
                    }
                    gm_net::msg::EffectEvent::BallExplosion { pos } => {
                        self.fx_rings.push((pos, now));
                    }
                    gm_net::msg::EffectEvent::ToolTracer {
                        player,
                        pos,
                        normal,
                    } => {
                        let seed = &mut self.fx_seed;
                        let speeds = std::array::from_fn(|_| particles::range(seed, 0.5, 1.5));
                        self.fx_tool.push((player, pos, normal, now, speeds));
                    }
                    gm_net::msg::EffectEvent::Explosion { pos } => {
                        self.spawn_explosion(pos, now);
                        self.fx_explosion_lights.push((pos, now));
                        // `CBaseGrenade::Explode`: a `Scorch` decal where the line from 8 u
                        // above the blast to 32 u below meets the world
                        let tr = self.sim.collision.trace_ray(
                            pos + Vec3::Z * 8.0,
                            pos - Vec3::Z * 32.0,
                            gm_collision::MASK_SOLID,
                        );
                        if tr.fraction < 1.0 {
                            if self.fx_decals.len() >= DECAL_RING {
                                self.fx_decals.pop_front();
                            }
                            let n = 1 + (pos.x.to_bits() ^ pos.y.to_bits()) % 2;
                            self.fx_decals.push_back((
                                tr.end,
                                tr.normal,
                                format!("decals/scorch{n}_subrect"),
                                now,
                            ));
                        }
                    }
                    gm_net::msg::EffectEvent::EntityRemove { mins, maxs } => {
                        // `entity_remove` (sandbox/entities/effects): BoundingRadius * 4
                        // sparks (32..256) at random points of the box, flung from its
                        // centre at 5x, 2 -> 0 u over 0.5..1 s under 700 u/s^2 gravity
                        if let Some(g) = self.gfx.as_mut() {
                            if !g.rope_mats.contains_key(REMOVE_SPARK) {
                                let m = g.world.sprite_material(&g.gpu, &self.mount, REMOVE_SPARK);
                                g.rope_mats.insert(REMOVE_SPARK.to_string(), m);
                            }
                            let center = (mins + maxs) * 0.5;
                            let radius = (maxs - mins).length() * 0.5;
                            let n = ((radius * 4.0) as usize).clamp(32, 256);
                            let r = &mut g.fx_rng;
                            for _ in 0..=n {
                                let p = Vec3::new(
                                    fx_range(r, mins.x, maxs.x),
                                    fx_range(r, mins.y, maxs.y),
                                    fx_range(r, mins.z, maxs.z),
                                );
                                g.thruster_fx.push(ThrusterParticle {
                                    mat: REMOVE_SPARK,
                                    additive: true,
                                    pos: p,
                                    vel: (p - center) * 5.0,
                                    grav: Vec3::new(0.0, 0.0, -700.0),
                                    born: now,
                                    life: fx_range(r, 0.5, 1.0),
                                    size: (2.0, 0.0),
                                    alpha: (fx_range(r, 200.0, 255.0) / 255.0, 0.0),
                                    color: [1.0, 1.0, 1.0],
                                    roll: fx_range(r, 0.0, 360.0),
                                    axis: None,
                                });
                            }
                        }
                    }
                    gm_net::msg::EffectEvent::Blood {
                        pos,
                        dir,
                        color,
                        decals,
                    } => {
                        // `UTIL_BloodImpact` dispatches `blood_impact_<colour>_01` there
                        if let Some(g) = self.gfx.as_mut() {
                            g.particles.ensure_loaded(&self.mount);
                            let name = match color {
                                1 => "blood_impact_yellow_01",
                                2 => "blood_impact_green_01",
                                _ => "blood_impact_red_01",
                            };
                            let angles = Vec3::new(
                                (-dir.z).asin().to_degrees(),
                                dir.y.atan2(dir.x).to_degrees(),
                                0.0,
                            );
                            g.particles.spawn(&particles::Spawn {
                                name: name.to_string(),
                                pos,
                                angles,
                                parent: 0,
                            });
                        }
                        // `UTIL_BloodDecalTrace`: the `Blood` / `YellowBlood` groups (green
                        // bleeds red decals as in the game)
                        for (p, n) in decals {
                            if self.fx_decals.len() >= DECAL_RING {
                                self.fx_decals.pop_front();
                            }
                            let k = 1 + (p.x.to_bits() ^ p.z.to_bits().rotate_left(7)) % 6;
                            let group = if color == 1 { "yblood" } else { "blood" };
                            self.fx_decals.push_back((
                                p,
                                n,
                                format!("decals/{group}{k}_subrect"),
                                now,
                            ));
                        }
                        // `UTIL_BloodDrips`: a short spray of drops carried on with the bullet
                        if let Some(g) = self.gfx.as_mut() {
                            if !g.rope_mats.contains_key(BLOOD_CORE) {
                                let m = g.world.sprite_material(&g.gpu, &self.mount, BLOOD_CORE);
                                g.rope_mats.insert(BLOOD_CORE.to_string(), m);
                            }
                            let tint = match color {
                                1 => [0.85, 0.75, 0.2],
                                2 => [0.3, 0.75, 0.2],
                                _ => [0.55, 0.02, 0.02],
                            };
                            let r = &mut g.fx_rng;
                            for _ in 0..6 {
                                let d = fx_vec_rand(r);
                                g.thruster_fx.push(ThrusterParticle {
                                    mat: BLOOD_CORE,
                                    additive: false,
                                    pos: pos + d * 2.0,
                                    vel: dir.normalize_or_zero() * 70.0 + d * 40.0,
                                    grav: Vec3::new(0.0, 0.0, -400.0),
                                    born: now,
                                    life: 0.35,
                                    size: (4.0, 12.0),
                                    alpha: (0.9, 0.0),
                                    color: tint,
                                    roll: d.x * 180.0,
                                    axis: None,
                                });
                            }
                        }
                    }
                    gm_net::msg::EffectEvent::Splash { pos } => {
                        // `FX_GunshotSplash` (clientfx.md §6.2, gap-report #130): the game
                        // has no water_splash .pcf (client.dll carries none), it is the
                        // legacy emitter set: 8 splash2 droplets, 8 splash1 mist puffs and a
                        // splashwake1 ripple for 1.5 s. scale 8 -> flScale 1.
                        if let Some(g) = self.gfx.as_mut() {
                            for name in [SPLASH_RIPPLE, SPLASH_DROP, SPLASH_MIST] {
                                if !g.rope_mats.contains_key(name) {
                                    let m = g.world.sprite_material(&g.gpu, &self.mount, name);
                                    g.rope_mats.insert(name.to_string(), m);
                                }
                            }
                            g.thruster_fx.push(ThrusterParticle {
                                mat: SPLASH_RIPPLE,
                                additive: false,
                                pos: pos + Vec3::Z * 0.5,
                                vel: Vec3::ZERO,
                                grav: Vec3::ZERO,
                                born: now,
                                life: 1.5,
                                size: (8.0, 56.0),
                                alpha: (0.6, 0.0),
                                color: [1.0; 3],
                                roll: 0.0,
                                axis: Some(Vec3::Z),
                            });
                            let r = &mut g.fx_rng;
                            let mut drops = Vec::with_capacity(16);
                            // droplets: dir = normal + rand(±0.8)^3, 150..300 u/s + z 32..64,
                            // gravity 800, 0.25..0.5 s, width 1..3 u, water colour ×0.75..1.25
                            for _ in 0..8 {
                                let d = (Vec3::Z + fx_vec_rand(r) * 0.8).normalize_or_zero();
                                let vel = d * fx_range(r, 150.0, 300.0)
                                    + Vec3::Z * fx_range(r, 32.0, 64.0);
                                let w = fx_range(r, 1.0, 3.0);
                                let c = fx_range(r, 0.75, 1.25).min(1.0);
                                let jitter = fx_vec_rand(r) * 8.0;
                                drops.push(ThrusterParticle {
                                    mat: SPLASH_DROP,
                                    additive: false,
                                    pos: pos + Vec3::new(jitter.x, jitter.y, 0.0),
                                    vel,
                                    grav: Vec3::new(0.0, 0.0, -800.0),
                                    born: now,
                                    life: fx_range(r, 0.25, 0.5),
                                    size: (w * 2.0, w * 2.0),
                                    alpha: (1.0, 0.0),
                                    color: [c; 3],
                                    roll: 0.0,
                                    axis: None,
                                });
                            }
                            // mist: 8 splash1 puffs, 2 s, speed 50·(8−i) along the normal
                            // with ±0.2 spread, size 24·remap(i, 7..0 -> 1..0.5) doubling
                            for i in 0..8 {
                                let d = (fx_vec_rand(r) * 0.2 + Vec3::Z * fx_range(r, 4.0, 6.0))
                                    .normalize_or_zero();
                                let start = 24.0 * (0.5 + 0.5 * i as f32 / 7.0);
                                let c = fx_range(r, 0.75, 1.25).min(1.0);
                                drops.push(ThrusterParticle {
                                    mat: SPLASH_MIST,
                                    additive: false,
                                    pos,
                                    vel: d * 50.0 * (8 - i) as f32,
                                    grav: Vec3::ZERO,
                                    born: now,
                                    life: 2.0,
                                    size: (start, start * 2.0),
                                    alpha: (1.0, 0.0),
                                    color: [c; 3],
                                    roll: fx_range(r, -4.0, 4.0),
                                    axis: None,
                                });
                            }
                            g.thruster_fx.extend(drops);
                        }
                    }
                    gm_net::msg::EffectEvent::PropImpact {
                        ent,
                        lpos,
                        lnormal,
                        surface,
                    } => {
                        if let Some(e) = self.sim.client.world.entities.get(&ent) {
                            let rot = gm_render::props::angle_matrix(e.angles.to_array());
                            let pos = e.pos + rot * lpos;
                            self.spawn_impact(pos, rot * lnormal, &surface, now);
                        }
                        if self.fx_prop_decals.len() >= 256 {
                            self.fx_prop_decals.pop_front();
                        }
                        self.fx_prop_decals.push_back((
                            ent,
                            lpos,
                            lnormal,
                            impact_decal(
                                self.gamematerials
                                    .get(&surface.to_ascii_lowercase())
                                    .copied(),
                                &surface,
                                lpos,
                            ),
                            now,
                        ));
                    }
                }
            }
            for s in std::mem::take(&mut self.sim.client.sounds) {
                if s.stop {
                    self.sounds.stop_entity(s.ent, s.fade);
                } else if s.change {
                    self.sounds.change_entity(
                        s.ent,
                        (s.channel != 0).then_some(s.channel),
                        s.volume,
                        s.pitch,
                        s.fade,
                    );
                } else {
                    self.sounds.play_at(
                        &self.mount,
                        &s.name,
                        Some(s.pos),
                        s.volume,
                        s.ent,
                        s.looping,
                        s.pitch,
                        s.level,
                    );
                }
            }
        }
    }

    fn render(&mut self) {
        self.sync_scripted_weapons();
        if self.wsel.is_some()
            && self
                .wsel_at
                .is_some_and(|t| t.elapsed().as_secs_f32() > 1.25)
        {
            self.wsel = None;
        }
        let muted = self.main_menu && !self.started;
        if muted != self.menu_muted {
            self.menu_muted = muted;
            self.sounds
                .mixer
                .set_master(if muted { 0.0 } else { self.settings.volume });
        }
        let alpha = self.acc / gm_core::tick::DEFAULT_TICK_INTERVAL;
        let dead = self.local_snap().is_some_and(|s| !s.alive);
        let vm = if dead { None } else { self.vm_of(self.weapon) };
        let vm_seq = vm.as_ref().map(|vm| self.vm_sequence(vm));
        // gravity gun claws (gravgun.md 3.2 / #18): the `active` pose parameter runs 0 -> 1 over
        // 0.2 s when the server opens them, 1 -> 0 over 0.5 s on close; the core sprite's opacity
        // follows the effect state (255 holding over 0.1 s, 128 ready over 0.2 s)
        {
            let now = Instant::now();
            let dt = now.duration_since(self.claw_at).as_secs_f32().min(0.1);
            self.claw_at = now;
            let me = self.sim.client.player_id;
            let mine = self
                .sim
                .client
                .world
                .players
                .iter()
                .find(|p| Some(p.id) == me);
            let open = mine.is_some_and(|p| p.claws);
            let holding = mine.is_some_and(|p| p.held_pos != Vec3::ZERO);
            self.claw_pose = if open {
                (self.claw_pose + dt / 0.2).min(1.0)
            } else {
                (self.claw_pose - dt / 0.5).max(0.0)
            };
            // effect state (gravgun.md 3.1 / 7.3, gap-report #88): NONE while the weapon is
            // away (StopEffects), CLOSED with the claws shut (sprites keep their values),
            // READY with them open, HOLDING with an object; the core follows READY (128 over
            // 0.2 s) / HOLDING (255 over 0.1 s) and is left alone in CLOSED
            self.gg_effect = if self.weapon != GRAVGUN_SLOT {
                0
            } else if holding {
                3
            } else if open {
                2
            } else {
                1
            };
            match self.gg_effect {
                0 => self.gg_core_alpha = 0.0,
                1 => {}
                st => {
                    let (target, time) = if st == 3 {
                        (1.0, 0.1)
                    } else {
                        (128.0 / 255.0, 0.2)
                    };
                    let step = dt / time;
                    self.gg_core_alpha += (target - self.gg_core_alpha).clamp(-step, step);
                }
            }
        }
        let vm_prev = self.vm_prev;
        // the viewmodel sequence's animation events as `C_BaseViewModel::FireEvent`: the
        // muzzle flash (AE_MUZZLEFLASH / 5001), brass out of attachment 2 (6001, the shell by
        // its option), a sound (AE_CL_PLAYSOUND / 5004); each fires once as the cycle passes
        if let (Some(vm), Some((seq, t))) = (vm.as_ref(), vm_seq) {
            let events = self
                .gfx
                .as_ref()
                .and_then(|g| g.models.get(vm.path))
                .and_then(|m| m.as_ref())
                .and_then(|m| m.sequence_events(seq));
            if let Some((len, events)) = events {
                let prev = match self.vm_event_t {
                    Some((s, p)) if s == seq && p <= t => Some(p),
                    _ => None,
                };
                self.vm_event_t = Some((seq, t));
                for (cycle, event, name, options) in &events {
                    let at = cycle * len;
                    let fired = match prev {
                        Some(p) => at > p && at <= t,
                        None => at <= t,
                    };
                    if !fired {
                        continue;
                    }
                    match (*event, name.as_str()) {
                        // the alt-fire sequences (the AR2's IR_fire2 launching the ball) carry
                        // no muzzle flash in the game: their event is skipped
                        (5001, _) | (_, "AE_MUZZLEFLASH")
                            if vm.alt.is_some_and(|alt| alt == seq) => {}
                        (5001, _) | (_, "AE_MUZZLEFLASH") => {
                            self.fx_muzzle_until =
                                Some(Instant::now() + std::time::Duration::from_millis(60));
                        }
                        (6001, _) => {
                            self.fx_brass_pending = Some(options.trim().parse().unwrap_or(0));
                        }
                        (5004, _) | (_, "AE_CL_PLAYSOUND") if !options.is_empty() => {
                            self.sounds.emit(&self.mount, options, None);
                        }
                        _ => {}
                    }
                }
            }
        }
        // shells fly a second under gravity, tumbling, and bounce off the world
        {
            let now = Instant::now();
            let dt = now.duration_since(self.fx_shells_at).as_secs_f32().min(0.1);
            self.fx_shells_at = now;
            self.tick_explosions(now, dt);
            self.fx_shells
                .retain(|s| now.duration_since(s.born).as_secs_f32() < 1.0);
            for sh in &mut self.fx_shells {
                sh.vel.z -= 800.0 * dt;
                let next = sh.pos + sh.vel * dt;
                let tr = self
                    .sim
                    .collision
                    .trace_ray(sh.pos, next, gm_collision::MASK_SOLID);
                if tr.fraction < 1.0 {
                    sh.pos = tr.end + tr.normal * 0.5;
                    sh.vel = (sh.vel - tr.normal * (2.0 * sh.vel.dot(tr.normal))) * 0.4;
                    sh.angvel *= 0.5;
                } else {
                    sh.pos = next;
                }
                sh.ang += sh.angvel * dt;
            }
            if !self.fx_shells.is_empty() {
                if let Some(g) = self.gfx.as_mut() {
                    for path in BRASS_MODELS {
                        if !g.models.contains_key(path) {
                            let m = g.world.load_model(&g.gpu, &self.mount, path);
                            g.models.insert(path.to_string(), m);
                        }
                    }
                }
            }
        }
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
                        ent,
                        open: Vec::new(),
                    });
                }
            }
        }
        let scripted_world = self.scripted_world.clone();
        let world_model_of = |active: u8| -> Option<&'static str> {
            world_model(active).or_else(|| {
                scripted_world
                    .get((active as usize).checked_sub(WEAPONS.len())?)
                    .copied()
            })
        };
        let fov_x = if self.weapon == CAMERA_SLOT {
            // gmod_camera `TranslateFOV`: its Zoom replaces the fov (toolgun.md F41)
            if self.cam_zoom <= 0.0 {
                self.cam_zoom = self.settings.fov_desired;
            }
            self.cam_zoom
        } else {
            self.camera_fov()
        };
        let Some(g) = &mut self.gfx else { return };
        // light styles 1..=12 animate at 10 Hz from the engine's pattern strings (render.md
        // §9, gap-report #150); the rest are the server's values
        let t = self.epoch.elapsed().as_secs_f32();
        let mut styles = self.sim.client.world.lightstyles.clone();
        for (style, b) in &mut styles {
            if let Some(v) = gm_render::lightmap::pattern_value(*style, t) {
                *b = v;
            }
        }
        for style in 1..=12u8 {
            if !styles.iter().any(|(s, _)| *s == style) {
                if let Some(v) = gm_render::lightmap::pattern_value(style, t) {
                    styles.push((style, v));
                }
            }
        }
        g.world.set_lightstyles(&g.gpu, &styles);
        for r in &self.sim.client.world.ropes {
            if !g.rope_mats.contains_key(&r.material) {
                let m = g.world.sprite_material(&g.gpu, &self.mount, &r.material);
                g.rope_mats.insert(r.material.clone(), m);
            }
        }
        for t in &self.sim.client.world.thrusters {
            let name = thruster_fx_material(&t.effect);
            if !name.is_empty() && !g.rope_mats.contains_key(name) {
                let m = g.world.sprite_material(&g.gpu, &self.mount, name);
                g.rope_mats.insert(name.to_string(), m);
            }
            if t.effect == "fire" && !g.rope_mats.contains_key(HEATWAVE) {
                let m = g.world.sprite_material(&g.gpu, &self.mount, HEATWAVE);
                g.rope_mats.insert(HEATWAVE.to_string(), m);
            }
        }
        if self.main_menu && !self.started {
            if let Some(name) = self.backgrounds.tick(&self.mount) {
                g.ui.load_image(&g.gpu, &self.mount, &name);
            }
        }
        if self.menu.is_some() {
            let visible = self.menu_cache.borrow().visible.clone();
            Self::render_spawn_icons(&visible, &self.mount, g);
        }
        if self.player_editor {
            let paths: Vec<String> = Self::player_model_list()
                .iter()
                .map(|(_, p)| (*p).to_string())
                .collect();
            Self::render_spawn_icons(&paths, &self.mount, g);
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
        // with bloom the scene goes offscreen and Post composites it onto the frame
        let bloom = g.world.hdr && self.settings.mat_bloomscale;
        let scene = if bloom {
            g.post.scene_view().clone()
        } else {
            view.clone()
        };
        // with MSAA the passes draw into the multisampled target and resolve into `scene`
        let attach = g.msaa_color.clone().unwrap_or_else(|| scene.clone());
        // the resolved scene the water copies for its refraction
        let scene_src: &wgpu::Texture = if bloom {
            g.post.scene_texture()
        } else {
            &frame.texture
        };
        let resolve = g.msaa_color.as_ref().map(|_| &scene);
        let o = self
            .sim
            .prev_origin
            .lerp(self.sim.origin, alpha.clamp(0.0, 1.0));
        // the eye follows the duck spline of the predicted state (`SetDuckedEyeOffset`)
        let eye = self
            .sim
            .pred
            .as_ref()
            .map_or(if self.sim.ducked { 28.0 } else { 64.0 }, |p| {
                gm_move::eye_height(&gm_move::MoveConfig::default(), &p.state)
            });
        // the view punch (landing roll, `move_roll_*`) and the `sv_rollangle` strafe roll
        let punch = self.sim.pred.as_ref().map_or(Vec3::ZERO, |p| {
            p.state.punch
                + Vec3::Z * gm_move::view_roll(&gm_move::MoveConfig::default(), &p.state, self.view)
        });
        let aspect = g.config.width as f32 / g.config.height as f32;
        let mut camera = Camera {
            position: o + Vec3::Z * eye,
            angles: self.view + punch,
            fov_x: scale_fov_by_width(fov_x, aspect),
            near: 3.0,
            far: 28000.0,
            gamma: self.settings.mat_monitorgamma,
        };
        let (shake_offset, shake_roll) = calc_shake(&mut self.shakes, &mut self.shake_seed);
        camera.position += shake_offset;
        camera.angles.z += shake_roll;
        if self.weapon == CAMERA_SLOT {
            camera.angles.z += self.cam_roll;
        }
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
        // dead: C_BasePlayer::CalcDeathCamView (Source SDK, srcds cannot show it) - the view
        // angles freeze at death, the eye starts at the ragdoll's `eyes` attachment (the root
        // body when the model has none) and over DEATH_ANIMTIME 1.5 s pulls back CHASE_CAM_DISTANCE_MAX
        // 96 u along -forward, a 12 u hull trace against the world keeping it out of walls
        let me_snap = self
            .sim
            .client
            .world
            .players
            .iter()
            .find(|p| Some(p.id) == self.sim.client.player_id)
            .map(|s| (s.alive, s.ragdoll, s.origin));
        match me_snap {
            Some((false, ragdoll, origin)) => {
                let at = *self.death_at.get_or_insert_with(|| {
                    self.death_view = self.view;
                    Instant::now()
                });
                let eye = self
                    .sim
                    .client
                    .world
                    .entities
                    .get(&ragdoll)
                    .map(|e| {
                        // the `eyes` attachment on its bone's ragdoll part; the root body
                        // when the model has none
                        let at_eyes = (|| {
                            let m = g.models.get(&e.model)?.as_ref()?;
                            let world =
                                ragdoll_bone_matrices(m, &e.bones, e.pos, e.angles, &e.manips)?;
                            Some(m.attachment("eyes", &world)?.w_axis.truncate())
                        })();
                        at_eyes.unwrap_or_else(|| e.bones.first().map_or(e.pos, |b| b.pos))
                    })
                    .unwrap_or(origin + Vec3::Z * 64.0);
                let t = (at.elapsed().as_secs_f32() / 1.5).clamp(0.0, 1.0);
                let (p, y) = (
                    self.death_view.x.to_radians(),
                    self.death_view.y.to_radians(),
                );
                let fwd = Vec3::new(p.cos() * y.cos(), p.cos() * y.sin(), -p.sin());
                let want = eye - fwd * (96.0 * t);
                let tr = self.sim.collision.trace_box(
                    eye,
                    want,
                    Vec3::splat(-6.0),
                    Vec3::splat(6.0),
                    gm_collision::MASK_SOLID,
                );
                camera.position = if tr.start_solid { eye } else { tr.end };
                camera.angles = self.death_view;
            }
            _ => {
                if self.death_at.take().is_some() {
                    // the respawn purges a stay-out fade and deploys the spawn weapon
                    if self.screen_fade.as_ref().is_some_and(|f| f.flags & 8 != 0) {
                        self.screen_fade = None;
                    }
                    self.pending_select = Some(PHYSGUN_SLOT);
                }
            }
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
        self.sounds
            .mixer
            .set_listener(gm_audio::Listener::from_view(
                camera.position,
                self.view.x,
                self.view.y,
            ));
        self.sounds.mixer.set_underwater(
            self.sim.collision.point_contents(camera.position)
                & gm_formats::bsp::lumps::contents::WATER
                != 0,
        );
        // `dsp_room` auto node (sound.md 3.2): every half second the room around the listener
        // is measured with 8 horizontal 4800 u traces and one up (sky), and the preset is
        // picked from its size and openness. The game's node table and `dsp_presets.txt`
        // processors are not reproduced: the decay / wet mapping here is an approximation
        // (outdoors nearly dry, small rooms short, halls up to 1.5 s).
        if self.room_at.elapsed().as_secs_f32() >= 0.5 {
            self.room_at = Instant::now();
            let eye = camera.position;
            let reach = 4800.0;
            let mut sum = 0.0;
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let d = Vec3::new(a.cos(), a.sin(), 0.0);
                let tr =
                    self.sim
                        .collision
                        .trace_ray(eye, eye + d * reach, gm_collision::MASK_SOLID);
                sum += tr.fraction * reach;
            }
            let size = sum / 8.0;
            let up =
                self.sim
                    .collision
                    .trace_ray(eye, eye + Vec3::Z * reach, gm_collision::MASK_SOLID);
            let height = up.fraction * reach;
            let open = up.fraction >= 1.0 || height > 2000.0;
            let room = if open && size > 1200.0 {
                gm_audio::Room {
                    reverb_times: [0.35, 0.3, 0.25],
                    wet: 0.12,
                }
            } else {
                let rt =
                    (0.35 + (size / 1600.0 + height.min(1200.0) / 1200.0) * 0.9).clamp(0.35, 1.5);
                gm_audio::Room {
                    reverb_times: [rt * 1.1, rt, rt * 0.8],
                    wet: (0.25 + size / 4000.0).clamp(0.25, 0.5),
                }
            };
            self.sounds.mixer.set_room(Some(room));
        }
        {
            let collision = self.sim.collision;
            let sees = |from: Vec3, to: Vec3| {
                collision
                    .trace_ray(from, to, gm_collision::MASK_SOLID)
                    .fraction
                    >= 1.0
            };
            self.soundscapes
                .update(&mut self.sounds, &self.mount, camera.position, &sees);
        }
        let aspect = g.config.width as f32 / g.config.height as f32;
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
            // env_sprite entities carry a sprite material, not a model; a model-less entity
            // (a point entity, an NPC body without a mesh) has nothing to load
            if model.is_empty() || is_sprite_ent(model) {
                continue;
            }
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
        let mut flashlight_attach: HashMap<u32, Vec3> = HashMap::new();
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
        self.sounds.pump(&self.mount);
        // looping entity sounds (thrusters) ride along with their interpolated entity
        for id in self.sounds.following() {
            if let Some(p) = self.sim.interp.sample(id, rt) {
                self.sounds.follow(id, p.pos);
            }
        }
        // the props are acoustic occluders: their hulls at their interpolated poses
        {
            let mut occluders = Vec::new();
            for e in self.sim.client.world.entities.values() {
                if e.model.is_empty() {
                    continue;
                }
                let mount = &self.mount;
                let hull = self
                    .occluder_hulls
                    .entry(e.model.clone())
                    .or_insert_with(|| {
                        let phy = e.model.strip_suffix(".mdl").map(|m| format!("{m}.phy"))?;
                        let phy = gm_formats::phy::Phy::parse(&mount.read(&phy).ok()?).ok()?;
                        let prop = phy
                            .props
                            .first()
                            .map(|p| p.surfaceprop.to_ascii_lowercase())
                            .unwrap_or_default();
                        let g = gm_audio::Geometry::from_phy(&phy, &prop);
                        (!g.triangles.is_empty()).then(|| {
                            (
                                std::sync::Arc::from(e.model.as_str()),
                                std::sync::Arc::new(g),
                            )
                        })
                    });
                let Some((key, hull)) = hull else {
                    continue;
                };
                let (pos, ang) = match self.sim.interp.sample(e.id, rt) {
                    Some(p) => (p.pos, p.angles),
                    None => (e.pos, e.angles),
                };
                occluders.push(gm_audio::Occluder {
                    ent: e.id,
                    key: key.clone(),
                    hull: hull.clone(),
                    transform: gm_render::world::model_transform(pos, ang),
                });
            }
            self.sounds.mixer.set_occluders(occluders);
        }
        // voice chat: our microphone out, the others' frames in at their mouths
        {
            let own = self.sim.client.player_id;
            let speakers: Vec<voice::Speaker> = self
                .sim
                .client
                .world
                .players
                .iter()
                .filter(|p| Some(p.id) != own)
                .map(|p| voice::Speaker {
                    id: p.id,
                    mouth: p.origin + Vec3::Z * if p.ducked { 28.0 } else { 64.0 },
                })
                .collect();
            let received = std::mem::take(&mut self.sim.client.voice);
            let now = Instant::now();
            for (pid, _, _) in &received {
                self.voice_seen.insert(*pid, now);
            }
            let packets = self.voice.update(
                &mut self.sounds,
                received,
                &speakers,
                own,
                self.settings.voice_modenable,
                self.settings.voice_scale,
                self.settings.voice_loopback,
            );
            self.sim.client.pending_voice.extend(packets);
        }
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
            Vec3,
        );
        let mut player_poses: Vec<PlayerPose<'_>> = Vec::new();
        // prop_ragdoll: skinning from the networked physics objects (world space, so the draw
        // transform is the identity), bone manipulations on top
        let mut ent_bones: std::collections::HashMap<u32, particles::EntBones> =
            std::collections::HashMap::new();
        let rag_poses: Vec<(u32, Vec<glam::Mat4>)> = ents
            .values()
            .filter(|e| !e.bones.is_empty() || !e.sequence.is_empty())
            .filter_map(|e| {
                let p = self.sim.interp.sample(e.id, rt);
                let (pos, ang) = p.as_ref().map_or((e.pos, e.angles), |p| (p.pos, p.angles));
                if e.bones.is_empty() {
                    // an NPC plays its sequence in place (the server's idle), looping on the
                    // render clock; the bones come out in model space, so place them
                    let m = g
                        .models
                        .get(&format!("{}#{}#{}", e.model, e.skin, e.body))
                        .or_else(|| g.models.get(&e.model))?
                        .as_ref()?;
                    // the server animator's clock: its cycle at the snapshot, run on at its
                    // rate for the render time since; the gesture layer at its cycle
                    let since = (rt - self.sim.client.world.tick as f32).max(0.0)
                        * gm_core::tick::DEFAULT_TICK_INTERVAL;
                    let (len, looping) = m.sequence_length(&e.sequence)?;
                    let t = e.cycle * len + e.playback_rate * since;
                    let t = if looping {
                        t.rem_euclid(len.max(1e-3))
                    } else {
                        t.min(len)
                    };
                    let gesture = e.gesture.as_ref().and_then(|g| {
                        let (glen, _) = m.sequence_length(&g.sequence)?;
                        Some((g.sequence.as_str(), g.cycle * glen))
                    });
                    let params: Vec<(&str, f32)> =
                        e.pose.iter().map(|(n, v)| (n.as_str(), *v)).collect();
                    let local = m.bone_world_gesture(&e.sequence, t, None, &params, gesture)?;
                    let place = gm_render::world::model_transform(pos, ang)
                        * glam::Mat4::from_scale(Vec3::splat(e.model_scale.max(0.01)));
                    let world: Vec<glam::Mat4> = local.iter().map(|b| place * *b).collect();
                    ent_bones.insert(e.id, (world.clone(), m.hitboxes().to_vec()));
                    return Some((e.id, m.skin_matrices(&world)));
                }
                let m = g.models.get(&e.model)?.as_ref()?;
                let parts = p.as_ref().map_or(&e.bones, |p| &p.bones);
                let world = ragdoll_bone_matrices(m, parts, pos, ang, &e.manips)?;
                ent_bones.insert(e.id, (world.clone(), m.hitboxes().to_vec()));
                Some((e.id, m.skin_matrices(&world)))
            })
            .collect();
        let classes = &self.sim.client.world.classes;
        let is_ball = |id: u32| classes.iter().any(|(i, c)| *i == id && c == "sent_ball");
        let mut items: Vec<_> = ents
            .values()
            .filter_map(|e| {
                if is_ball(e.id) {
                    return None;
                }
                let (pos, ang) = match self.sim.interp.sample(e.id, rt) {
                    Some(p) => (p.pos, p.angles),
                    None => (e.pos, e.angles),
                };
                let model = if e.skin == 0 && e.body.is_empty() {
                    g.models.get(&e.model)?.as_ref()?
                } else {
                    g.models
                        .get(&format!("{}#{}#{}", e.model, e.skin, e.body))?
                        .as_ref()?
                };
                if let Some((_, skin)) = rag_poses.iter().find(|(id, _)| *id == e.id) {
                    return Some(DrawItem {
                        tint: Vec3::ONE,
                        model,
                        transform: glam::Mat4::IDENTITY,
                        bones: Some(skin.as_slice()),
                    });
                }
                Some(DrawItem {
                    tint: Vec3::ONE,
                    model,
                    transform: gm_render::world::model_transform(pos, ang)
                        * glam::Mat4::from_scale(Vec3::splat(e.model_scale.max(0.01))),
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
                    // a script gesture from the server wins over the weapon one-shot
                    let gesture = p
                        .gesture
                        .as_ref()
                        .and_then(|g| {
                            let (glen, _) = m.sequence_length(&g.sequence)?;
                            Some((g.sequence.as_str(), g.cycle * glen))
                        })
                        .or_else(|| {
                            gestures.get(&p.id).and_then(|(g, since, _)| {
                                let t = since.elapsed().as_secs_f32();
                                let (len, _) = m.sequence_length(g)?;
                                (t < len).then_some((*g, t))
                            })
                        });
                    let world = m.bone_world_gesture(&seq, t, None, &params, gesture);
                    let pose = world.as_ref().map(|w| m.skin_matrices(w));
                    let (origin, yaw) = match self.sim.player_interp.sample(p.id, rt) {
                        Some(s) => (s.pos, s.angles.y),
                        None => (p.origin, p.view_angles.y),
                    };
                    // other players' flashlights shine from `gmod_flashlight`, else
                    // `anim_attachment_RH` (render.md 3.5, gap-report #139)
                    if p.flashlight {
                        if let Some(at) = world.as_ref().and_then(|w| {
                            m.attachment("gmod_flashlight", w)
                                .or_else(|| m.attachment("anim_attachment_RH", w))
                        }) {
                            let to_world = glam::Mat4::from_rotation_translation(
                                glam::Quat::from_rotation_z(yaw.to_radians()),
                                origin,
                            );
                            flashlight_attach.insert(p.id, (to_world * at).w_axis.truncate());
                        }
                    }
                    // the weapon's world model rides the right hand (bone merge by name)
                    let held = world.as_ref().and_then(|w| {
                        let path = world_model_of(p.active)?;
                        let wm = g.models.get(path)?.as_ref()?;
                        let (bones, skin) = wm.bone_world_merged("idle", 0.0, m, w)?;
                        match wm.attachment("muzzle", &bones) {
                            Some(at) => {
                                let to_world = gm_render::world::model_transform(
                                    origin,
                                    Vec3::new(0.0, yaw, 0.0),
                                );
                                self.other_muzzles
                                    .insert(p.id, (to_world * at).w_axis.truncate());
                            }
                            None => {
                                self.other_muzzles.remove(&p.id);
                            }
                        }
                        Some((path, skin))
                    });
                    let tint = Vec3::from(p.player_color.map(|c| f32::from(c) / 255.0));
                    player_poses.push((origin, yaw, pose, held, m, tint));
                }
                None => {
                    let h = if p.ducked { 36.0 } else { 72.0 };
                    items.push(DrawItem {
                        tint: Vec3::ONE,
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
            for (origin, yaw, pose, held, m, tint) in &player_poses {
                let transform =
                    gm_render::world::model_transform(*origin, Vec3::new(0.0, *yaw, 0.0));
                items.push(DrawItem {
                    tint: *tint,
                    model: m,
                    transform,
                    bones: pose.as_deref(),
                });
                if let Some((path, bones)) = held {
                    if let Some(wm) = g.models.get(*path).and_then(|w| w.as_ref()) {
                        items.push(DrawItem {
                            tint: Vec3::ONE,
                            model: wm,
                            transform,
                            bones: Some(bones),
                        });
                    }
                }
            }
        }
        for sh in &self.fx_shells {
            if let Some(m) = g.models.get(sh.model).and_then(|m| m.as_ref()) {
                items.push(DrawItem {
                    tint: Vec3::ONE,
                    model: m,
                    transform: gm_render::world::model_transform(sh.pos, sh.ang),
                    bones: None,
                });
            }
        }
        // physgun beam: from the viewmodel's `muzzle` attachment to the held prop, or along the
        // aim; leaves along the barrel and bends to the target (the game's curved beam), drawn
        // additive with the game's `sprites/physbeam`, a glow at each end
        let mut additive: Vec<(&wgpu::BindGroup, Vec<gm_render::world::Vertex>, Vec<u32>)> =
            Vec::new();
        let mut refract: Vec<(&wgpu::BindGroup, Vec<gm_render::world::Vertex>, Vec<u32>)> =
            Vec::new();
        let mut alpha: Vec<(&wgpu::BindGroup, Vec<gm_render::world::Vertex>, Vec<u32>)> =
            Vec::new();
        let mut dlights: Vec<gm_render::world::DLight> = Vec::new();
        // Lua DynamicLights (render.md §4.1, gap-report #146-#147): radius -= Decay * dt
        // since the last update, gone at DieTime; nomodel/noworld are carried but the
        // renderer lights both (open)
        {
            let now = Instant::now();
            self.lua_dlights
                .retain(|_, (_, _, die, _, _, _)| *die > now);
            for (l, decay, _, since, _, _) in self.lua_dlights.values() {
                let r = l.radius - decay * now.duration_since(*since).as_secs_f32();
                if r > 0.0 {
                    dlights.push(gm_render::world::DLight { radius: r, ..*l });
                }
            }
        }
        // gmod_light (light stool): a DynamicLight of the tool's colour on the tube model
        // (light.lua: Size 256, Brightness 2 by default; the colour rides the entity colour)
        for e in self.sim.client.world.entities.values() {
            if e.model
                .eq_ignore_ascii_case("models/maxofs2d/light_tubular.mdl")
            {
                let c = Vec3::new(e.color[0] as f32, e.color[1] as f32, e.color[2] as f32) / 255.0;
                dlights.push(gm_render::world::DLight {
                    pos: e.pos,
                    radius: 256.0,
                    color: c * 2.0,
                    ..Default::default()
                });
            }
        }
        let mut modulate: Vec<(&wgpu::BindGroup, Vec<gm_render::world::Vertex>, Vec<u32>)> =
            Vec::new();
        // a viewmodel attachment in world space (the viewmodel sits at the eye)
        // a viewmodel attachment's position and forward (x axis) in world space
        let vm_attach_dir = |name: &str| -> Option<(Vec3, Vec3)> {
            let (Some(vm), Some((seq, t))) = (vm, vm_seq) else {
                return None;
            };
            let m = g.models.get(vm.path)?.as_ref()?;
            let w = m.bone_world_transition(
                seq,
                t,
                &[("active", self.claw_pose)],
                Self::vm_transition(vm_prev, |s| m.sequence_fade_out(s)),
            )?;
            let a = m.attachment(name, &w)?;
            let mt = gm_render::world::model_transform(camera.position, self.view);
            let p = mt.transform_point3(a.w_axis.truncate());
            let f = mt.transform_vector3(a.x_axis.truncate());
            // the viewmodel is drawn with its own FOV: put the point where it appears on
            // screen under the world camera (same clip position, world projection)
            let aspect = g.config.width as f32 / g.config.height as f32;
            let vm_fov = scale_fov_by_width(vm.fov, aspect);
            let k = (camera.fov_x.to_radians() * 0.5).tan() / (vm_fov.to_radians() * 0.5).tan();
            let view = glam::Mat4::look_to_rh(camera.position, camera.forward(), Vec3::Z);
            let mut v = view.transform_point3(p);
            v.x *= k;
            v.y *= k;
            let mut d = view.transform_vector3(f);
            d.x *= k;
            d.y *= k;
            Some((
                view.inverse().transform_point3(v),
                view.inverse().transform_vector3(d).normalize_or_zero(),
            ))
        };
        let vm_attach = |name: &str| -> Option<Vec3> { vm_attach_dir(name).map(|(p, _)| p) };
        let held_pos = self
            .sim
            .client
            .world
            .players
            .iter()
            .find(|p| Some(p.id) == me)
            .filter(|p| p.held.is_some())
            .map(|p| p.held_pos);
        let mut beams: Vec<(Vec3, Vec3, Vec3, bool)> = Vec::new();
        if self.weapon == 0 && self.mouse.contains(&0) {
            let fwd = camera.forward();
            let right = fwd.cross(Vec3::Z).normalize_or_zero();
            let (start, _start_dir) = vm_attach_dir("muzzle").unwrap_or((
                camera.position + right * 6.0 - Vec3::Z * 6.0 + fwd * 12.0,
                fwd,
            ));
            // `cl_weaponcolor` run through the game's clamping helper
            let wc = gm_render::world::DEFAULT_WEAPON_COLOR.clamp(Vec3::ZERO, Vec3::ONE);
            let color = wc.to_array();
            // muzzle glow in both states (physg_glow1 / physg_glow2; the game's helper is not
            // in the dump, sizes are ours)
            let muzzle_glow = if held_pos.is_some() {
                &g.glow1
            } else {
                &g.glow2
            };
            if let Some(gl) = muzzle_glow {
                let (v, i) = g.world.sprite_quad(start, 6.0, color, &camera);
                additive.push((gl, v, i));
            }
            dlights.push(gm_render::world::DLight {
                pos: start,
                radius: 48.0,
                color: wc * 0.5,
                ..Default::default()
            });
            // the beam only while something is held (binary study physgun.md §10): three
            // quadratic Beziers start -> control -> end with control = start + aim * |start - end|
            // * K2 (K2 unresolved, 0.5), widths 2 / rand 2..5 / rand 2..6 re-rolled every frame,
            // `sprites/physbeama`, and 7 `sprites/physbeam` sprites of rand 1..16 u, alpha
            // rand 100..255, at the end
            // held: to the grab point; trigger down on nothing: to where the aim hits (the
            // game's beam shows for the whole press, as the player sees it)
            let end = held_pos.unwrap_or_else(|| {
                self.sim
                    .collision
                    .trace_ray(
                        camera.position,
                        camera.position + fwd * 8192.0,
                        gm_collision::MASK_SOLID,
                    )
                    .end
            });
            beams.push((start, fwd, end, held_pos.is_some()));
        }
        // every other player's beam (physgun.md 10: drawn for all players): from beside their
        // eye to the networked grab point (no world-model physgun attachment yet)
        for p in &self.sim.client.world.players {
            if Some(p.id) == me || p.held.is_none() || !p.alive {
                continue;
            }
            let (sp, cp) = p.view_angles.x.to_radians().sin_cos();
            let (sy, cy) = p.view_angles.y.to_radians().sin_cos();
            let f = Vec3::new(cp * cy, cp * sy, -sp);
            let r = Vec3::new(sy, -cy, 0.0);
            let eye = p.origin + Vec3::Z * if p.ducked { 28.0 } else { 64.0 };
            beams.push((
                eye + r * 8.0 - Vec3::Z * 8.0 + f * 16.0,
                f,
                p.held_pos,
                true,
            ));
        }
        for (start, fwd, end, holding) in beams {
            let wc = gm_render::world::DEFAULT_WEAPON_COLOR.clamp(Vec3::ZERO, Vec3::ONE);
            let color = wc.to_array();
            {
                let len = (end - start).length();
                if len > 1.0 {
                    dlights.push(gm_render::world::DLight {
                        pos: end,
                        radius: 160.0,
                        color: wc,
                        ..Default::default()
                    });
                    let (p0, p1, p2) = (start, start + fwd * (len * 0.5), end);
                    let n = 24;
                    let points: Vec<Vec3> = (0..=n)
                        .map(|i| {
                            let t = i as f32 / n as f32;
                            let u = 1.0 - t;
                            p0 * (u * u) + p1 * (2.0 * u * t) + p2 * (t * t)
                        })
                        .collect();
                    let reversed: Vec<Vec3> = points.iter().rev().copied().collect();
                    let t = self.fx_epoch.elapsed().as_secs_f32();
                    let seed = &mut self.fx_seed;
                    let widths = [
                        (2.0, -t * 2.0, false),
                        (particles::range(seed, 2.0, 5.0), -t * 3.0, true),
                        (particles::range(seed, 2.0, 6.0), -t * 1.5, true),
                    ];
                    // `sprites/physbeama` while something is held, `sprites/physbeam` else
                    let mat = if holding {
                        g.beam_active_mat.as_ref().or(g.beam_mat.as_ref())
                    } else {
                        g.beam_mat.as_ref()
                    };
                    if let Some(bm) = mat {
                        for (w, scroll, rev) in widths {
                            let pts = if rev { &reversed } else { &points };
                            let (v, i) = g.world.beam_strip(
                                pts,
                                w,
                                color,
                                camera.position,
                                scroll,
                                1.0 / 128.0,
                            );
                            additive.push((bm, v, i));
                        }
                    }
                    if let Some(bm) = &g.beam_mat {
                        for _ in 0..if holding { 7 } else { 2 } {
                            let size = particles::range(seed, 1.0, 16.0);
                            let a = particles::range(seed, 100.0, 255.0) / 255.0;
                            let (v, i) = g.world.sprite_quad_var(
                                end,
                                size,
                                [color[0], color[1], color[2], a],
                                particles::range(seed, 0.0, std::f32::consts::TAU),
                                None,
                                &camera,
                            );
                            additive.push((bm, v, i));
                        }
                    }
                }
            }
        }
        // gmod_thruster effects (`ThrusterEffects`, gmod_thruster.lua): beams from GetOffset()
        // along its direction from the origin, scaling in over 0.2 s from `OnStart`
        {
            let now = std::time::Instant::now();
            let on: std::collections::HashSet<u32> = self
                .sim
                .client
                .world
                .thrusters
                .iter()
                .map(|t| t.ent)
                .collect();
            g.thruster_on.retain(|id, _| on.contains(id));
            for t in &self.sim.client.world.thrusters {
                let Some(e) = ents.get(&t.ent) else {
                    continue;
                };
                let name = thruster_fx_material(&t.effect);
                let Some(bm) = g.rope_mats.get(name).and_then(|m| m.as_ref()) else {
                    continue;
                };
                let t0 = *g.thruster_on.entry(t.ent).or_insert(now);
                let age = now.duration_since(t0).as_secs_f32();
                let scale = (age * 5.0).clamp(0.0, 1.0);
                let off = e.pos + gm_render::props::angle_matrix(e.angles.to_array()) * t.offset;
                let n = (off - e.pos).normalize_or_zero();
                let obb = e.maxs - e.mins;
                let size = if obb.x > 0.0 && obb.y > 0.0 {
                    obb.x.min(obb.y).min(50.0)
                } else {
                    32.0
                };
                let seed = (t.ent % 97) as f32;
                let eye = camera.position;
                match t.effect.as_str() {
                    "fire" => {
                        let blue = [0.0, 0.0, 1.0, 0.5];
                        let white = [1.0, 1.0, 1.0, 0.5];
                        let clear = [1.0, 1.0, 1.0, 0.0];
                        let scroll = (seed - age * 10.0) % 1000.0;
                        for (w, s) in [(32.0, scroll), (16.0, scroll * 1.3)] {
                            let (v, i) = g.world.beam_strip_var(
                                &[
                                    (off, size * scale, blue, s),
                                    (off + n * 60.0 * scale, w * scale, white, s + 1.0),
                                    (off + n * 148.0 * scale, w * scale, clear, s + 3.0),
                                ],
                                eye,
                            );
                            additive.push((bm, v, i));
                        }
                        // the flame's heat-wave: sprites/heatwave over the scene behind it
                        if let Some(hm) = g.rope_mats.get(HEATWAVE).and_then(|m| m.as_ref()) {
                            let s = scroll * 0.5;
                            let (v, i) = g.world.beam_strip_var(
                                &[
                                    (off, size * scale, [0.0, 0.0, 1.0, 0.5], s),
                                    (off + n * 32.0 * scale, 32.0 * scale, [1.0; 4], s + 2.0),
                                    (off + n * 128.0 * scale, 48.0 * scale, [0.0; 4], s + 5.0),
                                ],
                                eye,
                            );
                            refract.push((hm, v, i));
                        }
                    }
                    "magic" | "rings" | "smoke" => {
                        let interval = if t.effect == "smoke" { 0.015 } else { 0.01 };
                        let next = g.thruster_emit.entry(t.ent).or_insert(now);
                        let mut burst = 0;
                        while *next <= now && burst < 8 {
                            *next += std::time::Duration::from_secs_f32(interval);
                            burst += 1;
                            let r = &mut g.fx_rng;
                            let p = match t.effect.as_str() {
                                "magic" => ThrusterParticle {
                                    mat: name,
                                    additive: true,
                                    pos: off + fx_vec_rand(r) * (size / 3.0),
                                    vel: n * fx_range(r, 80.0, 160.0),
                                    grav: Vec3::ZERO,
                                    born: now,
                                    life: 0.5,
                                    size: (fx_range(r, 1.0, 3.0), 0.0),
                                    alpha: (1.0, 1.0),
                                    color: [1.0; 3],
                                    roll: fx_range(r, -0.2, 0.2),
                                    axis: None,
                                },
                                "rings" => ThrusterParticle {
                                    mat: name,
                                    additive: true,
                                    pos: off + n * 5.0,
                                    vel: n * 300.0,
                                    grav: Vec3::ZERO,
                                    born: now,
                                    life: 0.2,
                                    size: (size / 2.0, 10.0),
                                    alpha: (1.0, 0.0),
                                    color: [
                                        fx_range(r, 10.0, 100.0) / 255.0,
                                        fx_range(r, 100.0, 220.0) / 255.0,
                                        fx_range(r, 240.0, 255.0) / 255.0,
                                    ],
                                    roll: 0.0,
                                    axis: Some(n),
                                },
                                _ => {
                                    // the normal from a point 32 u out, jittered by 3 u; a
                                    // thruster moving faster than 200 u/s slows its smoke
                                    let nr = (off + n * 32.0 + fx_vec_rand(r) * 3.0 - e.pos)
                                        .normalize_or_zero();
                                    let speed = fx_range(r, 10.0, 30.0) * 10.0
                                        / (e.vel.length() / 200.0).clamp(1.0, 10.0);
                                    ThrusterParticle {
                                        mat: name,
                                        additive: false,
                                        pos: off + fx_vec_rand(r) * 3.0,
                                        vel: nr * speed,
                                        grav: Vec3::new(0.0, 0.0, 32.0),
                                        born: now,
                                        life: 2.0,
                                        size: (size / 2.0, fx_range(r, 64.0, 128.0)),
                                        alpha: (fx_range(r, 50.0, 150.0) / 255.0, 0.0),
                                        color: [200.0 / 255.0, 200.0 / 255.0, 210.0 / 255.0],
                                        roll: fx_range(r, -0.2, 0.2),
                                        axis: None,
                                    }
                                }
                            };
                            g.thruster_fx.push(p);
                        }
                        if *next < now {
                            *next = now;
                        }
                    }
                    "plasma" => {
                        let cyan = [0.0, 1.0, 1.0, 1.0];
                        let white = [1.0, 1.0, 1.0, 1.0];
                        let clear = [0.0, 1.0, 1.0, 0.0];
                        let size = obb.x.min(obb.y).max(1.0) * 1.5;
                        let mut scroll = (seed - age * 20.0) % 1000.0;
                        for _ in 0..3 {
                            scroll *= 0.9;
                            let (v, i) = g.world.beam_strip_var(
                                &[
                                    (off, size, cyan, scroll),
                                    (off + n * 8.0, size, white, scroll + 0.01),
                                    (off + n * 64.0, size, clear, scroll + 0.02),
                                ],
                                eye,
                            );
                            additive.push((bm, v, i));
                        }
                    }
                    _ => {}
                }
            }
            g.thruster_emit.retain(|id, _| on.contains(id));
            g.thruster_fx
                .retain(|p| now.duration_since(p.born).as_secs_f32() < p.life);
            for p in &g.thruster_fx {
                let Some(bm) = g.rope_mats.get(p.mat).and_then(|m| m.as_ref()) else {
                    continue;
                };
                let age = now.duration_since(p.born).as_secs_f32();
                let f = age / p.life;
                let pos = p.pos + p.vel * age + p.grav * (0.5 * age * age);
                let size = p.size.0 + (p.size.1 - p.size.0) * f;
                let a = p.alpha.0 + (p.alpha.1 - p.alpha.0) * f;
                let rgba = if p.additive {
                    [p.color[0] * a, p.color[1] * a, p.color[2] * a, 0.0]
                } else {
                    [p.color[0], p.color[1], p.color[2], a.max(1e-3)]
                };
                let (v, i) = g
                    .world
                    .sprite_quad_var(pos, size, rgba, p.roll, p.axis, &camera);
                if p.additive {
                    additive.push((bm, v, i));
                } else {
                    alpha.push((bm, v, i));
                }
            }
        }
        // `RocketTrail` on every RPG missile (`CMissile::CreateSmokeTrail`, clientfx.md 7.2,
        // gap-report #132): the game has no Rocket_Smoke_Trail .pcf (client.dll carries
        // none); numPuffs = moved / (startSize/2) smoke puffs per frame plus the 8-sprite
        // muzzle flare at the nozzle
        let now = std::time::Instant::now();
        g.missile_trails.retain(|id, _| ents.contains_key(id));
        let mut puffs = Vec::new();
        for e in ents.values().filter(|e| e.model.contains("w_missile")) {
            let Some(last) = g.missile_trails.insert(e.id, e.pos) else {
                continue;
            };
            let r = &mut g.fx_rng;
            let moved = e.pos - last;
            let n = (moved.length() / 17.5) as usize;
            for i in 0..n {
                let jitter = fx_vec_rand(r) * 2.0;
                let kind = if fx_rand(r) < 0.5 {
                    FxKind::TrailSmoke
                } else {
                    FxKind::TrailNoise
                };
                let c = 0.5 * fx_range(r, 0.75, 1.25);
                puffs.push(FxSprite {
                    kind,
                    variant: 0,
                    pos: last + moved * (i as f32 / n as f32) + jitter,
                    vel: fx_vec_rand(r) * fx_range(r, 2.0, 4.0),
                    born: now,
                    life: 5.0 + fx_range(r, 4.5, 5.5),
                    size0: 35.0 * fx_range(r, 0.75, 1.25),
                    size1: 55.0 * fx_range(r, 1.0, 1.25),
                    color: [c; 3],
                    roll: fx_range(r, 0.0, 360.0),
                    roll_d: fx_range(r, -8.0, 8.0),
                    alpha: 0.5 * fx_range(r, 0.75, 1.25),
                });
            }
            // nozzle flare: muzzleflash1..4 sprites behind the missile, 0.01 s (the
            // attachment is the model's rear; flareScale 1 -> rand(0.5,1.5))
            let fwd = gm_render::props::angle_matrix(e.angles.to_array()) * Vec3::X;
            let scale = fx_range(r, 0.5, 1.5);
            for (pos, size, roll, variant) in muzzle_sprites(r, e.pos - fwd * 6.0, -fwd, scale) {
                puffs.push(FxSprite {
                    kind: FxKind::Muzzle,
                    variant,
                    pos,
                    vel: Vec3::ZERO,
                    born: now,
                    life: 0.01,
                    size0: size,
                    size1: size,
                    color: [1.0; 3],
                    roll,
                    roll_d: 0.0,
                    alpha: 1.0,
                });
            }
        }
        self.fx_sprites.extend(puffs);
        if !g.particles.is_empty() {
            let dt = g
                .particles_tick
                .map(|t| now.duration_since(t).as_secs_f32())
                .unwrap_or(0.0);
            let players = &self.sim.client.world.players;
            let pose = |ent: u32| -> Option<(Vec3, Vec3)> {
                if let Some(e) = ents.get(&ent) {
                    return Some((e.pos, particles::forward_of(e.angles)));
                }
                players
                    .iter()
                    .find(|p| p.id == ent)
                    .map(|p| (p.origin, particles::forward_of(p.view_angles)))
            };
            let collision = &self.sim.collision;
            let trace = |start: Vec3, end: Vec3| -> Option<(Vec3, Vec3)> {
                let tr = collision.trace_ray(start, end, gm_collision::MASK_SOLID);
                (tr.fraction < 1.0 && !tr.start_solid).then_some((tr.end, tr.normal))
            };
            g.particles.tick(dt, &pose, &ent_bones, &trace);
            g.particles.prepare(&g.gpu, &self.mount, &g.world);
        }
        g.particles_tick = Some(now);
        g.particles
            .draw(&g.world, &camera, &mut additive, &mut alpha);
        // constraint ropes (`keyframe_rope`): a strip between the endpoints, sagging by the
        // slack (rest length over the distance) along a quadratic curve
        for r in &self.sim.client.world.ropes {
            let pose = |ent: u32, local: Vec3| -> Option<Vec3> {
                if ent == 0 {
                    return Some(local);
                }
                if let Some(e) = ents.get(&ent) {
                    return Some(
                        e.pos + gm_render::props::angle_matrix(e.angles.to_array()) * local,
                    );
                }
                self.sim
                    .client
                    .world
                    .players
                    .iter()
                    .find(|p| p.id == ent)
                    .map(|p| p.origin + local)
            };
            let (Some(a), Some(b)) = (pose(r.ent1, r.lpos1), pose(r.ent2, r.lpos2)) else {
                continue;
            };
            let Some(bm) = g
                .rope_mats
                .get(&r.material)
                .and_then(|m| m.as_ref())
                .or(g.beam_mat.as_ref())
            else {
                continue;
            };
            let dist = a.distance(b);
            let sag = (r.length - dist).max(0.0) * 0.5;
            let ctrl = (a + b) * 0.5 - Vec3::Z * (2.0 * sag);
            let n = if sag > 0.5 { 12 } else { 1 };
            let points: Vec<Vec3> = (0..=n)
                .map(|k| {
                    let t = k as f32 / n as f32;
                    let u = 1.0 - t;
                    a * (u * u) + ctrl * (2.0 * u * t) + b * (t * t)
                })
                .collect();
            let color = [
                f32::from(r.color[0]) / 255.0,
                f32::from(r.color[1]) / 255.0,
                f32::from(r.color[2]) / 255.0,
            ];
            let (v, i) = g.world.beam_strip(
                &points,
                r.width.max(1.0),
                color,
                camera.position,
                0.0,
                1.0 / 32.0,
            );
            alpha.push((bm, v, i));
        }
        // gravity gun (gravgun.md 7.2-7.4, the `_gmod` sprite set): the core at the muzzle
        // (14 u, alpha 128 ready / 255 holding), six flickering fork glows (6.4..9.6 u, alpha
        // 16..24, orange), three end caps and three noisy beams to the held prop while holding,
        // the launch blast (8 -> 64 u over 0.1 s, fading over 0.2 s) and a 0.1 s beam
        // (12 -> 4 u) to the punt point
        if self.weapon == GRAVGUN_SLOT {
            let fwd = camera.forward();
            let right = fwd.cross(Vec3::Z).normalize_or_zero();
            let core = vm_attach("muzzle")
                .unwrap_or(camera.position + right * 6.0 - Vec3::Z * 5.0 + fwd * 14.0);
            let holding = held_pos.is_some();
            let t = self.fx_epoch.elapsed().as_secs_f32();
            let seed = &mut self.fx_seed;
            let noise = |k: u32, frame: u32| -> f32 {
                let h = (k.wrapping_mul(0x9E37_79B9) ^ frame.wrapping_mul(0x85EB_CA6B))
                    .wrapping_mul(0xC2B2_AE35);
                (h >> 8) as f32 / 16_777_216.0 * 2.0 - 1.0
            };
            if let Some(m) = &g.gg_core {
                let a = self.gg_core_alpha;
                let (v, i) =
                    g.world
                        .sprite_quad_var(core, 14.0, [a, a, a, 0.0], 0.0, None, &camera);
                additive.push((m, v, i));
            }
            for (k, name) in ["fork1m", "fork1t", "fork2m", "fork2t", "fork3m", "fork3t"]
                .iter()
                .enumerate()
            {
                let Some(p) = vm_attach(name) else {
                    continue;
                };
                if let Some(m) = &g.gg_glow {
                    let size = particles::range(seed, 6.4, 9.6);
                    let a = particles::range(seed, 16.0, 24.0) / 255.0;
                    let (v, i) = g.world.sprite_quad_var(
                        p,
                        size,
                        [a, 0.5 * a, 0.0, 0.0],
                        0.0,
                        None,
                        &camera,
                    );
                    additive.push((m, v, i));
                }
                // end caps and hold beams only in HOLDING (hidden in READY / CLOSED)
                let (Some(end), true, 3) = (held_pos, name.ends_with('t'), self.gg_effect) else {
                    continue;
                };
                if let Some(m) = &g.gg_endcap {
                    let size = particles::range(seed, 3.0, 5.0);
                    let a = particles::range(seed, 200.0, 255.0) / 255.0;
                    let (v, i) =
                        g.world
                            .sprite_quad_var(p, size, [a, a, a, 0.0], 0.0, None, &camera);
                    additive.push((m, v, i));
                }
                if let Some(m) = &g.gg_beam_noz {
                    // `TE_BEAMPOINTS` width 4, amplitude 16, 8 segments, noise at 30 fps
                    let frame = (t * 30.0) as u32;
                    let axis = (end - p).normalize_or_zero();
                    let side = axis.cross(Vec3::Z).normalize_or_zero();
                    let up = side.cross(axis);
                    let points: Vec<Vec3> = (0..=8u32)
                        .map(|sg| {
                            let f = sg as f32 / 8.0;
                            let env = (f * std::f32::consts::PI).sin() * 16.0;
                            let kk = ((k as u32) << 8) | sg;
                            p.lerp(end, f)
                                + side * (noise(kk, frame) * env)
                                + up * (noise(kk ^ 0x55, frame) * env)
                        })
                        .collect();
                    let (v, i) = g.world.beam_strip(
                        &points,
                        4.0,
                        [1.0, 1.0, 1.0],
                        camera.position,
                        -t * 150.0 / 32.0,
                        1.0 / 32.0,
                    );
                    additive.push((m, v, i));
                }
            }
            if holding {
                dlights.push(gm_render::world::DLight {
                    pos: core,
                    radius: 128.0,
                    color: Vec3::new(1.0, 0.5, 0.1),
                    ..Default::default()
                });
            }
            self.fx_cannon
                .retain(|(_, _, at)| at.elapsed().as_secs_f32() < 0.2);
            for (player, pos, at) in &self.fx_cannon {
                if Some(*player) != me {
                    continue;
                }
                let age = at.elapsed().as_secs_f32();
                if let Some(m) = &g.gg_core2 {
                    let size = 8.0 + 56.0 * (age / 0.1).min(1.0);
                    let a = 1.0 - (age / 0.2).min(1.0);
                    let (v, i) =
                        g.world
                            .sprite_quad_var(core, size, [a, a, a, 0.0], 0.0, None, &camera);
                    additive.push((m, v, i));
                }
                if let (Some(m), true) = (&g.gg_beam, age < 0.1) {
                    let width = 12.0 - 8.0 * (age / 0.1);
                    let (v, i) = g.world.beam_strip(
                        &[core, *pos],
                        width,
                        [1.0, 1.0, 1.0],
                        camera.position,
                        0.0,
                        1.0 / 32.0,
                    );
                    additive.push((m, v, i));
                }
            }
        }
        // tool gun click effects (toolgun.md F30-F32): `ToolTracer` - three beams of
        // `effects/tool_tracer` width 8 whose start slides from the muzzle to the hit over
        // 0.25 s while the alpha fades; `selection_indicator` - a 4 u `select_dot` in the
        // surface plane for 1 s; six `selection_ring`s growing 256 * speed u/s, alpha gone in
        // 0.2 / speed s, a new random blue each frame
        self.fx_tool
            .retain(|(_, _, _, at, _)| at.elapsed().as_secs_f32() < 1.0);
        if !self.fx_tool.is_empty() {
            let now = Instant::now();
            let fwd = camera.forward();
            let right = fwd.cross(Vec3::Z).normalize_or_zero();
            let own_muzzle = vm_attach("muzzle")
                .unwrap_or(camera.position + fwd * 16.0 + right * 6.0 - Vec3::Z * 4.0);
            let seed = &mut self.fx_seed;
            for &(player, hit, normal, at, speeds) in &self.fx_tool {
                let age = now.duration_since(at).as_secs_f32();
                let start = if Some(player) == me {
                    Some(own_muzzle)
                } else {
                    self.sim
                        .client
                        .world
                        .players
                        .iter()
                        .find(|p| p.id == player)
                        .map(|p| p.origin + Vec3::Z * 60.0)
                };
                let life = age * 4.0;
                if let (Some(m), Some(start), true) = (&g.fx_tool_tracer, start, life < 1.0) {
                    let a = 1.0 - life;
                    let s = start - (start - hit) * life;
                    for _ in 0..3 {
                        let u0 = particles::rng(seed);
                        let (v, i) = g.world.beam_strip(
                            &[s, hit],
                            8.0,
                            [a, a, a],
                            camera.position,
                            u0,
                            1.0 / 32.0,
                        );
                        additive.push((m, v, i));
                    }
                }
                if let Some(m) = &g.fx_select_dot {
                    let a = 1.0 - age;
                    let (v, i) = g.world.sprite_quad_var(
                        hit,
                        4.0,
                        [a, a, a, 0.0],
                        0.0,
                        Some(normal),
                        &camera,
                    );
                    additive.push((m, v, i));
                }
                if let Some(m) = &g.fx_select_ring {
                    for speed in speeds {
                        let a = 1.0 - age * 5.0 * speed;
                        if a <= 0.0 {
                            continue;
                        }
                        let size = 4.0 + age * 256.0 * speed;
                        let c = [
                            particles::range(seed, 10.0, 150.0) / 255.0 * a,
                            particles::range(seed, 170.0, 220.0) / 255.0 * a,
                            particles::range(seed, 240.0, 255.0) / 255.0 * a,
                            0.0,
                        ];
                        let (v, i) = g.world.sprite_quad_var(
                            hit + normal * 2.0,
                            size,
                            c,
                            0.0,
                            Some(normal),
                            &camera,
                        );
                        additive.push((m, v, i));
                    }
                }
            }
        }
        // combine ball explosion (hl2weapons.md 8): three `sprites/lgtning` beam rings from the
        // ball's radius - to 128 u in 0.25 s width 48, to 384 u in 0.2 s and 0.5 s width 64,
        // colour (255,255,225) alpha 64 fading out
        self.fx_rings
            .retain(|(_, at)| at.elapsed().as_secs_f32() < 0.5);
        if let (Some(m), false) = (&g.fx_lgtning, self.fx_rings.is_empty()) {
            for &(pos, at) in &self.fx_rings {
                let age = at.elapsed().as_secs_f32();
                for (end, life, width) in
                    [(128.0, 0.25, 48.0), (384.0, 0.2, 64.0), (384.0, 0.5, 64.0)]
                {
                    let t = age / life;
                    if t >= 1.0 {
                        continue;
                    }
                    let r = 12.0 + (end - 12.0) * t;
                    let a = 64.0 / 255.0 * (1.0 - t);
                    let points: Vec<Vec3> = (0..=32)
                        .map(|k| {
                            let ang = k as f32 / 32.0 * std::f32::consts::TAU;
                            pos + Vec3::new(ang.cos() * r, ang.sin() * r, 0.0)
                        })
                        .collect();
                    let (v, i) = g.world.beam_strip(
                        &points,
                        width,
                        [a, a, a * 225.0 / 255.0],
                        camera.position,
                        0.0,
                        1.0 / 64.0,
                    );
                    additive.push((m, v, i));
                }
            }
        }
        let mut vm_flash: Option<Vec3> = None;
        // shot effects: tracers (128 u streak at 5000 u/s), own muzzle flash, impact and
        // explosion flashes, bullet decals (DecalModulate)
        {
            let now = Instant::now();
            let fwd = camera.forward();
            let right = fwd.cross(Vec3::Z).normalize_or_zero();
            let own_muzzle = vm_attach("muzzle")
                .unwrap_or(camera.position + fwd * 16.0 + right * 6.0 - Vec3::Z * 4.0);
            // `CTempEnts::EjectBrass` out of attachment 2: along its axis with a kick up,
            // tumbling at random, gone after a second
            if let Some(kind) = self.fx_brass_pending.take() {
                let at = (|| {
                    let (vm, (seq, t)) = (vm.as_ref()?, vm_seq?);
                    let m = g.models.get(vm.path)?.as_ref()?;
                    let w = m.bone_world_transition(
                        seq,
                        t,
                        &[("active", self.claw_pose)],
                        Self::vm_transition(vm_prev, |s| m.sequence_fade_out(s)),
                    )?;
                    let a = m.attachment_at(1, &w)?;
                    let mut a = gm_render::world::model_transform(camera.position, self.view) * a;
                    // FormatViewModelAttachment: the shell starts where the viewmodel's
                    // attachment appears on screen (tan(fov/2) / tan(vm fov/2) in view x/y),
                    // else it pops out nearer the eye and twice as big
                    let aspect = g.config.width as f32 / g.config.height as f32;
                    let vm_fov = scale_fov_by_width(vm.fov, aspect);
                    let k =
                        (camera.fov_x.to_radians() * 0.5).tan() / (vm_fov.to_radians() * 0.5).tan();
                    let view = glam::Mat4::look_to_rh(camera.position, camera.forward(), Vec3::Z);
                    let mut v = view.transform_point3(a.w_axis.truncate());
                    v.x *= k;
                    v.y *= k;
                    a.w_axis = view.inverse().transform_point3(v).extend(1.0);
                    Some(a)
                })();
                if let Some(a) = at {
                    let r = &mut g.fx_rng;
                    let dir = a.x_axis.truncate().normalize_or_zero();
                    let (jitter, spin) = (fx_vec_rand(r), fx_vec_rand(r));
                    self.fx_shells.push(Shell {
                        model: BRASS_MODELS[(kind as usize).min(2)],
                        pos: a.w_axis.truncate(),
                        vel: dir * fx_range(r, 50.0, 70.0)
                            + Vec3::Z * fx_range(r, 100.0, 150.0)
                            + jitter * 16.0,
                        ang: self.view,
                        angvel: spin * 1024.0,
                        born: Instant::now(),
                    });
                }
            }
            let players = &self.sim.client.world.players;
            let mut keep = Vec::new();
            for &(player, end, born, ar2) in &self.fx_tracers {
                let start = if Some(player) == me {
                    own_muzzle
                } else {
                    match players.iter().find(|p| p.id == player) {
                        Some(p) => p.origin + Vec3::new(0.0, 0.0, 60.0),
                        None => continue,
                    }
                };
                let total = (end - start).length();
                let head = now.duration_since(born).as_secs_f32() * 5000.0;
                // FX_Tracer draws nothing under 256 u (clientfx.md 5.3, gap-report #129)
                if total < 256.0 || head >= total {
                    continue;
                }
                keep.push((player, end, born, ar2));
                let mat = if ar2 {
                    g.fx_ar2_tracer.as_ref()
                } else {
                    g.fx_spark.as_ref()
                };
                if let Some(m) = mat {
                    let dir = (end - start) / total;
                    let tail = (head - 128.0).max(0.0);
                    let (v, i) = g.world.beam_strip(
                        &[start + dir * tail, start + dir * head],
                        if ar2 { 5.0 } else { 1.5 },
                        if ar2 {
                            [1.0, 1.0, 1.0]
                        } else {
                            [1.0, 0.9, 0.6]
                        },
                        camera.position,
                        0.0,
                        1.0 / 128.0,
                    );
                    additive.push((m, v, i));
                }
            }
            self.fx_tracers = keep;
            // the crossbow charger (#105): with a bolt loaded the blueflare1 sprite at the
            // muzzle brightens 0 -> 32 and grows to scale 0.025 over 0.5 s (START_CHARGE), then
            // to 80 and 0.1 over 1 s (READY); DISCHARGE (clip 0) turns it off
            {
                let me = self.sim.client.player_id;
                let clip = self
                    .sim
                    .client
                    .world
                    .players
                    .iter()
                    .find(|p| Some(p.id) == me)
                    .map_or(0, |p| p.clip);
                let loaded = vm.is_some_and(|v| v.class == "weapon_crossbow") && clip > 0;
                match (loaded, self.xbow_charge_since) {
                    (true, None) => self.xbow_charge_since = Some(now),
                    (false, Some(_)) => self.xbow_charge_since = None,
                    _ => {}
                }
                if let (Some(since), Some(m), Some(at)) = (
                    self.xbow_charge_since,
                    g.fx_blueflare.as_ref(),
                    vm_attach("muzzle"),
                ) {
                    let t = now.duration_since(since).as_secs_f32();
                    let (bright, scale) = if t < 0.5 {
                        (32.0 * t / 0.5, 0.025 * t / 0.5)
                    } else {
                        let u = ((t - 0.5) / 1.0).min(1.0);
                        (32.0 + 48.0 * u, 0.025 + 0.075 * u)
                    };
                    let c = bright / 255.0;
                    let (v, i) = g.world.sprite_quad(at, scale * 64.0, [c, c, c], &camera);
                    additive.push((m, v, i));
                }
            }
            // the explosion's dynamic light (#117): 350 u shrinking at 1166.67 u/s, 0.3 s
            self.fx_explosion_lights
                .retain(|(_, born)| now.duration_since(*born).as_secs_f32() < 0.3);
            for &(pos, born) in &self.fx_explosion_lights {
                let t = now.duration_since(born).as_secs_f32();
                dlights.push(gm_render::world::DLight {
                    pos,
                    radius: (350.0 - 1166.67 * t).max(0.0),
                    color: Vec3::new(1.0, 145.0 / 255.0, 30.0 / 255.0),
                    ..Default::default()
                });
            }
            // `AR2Impact`: a combinemuzzle2 flash growing 16 -> 32 u and fading over 0.1 s
            self.fx_ar2_impacts
                .retain(|(_, born)| now.duration_since(*born).as_secs_f32() < 0.1);
            if let Some(m) = &g.fx_ar2_impact {
                for &(pos, born) in &self.fx_ar2_impacts {
                    let age = now.duration_since(born).as_secs_f32() / 0.1;
                    let c = 1.0 - age;
                    let (v, i) = g
                        .world
                        .sprite_quad(pos, 16.0 + 16.0 * age, [c, c, c], &camera);
                    additive.push((m, v, i));
                }
            }
            if self.fx_muzzle_until.is_some_and(|t| now < t) {
                // the sprites are drawn in the viewmodel pass: the gun covers the world pass
                vm_flash = Some(own_muzzle);
                if self.fx_muzzle_spawned != self.fx_muzzle_until {
                    self.fx_muzzle_spawned = self.fx_muzzle_until;
                    // per-weapon flash callbacks (clientfx.md 2.2, #121): the core's scale
                    // range and lifetime; the generic attached flash for everything else
                    let (lo, hi, life) = match viewmodel(self.weapon).map(|v| v.class) {
                        Some("weapon_pistol") => (1.0, 1.25, 0.01),
                        Some("weapon_357") => (1.25, 1.5, 0.01),
                        Some("weapon_smg1") => (1.25, 1.5, 0.025),
                        Some("weapon_shotgun") => (1.25, 1.5, 0.0001),
                        Some("weapon_ar2") => (2.0, 2.25, 0.025),
                        _ => (1.0, 1.0, 0.1),
                    };
                    let scale = fx_range(&mut self.fx_seed, lo, hi);
                    for (pos, size, roll, variant) in
                        muzzle_sprites(&mut self.fx_seed, own_muzzle, fwd, scale)
                    {
                        self.fx_vm_muzzle
                            .push((pos, size, roll, variant, now, life));
                    }
                    let radius = 32.0 + (fx_rand(&mut self.fx_seed) * 33.0).floor();
                    self.fx_muzzle_light = Some((radius, now));
                }
            }
            // the muzzle event's entity light (#122): radius randint(32,64), (255,192,64),
            // decaying to nothing over 0.05 s
            if let Some((radius, born)) = self.fx_muzzle_light {
                let t = now.duration_since(born).as_secs_f32();
                if t < 0.05 {
                    dlights.push(gm_render::world::DLight {
                        pos: own_muzzle,
                        radius: radius * (1.0 - t / 0.05),
                        color: Vec3::new(1.0, 192.0 / 255.0, 64.0 / 255.0),
                        ..Default::default()
                    });
                } else {
                    self.fx_muzzle_light = None;
                }
            }
            let due: Vec<String> = self
                .fx_sound_queue
                .iter()
                .filter(|(at, _)| *at <= now)
                .map(|(_, n)| n.clone())
                .collect();
            self.fx_sound_queue.retain(|(at, _)| *at > now);
            for name in due {
                self.sounds.emit(&self.mount, &name, None);
            }
            self.fx_bursts.retain(|(_, size, born, _)| {
                now.duration_since(*born).as_secs_f32() < burst_life(*size)
            });
            // env_sun (C_Sun's glow overlay; an edit_sun entity's values and forward replace
            // the map's): the sprite sits far along the sun direction, so the world hides it
            // like the game's visibility query; its size is an approximation of CGlowOverlay's
            if let (Some(bm), Some(sun)) = (&g.fx_sun, g.world.env_sun.as_ref()) {
                let (edit, size, osize, color, ocolor) = edit_sun_of(
                    sun,
                    &self.sim.client.world.classes,
                    &self.sim.client.world.edits,
                );
                let dir = edit
                    .and_then(|id| ents.get(&id))
                    .map(|e| {
                        let (p, y) = (e.angles.x.to_radians(), e.angles.y.to_radians());
                        Vec3::new(p.cos() * y.cos(), p.cos() * y.sin(), -p.sin())
                    })
                    .unwrap_or(sun.dir);
                let d = camera.far * 0.9;
                // `dir` is the light's travel direction (env_sun / light_environment angles);
                // C_Sun draws the glow where the light comes from
                let pos = camera.position - dir * d;
                let width = |s: f32| 2.0 * d * (s * 0.3_f32).to_radians().tan();
                let (v, i) = g
                    .world
                    .sprite_quad(pos, width(size), color.to_array(), &camera);
                additive.push((bm, v, i));
                if osize > 0.0 {
                    let (v, i) = g
                        .world
                        .sprite_quad(pos, width(osize), ocolor.to_array(), &camera);
                    additive.push((bm, v, i));
                }
            }
            // env_sprite: a billboard of its material at the entity, texels times `scale`
            // across, in its colour; additive for the additive / glow render modes
            for e in ents.values().filter(|e| is_sprite_ent(&e.model)) {
                if !g.fx_sprite_mats.contains_key(&e.model) {
                    let m = g.world.sprite_material_sized(&g.gpu, &self.mount, &e.model);
                    g.fx_sprite_mats.insert(e.model.clone(), m);
                }
            }
            g.ball_last.retain(|id, _| ents.contains_key(id));
            for e in ents.values().filter(|e| is_sprite_ent(&e.model)) {
                let Some(Some((bm, w, mat_additive))) = g.fx_sprite_mats.get(&e.model) else {
                    continue;
                };
                if e.model == CBALL_BODY {
                    // C_PropCombineBall in flight (clientfx.md 8.2): flicker halo grey
                    // rand(0.2,0.3) at radius*rand(1.5,2.5); 8 motion-blur halos stepping back
                    // along -moveDir by min(speed/2, 4) u, grey base*(1 - i/12) with base =
                    // remap(speed, 4..32 -> 0..1); the oriented body halo radius + sin(25 t)
                    // rolled by its spawn time. Sizes are DrawHalo widths; everything additive.
                    const RADIUS: f32 = 10.0;
                    let last = g.ball_last.insert(e.id, e.pos).unwrap_or(e.pos);
                    let moved = e.pos - last;
                    let speed = moved.length().min(32.0);
                    let dir = moved.normalize_or_zero();
                    let r = &mut g.fx_rng;
                    let grey = fx_range(r, 0.2, 0.3);
                    let flicker = RADIUS * fx_range(r, 1.5, 2.5);
                    if let Some(m) = &g.fx_cball_flicker {
                        let (v, i) = g.world.sprite_quad(e.pos, flicker, [grey; 3], &camera);
                        additive.push((m, v, i));
                    }
                    if let (Some(m), true) = (&g.fx_cball_blur, speed > 0.0) {
                        let base = ((speed - 4.0) / 28.0).clamp(0.0, 1.0);
                        let step = (speed * 0.5).min(4.0);
                        for i in 0..8 {
                            let k = base * (1.0 - i as f32 / 12.0);
                            let (v, idx) = g.world.sprite_quad(
                                e.pos - dir * step * i as f32,
                                RADIUS,
                                [k; 3],
                                &camera,
                            );
                            additive.push((m, v, idx));
                        }
                    }
                    let t = now.duration_since(self.epoch).as_secs_f32();
                    let roll = (e.id as f32 * 37.0) % 360.0;
                    let (v, i) = g.world.sprite_quad_var(
                        e.pos,
                        RADIUS + (t * 25.0).sin(),
                        [1.0; 4],
                        roll,
                        None,
                        &camera,
                    );
                    additive.push((bm, v, i));
                    continue;
                }
                let c = |v: u8| v as f32 / 255.0;
                let (v, i) = g.world.sprite_quad_var(
                    e.pos,
                    w * e.model_scale,
                    [c(e.color[0]), c(e.color[1]), c(e.color[2]), c(e.color[3])],
                    0.0,
                    None,
                    &camera,
                );
                // an `$additive` material blends additively whatever the render mode
                if *mat_additive || matches!(e.render_mode, 3 | 5 | 9) {
                    additive.push((bm, v, i));
                } else {
                    alpha.push((bm, v, i));
                }
            }
            // sent_ball.lua `Draw`: a sprite of the ball's size in its colour where its body is
            if let Some(bm) = &g.fx_ball {
                for e in ents.values().filter(|e| is_ball(e.id)) {
                    let pos = self.sim.interp.sample(e.id, rt).map_or(e.pos, |p| p.pos);
                    // sent_ball.lua: the colour times (ComputeLighting(pos, up) + 0.5)
                    let l = g.world.light_at(pos, Vec3::Z);
                    let c = |v: u8, l: f32| v as f32 / 255.0 * (l.clamp(0.0, 1.0) + 0.5);
                    let (v, i) = g.world.sprite_quad_var(
                        pos,
                        32.0 * e.model_scale,
                        [
                            c(e.color[0], l.x),
                            c(e.color[1], l.y),
                            c(e.color[2], l.z),
                            1.0,
                        ],
                        0.0,
                        None,
                        &camera,
                    );
                    alpha.push((bm, v, i));
                }
            }
            for &(pos, size, born, splash) in &self.fx_bursts {
                let age = now.duration_since(born).as_secs_f32() / burst_life(size);
                let k = 1.0 - age;
                let (m, color) = if splash {
                    (g.fx_splash.as_ref(), [k, k, k])
                } else {
                    (g.fx_muzzle.as_ref(), [k, k * 0.7, k * 0.4])
                };
                if let Some(m) = m {
                    let (v, i) = g.world.sprite_quad(pos, size * (0.6 + age), color, &camera);
                    additive.push((m, v, i));
                }
            }
            // `C_BaseExplosionEffect`: size is the half-width, the alpha ramp is (1 - t)^2
            for sp in &self.fx_sprites {
                let t = (now.duration_since(sp.born).as_secs_f32() / sp.life).clamp(0.0, 1.0);
                let ramp = 1.0 - t;
                let size = (sp.size0 + (sp.size1 - sp.size0) * t) * 2.0;
                let c = sp.color;
                if sp.kind == FxKind::Spark {
                    if let Some(m) = &g.fx_spark {
                        let tail = sp.pos - sp.vel * sp.size1;
                        let (v, i) = g.world.beam_strip(
                            &[tail, sp.pos],
                            sp.size0,
                            [1.0, 1.0, 1.0],
                            camera.position,
                            0.0,
                            1.0 / 16.0,
                        );
                        additive.push((m, v, i));
                    }
                    continue;
                }
                let (m, a, add) = match sp.kind {
                    FxKind::Smoke => (g.fx_explo_smoke.as_ref(), ramp * ramp, false),
                    FxKind::Ember => (g.fx_explo_embers.get(sp.variant), ramp * ramp, true),
                    FxKind::FireCloud => (g.fx_explo_fire.as_ref(), ramp * ramp, true),
                    FxKind::Fleck => (g.fx_explo_flecks.get(sp.variant), ramp, false),
                    FxKind::WoodFleck => (g.fx_wood_flecks.get(sp.variant), ramp, false),
                    FxKind::Dust => (g.fx_explo_smoke.as_ref(), ramp * sp.alpha, false),
                    FxKind::DustNoise => (g.fx_noise.as_ref(), ramp, false),
                    FxKind::Flare => (g.fx_flare.as_ref(), ramp, true),
                    FxKind::Muzzle => (g.fx_muzzles.get(sp.variant), 0.5 + 0.5 * ramp, true),
                    FxKind::TrailSmoke => (
                        g.fx_explo_smoke.as_ref(),
                        sp.alpha * (std::f32::consts::PI * t).sin(),
                        false,
                    ),
                    FxKind::TrailNoise => (
                        g.fx_noise.as_ref(),
                        sp.alpha * (std::f32::consts::PI * t).sin(),
                        false,
                    ),
                    FxKind::Spark => unreachable!(),
                };
                if let Some(m) = m {
                    let (v, i) = g.world.sprite_quad_var(
                        sp.pos,
                        size,
                        [c[0], c[1], c[2], a],
                        sp.roll,
                        None,
                        &camera,
                    );
                    if add {
                        additive.push((m, v, i));
                    } else {
                        alpha.push((m, v, i));
                    }
                }
            }
            // the RPG's laser dot (`CLaserDot`): a red glow where the aim meets the world,
            // and the 128 u `laser1_noz` beam out of the muzzle (hl2weapons.md, #110)
            if viewmodel(self.weapon).is_some_and(|v| v.class == "weapon_rpg") {
                if let Some(m) = &g.fx_laserdot {
                    let hit = self.sim.collision.trace_ray(
                        camera.position,
                        camera.position + fwd * 8192.0,
                        gm_collision::MASK_SOLID,
                    );
                    let (v, i) =
                        g.world
                            .sprite_quad(hit.end - fwd * 1.0, 6.0, [1.0, 0.2, 0.2], &camera);
                    additive.push((m, v, i));
                }
                if let Some(m) = &g.fx_laserbeam {
                    let muzzle = vm_attach("muzzle")
                        .unwrap_or(camera.position + fwd * 16.0 + right * 6.0 - Vec3::Z * 4.0);
                    let (v, i) = g.world.beam_strip(
                        &[muzzle, muzzle + fwd * 128.0],
                        2.0,
                        [1.0, 0.1, 0.1],
                        camera.position,
                        0.0,
                        1.0 / 128.0,
                    );
                    additive.push((m, v, i));
                }
            }
            self.fx_decals
                .retain(|(_, _, _, born)| now.duration_since(*born).as_secs_f32() < DECAL_LIFE);
            for (_, _, name, _) in &self.fx_decals {
                if !name.is_empty() && !g.fx_decal_mats.contains_key(name) {
                    let m = g.world.decal(&g.gpu, &self.mount, name);
                    match &m {
                        Some(d) => {
                            println!("decal {name}: {:.1} u, modulate {}", d.size, d.modulate)
                        }
                        None => println!("decal {name}: material or texture not loaded"),
                    }
                    g.fx_decal_mats.insert(name.clone(), m);
                }
            }
            self.fx_prop_decals
                .retain(|(_, _, _, _, born)| now.duration_since(*born).as_secs_f32() < DECAL_LIFE);
            for (_, _, _, name, _) in &self.fx_prop_decals {
                if !name.is_empty() && !g.fx_decal_mats.contains_key(name) {
                    let m = g.world.decal(&g.gpu, &self.mount, name);
                    g.fx_decal_mats.insert(name.clone(), m);
                }
            }
            for (pos, normal, name, _) in &self.fx_decals {
                if let Some(Some(d)) = g.fx_decal_mats.get(name) {
                    let (v, i) = g.world.decal_quad_uv(*pos, *normal, d.size, [1.0; 3], d.uv);
                    if d.modulate {
                        modulate.push((&d.bind, v, i));
                    } else {
                        alpha.push((&d.bind, v, i));
                    }
                }
            }
            // decals on props ride their entity
            for (ent, lpos, lnormal, name, _) in &self.fx_prop_decals {
                let (Some(e), Some(Some(d))) = (ents.get(ent), g.fx_decal_mats.get(name)) else {
                    continue;
                };
                let rot = gm_render::props::angle_matrix(e.angles.to_array());
                let (v, i) = g.world.decal_quad_uv(
                    e.pos + rot * *lpos,
                    (rot * *lnormal).normalize_or_zero(),
                    d.size,
                    [1.0; 3],
                    d.uv,
                );
                if d.modulate {
                    modulate.push((&d.bind, v, i));
                } else {
                    alpha.push((&d.bind, v, i));
                }
            }
        }
        // flashlights (`r_flashlightfov` 53.7, `r_flashlightfar` 750): a spot from each lit
        // player's eye along the view
        let mut flashlights: Vec<(usize, gm_render::world::DLight)> = Vec::new();
        for p in &self.sim.client.world.players {
            if !p.flashlight {
                continue;
            }
            let (pos, dir) = if Some(p.id) == me {
                // CFlashlightEffect::UpdateLight (render.md 3.3, gap-report #137): the
                // origin sits r_flashlightoffsety (-20) below the eye, raised by the swing
                // (-12 u along the view) when looking down, reset to the eye when a 4 u hull
                // cannot reach it; the beam aims at eye + 750 u and is pulled back from a
                // wall nearer than 128 u, smoothed 20 % a frame (the ladder case is not done)
                let eye = camera.position;
                let fwd = camera.forward();
                let up = camera.up();
                let col = &self.sim.collision;
                let hull = (Vec3::splat(-4.0), Vec3::splat(4.0));
                let mut off_y = -20.0;
                let swing = eye - fwd * 12.0;
                if swing.z > eye.z {
                    off_y += swing.z - eye.z;
                }
                let mut origin = eye + up * off_y;
                let tr = col.trace_box(eye, origin, hull.0, hull.1, gm_collision::MASK_SOLID);
                if tr.start_solid || tr.fraction < 1.0 {
                    origin = eye;
                }
                let target = eye + fwd * 750.0;
                let dir = (target - origin).normalize_or_zero();
                let tr = col.trace_box(origin, target, hull.0, hull.1, gm_collision::MASK_SOLID);
                let dist = tr.end.distance(origin);
                let dm = &mut self.flashlight_dist_mod;
                if tr.fraction < 1.0 && dist < 128.0 {
                    *dm += (128.0 - dist - *dm) * 0.2;
                    let back = col.trace_box(
                        origin,
                        origin - dir * (dist - 0.1).max(0.0),
                        hull.0,
                        hull.1,
                        gm_collision::MASK_SOLID,
                    );
                    *dm = dm.min((back.end.distance(origin) - 0.1).max(0.0));
                } else {
                    *dm *= 0.8;
                }
                (origin - dir * *dm, dir)
            } else {
                let (pitch, yaw) = (p.view_angles.x.to_radians(), p.view_angles.y.to_radians());
                // the model's flashlight attachment, else the eye (the game's fallback)
                let eye = if p.ducked { 28.0 } else { 64.0 };
                (
                    flashlight_attach
                        .get(&p.id)
                        .copied()
                        .unwrap_or(p.origin + Vec3::new(0.0, 0.0, eye)),
                    Vec3::new(
                        pitch.cos() * yaw.cos(),
                        pitch.cos() * yaw.sin(),
                        -pitch.sin(),
                    ),
                )
            };
            let light = gm_render::world::DLight {
                pos,
                radius: 750.0,
                color: Vec3::new(1.0, 1.0, 0.95) * 1.5,
                dir,
                cone: 26.85f32.to_radians().cos(),
            };
            if Some(p.id) == me {
                flashlights.insert(0, (dlights.len(), light));
            } else {
                flashlights.push((dlights.len(), light));
            }
            dlights.push(light);
        }
        if self.weapon == 1 {
            paint_tool_screen(
                &mut self.tool_screen,
                &self.mount,
                &self.tool,
                self.content_lang.as_ref(),
                self.fx_epoch.elapsed().as_secs_f32(),
                g,
            );
        }
        g.world.set_dlights(&dlights);
        g.world.set_flashlights(&flashlights);
        let target = ModelTarget {
            color: &attach,
            resolve,
            depth: &g.depth,
            aspect,
            clear_depth: false,
            alpha: 1.0,
        };
        // an edit_fog entity's SetupWorldFog hook: the newest one's fog replaces the map's
        g.world.fog_override =
            edit_fog_of(&self.sim.client.world.classes, &self.sim.client.world.edits);
        // an edit_sky entity (env_skypaint's vars): a painted sky instead of the skybox
        g.world.sky_paint = edit_sky_of(
            &self.sim.client.world.classes,
            &self.sim.client.world.edits,
            g.world.env_sun.as_ref().map_or(g.world.sun, |s| s.dir),
        )
        .or(g.world.map_sky_paint);
        // the world draws the entity models itself, between its opaque and translucent
        // surfaces (and into the water reflection)
        g.world.draw(
            &g.gpu,
            &camera,
            &attach,
            resolve,
            &g.depth,
            aspect,
            Some(scene_src),
            &items,
        );
        if !modulate.is_empty() {
            g.world.draw_modulate(&g.gpu, &camera, &target, &modulate);
        }
        if !additive.is_empty() {
            g.world.draw_alpha(&g.gpu, &camera, &target, &alpha);
            g.world.draw_additive(&g.gpu, &camera, &target, &additive);
        }
        if !refract.is_empty() {
            g.world
                .draw_refract(&g.gpu, &camera, &target, scene_src, &refract);
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
                    tint: Vec3::ONE,
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
            // the crossbow's viewmodel wears skin 1 (the glowing bolt) while loaded (#105)
            let me_clip = self
                .sim
                .client
                .world
                .players
                .iter()
                .find(|p| Some(p.id) == self.sim.client.player_id)
                .map_or(0, |p| p.clip);
            let xbow_loaded = vm.class == "weapon_crossbow" && me_clip > 0;
            let variant_key;
            let path: &str = if xbow_loaded {
                variant_key = format!("{}#1#", vm.path);
                if !g.models.contains_key(&variant_key) {
                    let m = g
                        .world
                        .load_model_variant(&g.gpu, &self.mount, vm.path, 1, "");
                    g.models.insert(variant_key.clone(), m);
                }
                &variant_key
            } else {
                if !g.models.contains_key(vm.path) {
                    let m = g.world.load_model(&g.gpu, &self.mount, vm.path);
                    g.models.insert(vm.path.to_string(), m);
                }
                vm.path
            };
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
                let world = m.bone_world_transition(
                    seq,
                    t,
                    &[("active", self.claw_pose)],
                    Self::vm_transition(vm_prev, |s| m.sequence_fade_out(s)),
                );
                let pose = world.as_ref().map(|w| m.skin_matrices(w));
                let hands = g.models.get(&hands_key).and_then(|h| h.as_ref());
                let hands_pose = match (hands, &world) {
                    (Some(h), Some(w)) => h.pose_merged("idle", 0.0, m, w),
                    _ => None,
                };
                // Source fovs are given for 4:3; wider screens widen the horizontal fov
                let fov_x = scale_fov_by_width(vm.fov, aspect);
                let vm_camera = Camera {
                    fov_x,
                    near: 1.0,
                    far: 512.0,
                    ..camera
                };
                let me = self.sim.client.player_id;
                let speed = self
                    .sim
                    .client
                    .world
                    .players
                    .iter()
                    .find(|p| Some(p.id) == me)
                    .map(|p| p.velocity.truncate().length())
                    .unwrap_or(0.0);
                let (vm_pos, vm_ang) = self.vm_view.view(speed, camera.position, self.view);
                let transform = gm_render::world::model_transform(vm_pos, vm_ang);
                let mut vm_items = vec![DrawItem {
                    tint: Vec3::ONE,
                    model: m,
                    transform,
                    bones: pose.as_deref(),
                }];
                if let (Some(h), Some(hp)) = (hands, &hands_pose) {
                    vm_items.push(DrawItem {
                        tint: Vec3::ONE,
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
                self.fx_vm_muzzle
                    .retain(|(_, _, _, _, born, life)| born.elapsed().as_secs_f32() < *life);
                if vm_flash.is_some() || !self.fx_vm_muzzle.is_empty() {
                    // 8 sprites along the barrel, white, alpha 255 -> 128 over 0.1 s: drawn
                    // over the viewmodel but with the world camera and world sizes (the
                    // game's muzzle particles are world particles at the corrected muzzle;
                    // under the viewmodel's FOV they came out twice as big)
                    let mut quads = Vec::new();
                    for &(pos, size, roll, variant, born, life) in &self.fx_vm_muzzle {
                        let Some(m) = g.fx_muzzles.get(variant) else {
                            continue;
                        };
                        let a = 1.0 - 0.5 * (born.elapsed().as_secs_f32() / life).min(1.0);
                        let (v, i) =
                            g.world
                                .sprite_quad_var(pos, size, [a, a, a, 0.0], roll, None, &camera);
                        quads.push((m, v, i));
                    }
                    if !quads.is_empty() {
                        let target = ModelTarget {
                            clear_depth: false,
                            ..target
                        };
                        g.world.draw_additive(&g.gpu, &camera, &target, &quads);
                    }
                }
            }
        }
        // 3D2D planes of the previous frame's HUD handlers, on the world's depth
        {
            let g = self.gfx.as_ref().unwrap();
            let planes = hud_planes(&self.sim.hud_draws, &self.mount, g);
            if !planes.is_empty() {
                let aspect = g.config.width as f32 / g.config.height as f32;
                g.ui.draw_world(
                    &g.gpu,
                    &attach,
                    resolve,
                    &g.depth,
                    camera.view_proj(aspect),
                    &planes,
                );
            }
        }
        {
            let g = self.gfx.as_ref().unwrap();
            let (w, h) = (g.config.width, g.config.height);
            self.sim.script.cursor = (!self.grabbed).then_some(self.cursor);
            self.report_audio();
            self.report_servers();
            self.sim.hud_draws = self.sim.script.client_render(&self.sim.client.world, w, h);
            // `gui.EnableScreenClicker`: a script shows the cursor over the game (the mouse
            // look stops) until it turns it off again
            let mut clicker = None;
            let mut audio = Vec::new();
            let mut servers_ops = Vec::new();
            for d in &self.sim.hud_draws {
                match d {
                    gm_script::DrawCmd::ScreenClicker { on } => clicker = Some(*on),
                    gm_script::DrawCmd::Audio { id, op } => audio.push((*id, op.clone())),
                    gm_script::DrawCmd::Servers { id, op } => servers_ops.push((*id, op.clone())),
                    gm_script::DrawCmd::OpenUrl { url } => {
                        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
                    }
                    gm_script::DrawCmd::DLight {
                        key,
                        pos,
                        color,
                        brightness,
                        size,
                        decay,
                        remaining,
                        dir,
                        outer,
                        nomodel,
                        noworld,
                        ..
                    } => {
                        let now = Instant::now();
                        let light = gm_render::world::DLight {
                            pos: Vec3::from_array(*pos),
                            radius: *size,
                            color: Vec3::from_array(*color) * *brightness,
                            dir: Vec3::from_array(*dir).normalize_or_zero(),
                            cone: if *outer > 0.0 {
                                (*outer).to_radians().cos()
                            } else {
                                0.0
                            },
                        };
                        let die = now + std::time::Duration::from_secs_f32(remaining.max(0.0));
                        self.lua_dlights
                            .insert(*key, (light, *decay, die, now, *nomodel, *noworld));
                    }
                    gm_script::DrawCmd::Particle {
                        name,
                        pos,
                        ang,
                        parent,
                    } => {
                        if let Some(g) = self.gfx.as_mut() {
                            g.particles.ensure_loaded(&self.mount);
                            if !g.particles.spawn(&particles::Spawn {
                                name: name.clone(),
                                pos: Vec3::from_array(*pos),
                                angles: Vec3::from_array(*ang),
                                parent: *parent,
                            }) {
                                eprintln!("ParticleEffect: unknown system {name}");
                            }
                        }
                    }
                    _ => {}
                }
            }
            for (id, op) in audio {
                self.apply_audio(id, &op);
            }
            for (id, op) in servers_ops {
                self.apply_servers(id, &op);
            }
            if let Some(on) = clicker.filter(|on| *on != self.screen_clicker) {
                self.screen_clicker = on;
                if self.menu.is_none() && !self.context_held && !self.main_menu {
                    self.set_grab(!on);
                }
            }
            if let Some(gm) = self.gfx.as_mut() {
                // `draw.font` builds its atlas now and reports the metrics to the runtime;
                // `draw.texture` decodes its VTF once
                for d in &self.sim.hud_draws {
                    match d {
                        gm_script::DrawCmd::Font { name, family, size } => {
                            self.script_fonts.retain(|(n, _, _)| n != name);
                            self.script_fonts
                                .push((name.clone(), family.clone(), *size));
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
        let ui_items = self.ui_items(o, camera.view_proj(aspect));
        let g = self.gfx.as_ref().unwrap();
        if bloom {
            g.post.apply(&g.gpu, &view, g.world.bloom_amount);
        }
        // the auto-exposure probe sees the scene only (Source measures the HDR scene
        // buffer before the HUD / menus are drawn on it)
        if g.world.hdr && self.frames.is_multiple_of(15) {
            let (bright, avg) =
                frame_bright_pixels(&g.gpu, &frame.texture, g.config.width, g.config.height);
            let dt = self.exposure_at.elapsed().as_secs_f32().min(1.0);
            self.exposure_at = Instant::now();
            let g = self.gfx.as_mut().unwrap();
            g.world.update_exposure(bright, avg, dt);
            if std::env::var_os("GM_HDR_LOG").is_some() {
                println!(
                    "hdr: bright {bright:.3} avg {avg:.3} exposure {:.3}",
                    g.world.exposure
                );
            }
        }
        let g = self.gfx.as_ref().unwrap();
        g.ui.draw(&g.gpu, &view, g.config.width, g.config.height, &ui_items);
        let g = self.gfx.as_ref().unwrap();
        if std::mem::take(&mut self.cam_photo_pending) {
            // the game's `jpeg` console command writes screenshots/<map>NNNN.jpg; we have a PNG
            // encoder, so the photo is a .png next to it
            let (w, h) = (g.config.width, g.config.height);
            if let Ok(mut rgba) = g.gpu.read_texture(&frame.texture, w, h) {
                if matches!(
                    g.config.format,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
                ) {
                    for px in rgba.as_chunks_mut::<4>().0 {
                        px.swap(0, 2);
                    }
                }
                let dir = settings::home_dir().join("screenshots");
                let _ = std::fs::create_dir_all(&dir);
                let map = self
                    .map_name
                    .rsplit('/')
                    .next()
                    .unwrap_or("photo")
                    .to_owned();
                let n = (0..10_000)
                    .find(|n| !dir.join(format!("{map}{n:04}.png")).exists())
                    .unwrap_or(0);
                let path = dir.join(format!("{map}{n:04}.png"));
                if let Err(e) = gm_render::gpu::write_png(&path, w, h, &rgba) {
                    eprintln!("photo: {e}");
                }
            }
        }
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

    fn ui_items(&self, o: Vec3, view_proj: glam::Mat4) -> Vec<UiItem> {
        let Some(g) = &self.gfx else {
            return Vec::new();
        };
        let (w, h) = (g.config.width as f32, g.config.height as f32);
        let lh = g.ui.line_height();
        let mut items = Vec::new();
        let white = [0.92, 0.92, 0.92, 1.0];
        if let Some(a) = self
            .screen_fade
            .as_ref()
            .and_then(|f| f.alpha(Instant::now()))
        {
            let [r, g, b, _] = self.screen_fade.as_ref().map_or([0.0; 4], |f| f.color);
            items.push(UiItem::Rect {
                x: 0.0,
                y: 0.0,
                w,
                h,
                color: [r, g, b, a],
            });
        }
        if self.main_menu {
            items.extend(self.main_menu_layout(g).items);
            if let Some(st) = &self.options {
                items.extend(self.options_layout(g, st).items);
            }
            if self.console.open {
                self.console_items(g, w, h, lh, &mut items);
            }
            return items;
        }
        if self.context_held || self.context.is_some() {
            // contextmenu.lua: the tool's ControlPanel moves into a 320 wide canvas bottom-right
            // (an editor entity's "Edit Properties" sheet takes the canvas while it is open)
            let title: &str = if self.edit_sheet.is_some() {
                "Edit Properties"
            } else {
                &self.tool
            };
            let font = g.ui.font_metrics(Some(derma::FONT));
            let bold = g.ui.font_metrics(Some(derma::FONT_BOLD));
            let header = g.ui.font_metrics(Some(derma::HEADER_FONT));
            let has_image = |name: &str| g.ui.has_image(name);
            let pp_set = self.pp_enabled.clone();
            let pp_on = |k: &str| pp_set.contains(k);
            let dctx = derma::Ctx {
                w,
                h,
                cursor: self.cursor,
                font: &font,
                bold: &bold,
                header: &header,
                lists: &self.spawnlists,
                content: [
                    &self.content[0],
                    &self.content[1],
                    &self.content[2],
                    &self.content[3],
                    &self.content[4],
                ],
                results: &self.search_results,
                tool: title,
                has_image: &has_image,
                pp_on: &pp_on,
                lang: &self.lang,
                ugc: None,
            };
            let mut layout = derma::Layout::default();
            if let (Some(schema), true) = (self.panel_schema(), self.menu.is_none()) {
                let title = self
                    .lang
                    .get("smwidget.playermodel")
                    .map_or("Player Model", String::as_str);
                derma::desktop_icons(
                    &mut layout,
                    &dctx,
                    &[("PlayerEditor", "icon64/playermodel.png", title)],
                );
                if self.player_editor {
                    let models = Self::player_model_list();
                    derma::player_editor(&mut layout, &dctx, &models, &self.current_player_model());
                }
                let rect = derma::context_canvas(&mut layout, &dctx, &schema);
                let sheet_vals = self
                    .edit_sheet
                    .filter(|_| self.menu.is_none())
                    .map(|ent| self.edit_values(ent));
                let set = sheet_vals.as_ref().or_else(|| self.settings_map());
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
                let entries = self.context_entries(ctx);
                derma::dmenu_tree(&mut layout, &dctx, ctx.x, ctx.y, &entries, &ctx.open);
            }
            items.extend(layout.items);
        }
        if let Some(menu) = &self.menu {
            let font = g.ui.font_metrics(Some(derma::FONT));
            let bold = g.ui.font_metrics(Some(derma::FONT_BOLD));
            let header = g.ui.font_metrics(Some(derma::HEADER_FONT));
            let has_image = |name: &str| g.ui.has_image(name);
            let pp_set = self.pp_enabled.clone();
            let pp_on = |k: &str| pp_set.contains(k);
            let uf = ugc_font_set(g);
            let ufonts = uf.fonts();
            let ctx = derma::Ctx {
                w,
                h,
                cursor: self.cursor,
                font: &font,
                bold: &bold,
                header: &header,
                lists: &self.spawnlists,
                content: [
                    &self.content[0],
                    &self.content[1],
                    &self.content[2],
                    &self.content[3],
                    &self.content[4],
                ],
                results: &self.search_results,
                tool: &self.tool,
                has_image: &has_image,
                pp_on: &pp_on,
                lang: &self.lang,
                ugc: Some((&self.qm_dupes, &self.qm_saves, &ufonts)),
            };
            let mut layout = derma::build(&ctx, &menu.st);
            if layout.ugc_per_page > 0 {
                self.menu_cache.borrow_mut().qm_per_page = layout.ugc_per_page;
            }
            let rows = match self.panel_schema() {
                Some(schema) => {
                    let set = self.settings_map();
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
            let (cx, cy) = self.cursor;
            let hovered_tip = layout
                .tips
                .iter()
                .find(|(x, y, tw, th, _)| cx >= *x && cy >= *y && cx < x + tw && cy < y + th);
            let hovered = hovered_tip.map(|t| t.4.clone());
            let hover_y = hovered_tip.map(|t| t.1).unwrap_or(cy);
            let tip = hovered.map(|n| match self.menu_cache.borrow().tip.clone() {
                Some((prev, t0)) if prev == n => (n, t0),
                _ => (n, std::time::Instant::now()),
            });
            *self.menu_cache.borrow_mut() = MenuCache {
                visible: layout.visible.clone(),
                max_scroll: layout.max_scroll,
                rows,
                opt_bars: Vec::new(),
                ng_max_scroll: 0.0,
                ugc_per_page: 0,
                qm_per_page: layout.ugc_per_page,
                tip: tip.clone(),
            };
            items.extend(layout.items);
            // DTooltip (`dtooltip.lua`): after `tooltip_delay` (0.5 s) the text in the skin's
            // Tooltip border, w + 8 / h + 6, centred on the cursor and above it (cursor - 50 or
            // over the hovered icon, whichever is higher).
            if let Some((name, t0)) = &tip {
                if t0.elapsed().as_secs_f32() >= 0.5 {
                    let tw = ctx.font.text_width(name) + 8.0;
                    let th = ctx.font.line_height + 6.0;
                    let x = (cx - tw / 2.0).clamp(0.0, (w - tw).max(0.0));
                    let y = (cy - 50.0)
                        .min(hover_y - th - 10.0)
                        .max(2.0)
                        .min((h - th).max(0.0));
                    derma::nine(&mut items, &derma::TOOLTIP, x, y, tw, th, [1.0; 4]);
                    items.push(UiItem::Text {
                        font: Some(derma::FONT.to_string()),
                        x: x + 4.0,
                        y: y + 3.0,
                        text: name.clone(),
                        color: [0.0, 0.0, 0.0, 1.0],
                    });
                }
            }
            if self.console.open {
                self.console_items(g, w, h, lh, &mut items);
            }
            return items;
        }
        if self.scoreboard && !self.console.open {
            // base cl_scoreboard.lua: a 700 x (ScrH - 200) panel at (ScrW/2 - 350, 100) with
            // no background, a 100 px header carrying the host name (ScoreboardDefaultTitle,
            // centred, shadow 2), then PLAYER_LINE rows: 38 tall, margin 2/0/2/2, a
            // draw.RoundedBox(4) in the line colour, 3 px padding, the 32 px avatar, the name
            // (ScoreboardDefault, 93 93 93) 8 px further, and from the right the 32 px mute
            // button and the 50 px Ping / Deaths / Kills columns
            let bw = 700.0;
            let x0 = (w * 0.5 - 350.0).floor();
            let y0 = 100.0;
            let title_fm = g.ui.font_metrics(Some("ScoreboardDefaultTitle"));
            let fm = g.ui.font_metrics(Some("ScoreboardDefault"));
            let hostname = "Garry's Mod";
            let tw = title_fm.text_width(hostname);
            let tx = (x0 + (bw - tw) * 0.5).floor();
            let ty = (y0 + (40.0 - title_fm.line_height) * 0.5).floor();
            items.push(UiItem::Text {
                font: Some("ScoreboardDefaultTitle".to_string()),
                x: tx + 2.0,
                y: ty + 2.0,
                text: hostname.to_string(),
                color: [0.0, 0.0, 0.0, 200.0 / 255.0],
            });
            items.push(UiItem::Text {
                font: Some("ScoreboardDefaultTitle".to_string()),
                x: tx,
                y: ty,
                text: hostname.to_string(),
                color: white,
            });
            let players = &self.sim.client.world.players;
            let mut sorted: Vec<_> = players.iter().collect();
            sorted.sort_by(|a, b| {
                b.frags
                    .cmp(&a.frags)
                    .then(a.deaths.cmp(&b.deaths))
                    .then(a.id.cmp(&b.id))
            });
            let grey = [93.0 / 255.0, 93.0 / 255.0, 93.0 / 255.0, 1.0];
            let row_h = 38.0;
            let mut y = y0 + 100.0;
            let bottom = h - 100.0;
            let centred = |text: &str, cx: f32, cy: f32| UiItem::Text {
                font: Some("ScoreboardDefault".to_string()),
                x: (cx - fm.text_width(text) * 0.5).floor(),
                y: (cy - fm.line_height * 0.5).floor(),
                text: text.to_string(),
                color: grey,
            };
            for p in sorted {
                if y + row_h > bottom {
                    break;
                }
                let rx = x0 + 2.0;
                let rw = bw - 4.0;
                let line = if p.alive {
                    [230.0 / 255.0, 230.0 / 255.0, 230.0 / 255.0, 1.0]
                } else {
                    [230.0 / 255.0, 200.0 / 255.0, 200.0 / 255.0, 1.0]
                };
                rounded_box(&mut items, 4.0, rx, y, rw, row_h, line);
                items.push(UiItem::Image {
                    x: rx + 3.0,
                    y: y + 3.0,
                    w: 32.0,
                    h: 32.0,
                    material: "vgui/avatar_default".to_string(),
                    color: white,
                    rot: 0,
                });
                let cy = y + row_h * 0.5;
                items.push(UiItem::Text {
                    font: Some("ScoreboardDefault".to_string()),
                    x: rx + 3.0 + 32.0 + 8.0,
                    y: (cy - fm.line_height * 0.5).floor(),
                    text: p.name.clone(),
                    color: grey,
                });
                let right = rx + rw - 3.0;
                let me = Some(p.id) == self.sim.client.player_id;
                if !me {
                    items.push(UiItem::Image {
                        x: right - 32.0,
                        y: y + 3.0,
                        w: 32.0,
                        h: 32.0,
                        material: "icon32/unmuted.png".to_string(),
                        color: white,
                        rot: 0,
                    });
                }
                // no ping is measured yet: the column stays blank
                items.push(centred(&p.deaths.to_string(), right - 82.0 - 25.0, cy));
                items.push(centred(&p.frags.to_string(), right - 132.0 - 25.0, cy));
                y += row_h + 2.0;
            }
        }
        if !self.console.open {
            // base cl_voice.lua: g_VoicePanelList at (ScrW - 250 - 50, ScrH * 0.13), 250 x
            // ScrH * 0.74, panels docked to its bottom - 250 x 40, margin 2, padding 4, a
            // draw.RoundedBox(4) whose green is the voice volume, the 32 px avatar and the
            // name (GModVoiceNotify) 8 px right of it; a panel stays while the player speaks
            let now = Instant::now();
            let fm = g.ui.font_metrics(Some("GModVoiceNotify"));
            let mut talkers: Vec<(String, f32)> = Vec::new();
            if self.voice.recording {
                let me = self
                    .sim
                    .client
                    .world
                    .players
                    .iter()
                    .find(|p| Some(p.id) == self.sim.client.player_id)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "Player".to_string());
                talkers.push((me, 1.0));
            }
            for p in &self.sim.client.world.players {
                if Some(p.id) == self.sim.client.player_id {
                    continue;
                }
                if let Some(t) = self.voice_seen.get(&p.id) {
                    if now.duration_since(*t).as_secs_f32() < 0.5 {
                        talkers.push((p.name.clone(), 1.0));
                    }
                }
            }
            let px = w - 250.0 - 50.0;
            let mut py = (h * 0.13 + h * 0.74).floor() - 2.0 - 40.0;
            for (name, volume) in talkers {
                let bx = px + 2.0;
                let bw = 250.0 - 4.0;
                rounded_box(
                    &mut items,
                    4.0,
                    bx,
                    py,
                    bw,
                    40.0,
                    [0.0, volume, 0.0, 240.0 / 255.0],
                );
                items.push(UiItem::Image {
                    x: bx + 4.0,
                    y: py + 4.0,
                    w: 32.0,
                    h: 32.0,
                    material: "vgui/avatar_default".to_string(),
                    color: white,
                    rot: 0,
                });
                items.push(UiItem::Text {
                    font: Some("GModVoiceNotify".to_string()),
                    x: bx + 4.0 + 32.0 + 8.0,
                    y: (py + (40.0 - fm.line_height) * 0.5).floor(),
                    text: name,
                    color: white,
                });
                py -= 40.0 + 4.0;
            }
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
                    // answered `HUDShouldDraw`: applied when the native HUD is built
                    gm_script::DrawCmd::HideHud { .. } => continue,
                    gm_script::DrawCmd::WorldTri { .. } => continue,
                    gm_script::DrawCmd::ScreenClicker { .. }
                    | gm_script::DrawCmd::Particle { .. }
                    | gm_script::DrawCmd::DLight { .. }
                    | gm_script::DrawCmd::OpenUrl { .. }
                    | gm_script::DrawCmd::Audio { .. }
                    | gm_script::DrawCmd::Servers { .. } => continue,
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
            // sandbox HUD: health and suit boxes at the bottom left; an element a script's
            // `HUDShouldDraw` answered false for this frame is skipped
            let hidden = |name: &str| {
                // gmod_camera's HUDShouldDraw: only the weapon selection and the chat (#96)
                (self.weapon == CAMERA_SLOT && name != "CHudWeaponSelection" && name != "CHudChat")
                    || self
                        .sim
                        .hud_draws
                        .iter()
                        .any(|d| matches!(d, gm_script::DrawCmd::HideHud { name: n } if n == name))
            };
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
                // ClientScheme FgColorHud 255 220 0 220 (ui.md §2.1, gap-report #153)
                let fg = [1.0, 220.0 / 255.0, 0.0, 220.0 / 255.0];
                let numbers = Some("HudNumbers".to_string());
                let small = Some("HudNumbersSmall".to_string());
                let label_font = Some("HudDefault".to_string());
                let y = 432.0 * s;
                let tall = 36.0 * s;
                let panel = |items: &mut Vec<UiItem>,
                             x: f32,
                             wide: f32,
                             label: &str,
                             fg: [f32; 4],
                             bg: [f32; 4]| {
                    hud_corner_box(items, x, y, wide, tall, s, bg);
                    items.push(UiItem::Text {
                        font: label_font.clone(),
                        x: x + 8.0 * s,
                        y: y + 20.0 * s,
                        text: label.to_string(),
                        color: fg,
                    });
                };
                // HudAnimations.txt: digits in TextColor, then `Blur` passes of the
                // HudNumbersGlow font (CHudNumericDisplay::PaintNumbers: one copy per whole
                // unit of Blur plus a copy at the fractional alpha; ui.md 2.2, #152), label
                // in FgColor, panel in BgColor, the whole panel scaled by Alpha
                let digits =
                    |items: &mut Vec<UiItem>, x: f32, text: String, look: &HudPanelLook| {
                        let a = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * look.alpha];
                        let mut fl = look.blur;
                        while fl > 0.0 {
                            let mut glow = a(look.text);
                            if fl < 1.0 {
                                glow[3] *= fl;
                            }
                            items.push(UiItem::Text {
                                font: Some("HudNumbersGlow".to_string()),
                                x,
                                y: y + 2.0 * s,
                                text: text.clone(),
                                color: glow,
                            });
                            fl -= 1.0;
                        }
                        items.push(UiItem::Text {
                            font: numbers.clone(),
                            x,
                            y: y + 2.0 * s,
                            text,
                            color: a(look.text),
                        });
                    };
                let hud_now = Instant::now();
                let (health_look, suit_look) = {
                    let mut hh = self.hud_health_anim.borrow_mut();
                    let mut sa = self.hud_suit_anim.borrow_mut();
                    hh.update(i32::from(p.health).max(0), hud_now, true);
                    sa.update(i32::from(p.armor).max(0), hud_now, false);
                    (hh.look(hud_now), sa.look(hud_now))
                };
                let ammo_look = {
                    let mut aa = self.hud_ammo_anim.borrow_mut();
                    aa.update(self.weapon, i32::from(p.clip), hud_now);
                    aa.look(hud_now)
                };
                // HudCrosshair: the weapon script's Crosshairs glyph (Q for every weapon the
                // loadout has) centred, ClientScheme Normal 255 208 64
                if p.alive
                    && self.menu.is_none()
                    && !self.context_held
                    && !self.main_menu
                    && !hidden("CHudCrosshair")
                {
                    let classes = gm_net::msg::weapon_classes(&self.sim.client.world.weapon_defs);
                    let class = classes.get(self.weapon as usize).map(String::as_str);
                    if class.is_some_and(weapon_has_crosshair_glyph) {
                        let cm = g.ui.font_metrics(Some("Crosshairs"));
                        // centred on the glyph's inked bounds (ui.md §7.2, gap-report #156),
                        // the advance / line box when the font has no bounds
                        let (cx, cy) = match cm.ink_bounds("Q") {
                            Some(b) => ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0),
                            None => (cm.text_width("Q") / 2.0, cm.line_height / 2.0),
                        };
                        items.push(UiItem::Text {
                            font: Some("Crosshairs".to_string()),
                            x: (w / 2.0 - cx).round(),
                            y: (h / 2.0 - cy).round(),
                            text: "Q".to_string(),
                            color: [1.0, 208.0 / 255.0, 64.0 / 255.0, 1.0],
                        });
                    } else {
                        // no glyph (stunstick, scripted weapons): the white crosshair_default,
                        // sprites/crosshairs (128x128) x 0 y 48 24x24 (ui.md §7.1, gap-report #157)
                        let sz = 24.0 * s;
                        items.push(UiItem::Slice {
                            x: w / 2.0 - sz / 2.0,
                            y: h / 2.0 - sz / 2.0,
                            w: sz,
                            h: sz,
                            material: "sprites/crosshairs".to_string(),
                            color: [1.0, 1.0, 1.0, 1.0],
                            uv: [0.0, 48.0 / 128.0, 24.0 / 128.0, 72.0 / 128.0],
                        });
                    }
                }
                // CHudZoom (ui.md 7.5, gap-report #158): shown while the camera FOV is below
                // fov_desired; alpha t/0.4 in, (1 - t/0.4) * 0.25 out with the circles shrinking
                // by 1 - scale * 0.5; everything x 64/255; two dashed circles r 66 / 74 (dash
                // every 16 px, 4 high) in ZoomReticleColor 255 220 0, the dark vgui/zoom quad
                // around them. Dashes are axis-aligned 2x4 rects (approximation).
                {
                    let cur_fov = self.fov_blend.map_or(self.settings.fov_desired, |b| b.3);
                    let zoomed = cur_fov < self.settings.fov_desired - 0.5;
                    let mut za = self.zoom_at.borrow_mut();
                    if zoomed && za.0.is_none() {
                        *za = (Some(Instant::now()), None);
                    } else if !zoomed && za.0.is_some() {
                        *za = (None, Some(Instant::now()));
                    }
                    let (alpha, scale) = match *za {
                        (Some(t), _) => ((t.elapsed().as_secs_f32() / 0.4).min(1.0), 1.0),
                        (None, Some(t)) => {
                            let k = (t.elapsed().as_secs_f32() / 0.4).min(1.0);
                            ((1.0 - k) * 0.25, 1.0 - k * 0.5)
                        }
                        _ => (0.0, 1.0),
                    };
                    if alpha > 0.0 && !hidden("CHudZoom") {
                        let a = alpha * 64.0 / 255.0;
                        let ratio = w / h;
                        // the dark edges: the vgui/zoom quadrant mirrored into the four
                        // corners (CHudZoom draws it as four quads, uv 1/255..254/255)
                        let _ = ratio;
                        let (u0, u1) = (1.0 / 255.0, 1.0 - 1.0 / 255.0);
                        for (qx, qy) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
                            let (ua, ub) = if qx == 0.0 { (u1, u0) } else { (u0, u1) };
                            let (va, vb) = if qy == 0.0 { (u0, u1) } else { (u1, u0) };
                            items.push(UiItem::Slice {
                                x: qx * w / 2.0,
                                y: qy * h / 2.0,
                                w: w / 2.0,
                                h: h / 2.0,
                                material: "vgui/zoom".to_string(),
                                color: [1.0, 1.0, 1.0, alpha],
                                uv: [ua, va, ub, vb],
                            });
                        }
                        let col = [1.0, 220.0 / 255.0, 0.0, a];
                        for r in [66.0_f32, 74.0] {
                            let r = (r * s * scale).round();
                            let n = (std::f32::consts::TAU * r / (16.0 * s)).round().max(4.0);
                            // each dash a 4x2 (layout px) quad tangent to the circle
                            let (hl, ht) = (2.0 * s, 1.0 * s);
                            for k in 0..n as usize {
                                let ang = std::f32::consts::TAU * k as f32 / n;
                                let (c, sn) = (ang.cos(), ang.sin());
                                let (cx, cy) = (w / 2.0 + c * r, h / 2.0 + sn * r);
                                // tangent (-sin, cos), radial (cos, sin)
                                let q = |a: f32, b: f32| [cx - sn * a + c * b, cy + c * a + sn * b];
                                let (p0, p1, p2, p3) =
                                    (q(-hl, -ht), q(hl, -ht), q(hl, ht), q(-hl, ht));
                                items.push(UiItem::Tri {
                                    pts: [p0, p1, p2],
                                    color: col,
                                });
                                items.push(UiItem::Tri {
                                    pts: [p0, p2, p3],
                                    color: col,
                                });
                            }
                        }
                    }
                }
                // HUD_PRINTCENTER through CHudMessage: centred lines in the Default font at
                // 65 % of the height, effect 0 (fade in 0.01 s, hold ~3 s, fade out 1.5 s).
                // The position/timing are the SDK custom-message defaults as recalled, not
                // read from the binary (gap-report #164, conf M).
                if let Some((text, at)) = &self.center_print {
                    let t = at.elapsed().as_secs_f32();
                    let a = if t < 3.0 { 1.0 } else { 1.0 - (t - 3.0) / 1.5 };
                    if a > 0.0 && !hidden("CHudMessage") {
                        let fm = g.ui.font_metrics(Some("Default"));
                        let lines: Vec<&str> = text.split('\n').collect();
                        let th = fm.line_height * lines.len() as f32;
                        for (k, l) in lines.iter().enumerate() {
                            items.push(UiItem::Text {
                                font: Some("Default".to_string()),
                                x: (w - fm.text_width(l)) / 2.0,
                                y: h * 0.65 - th / 2.0 + k as f32 * fm.line_height,
                                text: l.to_string(),
                                color: [1.0, 1.0, 1.0, a],
                            });
                        }
                    }
                }
                // gmod_tool SWEP:DrawHUD: the tool's name and description on a dark panel
                // at (50, 40), then its Information rows (LMB/RMB/R/E icons, text from
                // tool.<mode>.<row>) filtered by stage and operation
                if self.weapon == 1 && p.alive && self.menu.is_none() && !self.main_menu {
                    let mode = self.tool.clone();
                    let loc = |k: String| self.lang.get(&k).cloned();
                    let name = loc(format!("tool.{mode}.name")).unwrap_or_else(|| mode.clone());
                    let desc = loc(format!("tool.{mode}.desc")).unwrap_or_default();
                    let (x, y) = (50.0, 40.0);
                    let nm = g.ui.font_metrics(Some("GModToolName"));
                    let sm = g.ui.font_metrics(Some("GModToolSubtitle"));
                    let hm = g.ui.font_metrics(Some("GModToolHelp"));
                    let white = [1.0, 1.0, 1.0, 1.0];
                    let dark = |a: f32| [10.0 / 255.0, 10.0 / 255.0, 10.0 / 255.0, a / 255.0];
                    let title_h =
                        nm.line_height + if desc.is_empty() { 0.0 } else { sm.line_height };
                    items.push(UiItem::Rect {
                        x: x - 10.0,
                        y: y - 8.0,
                        w: 600.0,
                        h: title_h + 16.0,
                        color: dark(180.0),
                    });
                    items.push(UiItem::Text {
                        font: Some("GModToolName".to_string()),
                        x,
                        y,
                        text: name,
                        color: white,
                    });
                    if !desc.is_empty() {
                        items.push(UiItem::Text {
                            font: Some("GModToolSubtitle".to_string()),
                            x,
                            y: y + nm.line_height,
                            text: desc,
                            color: white,
                        });
                    }
                    let rows: Vec<(String, &str, bool)> = gm_server::tool_panels::panel(&mode)
                        .map(|pan| pan.info)
                        .unwrap_or(&[])
                        .iter()
                        .filter(|(_, stage, op)| {
                            (*stage < 0 || i64::from(*stage) == i64::from(p.tool_stage)) && *op <= 0
                        })
                        .filter_map(|(row, _, _)| {
                            let text = loc(format!("tool.{mode}.{row}"))?;
                            let icon = if row.starts_with("left") {
                                "gui/lmb.png"
                            } else if row.starts_with("right") {
                                "gui/rmb.png"
                            } else if row.starts_with("reload") {
                                "gui/r.png"
                            } else if row.starts_with("use") {
                                "gui/e.png"
                            } else {
                                "gui/info"
                            };
                            let use_too = !row.starts_with("use") && row.ends_with("use");
                            Some((text, icon, use_too))
                        })
                        .collect();
                    if !rows.is_empty() {
                        let lh = hm.line_height.max(18.0) + 4.0;
                        let ry = y + title_h + 16.0;
                        items.push(UiItem::Rect {
                            x: x - 10.0,
                            y: ry,
                            w: 600.0,
                            h: rows.len() as f32 * lh + 8.0,
                            color: dark(230.0),
                        });
                        for (i, (text, icon, use_too)) in rows.iter().enumerate() {
                            let yy = ry + 4.0 + i as f32 * lh;
                            let iy = yy + (lh - 4.0 - 16.0) / 2.0;
                            items.push(UiItem::Image {
                                x,
                                y: iy,
                                w: 16.0,
                                h: 16.0,
                                material: (*icon).to_string(),
                                color: white,
                                rot: 0,
                            });
                            let mut tx = x + 21.0;
                            if *use_too {
                                items.push(UiItem::Image {
                                    x: tx,
                                    y: iy,
                                    w: 16.0,
                                    h: 16.0,
                                    material: "gui/e.png".to_string(),
                                    color: white,
                                    rot: 0,
                                });
                                tx += 21.0;
                            }
                            items.push(UiItem::Text {
                                font: Some("GModToolHelp".to_string()),
                                x: tx,
                                y: yy,
                                text: text.clone(),
                                color: white,
                            });
                        }
                    }
                }
                // CHudDamageIndicator (hudlayout.res: dmg_xpos 30, dmg_ypos 100, dmg_wide 36,
                // dmg_tall1 240 outer / dmg_tall2 200 inner, DmgColor 255 0 0, alpha from the attack
                // strength): left / right trapezoids, front / rear a full-screen red wash; the SDK fades
                // each 2/s and drops it below 0.4. Flat quads stand in for the game's alpha gradient
                // (outer half full, inner half a third); the wash's 0.3 factor is not measured.
                {
                    if !hidden("CHudDamageIndicator") {
                        let red = |a: f32| [1.0, 0.0, 0.0, a];
                        let quad = |items: &mut Vec<UiItem>, pts: [[f32; 2]; 4], c: [f32; 4]| {
                            items.push(UiItem::Tri {
                                pts: [pts[0], pts[1], pts[2]],
                                color: c,
                            });
                            items.push(UiItem::Tri {
                                pts: [pts[0], pts[2], pts[3]],
                                color: c,
                            });
                        };
                        let (x0, y0, wd, t1, t2) =
                            (30.0 * s, 100.0 * s, 36.0 * s, 240.0 * s, 200.0 * s);
                        let inset = (t1 - t2) * 0.5;
                        for (side, c) in [
                            (2usize, self.dmg_side_color(0)),
                            (3, self.dmg_side_color(1)),
                        ] {
                            let Some(c) = c else { continue };
                            let xo = if side == 2 { x0 } else { w - x0 };
                            let dir = if side == 2 { 1.0 } else { -1.0 };
                            let xm = xo + dir * wd * 0.5;
                            let xi = xo + dir * wd;
                            quad(
                                &mut items,
                                [
                                    [xo, y0],
                                    [xm, y0 + inset * 0.5],
                                    [xm, y0 + t1 - inset * 0.5],
                                    [xo, y0 + t1],
                                ],
                                c,
                            );
                            quad(
                                &mut items,
                                [
                                    [xm, y0 + inset * 0.5],
                                    [xi, y0 + inset],
                                    [xi, y0 + t2 + inset],
                                    [xm, y0 + t1 - inset * 0.5],
                                ],
                                [c[0], c[1], c[2], c[3] / 3.0],
                            );
                        }
                        if let Some(wash) = self.dmg_full_alpha() {
                            items.push(UiItem::Rect {
                                x: 0.0,
                                y: 0.0,
                                w,
                                h,
                                color: red(0.3 * wash),
                            });
                        }
                    }
                }
                if !hidden("CHudHealth") {
                    panel(
                        &mut items,
                        16.0 * s,
                        102.0 * s,
                        "HEALTH",
                        health_look.fg,
                        health_look.bg,
                    );
                    digits(
                        &mut items,
                        16.0 * s + 50.0 * s,
                        p.health.max(0).to_string(),
                        &health_look,
                    );
                }
                if suit_look.alpha > 0.0 && !hidden("CHudBattery") {
                    let a = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * suit_look.alpha];
                    panel(
                        &mut items,
                        140.0 * s,
                        102.0 * s,
                        "SUIT",
                        a(suit_look.fg),
                        a(suit_look.bg),
                    );
                    digits(
                        &mut items,
                        140.0 * s + 50.0 * s,
                        p.armor.max(0).to_string(),
                        &suit_look,
                    );
                }
                // PaintNotes -> notification Think: spring the panels to the anchor at 80 %
                // height, charge left 4 % at 0.7 s left, leave right at 0.2 s
                self.notices.borrow_mut().draw(&mut items, w, h, &g.ui);
                if !hidden("CHudWeaponSelection") {
                    self.draw_weapon_select(&mut items, w, h, &g.ui);
                }
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
                // cl_worldtips.lua: the world tip of the scripted entity under the crosshair
                // within its MaxWorldTipDistance 256 (base_gmodentity); GModWorldtip text in a
                // rounded 250 250 200 box, black 2 px rim, a pointer towards the entity's origin,
                // "\n(owner)" appended outside singleplayer; cl_drawworldtooltips 1
                if !self.sim.client.world.overlays.is_empty() {
                    let (pt, yw) = (self.view.x.to_radians(), self.view.y.to_radians());
                    let fwd = Vec3::new(pt.cos() * yw.cos(), pt.cos() * yw.sin(), -pt.sin());
                    let world_d = (self
                        .sim
                        .collision
                        .trace_ray(o, o + fwd * 256.0, gm_collision::MASK_SOLID)
                        .end
                        - o)
                        .length();
                    let mut best: Option<(f32, u32)> = None;
                    for (id, _) in &self.sim.client.world.overlays {
                        let Some(e) = self.sim.client.world.entities.get(id) else {
                            continue;
                        };
                        let Some((lo, hi)) = g
                            .models
                            .get(e.model.as_str())
                            .and_then(|m| m.as_ref())
                            .map(|m| m.bounds())
                        else {
                            continue;
                        };
                        // the model's rest bounds as a world box around the origin (the game
                        // traces against the collision model; the local box is close enough)
                        let r = lo.abs().max(hi.abs());
                        let (mins, maxs) = (e.pos - r, e.pos + r);
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
                            best = Some((t0, *id));
                        }
                    }
                    if let Some((_, id)) = best {
                        let e = &self.sim.client.world.entities[&id];
                        let mut text = self
                            .sim
                            .client
                            .world
                            .overlays
                            .iter()
                            .find(|(i, _)| *i == id)
                            .map_or(String::new(), |(_, t)| t.clone());
                        if self.sim.remote {
                            if let Some(name) = self
                                .sim
                                .client
                                .world
                                .players
                                .iter()
                                .find(|q| q.id == e.owner)
                                .map(|q| q.name.as_str())
                                .filter(|n| !n.is_empty())
                            {
                                text = format!("{text}\n({name})");
                            }
                        }
                        let clip = view_proj * e.pos.extend(1.0);
                        if clip.w > 0.0 {
                            let px = (clip.x / clip.w * 0.5 + 0.5) * w;
                            let py = (0.5 - clip.y / clip.w * 0.5) * h;
                            let fm = g.ui.font_metrics(Some("GModWorldtip"));
                            let lines: Vec<&str> = text.split('\n').collect();
                            let tw = lines
                                .iter()
                                .map(|l| fm.text_width(l))
                                .fold(0.0f32, f32::max);
                            let th = fm.line_height * lines.len() as f32;
                            let (pad, off) = (10.0, 50.0);
                            let (x, y) = (px - tw - off, py - th - off);
                            let bg = [0.0, 0.0, 0.0, 1.0];
                            let fg = [250.0 / 255.0, 250.0 / 255.0, 200.0 / 255.0, 1.0];
                            rounded_box(
                                &mut items,
                                8.0,
                                x - pad - 2.0,
                                y - pad - 2.0,
                                tw + pad * 2.0 + 4.0,
                                th + pad * 2.0 + 4.0,
                                bg,
                            );
                            items.push(UiItem::Tri {
                                pts: [
                                    [x + tw / 1.5 - 2.0, y + th + 2.0],
                                    [x + tw + 2.0, y + th / 2.0 - 1.0],
                                    [px - off / 2.0 + 2.0, py - off / 2.0 + 2.0],
                                ],
                                color: bg,
                            });
                            rounded_box(
                                &mut items,
                                8.0,
                                x - pad,
                                y - pad,
                                tw + pad * 2.0,
                                th + pad * 2.0,
                                fg,
                            );
                            items.push(UiItem::Tri {
                                pts: [
                                    [x + tw / 1.5, y + th],
                                    [x + tw, y + th / 2.0],
                                    [px - off / 2.0, py - off / 2.0],
                                ],
                                color: fg,
                            });
                            for (n, line) in lines.iter().enumerate() {
                                items.push(UiItem::Text {
                                    font: Some("GModWorldtip".to_string()),
                                    x: x + tw / 2.0 - fm.text_width(line) / 2.0,
                                    y: y + fm.line_height * n as f32,
                                    text: line.to_string(),
                                    color: bg,
                                });
                            }
                        }
                    }
                }
                // CHudSuitPower (ui.md §6, gap-report #155): with gmod_suit 0 the power never
                // drains (player.md D6), so the panel only shows while a device is active -
                // sprint (moving with the speed key), flashlight, oxygen (eyes under water) -
                // and fades out over 0.4 s when none is. x 16 y 396, 102x26 plus 10 px per
                // device label (the growth per label is an approximation), AUX POWER label at
                // (8,4), the 92x4 bar at (8,15) in 6 px chunks with 3 px gaps, all lit at 100.
                {
                    let under = self.sim.collision.point_contents(o)
                        & gm_formats::bsp::lumps::contents::WATER
                        != 0;
                    // CHudSuitPower::ShouldDraw needs the HEV suit; sandbox players have none
                    // with the game's default gmod_suit 0, so the panel never appears
                    let devices: Vec<&str> = [
                        (self.shift_held && p.velocity.length() > 0.5, "SPRINT"),
                        (p.flashlight, "FLASHLIGHT"),
                        (under, "OXYGEN"),
                    ]
                    .into_iter()
                    .filter(|_| GMOD_SUIT)
                    .filter_map(|(on, l)| on.then_some(l))
                    .collect();
                    let mut off = self.aux_off_at.borrow_mut();
                    let alpha = if devices.is_empty() {
                        let at = *off.get_or_insert_with(Instant::now);
                        1.0 - (at.elapsed().as_secs_f32() / 0.4).min(1.0)
                    } else {
                        *off = None;
                        1.0
                    };
                    if alpha > 0.0 && !hidden("CHudSuitPower") {
                        let a = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * alpha];
                        let (ax, ay) = (16.0 * s, 396.0 * s);
                        let ah = 26.0 * s + 10.0 * s * devices.len() as f32;
                        hud_corner_box(&mut items, ax, ay, 102.0 * s, ah, s, a(BG_COLOR_HUD));
                        items.push(UiItem::Text {
                            font: label_font.clone(),
                            x: ax + 8.0 * s,
                            y: ay + 4.0 * s,
                            text: "AUX POWER".to_string(),
                            color: a(FG_COLOR_HUD),
                        });
                        for k in 0..10 {
                            items.push(UiItem::Rect {
                                x: ax + 8.0 * s + k as f32 * 9.0 * s,
                                y: ay + 15.0 * s,
                                w: 6.0 * s,
                                h: 4.0 * s,
                                color: a(FG_COLOR_HUD),
                            });
                        }
                        for (k, l) in devices.iter().enumerate() {
                            items.push(UiItem::Text {
                                font: label_font.clone(),
                                x: ax + 8.0 * s,
                                y: ay + 22.0 * s + 10.0 * s * k as f32,
                                text: l.to_string(),
                                color: a(FG_COLOR_HUD),
                            });
                        }
                    }
                }
                if (p.clip >= 0 || p.reserve > 0) && !hidden("CHudAmmo") {
                    let ax = w - 150.0 * s;
                    panel(
                        &mut items,
                        ax,
                        136.0 * s,
                        "AMMO",
                        FG_COLOR_HUD,
                        ammo_look.bg,
                    );
                    let clip = if p.clip >= 0 { p.clip } else { p.reserve };
                    digits(&mut items, ax + 40.0 * s, clip.to_string(), &ammo_look);
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
                        panel(&mut items, bx, 60.0 * s, "ALT", FG_COLOR_HUD, BG_COLOR_HUD);
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
        if self.console.open {
            self.console_items(g, w, h, lh, &mut items);
        }
        items
    }

    /// The developer console over everything else (menus included): the top half of the
    /// screen, the log lines above the input line.
    fn console_items(&self, g: &Gfx, w: f32, h: f32, lh: f32, items: &mut Vec<UiItem>) {
        let white = [0.92, 0.92, 0.92, 1.0];
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
    }

    fn toggle_console(&mut self) {
        self.console.open = !self.console.open;
        self.console.opened_at = Instant::now();
        self.keys.clear();
        self.mouse.clear();
        let open = self.console.open;
        // closing it under an open menu leaves the mouse to the menu
        self.set_grab(!open && self.menu.is_none() && !self.context_held && !self.main_menu);
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
                                    gm_server::ToolSettingKind::Text
                                    | gm_server::ToolSettingKind::Help
                                    | gm_server::ToolSettingKind::Key
                                    | gm_server::ToolSettingKind::Color
                                    | gm_server::ToolSettingKind::Choice
                                    | gm_server::ToolSettingKind::Presets
                                    | gm_server::ToolSettingKind::Button => " text".into(),
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
            // `cl_playercolor r g b` / `cl_weaponcolor r g b` (0..1): both ride in one command,
            // an empty string keeps the other one
            "playercolor" | "cl_playercolor" if rest.len() >= 3 => {
                self.sim.client.command(Command::PlayerColors {
                    body: rest[..3].join(" "),
                    weapon: String::new(),
                });
            }
            "weaponcolor" | "cl_weaponcolor" if rest.len() >= 3 => {
                self.sim.client.command(Command::PlayerColors {
                    body: String::new(),
                    weapon: rest[..3].join(" "),
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
            // the game's spawnmenu console commands, usable by hand
            "gmod_spawnnpc" if !rest.is_empty() => self.sim.client.command(Command::SpawnNpc {
                key: rest[0].to_string(),
                weapon: rest.get(1).map(|w| w.to_string()).unwrap_or_default(),
            }),
            "gmod_spawnvehicle" if !rest.is_empty() => {
                self.sim.client.command(Command::SpawnVehicle {
                    key: rest[0].to_string(),
                })
            }
            "gm_spawnsent" if !rest.is_empty() => self.sim.client.command(Command::SpawnSent {
                class: rest[0].to_string(),
            }),
            "ent_fire" if rest.len() >= 2 => self.sim.client.command(Command::EntFire {
                target: rest[0].to_string(),
                input: rest[1].to_string(),
                param: rest[2..].join(" "),
            }),
            // a weapon by class, scripted ones included (their index follows the stock table)
            "weapon" if !rest.is_empty() => {
                let class = rest.join(" ");
                let classes = gm_net::msg::weapon_classes(&self.sim.client.world.weapon_defs);
                match classes.iter().position(|c| *c == class) {
                    Some(i) if i < 255 => self.select_weapon(i as u8),
                    _ => self.console.log(format!("unknown weapon {class}")),
                }
            }
            "say" | "say_team" => self.sim.client.command(Command::Chat {
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
        self.sounds.mixer.set_master(self.settings.volume);
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
                    let old_height = g.config.height;
                    g.config.width = s.width.max(1);
                    g.config.height = s.height.max(1);
                    g.surface.configure(&g.gpu.device, &g.config);
                    let (w, h, f, n) = (
                        g.config.width,
                        g.config.height,
                        g.config.format,
                        g.world.quality.msaa,
                    );
                    g.depth = depth_view(&g.gpu, w, h, f, n);
                    g.msaa_color = msaa_view(&g.gpu, w, h, f, n);
                    g.post.resize(&g.gpu, g.config.width, g.config.height);
                    if g.config.height != old_height {
                        register_ui_fonts(&self.mount, &mut g.ui, &g.gpu, g.config.height);
                        for (name, family, size) in &self.script_fonts {
                            if g.ui.register_font(&g.gpu, &self.mount, name, family, *size) {
                                let m = g.ui.font_metrics(Some(name));
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
                        let m = g.ui.font_metrics(None);
                        gm_script::set_font_metrics(
                            "",
                            gm_script::TextMetrics {
                                line_height: m.line_height,
                                advances: m.advances,
                                fallback: m.fallback,
                            },
                        );
                    }
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
                if matches!(code, Some(KeyCode::ShiftLeft | KeyCode::ShiftRight)) {
                    self.shift_held = down;
                }
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
                if self.main_menu && self.newgame.as_ref().is_some_and(|n| n.focus.is_some()) {
                    if down {
                        self.newgame_key(code, event.text.as_deref());
                    }
                    return;
                }
                if self.main_menu && self.servers.as_ref().is_some_and(|s| s.focus) {
                    if down {
                        self.servers_key(code, event.text.as_deref());
                    }
                    return;
                }
                if self.main_menu && self.addons.as_ref().is_some_and(|a| a.focus) {
                    if down {
                        self.addons_key(code, event.text.as_deref());
                    }
                    return;
                }
                // the key holding the menu open (+menu) keeps going to the binds
                let holds_menu = code
                    .and_then(settings::key_name)
                    .and_then(|n| self.settings.binds.get(n))
                    .is_some_and(|c| c == "+menu");
                // a spawnlist being renamed in its tree row: Enter keeps the name, Escape drops it
                if self.menu.as_ref().is_some_and(|m| m.st.rename.is_some()) && !holds_menu {
                    if down && !event.repeat {
                        let mut save = false;
                        if let Some(m) = self.menu.as_mut() {
                            let mut done: Option<Option<(usize, String)>> = None;
                            if let Some((i, name)) = m.st.rename.as_mut() {
                                match code {
                                    Some(KeyCode::Enter | KeyCode::NumpadEnter) => {
                                        done = Some(Some((*i, name.trim().to_string())));
                                    }
                                    Some(KeyCode::Escape) => done = Some(None),
                                    Some(KeyCode::Backspace) => {
                                        name.pop();
                                    }
                                    _ => {
                                        if let Some(t) = event.text.as_deref() {
                                            name.extend(t.chars().filter(|c| !c.is_control()));
                                            name.truncate(48);
                                        }
                                    }
                                }
                            }
                            if let Some(d) = done {
                                m.st.rename = None;
                                if let Some((i, n)) = d {
                                    if let Some(l) = self.spawnlists.get_mut(i) {
                                        if !n.is_empty() {
                                            l.name = n;
                                            save = true;
                                        }
                                    }
                                }
                            }
                        }
                        if save {
                            if let Err(e) = derma::save_spawnlists(&self.spawnlists) {
                                eprintln!("spawnlist: {e}");
                            }
                        }
                    }
                    return;
                }
                if self.menu.as_ref().is_some_and(|m| m.st.search_focus) && !holds_menu {
                    if down && !event.repeat {
                        let mut refresh = false;
                        if let Some(m) = self.menu.as_mut() {
                            match code {
                                Some(KeyCode::Enter | KeyCode::NumpadEnter) => refresh = true,
                                Some(KeyCode::Escape) => {
                                    m.st.search_focus = false;
                                    m.st.search.clear();
                                    refresh = true;
                                }
                                Some(KeyCode::Backspace) => {
                                    m.st.search.pop();
                                    refresh = true;
                                }
                                _ => {
                                    if let Some(t) = event.text.as_deref() {
                                        let n = m.st.search.len();
                                        m.st.search.extend(t.chars().filter(|c| !c.is_control()));
                                        m.st.search.truncate(64);
                                        refresh = m.st.search.len() != n;
                                    }
                                }
                            }
                        }
                        if refresh {
                            self.refresh_search();
                            if let Some(m) = self.menu.as_mut() {
                                m.st.scroll = 0.0;
                            }
                        }
                    }
                    return;
                }
                if self
                    .options
                    .as_ref()
                    .is_some_and(|o| o.entry_focus.is_some())
                {
                    if down {
                        if let Some(o) = self.options.as_mut() {
                            match code {
                                Some(KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Tab) => {
                                    o.commit_entry()
                                }
                                Some(KeyCode::Escape) => {
                                    o.entry_focus = None;
                                    o.entry_text.clear();
                                }
                                Some(KeyCode::Backspace) => {
                                    o.entry_text.pop();
                                }
                                _ => {
                                    if let Some(t) = event.text.as_deref() {
                                        o.entry_text.extend(
                                            t.chars().filter(|c| c.is_ascii_digit() || *c == '.'),
                                        );
                                        o.entry_text.truncate(8);
                                    }
                                }
                            }
                        }
                    }
                    return;
                }
                let Some(code) = code else { return };
                if !event.repeat {
                    self.pending_input.push((format!("{code:?}"), down));
                }
                if down && !event.repeat {
                    match code {
                        _ if self.options.as_ref().is_some_and(|o| o.waiting_key) => {
                            if code == KeyCode::Escape {
                                if let Some(o) = self.options.as_mut() {
                                    o.waiting_key = false;
                                }
                            } else if let Some(name) = settings::key_name(code) {
                                self.options_bind(name);
                            }
                        }
                        KeyCode::Escape
                            if self.options.as_ref().is_some_and(|o| o.sub.is_some()) =>
                        {
                            if let Some(o) = self.options.as_mut() {
                                if let Some(b) = o.sub_backup.take() {
                                    o.pending = b;
                                }
                                o.sub = None;
                            }
                        }
                        KeyCode::Escape if self.options.is_some() => self.options = None,
                        KeyCode::Escape if self.newgame.is_some() => self.newgame = None,
                        KeyCode::Escape if self.servers.is_some() => self.servers = None,
                        KeyCode::Escape if self.ugc.is_some() => self.ugc = None,
                        KeyCode::Escape if self.addons.is_some() => self.addons = None,
                        KeyCode::Escape if self.main_menu && !self.started => {}
                        KeyCode::Escape if self.main_menu => {
                            self.main_menu = false;
                            self.set_grab(true);
                        }
                        KeyCode::Escape if self.grabbed => {
                            self.set_grab(false);
                            self.main_menu = true;
                        }
                        KeyCode::Escape => el.exit(),
                        _ => {
                            if let Some(cmd) = self.bound(code) {
                                self.run_bind(&cmd, true);
                            }
                        }
                    }
                }
                if down {
                    self.keys.insert(code);
                } else {
                    self.keys.remove(&code);
                    if let Some(cmd) = self.bound(code) {
                        self.run_bind(&cmd, false);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                self.context_hover();
                self.panel_drag_move();
                self.opt_drag_move();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(p) => p.y as f32 / 16.0,
                };
                let ng_max = self.menu_cache.borrow().ng_max_scroll;
                let screen_w = self.gfx.as_ref().map_or(1280.0, |g| g.config.width as f32);
                let in_menu = self.main_menu;
                let cursor_x = self.cursor.0;
                let me = self.sim.client.player_id;
                let hold_wheel = !in_menu
                    && self.options.is_none()
                    && self.weapon == 0
                    && self
                        .sim
                        .client
                        .world
                        .players
                        .iter()
                        .any(|p| Some(p.id) == me && p.held.is_some());
                if hold_wheel {
                    self.wheel_delta += lines;
                } else if let Some(ng) = self.newgame.as_mut().filter(|_| in_menu) {
                    if cursor_x > screen_w - 16.0 - 226.0 {
                        ng.settings_scroll = (ng.settings_scroll - lines * 40.0).max(0.0);
                    } else {
                        ng.scroll = (ng.scroll - lines * 64.0).clamp(0.0, ng_max);
                    }
                } else if let Some(o) = &mut self.options {
                    if o.waiting_key {
                        let key = if lines > 0.0 {
                            "MWHEELUP"
                        } else {
                            "MWHEELDOWN"
                        };
                        self.options_bind(key);
                    } else {
                        o.kb_scroll = (o.kb_scroll - lines * 60.0).max(0.0);
                    }
                } else if let Some(menu) = &mut self.menu {
                    let max = self.menu_cache.borrow().max_scroll;
                    menu.st.scroll = (menu.st.scroll - lines * 64.0).clamp(0.0, max);
                } else if !self.console.open && !self.context_held && self.grabbed {
                    // MWHEELUP / MWHEELDOWN binds (invprev / invnext by default)
                    let key = if lines > 0.0 {
                        "MWHEELUP"
                    } else {
                        "MWHEELDOWN"
                    };
                    if lines != 0.0 {
                        if let Some(cmd) = self.settings.binds.get(key).cloned() {
                            self.run_bind(&cmd, true);
                            self.run_bind(&cmd, false);
                        }
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
                if self.main_menu {
                    if state == ElementState::Released {
                        self.opt_drag = None;
                    } else if self.options.as_ref().is_some_and(|o| o.waiting_key) {
                        let b = match button {
                            MouseButton::Left => 0,
                            MouseButton::Right => 1,
                            MouseButton::Middle => 2,
                            MouseButton::Back => 3,
                            MouseButton::Forward => 4,
                            MouseButton::Other(_) => return,
                        };
                        self.options_bind(settings::mouse_name(b));
                    } else if button == MouseButton::Left {
                        self.main_menu_click();
                    }
                    return;
                }
                if self.menu.is_some() {
                    if state == ElementState::Released && button == MouseButton::Left {
                        self.menu_drop();
                        return;
                    }
                    if state == ElementState::Pressed && button == MouseButton::Right {
                        self.menu_right_click();
                        return;
                    }
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        self.menu_drag_start();
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
                                if self.player_editor_click() {
                                    return;
                                }
                                if self.panel_click() {
                                    return;
                                }
                                if let Some(name) = self.context_row() {
                                    if name == "editentity" {
                                        self.edit_sheet =
                                            self.sim.client.properties.as_ref().map(|p| p.0);
                                    } else {
                                        self.sim.client.command(Command::Property { name });
                                    }
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
                            // CloseWeaponSelectionMenu: Player.WeaponSelectionClose
                            self.sounds
                                .emit(&self.mount, "Player.WeaponSelectionClose", None);
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
                    MouseButton::Middle => 2,
                    MouseButton::Back => 3,
                    MouseButton::Forward => 4,
                    MouseButton::Other(_) => return,
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
                            // the shared PrimaryAttack is predicted (toolgun.md F13/F15,
                            // gap-report #91): the first prediction plays Toolgun.Single and
                            // ACT_VM_PRIMARYATTACK at once when the trace lands; a stool
                            // refusing the hit is not predicted (the game runs the stool's
                            // LeftClick on the client, our stools are server-side)
                            if b < 2 && self.predict_tool_hit() {
                                if let Some(vm) = self.vm_of(1) {
                                    self.vm_anim = Some((vm.fire, Instant::now()));
                                }
                                self.sounds.emit(&self.mount, "Toolgun.Single", None);
                                self.tool_predicted = Some(Instant::now());
                            }
                        }
                        // gmod_camera `PrimaryAttack` (toolgun.md F35/F39, gap-report #95):
                        // 0.4 s between photos, the attack animation, then the screenshot
                        if self.weapon == CAMERA_SLOT && b == 0 {
                            let now = Instant::now();
                            if self
                                .cam_photo_at
                                .is_none_or(|t| now.duration_since(t).as_secs_f32() >= 0.4)
                            {
                                self.cam_photo_at = Some(now);
                                self.cam_photo_pending = true;
                                if let Some(vm) = self.vm_of(CAMERA_SLOT) {
                                    self.vm_anim = Some((vm.fire, now));
                                }
                            }
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
            let me = self.sim.client.player_id;
            let rotating = self.weapon == 0
                && self.down("+use")
                && self
                    .sim
                    .client
                    .world
                    .players
                    .iter()
                    .any(|p| Some(p.id) == me && p.held.is_some());
            let cam_turn = self.weapon == CAMERA_SLOT && self.down("+attack2");
            if self.grabbed && rotating {
                self.rotate_delta.0 += delta.0 as f32;
                self.rotate_delta.1 += delta.1 as f32;
            } else if self.grabbed && cam_turn {
                // gmod_camera `Tick` (toolgun.md F40): with +attack2 the mouse zooms
                // (y * dt * 6.6, 0.1..175) and rolls (x * dt * 1.65) instead of turning
                let now = Instant::now();
                let dt = now.duration_since(self.cam_at).as_secs_f32().min(0.1);
                self.cam_at = now;
                if self.cam_zoom <= 0.0 {
                    self.cam_zoom = self.settings.fov_desired;
                }
                self.cam_zoom = (self.cam_zoom + delta.1 as f32 * dt * 6.6).clamp(0.1, 175.0);
                self.cam_roll += delta.0 as f32 * dt * 1.65;
            } else if self.grabbed {
                // m_yaw / m_pitch 0.022 * sensitivity (3 by default), scaled while zoomed by
                // zoom_sensitivity_ratio (1) * fov / default fov (render.md §1.2, #143)
                let zoom = self
                    .fov_blend
                    .map_or(1.0, |(_, _, _, cur)| {
                        cur / self.settings.fov_desired.max(1.0)
                    })
                    .clamp(0.01, 1.0);
                let k = 0.022 * self.settings.sensitivity * zoom;
                let pitch = if self.settings.m_pitch_invert { -k } else { k };
                self.view.y -= delta.0 as f32 * k;
                self.view.x = (self.view.x + delta.1 as f32 * pitch).clamp(-89.0, 89.0);
                self.view.y = self.view.y.rem_euclid(360.0);
            }
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        self.step();
        if let Some(sv) = &mut self.servers {
            sv.poll();
        }
        if let Some(a) = &mut self.addons {
            a.poll();
        }
        if self.settings.fps_max > 0.0 {
            let min = std::time::Duration::from_secs_f32(1.0 / self.settings.fps_max);
            let since = self.last_frame.elapsed();
            if since < min {
                std::thread::sleep(min - since);
            }
        }
        self.last_frame = Instant::now();
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
    // like the game: a map on the command line (`+map`) or a server to join goes straight into
    // the game, otherwise the main menu comes first
    let started_at_launch = positional.is_some() || connect.is_some();
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
    let mut mount = gm_content::Mount::for_install(&inst)?;
    let sounds = gm_audio::Sounds::new(&mount);
    eprintln!(
        "audio: {} ({} script entries)",
        sounds.mixer.device,
        sounds.scripts.len()
    );
    let bsp_data = mount.read(&format!("maps/{map}.bsp"))?;
    if let Ok(pak) = gm_formats::bsp::pakfile(&bsp_data) {
        mount.add_pak(&format!("maps/{map}.bsp pak"), pak)?;
    }
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
        map_props,
        collision,
        physics,
        world_body,
        nav,
        spawn_points,
        ai_nodes,
        func_tanks,
        ai_graph,
        map_revision,
        sun_direction,
        static_convexes,
        map_entities,
        surface,
    } = gm_dedicated::load_world(&mount, &map)?;
    let collision: &'static gm_collision::World = Box::leak(Box::new(collision));
    let mut server = gm_server::Server::new(collision, physics, world_body);
    server.set_navmesh(nav);
    server.static_convexes = static_convexes;
    server.set_map_entities(map_entities, surface);
    server.set_ai_nodes(ai_nodes);
    server.set_func_tanks(func_tanks);
    if let Some(g) = &ai_graph {
        server.set_ai_graph(gm_server::AiGraph::from_ain(g));
    }
    server.spawn_points = spawn_points;
    server.map_revision = map_revision;
    server.sun_direction = sun_direction;
    // cfg/server.cfg: what the New Game page's Start Game set (gamemode settings, maxplayers)
    if let Ok(text) = std::fs::read_to_string(settings::home_dir().join("cfg").join("server.cfg")) {
        for line in text.lines() {
            let mut parts = line.splitn(2, ' ');
            if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
                server.set_cvar(k, v.trim().trim_matches('"'));
            }
        }
    }
    let models = gm_content::Mount::for_install(&inst)?;
    server.model_loader = Some(Box::new(move |m: &str| {
        gm_dedicated::model_info(&models, m)
            .map_err(|e| eprintln!("model {m}: {e}"))
            .ok()
    }));
    // the map's prop_physics / prop_dynamic entities as server props (props.md #10-14)
    for p in &map_props {
        server.script_spawn_prop(0, &p.model, p.pos, p.angles, p.frozen);
    }
    let (steps_mount, table) = (
        gm_content::Mount::for_install(&inst)?,
        gm_dedicated::surface_table(&mount)?,
    );
    server.step_loader = Some(Box::new(move |m: Option<&str>| {
        gm_dedicated::step_sounds(&steps_mount, &table, m)
    }));
    let props_mount = gm_content::Mount::for_install(&inst)?;
    server.surfaceprop_loader = Some(Box::new(move |m: &str| {
        gm_dedicated::material_surfaceprop(&props_mount, m)
    }));
    server.mounted = mount.providers().iter().map(|p| p.label()).collect();
    let sound_mount = gm_content::Mount::for_install(&inst)?;
    let sound_duration: gm_script::api::SoundDuration =
        std::sync::Arc::new(move |n| gm_dedicated::wav_duration(&sound_mount, n));
    let mut net = ServerNet::new(&map, spawn);
    net.admins.extend(1..=16);
    net.saves_dir = Some(settings::home_dir().join("saves"));
    // `--addon <dir|file.gmr>` (repeatable): addons of the in-process server; their client realm
    // reaches the client realm below over the loopback like from a dedicated server
    let mut rt = gm_script::Runtime::new();
    rt.set_sound_duration(sound_duration.clone());
    let content_mount = gm_content::Mount::for_install(&inst)?;
    rt.set_content_read(std::sync::Arc::new(move |p| content_mount.read(p).ok()));
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
    // the addons installed from the menu (`~/.gmod-rust/addons`), minus the disabled ones
    let disabled = addons::load_disabled();
    for a in addons::scan_installed() {
        if disabled.contains(&a.id) {
            continue;
        }
        match rt.load_addon(&a.path) {
            Ok(m) => println!("addon {} {} loaded", m.id, m.version),
            Err(e) => eprintln!("addon {}: {e}", a.path.display()),
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
        // cl_interp_amount = max(cl_interp 0, cl_interp_ratio 2 / cl_updaterate 20) = 0.1 s;
        // the entity window is TICKS_TO_TIME(TIME_TO_TICKS(0.1) + 1) = 7 ticks at 66 Hz
        // (render.md §12, gap-report #151)
        interp: gm_net::Interpolator::new(7.0),
        player_interp: gm_net::Interpolator::new(7.0),
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
    let settings = settings::Settings::load(&mount);
    let mut lang = BTreeMap::new();
    for f in mount.find("resource/localization/en/*.properties") {
        lang.extend(derma::load_properties(&mount, &f));
    }
    let kb_rows = settings::load_kb_act(&mount);
    let mut gameui = BTreeMap::new();
    for f in ["valve", "gameui", "garrysmod"] {
        settings::load_tokens(&mount, &format!("resource/{f}_english.txt"), &mut gameui);
    }
    let gamematerials = load_gamematerials(&mount);
    let mut app = App {
        notices: std::cell::RefCell::new(Notices::default()),
        wsel: None,
        wsel_at: None,
        center_print: None,
        zoom_at: Default::default(),
        aux_off_at: Default::default(),
        lua_dlights: HashMap::new(),
        saved_menu: None,
        utility_values: BTreeMap::new(),
        wsel_opened: None,
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
        hud_health_anim: Default::default(),
        hud_suit_anim: Default::default(),
        hud_ammo_anim: Default::default(),
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
        impulse: 0,
        rotate_delta: (0.0, 0.0),
        wheel_delta: 0.0,
        vm_anim: None,
        vm_last: None,
        vm_event_t: None,
        fx_brass_pending: None,
        fx_shells: Vec::new(),
        fx_shells_at: Instant::now(),
        vm_view: VmView::new(),
        vm_prev: None,
        vm_serial_seen: 0,
        fx_tracers: Vec::new(),
        fx_ar2_impacts: Vec::new(),
        xbow_charge_since: None,
        fx_explosion_lights: Vec::new(),
        other_muzzles: HashMap::new(),
        fx_decals: Default::default(),
        fx_prop_decals: Default::default(),
        fx_bursts: Vec::new(),
        fx_cannon: Vec::new(),
        fx_tool: Vec::new(),
        fx_rings: Vec::new(),
        cam_zoom: 0.0,
        cam_roll: 0.0,
        cam_at: Instant::now(),
        claw_pose: 0.0,
        claw_at: Instant::now(),
        gg_core_alpha: 0.0,
        gg_effect: 0,
        tool_predicted: None,
        cam_photo_at: None,
        cam_photo_pending: false,
        room_at: Instant::now() - std::time::Duration::from_secs(1),
        fx_sprites: Vec::new(),
        fx_seed: 0x9e37_79b9,
        fx_sound_queue: Vec::new(),
        fx_muzzle_until: None,
        fx_vm_muzzle: Vec::new(),
        fx_muzzle_light: None,
        fx_muzzle_spawned: None,
        gamematerials,
        tool_screen: ToolScreen::default(),
        weapon_synced: false,
        air_since: Default::default(),
        anim_time: Default::default(),
        gestures: Default::default(),
        sounds,
        voice: voice::VoiceChat::default(),
        soundscapes: gm_audio::Soundscapes::default(),
        occluder_hulls: std::collections::HashMap::new(),
        last_render: Instant::now(),
        menu: None,
        spawnlists: Vec::new(),
        shift_held: false,
        qm_dupes: ugc::UgcState::new("dupes", &lang),
        qm_saves: ugc::UgcState::new("saves", &lang),
        qm_ugc_dirty: 0,
        player_editor: false,
        lang,
        menu_cache: Default::default(),
        context_held: false,
        scoreboard: false,
        voice_seen: HashMap::new(),
        death_at: None,
        death_view: Vec3::ZERO,
        dmg_side: [None; 2],
        dmg_full: None,
        context: None,
        scripted_defs: Vec::new(),
        scripted_vms: Vec::new(),
        scripted_world: Vec::new(),
        pending_input: Vec::new(),
        spawn_lists_seen: usize::MAX,
        content: [Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()],
        pp_enabled: Default::default(),
        content_seen: usize::MAX,
        content_lang: None,
        search_models: None,
        search_results: derma::SearchResults::default(),
        cursor: (0.0, 0.0),
        panel_drag: None,
        edit_sheet: None,
        player_skin: 0,
        player_body: String::new(),
        panel_text: None,
        grabbed: false,
        screen_clicker: false,
        script_fonts: Vec::new(),
        audio_channels: Default::default(),
        script_servers: Vec::new(),
        main_menu: !started_at_launch,
        started: started_at_launch,
        was_paused: false,
        backgrounds: Backgrounds::default(),
        settings,
        fov_blend: None,
        flashlight_dist_mod: 0.0,
        hint_queue: Vec::new(),
        screen_fade: None,
        menu_muted: false,
        pending_select: None,
        shakes: Vec::new(),
        shake_seed: 0x9E37_79B9,
        options: None,
        newgame: None,
        servers: None,
        ugc: None,
        addons: None,
        map_name: map.clone(),
        opt_drag: None,
        opt_modes: Vec::new(),
        kb_rows,
        gameui,
        last_frame: Instant::now(),
        exposure_at: Instant::now(),
        last: Instant::now(),
        acc: 0.0,
        frames: 0,
        fps_at: Instant::now(),
        fx_epoch: Instant::now(),
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
            // a world triangle: its own plane whose (0,0) / (1,0) / (0,1) are the three points
            gm_script::DrawCmd::WorldTri { pts, color } => {
                let e1 = pts[1] - pts[0];
                let e2 = pts[2] - pts[0];
                let n = e1.cross(e2).normalize_or_zero();
                planes.push((
                    glam::Mat4::from_cols(
                        e1.extend(0.0),
                        e2.extend(0.0),
                        n.extend(0.0),
                        pts[0].extend(1.0),
                    ),
                    vec![UiItem::Tri {
                        pts: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
                        color: *color,
                    }],
                ));
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
        let victim = it.next()?;
        let attacker: u32 = it.next()?.parse().ok()?;
        let inflictor = it.next()?.to_string();
        let name = |id: u32| {
            players
                .iter()
                .find(|p| p.id == id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| format!("Player {id}"))
        };
        // `npc:<spawnmenu key>`: an NPC victim is named like the game's `#npc_name` (its list
        // name, e.g. "Zombie"), with spaces sent as underscores
        let (victim_id, right) = match victim.strip_prefix("npc:") {
            Some(key) => (0, key.replace('_', " ")),
            None => {
                let id: u32 = victim.parse().ok()?;
                (id, name(id))
            }
        };
        Some(Self {
            left: (attacker != 0 && attacker != victim_id).then(|| name(attacker)),
            right,
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
/// Whether the weapon's script carries a `crosshair` glyph (`Crosshairs` "Q"): every HL2
/// weapon script does except weapon_stunstick, which uses `crosshair_default`; scripted
/// weapons have no script texture data at all.
fn weapon_has_crosshair_glyph(class: &str) -> bool {
    matches!(
        class,
        "weapon_pistol"
            | "weapon_357"
            | "weapon_smg1"
            | "weapon_ar2"
            | "weapon_shotgun"
            | "weapon_crossbow"
            | "weapon_rpg"
            | "weapon_frag"
            | "weapon_crowbar"
            | "weapon_physcannon"
            | "weapon_physgun"
    )
}

fn weapon_icon(class: &str) -> Option<&'static str> {
    Some(match class {
        "weapon_crowbar" => "c",
        "weapon_stunstick" => "n",
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
        "weapon_stunstick" => "STUNSTICK",
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

/// The engine's 16-bucket luminance histogram of the frame (render.md §5, gap-report #148),
/// sampled on a 16 px grid of the presented sRGB frame (linear 0..1): where the brightest 2 %
/// of pixels start and the 50 % location, each interpolated inside its bucket. The game
/// measures every frame; the probe runs every 15th frame here (readback cost).
fn frame_bright_pixels(gpu: &gm_render::Gpu, frame: &wgpu::Texture, w: u32, h: u32) -> (f32, f32) {
    let Ok(px) = gpu.read_texture(frame, w, h) else {
        return (0.0, 0.0);
    };
    const BUCKETS: usize = 16;
    let mut hist = [0u32; BUCKETS];
    let mut n = 0u32;
    for y in (0..h).step_by(16) {
        for x in (0..w).step_by(16) {
            let i = ((y * w + x) * 4) as usize;
            let m = (px[i].max(px[i + 1]).max(px[i + 2]) as f32 / 255.0).powf(2.2);
            hist[((m * BUCKETS as f32) as usize).min(BUCKETS - 1)] += 1;
            n += 1;
        }
    }
    if n == 0 {
        return (0.0, 0.0);
    }
    // the brightness below which `frac` of the pixels lie
    let location = |frac: f32| -> f32 {
        let want = frac * n as f32;
        let mut acc = 0.0f32;
        for (b, &c) in hist.iter().enumerate() {
            let c = c as f32;
            if acc + c >= want && c > 0.0 {
                return (b as f32 + (want - acc) / c) / BUCKETS as f32;
            }
            acc += c;
        }
        1.0
    };
    (location(0.98), location(0.5))
}

/// `draw.RoundedBox( r, ... )`: the `gui/corner8` corners (the game's 8 px rounded corner
/// texture) and three rects filling the rest.
fn rounded_box(items: &mut Vec<UiItem>, r: f32, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    let corner = "gui/corner8";
    let r = r.min(w / 2.0).min(h / 2.0);
    for (cx, cy, rot) in [
        (x, y, 0u8),
        (x + w - r, y, 1),
        (x + w - r, y + h - r, 2),
        (x, y + h - r, 3),
    ] {
        items.push(UiItem::Image {
            x: cx,
            y: cy,
            w: r,
            h: r,
            material: corner.to_string(),
            color,
            rot,
        });
    }
    items.push(UiItem::Rect {
        x: x + r,
        y,
        w: w - r * 2.0,
        h: r,
        color,
    });
    items.push(UiItem::Rect {
        x,
        y: y + r,
        w,
        h: h - r * 2.0,
        color,
    });
    items.push(UiItem::Rect {
        x: x + r,
        y: y + h - r,
        w: w - r * 2.0,
        h: r,
        color,
    });
}

/// `Player:ScreenFade` on the wire: `r g b a time hold flags` (SCREENFADE_IN 1: the colour, held
/// `hold`, fading out over `time`; OUT 2: fading in over `time`, held `hold`; STAYOUT 8 keeps it).
#[derive(Clone, Debug, PartialEq)]
struct ScreenFade {
    color: [f32; 4],
    start: Instant,
    time: f32,
    hold: f32,
    flags: u32,
}

impl ScreenFade {
    fn parse(rest: &str) -> Option<Self> {
        let v: Vec<f32> = rest.split(' ').filter_map(|x| x.parse().ok()).collect();
        if v.len() < 7 {
            return None;
        }
        Some(Self {
            color: [v[0] / 255.0, v[1] / 255.0, v[2] / 255.0, v[3] / 255.0],
            start: Instant::now(),
            time: v[4].max(0.0),
            hold: v[5].max(0.0),
            flags: v[6] as u32,
        })
    }

    fn alpha(&self, now: Instant) -> Option<f32> {
        fade_alpha_at(
            self.flags,
            self.color[3],
            self.time,
            self.hold,
            now.duration_since(self.start).as_secs_f32(),
        )
    }
}

/// The fade's alpha `t` seconds in; None once it is over.
fn fade_alpha_at(flags: u32, a: f32, time: f32, hold: f32, t: f32) -> Option<f32> {
    if flags & 1 != 0 {
        if t < hold {
            Some(a)
        } else if time > 0.0 && t < hold + time {
            Some(a * (1.0 - (t - hold) / time))
        } else {
            None
        }
    } else if time > 0.0 && t < time {
        Some(a * t / time)
    } else if t < time + hold || flags & 8 != 0 {
        Some(a)
    } else {
        None
    }
}

/// A script audio channel: the mixer voice playing it, or why it could not start.
struct AudioChan {
    voice: u32,
    /// BASS error code (2 file open, 41 file format) reported once, then the channel is dropped.
    err: Option<i64>,
    /// Frames since `play` without the voice showing up in the mixer's states yet.
    pending: u32,
}

impl App {
    /// `DrawCmd::Audio`: a script channel operation onto the mixer.
    fn apply_audio(&mut self, id: u32, op: &gm_script::AudioOp) {
        use gm_script::AudioOp;
        if let AudioOp::Play {
            path,
            volume,
            pitch,
            looping,
            paused,
            pos,
        } = op
        {
            // the game's paths are game-relative (`sound/...`); the mixer's are under sound/
            let wave = path.trim_start_matches('/').trim_start_matches("sound/");
            let chan = match self.sounds.wave(&self.mount, wave) {
                Some(pcm) => {
                    let voice = self.sounds.mixer.play(gm_audio::Play {
                        pcm,
                        volume: *volume,
                        pitch: *pitch,
                        position: *pos,
                        level: 75.0,
                        looping: *looping,
                        ent: 0,
                        channel: 0,
                        stream: false,
                    });
                    if *paused {
                        self.sounds.mixer.set_paused(voice, true);
                    }
                    AudioChan {
                        voice,
                        err: None,
                        pending: 0,
                    }
                }
                None => AudioChan {
                    voice: 0,
                    err: Some(if self.mount.read(&format!("sound/{wave}")).is_ok() {
                        41
                    } else {
                        2
                    }),
                    pending: 0,
                },
            };
            self.audio_channels.insert(id, chan);
            return;
        }
        let Some(ch) = self.audio_channels.get(&id) else {
            return;
        };
        let mixer = &self.sounds.mixer;
        match op {
            AudioOp::Stop => mixer.stop(ch.voice),
            AudioOp::Pause(b) => mixer.set_paused(ch.voice, *b),
            AudioOp::Volume(v) => mixer.set_volume(ch.voice, *v),
            AudioOp::Pitch(p) => mixer.set_pitch(ch.voice, *p),
            AudioOp::Seek(t) => mixer.seek(ch.voice, *t),
            AudioOp::Pos(Some(p)) => mixer.move_voice(ch.voice, *p),
            AudioOp::Pos(None) => {}
            AudioOp::Loop(b) => mixer.set_looping(ch.voice, *b),
            AudioOp::Play { .. } => {}
        }
    }

    /// `DrawCmd::Servers`: a script's server browser request onto the native browser's
    /// queries (threads) and lists.
    fn apply_servers(&mut self, id: u32, op: &gm_script::ServerOp) {
        use gm_script::ServerOp;
        let (tx, rx) = std::sync::mpsc::channel();
        match op {
            ServerOp::Query { kind } => match kind.as_str() {
                "internet" => {
                    let master = servers::master_address();
                    std::thread::spawn(move || {
                        let _ =
                            tx.send(master.map(|m| servers::master_list(&m)).unwrap_or_default());
                    });
                }
                "lan" => {
                    std::thread::spawn(move || {
                        let _ = tx.send(servers::lan_scan());
                    });
                }
                other => {
                    let kind = if other == "favorite" {
                        "favorites"
                    } else {
                        other
                    };
                    let _ = tx.send(servers::load_list(kind));
                }
            },
            ServerOp::Ping { addr } => {
                let addr = if addr.contains(':') {
                    addr.clone()
                } else {
                    format!("{addr}:27015")
                };
                std::thread::spawn(move || {
                    let _ = tx.send(servers::probe(&addr).into_iter().collect());
                });
            }
            ServerOp::Favorites => {
                let _ = tx.send(servers::load_list("favorites"));
            }
            ServerOp::FavoriteAdd { addr } => {
                let mut list = servers::load_list("favorites");
                if !list.iter().any(|e| e.address == *addr) {
                    list.push(servers::ServerEntry {
                        address: addr.clone(),
                        name: addr.clone(),
                        map: String::new(),
                        players: 0,
                        maxplayers: 0,
                        ping: 0,
                        gamemode: String::new(),
                        last_played: 0,
                    });
                    servers::save_list("favorites", &list);
                }
                return;
            }
            ServerOp::FavoriteRemove { addr } => {
                let mut list = servers::load_list("favorites");
                list.retain(|e| e.address != *addr);
                servers::save_list("favorites", &list);
                return;
            }
        }
        if id != 0 {
            self.script_servers.push((id, rx));
        }
    }

    /// Once per frame before the HUD: finished server browser requests, as entries then done.
    fn report_servers(&mut self) {
        if self.script_servers.is_empty() {
            return;
        }
        let mut reports = Vec::new();
        self.script_servers.retain(|(id, rx)| match rx.try_recv() {
            Ok(list) => {
                for e in &list {
                    reports.push(gm_script::ServerReport::Entry {
                        id: *id,
                        info: gm_script::ServerInfo {
                            address: e.address.clone(),
                            name: e.name.clone(),
                            map: e.map.clone(),
                            players: e.players,
                            maxplayers: e.maxplayers,
                            ping: e.ping,
                            gamemode: e.gamemode.clone(),
                            last_played: e.last_played,
                        },
                    });
                }
                reports.push(gm_script::ServerReport::Done {
                    id: *id,
                    count: list.len() as u32,
                });
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                reports.push(gm_script::ServerReport::Done { id: *id, count: 0 });
                false
            }
        });
        self.sim
            .script
            .client_servers(&self.sim.client.world, &reports);
    }

    /// Once per frame before the HUD: what the mixer says about every script channel.
    fn report_audio(&mut self) {
        if self.audio_channels.is_empty() {
            return;
        }
        let states = self.sounds.mixer.voice_states();
        let mut reports = Vec::new();
        self.audio_channels.retain(|id, ch| {
            if let Some(err) = ch.err {
                reports.push(gm_script::AudioState {
                    id: *id,
                    state: 0,
                    time: 0.0,
                    length: 0.0,
                    rate: 0,
                    err,
                });
                return false;
            }
            let found = states.iter().find(|v| v.id == ch.voice);
            let (state, time, length, rate) = match found {
                Some(v) => (
                    if v.paused { 2 } else { 1 },
                    v.time,
                    v.length,
                    v.rate as i64,
                ),
                // the audio thread has not taken the play yet: stalled for a few frames, then
                // the voice really is gone (finished or the mixer is silent)
                None if ch.pending < 30 => {
                    ch.pending += 1;
                    (3, 0.0, 0.0, 0)
                }
                None => (0, 0.0, 0.0, 0),
            };
            if found.is_some() {
                ch.pending = 30;
            }
            reports.push(gm_script::AudioState {
                id: *id,
                state,
                time,
                length,
                rate,
                err: 0,
            });
            true
        });
        self.sim
            .script
            .client_audio(&self.sim.client.world, &reports);
    }
}

/// gmod_thruster.lua `matHeatWave`: the `Refract` beam over the fire effect.
const HEATWAVE: &str = "sprites/heatwave";
/// The combine ball's body material, which is also what the server sends as its "model".
const CBALL_BODY: &str = "effects/ar2_altfire1";

/// Entities drawn as a billboard of a material instead of a model (env_sprite, the combine
/// ball).
fn is_sprite_ent(model: &str) -> bool {
    model.starts_with("sprites/") || model == CBALL_BODY
}

/// TE_WaterSplash: the ring opening on the surface and the droplets thrown up.
const SPLASH_RIPPLE: &str = "effects/splashwake1";
const SPLASH_DROP: &str = "effects/splash2";
const SPLASH_MIST: &str = "effects/splash1";

/// `ThrusterEffects` (`gmod_thruster.lua`): the beam material of an effect; empty for none.
fn thruster_fx_material(effect: &str) -> &'static str {
    match effect {
        "fire" => "effects/fire_cloud1",
        "plasma" => "effects/strider_muzzle",
        "magic" => "sprites/gmdm_pickups/light",
        "rings" => "effects/select_ring",
        "smoke" => "particles/smokey",
        _ => "",
    }
}

/// One `emitter:Add` particle of a thruster effect (`ParticleEmitter`: position integrated
/// with the velocity and gravity, size / alpha lerped over its die time).
struct ThrusterParticle {
    mat: &'static str,
    additive: bool,
    pos: Vec3,
    vel: Vec3,
    grav: Vec3,
    born: std::time::Instant,
    life: f32,
    size: (f32, f32),
    alpha: (f32, f32),
    color: [f32; 3],
    roll: f32,
    /// `SetAngles`: the quad lies in the plane normal to it (camera-facing when none).
    axis: Option<Vec3>,
}

/// `decals_subrect.txt` `TranslationData`: the impact decal of a surface property's game
/// material (`scripts/surfaceproperties.txt`); `Impact.Concrete` for the untranslated
/// letters, nothing for `no_decal`; one of the group's five `shotN` by the hit position.
/// `Impact.Concrete` translated by the surface's game material letter through
/// `decals_subrect.txt` (clientfx.md §4.1-4.2, gap-report #127): C/T/P/L/D concrete (5
/// subrects), M/V/G metal (4), W/O wood, Y glass, F/B/H flesh, A/E antlion, N sand, I/S/X/-
/// nothing; letters outside the table (U grass, J snow) keep the untranslated concrete.
fn impact_decal(letter: Option<char>, surfaceprop: &str, pos: Vec3) -> String {
    if surfaceprop.eq_ignore_ascii_case("no_decal") {
        return String::new();
    }
    let (group, count) = match letter.map(|c| c.to_ascii_uppercase()) {
        Some('M' | 'V' | 'G') => ("metal", 4),
        Some('W' | 'O') => ("wood", 5),
        Some('Y') => ("glass", 5),
        // `Impact.Flesh` has no shot decals in the flesh directory: the blood group
        Some('F' | 'B' | 'H') => {
            return format!(
                "decals/blood{}_subrect",
                1 + (pos.x.to_bits() ^ pos.z.to_bits()) % 6
            )
        }
        Some('A' | 'E') => ("antlion", 5),
        Some('N') => ("sand", 5),
        Some('I' | 'S' | 'X' | '-') => return String::new(),
        Some(_) => ("concrete", 5),
        None => match surfaceprop.to_ascii_lowercase().as_str() {
            "solidmetal" | "metalpanel" | "metalvehicle" | "armorflesh" | "metalvent" => {
                ("metal", 4)
            }
            "wood" | "watermelon" => ("wood", 5),
            "glass" => ("glass", 5),
            "antlionsand" => ("sand", 5),
            _ => ("concrete", 5),
        },
    };
    let h = pos.x.to_bits() ^ pos.y.to_bits().rotate_left(11) ^ pos.z.to_bits().rotate_left(22);
    format!("decals/{group}/shot{}_subrect", 1 + h % count)
}

/// An `env_shake` on the view (`CViewEffects::CalcShake`): a new random offset and roll every
/// 1 / frequency s, the amplitude fading out over the duration.
struct Shake {
    amplitude: f32,
    duration: f32,
    frequency: f32,
    start: Instant,
    next_at: f32,
    offset: Vec3,
    roll: f32,
}

/// The shakes' eye offset and roll right now; the ones over are dropped.
fn calc_shake(shakes: &mut Vec<Shake>, seed: &mut u32) -> (Vec3, f32) {
    let now = Instant::now();
    let mut offset = Vec3::ZERO;
    let mut roll = 0.0;
    // CViewEffects::CalcShake (clientfx.md §1, gap-report #120): the offset and roll are
    // re-rolled at the full amplitude every 1/frequency s and applied scaled by
    // fraction^2 * sin(t * frequency / fraction), fraction = time left / duration
    shakes.retain_mut(|s| {
        let t = now.duration_since(s.start).as_secs_f32();
        if t >= s.duration {
            return false;
        }
        if t >= s.next_at {
            let r = |seed: &mut u32| fx_rand(seed) * 2.0 - 1.0;
            s.offset = Vec3::new(r(seed), r(seed), r(seed)) * s.amplitude;
            s.roll = r(seed) * s.amplitude * 0.25;
            s.next_at = t + 1.0 / s.frequency;
        }
        let fraction = 1.0 - t / s.duration;
        let freq = if fraction > 0.0 {
            s.frequency / fraction
        } else {
            0.0
        };
        let scale = fraction * fraction * (t * freq).sin();
        offset += s.offset * scale;
        roll += s.roll * scale;
        true
    });
    (offset, roll)
}

/// xorshift in [0, 1).
fn fx_rand(s: &mut u32) -> f32 {
    let mut x = *s;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *s = x;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// `math.Rand(a, b)`.
fn fx_range(s: &mut u32, a: f32, b: f32) -> f32 {
    a + (b - a) * fx_rand(s)
}

/// `VectorRand()`: each component in [-1, 1].
/// `CTempEnts::m_pShells`: the brass by 6001's option (pistol, rifle, shotgun).
const BRASS_MODELS: [&str; 3] = [
    "models/weapons/shell.mdl",
    "models/weapons/rifleshell.mdl",
    "models/weapons/shotgun_shell.mdl",
];

/// A shell out of the gun: position, velocity, angles and angular velocity (deg/s), birth.
impl App {
    /// `C_BaseExplosionEffect::Create` at `pos` (the frag/RPG/SLAM blast): the force is pinned to
    /// 2 (`EXPLOSION_FORCE_MIN == MAX == 2`, spread 0.7), the direction is the sum of six axis
    /// probes 125 u long pushed back by how much of each was blocked, and the particles are
    /// `CreateCore` (4 big + 8 near smoke puffs, a ring of 8 low puffs, 16 embers, 16 fire
    /// clouds) and `CreateDebris` (16..32 cement flecks). No fireball sprite and no dynamic
    /// light: the 2013 branch draws neither.
    /// The `Impact` effect's particles (clientfx.md 3.2-3.5, the game's legacy path): by the
    /// surface's game material letter - C/T concrete flecks + dust, W wood flecks + dust,
    /// L dust only, D/N/U/J dust impact, M/V/P metal sparks + flare, anything else nothing.
    /// The game has no impact flash sprite.
    fn spawn_impact(&mut self, pos: Vec3, normal: Vec3, surface: &str, now: Instant) {
        let letter = self
            .gamematerials
            .get(&surface.to_ascii_lowercase())
            .copied()
            .unwrap_or_else(|| gamematerial_guess(surface));
        let eye = {
            let me = self.sim.client.player_id;
            self.sim
                .client
                .world
                .players
                .iter()
                .find(|p| Some(p.id) == me)
                .map_or(pos - normal, |p| p.origin + Vec3::Z * 64.0)
        };
        let shot_dir = (pos - eye).normalize_or_zero();
        let seed = &mut self.fx_seed;
        let lin = |c: Vec3| c.powf(2.2).to_array();
        let randint = |seed: &mut u32, a: f32, b: f32| particles::range(seed, a, b + 1.0).floor();
        let surface_color = match letter {
            'W' => Vec3::new(0.55, 0.42, 0.28),
            'D' | 'N' | 'U' | 'J' => Vec3::new(0.45, 0.36, 0.26),
            _ => Vec3::splat(0.5),
        };
        let mut out = Vec::new();
        // dust puffs shared by the fleck and dust effects: `nId` scales speed and size
        let dust_puffs = |seed: &mut u32, out: &mut Vec<FxSprite>, n: usize| {
            for id in 0..n {
                let dir = (rand_vec(seed, 0.2) + normal * particles::range(seed, 1.0, 6.0))
                    .normalize_or_zero();
                let speed = particles::range(seed, 250.0, 500.0) * id as f32;
                let c = (surface_color * particles::range(seed, 0.75, 1.25))
                    .clamp(Vec3::ZERO, Vec3::ONE);
                let size = randint(seed, 3.0, 4.0) * (id as f32 + 1.0);
                out.push(FxSprite {
                    kind: FxKind::Dust,
                    variant: 0,
                    pos,
                    vel: dir * speed,
                    born: now,
                    life: particles::range(seed, 0.5, 1.0),
                    size0: size,
                    size1: size * 4.0,
                    color: lin(c),
                    roll: particles::range(seed, 0.0, std::f32::consts::TAU),
                    roll_d: particles::range(seed, -8.0, 8.0),
                    alpha: randint(seed, 32.0, 255.0) / 255.0,
                });
            }
        };
        match letter {
            'C' | 'T' | 'W' | 'L' => {
                if letter != 'L' {
                    // `FX_DebrisFlecks`: randint(4,8) flecks, 3 s, gravity 800
                    let n = randint(seed, 4.0, 8.0) as usize;
                    for _ in 0..n {
                        let d = (normal + rand_vec(seed, 1.0)).normalize_or_zero();
                        let size = randint(seed, 1.0, 2.0);
                        let c = (surface_color * particles::range(seed, 0.75, 1.25)).min(Vec3::ONE);
                        out.push(FxSprite {
                            kind: if letter == 'W' {
                                FxKind::WoodFleck
                            } else {
                                FxKind::Fleck
                            },
                            variant: (particles::rng(seed) * 2.0) as usize,
                            pos,
                            vel: d * particles::range(seed, 64.0, 128.0),
                            born: now,
                            life: 3.0,
                            size0: size,
                            size1: size,
                            color: lin(c),
                            roll: particles::range(seed, 0.0, std::f32::consts::TAU),
                            roll_d: particles::range(seed, -2.0, 2.0),
                            alpha: 1.0,
                        });
                    }
                }
                dust_puffs(seed, &mut out, 4);
                // one drifting puff beside the hit
                let at = pos + Vec3::new(rand_vec(seed, 8.0).x, rand_vec(seed, 8.0).y, 0.0);
                let size = randint(seed, 4.0, 8.0);
                out.push(FxSprite {
                    kind: FxKind::Dust,
                    variant: 0,
                    pos: at,
                    vel: Vec3::ZERO,
                    born: now,
                    life: particles::range(seed, 0.5, 1.0),
                    size0: size,
                    size1: size * 4.0,
                    color: lin(surface_color),
                    roll: particles::range(seed, 0.0, std::f32::consts::TAU),
                    roll_d: particles::range(seed, -1.0, 1.0),
                    alpha: randint(seed, 32.0, 64.0) / 255.0,
                });
            }
            'D' | 'N' | 'U' | 'J' => {
                // `FX_DustImpact`: 4 smoke puffs, 4 noise-sphere puffs, 4 drifting puffs
                dust_puffs(seed, &mut out, 4);
                for id in 0..4 {
                    let dir = (rand_vec(seed, 0.2) + normal * particles::range(seed, 1.0, 6.0))
                        .normalize_or_zero();
                    let speed = particles::range(seed, 250.0, 500.0) * id as f32;
                    let size = randint(seed, 2.0, 4.0) * (id as f32 + 1.0);
                    out.push(FxSprite {
                        kind: FxKind::DustNoise,
                        variant: 0,
                        pos,
                        vel: dir * speed,
                        born: now,
                        life: particles::range(seed, 0.25, 0.75),
                        size0: size,
                        size1: size * 2.0,
                        color: lin(surface_color),
                        roll: particles::range(seed, 0.0, std::f32::consts::TAU),
                        roll_d: particles::range(seed, -2.0, 2.0),
                        alpha: 1.0,
                    });
                }
                for _ in 0..4 {
                    let at = pos + Vec3::new(rand_vec(seed, 8.0).x, rand_vec(seed, 8.0).y, 0.0);
                    let dir = (rand_vec(seed, 1.0) + normal).normalize_or_zero();
                    let size = randint(seed, 1.0, 4.0);
                    out.push(FxSprite {
                        kind: FxKind::Dust,
                        variant: 0,
                        pos: at,
                        vel: dir * particles::range(seed, 0.0, 50.0),
                        born: now,
                        life: particles::range(seed, 0.5, 1.0),
                        size0: size,
                        size1: size * 4.0,
                        color: lin(surface_color),
                        roll: particles::range(seed, 0.0, std::f32::consts::TAU),
                        roll_d: particles::range(seed, -16.0, 16.0),
                        alpha: randint(seed, 32.0, 64.0) / 255.0,
                    });
                }
            }
            'M' | 'V' | 'P' => {
                // `FX_MetalSpark`: randint(4,8)*2 streaks off the reflected shot, 400 gravity,
                // plus a yellowflare quad randint(24,28) for 0.1 s (`P` is the plain `Sparks`
                // temp effect: half as many, straight off the surface)
                let reflect = shot_dir - normal * (2.0 * shot_dir.dot(normal));
                let base = if letter == 'P' {
                    normal
                } else {
                    reflect.normalize_or_zero()
                };
                let n = randint(seed, 4.0, 8.0) as usize * if letter == 'P' { 1 } else { 2 };
                for _ in 0..n {
                    let spread = particles::range(seed, 0.0, 2.0);
                    let dir = (base + rand_vec(seed, 0.5 * spread)).normalize_or_zero();
                    let speed =
                        particles::range(seed, 128.0 * (2.0 - spread), 512.0 * (2.0 - spread));
                    out.push(FxSprite {
                        kind: FxKind::Spark,
                        variant: 0,
                        pos: pos + normal * 0.5,
                        vel: dir * speed,
                        born: now,
                        life: particles::range(seed, 0.05, 0.1),
                        size0: particles::range(seed, 1.0, 4.0),
                        size1: particles::range(seed, 0.025, 0.1),
                        color: [1.0, 1.0, 1.0],
                        roll: 0.0,
                        roll_d: 0.0,
                        alpha: 1.0,
                    });
                }
                if letter != 'P' {
                    let size = randint(seed, 24.0, 28.0);
                    out.push(FxSprite {
                        kind: FxKind::Flare,
                        variant: 0,
                        pos: pos + normal * 1.0,
                        vel: Vec3::ZERO,
                        born: now,
                        life: 0.1,
                        size0: size,
                        size1: size,
                        color: [1.0, 1.0, 1.0],
                        roll: particles::range(seed, 0.0, std::f32::consts::TAU),
                        roll_d: 0.0,
                        alpha: 1.0,
                    });
                }
            }
            _ => {}
        }
        self.fx_sprites.extend(out);
    }

    fn spawn_explosion(&mut self, pos: Vec3, now: Instant) {
        let seed = &mut self.fx_seed;
        let mut dir = Vec3::ZERO;
        for d in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z] {
            let tr = self
                .sim
                .collision
                .trace_ray(pos, pos + d * 125.0, gm_collision::MASK_SOLID);
            dir += -d * (1.0 - tr.fraction);
        }
        let dir = dir.normalize_or_zero();
        let dir = if dir == Vec3::ZERO { Vec3::Z } else { dir };
        let force = 2.0f32;
        let spread = 1.0 - 0.15 * force;
        // the puffs take the colour of the light at 32 u along the blast direction
        let light = self
            .gfx
            .as_ref()
            .map(|g| g.world.light_at(pos + dir * 32.0, Vec3::Z))
            .unwrap_or(Vec3::splat(0.5));
        let light = light.clamp(Vec3::ZERO, Vec3::ONE);
        let lum = light.max_element();
        // the game's particle colours are display-space bytes; our vertex colours are linear
        let lin = |c: Vec3| c.powf(2.2).to_array();
        let dev = |v: Vec3, d: Vec3| spread * v.dot(d).abs();
        let mut out = Vec::new();
        let mut smoke = |seed: &mut u32,
                         at: Vec3,
                         life: (f32, f32),
                         fmax: f32,
                         size: (f32, f32),
                         roll_d: f32| {
            let v = (rand_vec(seed, spread) + dir * particles::range(seed, 1.0, 6.0))
                .normalize_or_zero();
            let f = particles::range(seed, 1.0, fmax) * force * dev(v, dir);
            let n = particles::range(seed, lum * 0.5, lum);
            let size0 = particles::range(seed, size.0, size.1).floor();
            out.push(FxSprite {
                kind: FxKind::Smoke,
                variant: 0,
                pos: at,
                vel: v * f,
                born: now,
                life: particles::range(seed, life.0, life.1),
                size0,
                size1: size0 * 2.0,
                color: lin(light * n),
                roll: particles::range(seed, 0.0, std::f32::consts::TAU),
                roll_d: particles::range(seed, -roll_d, roll_d).to_radians(),
                alpha: 1.0,
            });
        };
        for _ in 0..4 {
            smoke(seed, pos, (2.0, 3.0), 750.0, (72.0, 72.0), 2.0);
        }
        for _ in 0..8 {
            let at = pos + rand_vec(seed, 16.0);
            smoke(seed, at, (0.5, 1.0), 2000.0, (32.0, 64.0), 8.0);
        }
        for i in 0..8 {
            let yaw = std::f32::consts::TAU / 8.0 * (i + 1) as f32;
            let fwd = Vec3::new(yaw.cos(), yaw.sin(), 0.0);
            let at = pos + rand_vec(seed, 4.0) + fwd * particles::range(seed, 8.0, 16.0);
            let f = particles::range(seed, 500.0, 2000.0) * force * spread;
            let n = particles::range(seed, lum * 0.5, lum);
            let size0 = particles::range(seed, 16.0, 32.0).floor();
            out.push(FxSprite {
                kind: FxKind::Smoke,
                variant: 0,
                pos: at,
                vel: fwd * f,
                born: now,
                life: particles::range(seed, 0.5, 1.5),
                size0,
                size1: size0 * 4.0,
                color: lin(light * n),
                roll: particles::range(seed, 0.0, std::f32::consts::TAU),
                roll_d: particles::range(seed, -8.0, 8.0).to_radians(),
                alpha: 1.0,
            });
        }
        for _ in 0..16 {
            let at = pos + rand_vec(seed, 32.0);
            let v = (rand_vec(seed, spread * 2.0) + dir).normalize_or_zero();
            let f = particles::range(seed, 1.0, 400.0);
            let vd = dev(v, dir);
            let n = particles::range(seed, 192.0, 255.0) / 255.0;
            let size0 = (particles::range(seed, 8.0, 16.0).floor() * vd).clamp(4.0, 32.0);
            out.push(FxSprite {
                kind: FxKind::Ember,
                variant: (particles::rng(seed) * 2.0) as usize,
                pos: at,
                vel: v * (f * 16.0 * vd * vd * 0.5),
                born: now,
                life: particles::range(seed, 2.0, 3.0),
                size0,
                size1: size0,
                color: lin(Vec3::splat(n)),
                roll: particles::range(seed, 0.0, std::f32::consts::TAU),
                roll_d: particles::range(seed, -8.0, 8.0).to_radians(),
                alpha: 1.0,
            });
        }
        for _ in 0..16 {
            let at = pos + rand_vec(seed, 48.0);
            let v = (rand_vec(seed, spread * 0.75) + dir).normalize_or_zero();
            let f = particles::range(seed, 400.0, 800.0);
            let vd = dev(v, dir);
            let n = particles::range(seed, 128.0, 255.0) / 255.0;
            let size0 = (particles::range(seed, 32.0, 85.0).floor() * vd).clamp(32.0, 85.0);
            out.push(FxSprite {
                kind: FxKind::FireCloud,
                variant: 0,
                pos: at,
                vel: v * (f * 16.0 * vd * vd * 0.5),
                born: now,
                life: particles::range(seed, 0.2, 0.4),
                size0,
                size1: (size0 * 1.5).floor(),
                color: lin(Vec3::splat(n)),
                roll: particles::range(seed, 0.0, std::f32::consts::TAU),
                roll_d: particles::range(seed, -16.0, 16.0).to_radians(),
                alpha: 1.0,
            });
        }
        let flecks = 16 + (particles::rng(seed) * 17.0) as usize;
        for _ in 0..flecks {
            let at = pos + dir * 16.0 + rand_vec(seed, 8.0);
            let d = (dir + rand_vec(seed, 1.0)).normalize_or_zero();
            let size = 1.0 + (particles::rng(seed) * 3.0).floor();
            let f = particles::range(seed, 64.0, 256.0) * (4.0 - size);
            let fd = 0.8 * d.dot(dir).abs();
            let c = (0.25 * particles::range(seed, 0.5, 1.5)).min(1.0);
            out.push(FxSprite {
                kind: FxKind::Fleck,
                variant: (particles::rng(seed) * 2.0) as usize,
                pos: at,
                vel: d * (f * 16.0 * fd * fd * 0.5),
                born: now,
                life: 3.0,
                size0: size,
                size1: size,
                color: lin(Vec3::splat(c)),
                roll: particles::range(seed, 0.0, std::f32::consts::TAU),
                roll_d: particles::range(seed, 0.0, 360.0).to_radians(),
                alpha: 1.0,
            });
        }
        self.fx_sprites.extend(out);
    }

    /// `CExplosionParticle`: velocity decays to 1e-4 over 0.5 s but never below 32 u/s, the
    /// roll rate decays at 8/s to a floor of 0.5°/s; flecks (`CFleckParticles`) fall at 800 u/s²
    /// and bounce off the world at 0.9 of their speed, losing half along the surface.
    fn tick_explosions(&mut self, now: Instant, dt: f32) {
        self.fx_sprites
            .retain(|s| now.duration_since(s.born).as_secs_f32() < s.life);
        if self.fx_sprites.is_empty() {
            return;
        }
        let decay = (1e-4f32.ln() * dt / 0.5).exp();
        for s in &mut self.fx_sprites {
            if matches!(s.kind, FxKind::Dust | FxKind::DustNoise) {
                // `CDustParticle::UpdateVelocity`: the explosion law, 1e-4 over 0.5 s with a
                // 32 u/s floor (undecayed, the 250..1500 u/s puffs spread metres from the hit)
                let v = s.vel;
                s.vel *= decay;
                if s.vel.length_squared() < 32.0 * 32.0 {
                    s.vel = v.normalize_or_zero() * 32.0;
                }
                s.pos += s.vel * dt;
                s.roll += s.roll_d * dt;
                continue;
            }
            if matches!(s.kind, FxKind::Flare | FxKind::Muzzle) {
                s.pos += s.vel * dt;
                s.roll += s.roll_d * dt;
                continue;
            }
            if s.kind == FxKind::Spark {
                // `FX_MetalSpark`: gravity 400, velocity dampen 8, collision dampen 0.25
                s.vel.z -= 400.0 * dt;
                s.vel *= (-8.0 * dt).exp();
                let next = s.pos + s.vel * dt;
                let tr = self
                    .sim
                    .collision
                    .trace_ray(s.pos, next, gm_collision::MASK_SOLID);
                if tr.fraction < 1.0 {
                    let n = tr.normal;
                    s.vel = (s.vel - n * (2.0 * s.vel.dot(n))) * 0.25;
                    s.pos = tr.end + n * 0.5;
                } else {
                    s.pos = next;
                }
                continue;
            }
            if matches!(s.kind, FxKind::Fleck | FxKind::WoodFleck) {
                s.vel.z -= 800.0 * dt;
                let next = s.pos + s.vel * dt;
                let tr = self
                    .sim
                    .collision
                    .trace_ray(s.pos, next, gm_collision::MASK_SOLID);
                if tr.fraction < 1.0 {
                    let n = tr.normal;
                    let into = s.vel.dot(n);
                    let along = s.vel - n * into;
                    s.vel = (along * 0.5 - n * into) * 0.9;
                    s.pos = tr.end + n * 0.5;
                } else {
                    s.pos = next;
                }
                s.roll += s.roll_d * dt;
                continue;
            }
            s.pos += s.vel * dt;
            let save = s.vel;
            s.vel *= decay;
            if s.vel.length_squared() < 32.0 * 32.0 {
                s.vel = save.normalize_or_zero() * 32.0;
            }
            s.roll += s.roll_d * dt;
            s.roll_d += s.roll_d * (dt * -8.0);
            let floor = 0.5f32.to_radians();
            if s.roll_d.abs() < floor {
                s.roll_d = if s.roll_d > 0.0 { floor } else { -floor };
            }
        }
    }
}

/// One `C_BaseExplosionEffect` particle; sizes are half-widths in units, roll in radians.
struct FxSprite {
    kind: FxKind,
    variant: usize,
    pos: Vec3,
    vel: Vec3,
    born: Instant,
    life: f32,
    size0: f32,
    size1: f32,
    color: [f32; 3],
    roll: f32,
    roll_d: f32,
    /// Start opacity (the explosion's sprites are 1; dust puffs start at randint(32,255)/255).
    alpha: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum FxKind {
    Smoke,
    Ember,
    FireCloud,
    Fleck,
    WoodFleck,
    /// `particle_smokegrenade` / `particle_noisesphere` impact dust (no drag, straight flight).
    Dust,
    DustNoise,
    /// `effects/spark` streak: `size0` its width, `size1` its length in seconds of velocity.
    Spark,
    Flare,
    /// One of the 8 `muzzleflash1..4` sprites of another player's shot.
    Muzzle,
    /// `SmokeTrail`/`RocketTrail` puffs: `particle_smokegrenade` / `particle_noisesphere`
    /// with the `alpha * sin(pi t)` fade-in/out curve (clientfx.md 7.1).
    TrailSmoke,
    TrailNoise,
}

/// `FX_MuzzleEffect` (clientfx.md 2.1.2): 8 sprites along `forward` at 2*i u, half-size
/// rand(6,9) * (12 - i) / 9, a random `muzzleflash1..4` and roll each (scale 1).
fn muzzle_sprites(
    seed: &mut u32,
    origin: Vec3,
    forward: Vec3,
    scale: f32,
) -> Vec<(Vec3, f32, f32, usize)> {
    (1..=8)
        .map(|i| {
            let size = particles::range(seed, 6.0, 9.0) * (12.0 - i as f32) / 9.0 * scale;
            (
                origin + forward * (2.0 * i as f32 * scale),
                size,
                particles::range(seed, 0.0, std::f32::consts::TAU),
                (particles::rng(seed) * 4.0) as usize,
            )
        })
        .collect()
}

/// `scripts/surfaceproperties_manifest.txt` -> every surface property's `gamematerial` letter.
fn load_gamematerials(mount: &gm_content::Mount) -> HashMap<String, char> {
    let mut out = HashMap::new();
    let Ok(manifest) = mount.read_string("scripts/surfaceproperties_manifest.txt") else {
        return out;
    };
    let Ok(doc) = gm_formats::keyvalues::parse(&manifest) else {
        return out;
    };
    let files: Vec<String> = doc
        .nodes
        .iter()
        .flat_map(|n| n.children())
        .filter(|n| n.key.eq_ignore_ascii_case("file"))
        .filter_map(|n| n.as_str().map(str::to_string))
        .collect();
    for f in files {
        let Ok(src) = mount.read_string(&f) else {
            continue;
        };
        let Ok(doc) = gm_formats::keyvalues::parse(&src) else {
            continue;
        };
        for n in &doc.nodes {
            if let Some(c) = n.get_str("gamematerial").and_then(|g| g.chars().next()) {
                out.insert(n.key.to_ascii_lowercase(), c.to_ascii_uppercase());
            }
        }
    }
    out
}

/// The game material of a surface property the scripts did not list.
fn gamematerial_guess(surface: &str) -> char {
    let s = surface.to_ascii_lowercase();
    if s.contains("wood") {
        'W'
    } else if s.contains("metal") || s.contains("grate") || s.contains("chain") {
        'M'
    } else if s.contains("dirt")
        || s.contains("sand")
        || s.contains("grass")
        || s.contains("snow")
        || s.contains("mud")
    {
        'D'
    } else if s.contains("tile") {
        'T'
    } else if s.contains("plastic") {
        'L'
    } else if s.contains("flesh") || s.contains("glass") || s.contains("water") {
        'F'
    } else {
        'C'
    }
}

/// `random->RandomFloat(-r, r)` on each axis.
fn rand_vec(seed: &mut u32, r: f32) -> Vec3 {
    Vec3::new(
        particles::range(seed, -r, r),
        particles::range(seed, -r, r),
        particles::range(seed, -r, r),
    )
}

struct Shell {
    model: &'static str,
    pos: Vec3,
    vel: Vec3,
    ang: Vec3,
    angvel: Vec3,
    born: Instant,
}

/// The blood spray's drop sprite (`UTIL_BloodDrips`).
const BLOOD_CORE: &str = "effects/blood_core";
/// The remover's `entity_remove` spark sprite.
const REMOVE_SPARK: &str = "effects/spark";

fn fx_vec_rand(s: &mut u32) -> Vec3 {
    Vec3::new(
        fx_range(s, -1.0, 1.0),
        fx_range(s, -1.0, 1.0),
        fx_range(s, -1.0, 1.0),
    )
}

/// The vars of editor entity `ent` out of the `EntityEdits` list.
fn edit_values_of(edits: &[(u32, Vec<(String, String)>)], ent: u32) -> BTreeMap<String, String> {
    edits
        .iter()
        .find(|(id, _)| *id == ent)
        .map(|(_, vars)| vars.iter().cloned().collect())
        .unwrap_or_default()
}

/// edit_fog.lua `SetupWorldFog`: linear fog from the newest `edit_fog` entity's vars
/// (`fogstart`, `fogend`, `density`, `fogcolor_r/g/b` 0..255), none without one.
fn edit_fog_of(
    classes: &[(u32, String)],
    edits: &[(u32, Vec<(String, String)>)],
) -> Option<gm_render::world::Fog> {
    let ent = classes
        .iter()
        .filter(|(_, c)| c == "edit_fog")
        .map(|(id, _)| *id)
        .max()?;
    let vars = edit_values_of(edits, ent);
    let num = |k: &str, d: f32| vars.get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
    Some(gm_render::world::Fog {
        color: [
            num("fogcolor_r", 153.0) / 255.0,
            num("fogcolor_g", 179.0) / 255.0,
            num("fogcolor_b", 204.0) / 255.0,
        ],
        start: num("fogstart", 0.0),
        end: num("fogend", 10000.0),
        max_density: num("density", 0.9),
    })
}

/// edit_sun.lua: the newest `edit_sun` entity's `sunsize`, `overlaysize`, `suncolor` and
/// `overlaycolor` (0..255 channels) over the map's env_sun; its id so the caller can take its
/// forward as `sun_dir`. Without one, the map's values.
fn edit_sun_of(
    sun: &gm_render::world::EnvSun,
    classes: &[(u32, String)],
    edits: &[(u32, Vec<(String, String)>)],
) -> (Option<u32>, f32, f32, Vec3, Vec3) {
    let Some(ent) = classes
        .iter()
        .filter(|(_, c)| c == "edit_sun")
        .map(|(id, _)| *id)
        .max()
    else {
        return (
            None,
            sun.size,
            sun.overlay_size,
            sun.color,
            sun.overlay_color,
        );
    };
    let vars = edit_values_of(edits, ent);
    let num = |k: &str, d: f32| vars.get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
    let color = |k: &str| {
        Vec3::new(
            num(&format!("{k}_r"), 255.0),
            num(&format!("{k}_g"), 255.0),
            num(&format!("{k}_b"), 255.0),
        ) / 255.0
    };
    (
        Some(ent),
        num("sunsize", 20.0),
        num("overlaysize", 20.0),
        color("suncolor"),
        color("overlaycolor"),
    )
}

/// env_skypaint's vars of the newest `edit_sky` entity (defaults of its `Initialize`); the
/// sun normal is the map's sun (`env_sun`, else `light_environment`), as its `Think` copies.
fn edit_sky_of(
    classes: &[(u32, String)],
    edits: &[(u32, Vec<(String, String)>)],
    sun: Vec3,
) -> Option<gm_render::world::SkyPaint> {
    let ent = classes
        .iter()
        .filter(|(_, c)| c == "edit_sky")
        .map(|(id, _)| *id)
        .max()?;
    let vars = edit_values_of(edits, ent);
    let num = |k: &str, d: f32| vars.get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
    let color = |k: &str, d: [f32; 3]| {
        Vec3::new(
            num(&format!("{k}_r"), d[0] * 255.0),
            num(&format!("{k}_g"), d[1] * 255.0),
            num(&format!("{k}_b"), d[2] * 255.0),
        ) / 255.0
    };
    Some(gm_render::world::SkyPaint {
        top: color("topcolor", [0.2, 0.5, 1.0]),
        bottom: color("bottomcolor", [0.8, 1.0, 1.0]),
        fade_bias: num("fadebias", 1.0),
        hdr_scale: num("hdrscale", 0.66),
        sun_normal: sun,
        sun_size: num("sunsize", 2.0),
        sun_color: color("suncolor", [0.2, 0.1, 0.0]),
        dusk_scale: num("duskscale", 1.0),
        dusk_intensity: num("duskintensity", 1.0),
        dusk_color: color("duskcolor", [1.0, 0.2, 0.0]),
        star_scale: num("starscale", 0.5),
        star_fade: num("starfade", 1.5),
        star_layers: if num("drawstars", 1.0) > 0.5 {
            num("starlayers", 1.0)
        } else {
            0.0
        },
        star_drift: fx_time() * num("starspeed", 0.01),
    })
}

/// Seconds since the first call: the drift of effects that scroll with time.
fn fx_time() -> f32 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f32()
}

/// ClientScheme.res colours the HudAnimations.txt health / suit sequences move between.
const FG_COLOR_HUD: [f32; 4] = [1.0, 220.0 / 255.0, 0.0, 220.0 / 255.0];
const BRIGHT_FG: [f32; 4] = [1.0, 220.0 / 255.0, 0.0, 1.0];
const BRIGHT_DAMAGED_FG: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const DAMAGED_FG: [f32; 4] = [180.0 / 255.0, 0.0, 0.0, 230.0 / 255.0];
const DAMAGED_BG: [f32; 4] = [180.0 / 255.0, 0.0, 0.0, 200.0 / 255.0];
const BG_COLOR_HUD: [f32; 4] = [0.0, 0.0, 0.0, 90.0 / 255.0];
const PULSE_BG: [f32; 4] = [100.0 / 255.0, 0.0, 0.0, 80.0 / 255.0];

#[derive(Clone, Copy)]
enum Interp {
    Linear,
    Deaccel,
}

/// One `Animate <property> <value> <interp> <start> <duration>` line of HudAnimations.txt.
struct Seg<T>(T, Interp, f32, f32);

fn lerp4(a: [f32; 4], b: [f32; 4], f: f32) -> [f32; 4] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * f)
}

/// The property's value `t` seconds into a sequence of non-overlapping segments (the vgui
/// animation controller: Linear is the plain fraction, Deaccel its square root).
fn run_segments<T: Copy>(initial: T, segs: &[Seg<T>], t: f32, mix: impl Fn(T, T, f32) -> T) -> T {
    let mut cur = initial;
    for Seg(to, interp, start, dur) in segs {
        if t < *start {
            break;
        }
        let f = if *dur <= 0.0 {
            1.0
        } else {
            ((t - start) / dur).min(1.0)
        };
        let f = match interp {
            Interp::Linear => f,
            Interp::Deaccel => f.sqrt(),
        };
        cur = mix(cur, *to, f);
    }
    cur
}

#[derive(Clone, Copy, PartialEq)]
enum HudEvent {
    DamageTaken,
    Low,
    IncreasedBelow,
    IncreasedAbove,
    PowerZero,
}

/// The colours / glow / alpha a health or suit panel shows right now.
struct HudPanelLook {
    fg: [f32; 4],
    text: [f32; 4],
    bg: [f32; 4],
    blur: f32,
    alpha: f32,
}

/// `PaintBackgroundType 2` (ui.md 1.7, gap-report #153): the HUD panel box from the four
/// `vgui/hud/800corner1..4` sprites (top-left, top-right, bottom-right, bottom-left) in the
/// panel colour and flat strips between them. The 8 px corner size scales with the layout
/// (an approximation of the scheme's proportional corner size).
fn hud_corner_box(
    items: &mut Vec<UiItem>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    s: f32,
    color: [f32; 4],
) {
    let c = (8.0 * s).min(w / 2.0).min(h / 2.0);
    let corners = [
        ("vgui/hud/800corner1", x, y),
        ("vgui/hud/800corner2", x + w - c, y),
        ("vgui/hud/800corner3", x + w - c, y + h - c),
        ("vgui/hud/800corner4", x, y + h - c),
    ];
    for (m, cx, cy) in corners {
        items.push(UiItem::Slice {
            x: cx,
            y: cy,
            w: c,
            h: c,
            material: m.to_string(),
            color,
            uv: [0.0, 0.0, 1.0, 1.0],
        });
    }
    for (rx, ry, rw, rh) in [
        (x + c, y, w - 2.0 * c, c),
        (x, y + c, w, h - 2.0 * c),
        (x + c, y + h - c, w - 2.0 * c, c),
    ] {
        items.push(UiItem::Rect {
            x: rx,
            y: ry,
            w: rw,
            h: rh,
            color,
        });
    }
}

/// `CHudAmmo` HudAnimations (ui.md §5.3, gap-report #154): `WeaponChanged` flashes the panel
/// background 250 220 0 80 (0.1 s in, Deaccel back over 1.0 s); `AmmoDecreased` blurs 7 ->
/// 0 (Deaccel 1.5 s) with the digits BrightFg for 0.1 s then Deaccel back over 0.75 s;
/// `AmmoIncreased` digits BrightFg over 0.15 s then Deaccel 1.5 s, blur 5 -> 0 over 1.5 s.
#[derive(Default)]
struct HudAmmoAnim {
    weapon: Option<u8>,
    clip: Option<i32>,
    changed: Option<Instant>,
    decreased: Option<Instant>,
    increased: Option<Instant>,
}

impl HudAmmoAnim {
    fn update(&mut self, weapon: u8, clip: i32, now: Instant) {
        if self.weapon != Some(weapon) {
            if self.weapon.is_some() {
                self.changed = Some(now);
            }
            self.weapon = Some(weapon);
            self.clip = Some(clip);
            return;
        }
        if let Some(last) = self.clip {
            if clip < last {
                self.decreased = Some(now);
            } else if clip > last {
                self.increased = Some(now);
            }
        }
        self.clip = Some(clip);
    }

    fn look(&self, now: Instant) -> HudPanelLook {
        use Interp::{Deaccel, Linear};
        let mut look = HudPanelLook {
            fg: FG_COLOR_HUD,
            text: FG_COLOR_HUD,
            bg: BG_COLOR_HUD,
            blur: 0.0,
            alpha: 1.0,
        };
        let f1 = |a: f32, b: f32, f: f32| a + (b - a) * f;
        let t = |at: Option<Instant>| at.map(|a| now.duration_since(a).as_secs_f32());
        if let Some(t) = t(self.changed) {
            look.bg = run_segments(
                BG_COLOR_HUD,
                &[
                    Seg(
                        [250.0 / 255.0, 220.0 / 255.0, 0.0, 80.0 / 255.0],
                        Linear,
                        0.0,
                        0.1,
                    ),
                    Seg(BG_COLOR_HUD, Deaccel, 0.1, 1.0),
                ],
                t,
                lerp4,
            );
        }
        match (t(self.decreased), t(self.increased)) {
            (Some(td), ti) if ti.is_none_or(|ti| td <= ti) => {
                look.blur = run_segments(7.0, &[Seg(0.0, Deaccel, 0.1, 1.5)], td, f1);
                look.text = run_segments(
                    FG_COLOR_HUD,
                    &[
                        Seg(BRIGHT_FG, Linear, 0.0, 0.1),
                        Seg(FG_COLOR_HUD, Deaccel, 0.1, 0.75),
                    ],
                    td,
                    lerp4,
                );
            }
            (_, Some(ti)) => {
                look.blur = run_segments(5.0, &[Seg(0.0, Linear, 0.01, 1.5)], ti, f1);
                look.text = run_segments(
                    FG_COLOR_HUD,
                    &[
                        Seg(BRIGHT_FG, Linear, 0.0, 0.15),
                        Seg(FG_COLOR_HUD, Deaccel, 0.15, 1.5),
                    ],
                    ti,
                    lerp4,
                );
            }
            _ => {}
        }
        look
    }
}

/// `CHudHealth` / `CHudBattery` threshold machine driving HudAnimations.txt (ui.md 3.3-3.4,
/// 4.2): a drop flashes the digits bright yellow, health under 20 pulses red every 0.9 s,
/// the suit only flashes (its red loop is commented out in GMod's file) and fades at 0.
#[derive(Default)]
struct HudPanelAnim {
    last: Option<i32>,
    event: Option<(HudEvent, Instant)>,
}

impl HudPanelAnim {
    fn update(&mut self, value: i32, now: Instant, health: bool) {
        let Some(last) = self.last.replace(value) else {
            return;
        };
        if value == last {
            return;
        }
        let ev = if health {
            if (1..20).contains(&value) {
                if value < last {
                    HudEvent::Low
                } else {
                    HudEvent::IncreasedBelow
                }
            } else if value < last {
                HudEvent::DamageTaken
            } else if last < 20 {
                HudEvent::IncreasedAbove
            } else {
                return;
            }
        } else if value == 0 {
            HudEvent::PowerZero
        } else if value < last {
            HudEvent::DamageTaken
        } else if last < 20 {
            HudEvent::IncreasedAbove
        } else {
            return;
        };
        self.event = Some((ev, now));
    }

    fn look(&self, now: Instant) -> HudPanelLook {
        use Interp::{Deaccel, Linear};
        let mut look = HudPanelLook {
            fg: FG_COLOR_HUD,
            text: FG_COLOR_HUD,
            bg: BG_COLOR_HUD,
            blur: 0.0,
            alpha: if self.last == Some(0) { 0.0 } else { 1.0 },
        };
        let Some((ev, at)) = self.event else {
            return look;
        };
        let t = now.duration_since(at).as_secs_f32();
        let f1 = |a: f32, b: f32, f: f32| a + (b - a) * f;
        let pulse = |look: &mut HudPanelLook, tp: f32| {
            let tp = tp % 0.9;
            look.blur = run_segments(
                3.0,
                &[Seg(5.0, Linear, 0.0, 0.1), Seg(2.0, Deaccel, 0.1, 0.8)],
                tp,
                f1,
            );
            look.text = run_segments(
                DAMAGED_FG,
                &[
                    Seg(BRIGHT_DAMAGED_FG, Linear, 0.0, 0.1),
                    Seg(DAMAGED_FG, Deaccel, 0.1, 0.8),
                ],
                tp,
                lerp4,
            );
            look.bg = run_segments(
                BG_COLOR_HUD,
                &[
                    Seg(PULSE_BG, Linear, 0.0, 0.1),
                    Seg(BG_COLOR_HUD, Deaccel, 0.1, 0.8),
                ],
                tp,
                lerp4,
            );
            look.fg = DAMAGED_FG;
        };
        match ev {
            HudEvent::DamageTaken => {
                look.fg = run_segments(
                    FG_COLOR_HUD,
                    &[
                        Seg(BRIGHT_FG, Linear, 0.0, 0.25),
                        Seg(FG_COLOR_HUD, Linear, 0.3, 0.75),
                    ],
                    t,
                    lerp4,
                );
                look.text = run_segments(
                    FG_COLOR_HUD,
                    &[
                        Seg(BRIGHT_FG, Linear, 0.0, 0.1),
                        Seg(FG_COLOR_HUD, Deaccel, 0.1, 1.2),
                    ],
                    t,
                    lerp4,
                );
                look.blur = run_segments(
                    0.0,
                    &[Seg(3.0, Linear, 0.0, 0.1), Seg(0.0, Deaccel, 0.1, 2.0)],
                    t,
                    f1,
                );
            }
            HudEvent::Low => {
                if t >= 1.0 {
                    pulse(&mut look, t - 1.0);
                } else {
                    look.bg = run_segments(
                        BG_COLOR_HUD,
                        &[
                            Seg(DAMAGED_BG, Linear, 0.0, 0.1),
                            Seg(BG_COLOR_HUD, Deaccel, 0.1, 1.75),
                        ],
                        t,
                        lerp4,
                    );
                    look.fg = run_segments(
                        FG_COLOR_HUD,
                        &[
                            Seg(BRIGHT_FG, Linear, 0.0, 0.2),
                            Seg(DAMAGED_FG, Linear, 0.2, 1.2),
                        ],
                        t,
                        lerp4,
                    );
                    look.text = run_segments(
                        FG_COLOR_HUD,
                        &[
                            Seg(BRIGHT_FG, Linear, 0.0, 0.1),
                            Seg(DAMAGED_FG, Deaccel, 0.1, 1.2),
                        ],
                        t,
                        lerp4,
                    );
                    look.blur = run_segments(
                        0.0,
                        &[Seg(5.0, Linear, 0.0, 0.1), Seg(3.0, Deaccel, 0.1, 0.9)],
                        t,
                        f1,
                    );
                }
            }
            HudEvent::IncreasedBelow => {
                if t >= 0.4 {
                    pulse(&mut look, t - 0.4);
                } else {
                    look.fg = DAMAGED_FG;
                    look.text = DAMAGED_FG;
                    look.blur = run_segments(2.0, &[Seg(5.0, Deaccel, 0.0, 0.4)], t, f1);
                }
            }
            HudEvent::IncreasedAbove => {
                look.text = run_segments(
                    DAMAGED_FG,
                    &[Seg(FG_COLOR_HUD, Linear, 0.0, 0.04)],
                    t,
                    lerp4,
                );
                look.fg = run_segments(
                    DAMAGED_FG,
                    &[Seg(FG_COLOR_HUD, Linear, 0.0, 0.03)],
                    t,
                    lerp4,
                );
                look.blur = run_segments(
                    3.0,
                    &[Seg(3.0, Linear, 0.0, 0.1), Seg(0.0, Deaccel, 0.1, 2.0)],
                    t,
                    f1,
                );
            }
            HudEvent::PowerZero => {
                look.alpha = run_segments(1.0, &[Seg(0.0, Linear, 0.0, 0.4)], t, f1);
            }
        }
        look
    }
}

/// The HUD / menu fonts rasterised for a `height`-line screen: GMod sizes its HUD from the
/// 480-line layout (ScreenScaleH), so every atlas is rebuilt whenever the window height
/// changes - a 720-line atlas stretched to a 1440-line desktop is the blur it would show.
fn register_ui_fonts(mount: &gm_content::Mount, ui: &mut UiRenderer, gpu: &Gpu, height: u32) {
    // the HL2 HUD fonts of resource/ClientScheme.res: `tall` is absolute pixels (the layout
    // scales with the screen, the glyphs do not); only the `yres` blocks step at 600 lines
    let tall = height >= 600;
    // the HUD's `Slice` materials: the default crosshair sheet, the zoom overlay and the
    // rounded-box corners of PaintBackgroundType 2 (ui.md 1.7)
    for m in [
        "sprites/crosshairs",
        "vgui/zoom",
        "vgui/hud/800corner1",
        "vgui/hud/800corner2",
        "vgui/hud/800corner3",
        "vgui/hud/800corner4",
    ] {
        ui.load_image(gpu, mount, m);
    }
    ui.register_font(gpu, mount, "HudNumbers", "HalfLife2", 32.0);
    let glow = |blur| gm_render::ui::FontFx { blur, scanlines: 2 };
    ui.register_font_fx(gpu, mount, "HudNumbersGlow", "HalfLife2", 32.0, glow(4));
    ui.register_font_fx(
        gpu,
        mount,
        "WeaponIconsSelected",
        "HalfLife2",
        64.0,
        glow(5),
    );
    ui.register_font(gpu, mount, "HudNumbersSmall", "HalfLife2", 16.0);
    let hud_default = if tall { 13.0 } else { 9.0 };
    if !ui.register_font(gpu, mount, "HudDefault", "Verdana", hud_default) {
        ui.register_font(gpu, mount, "HudDefault", "Roboto-Bold", hud_default);
    }
    // ChatFont / TargetID: Verdana 700 with a drop shadow, 12 px below 600 lines, 14 at 600+
    let chat = if tall { 14.0 } else { 12.0 };
    for name in ["ChatFont", "TargetID"] {
        if !ui.register_font(gpu, mount, name, "Verdana", chat) {
            ui.register_font(gpu, mount, name, "Roboto-Bold", chat);
        }
    }
    // HL2MPTypeDeath: csd.ttf 64 (base gamemode Lua, absolute)
    ui.register_font(gpu, mount, "HL2MPTypeDeath", "csd", 64.0);
    // gmod_tool/cl_init.lua: the toolgun HUD's fonts (Roboto Bk 80 / 24 / 17)
    for (name, px) in [
        ("GModToolName", 80.0),
        ("GModToolSubtitle", 24.0),
        ("GModToolHelp", 17.0),
    ] {
        if !ui.register_font(gpu, mount, name, "Roboto Black", px) {
            ui.register_font(gpu, mount, name, "Roboto-Bold", px);
        }
    }
    // DermaDefaultBold: Tahoma 13 weight 800 (derma/init.lua), not scaled by yres
    if !ui.register_font(gpu, mount, "DermaDefaultBold", "Tahoma", 13.0) {
        ui.register_font(gpu, mount, "DermaDefaultBold", "Roboto-Bold", 13.0);
    }
    // ContentHeader: Helvetica 50 weight 1000 (contentheader.lua)
    if !ui.register_font(gpu, mount, "ContentHeader", "Helvetica", 50.0) {
        ui.register_font(gpu, mount, "ContentHeader", "Roboto-Bold", 50.0);
    }
    ui.load_image(gpu, mount, "gui/corner8");
    // GModNotify: Arial max(12, ceil(ScreenScaleH(9))) weight 500
    let text_h = (9.0 * height as f32 / 480.0).ceil().max(12.0);
    if !ui.register_font(gpu, mount, "GModNotify", "Arial", text_h) {
        ui.register_font(gpu, mount, "GModNotify", "Roboto-Regular", text_h);
    }
    for n in ["generic", "error", "undo", "hint", "cleanup"] {
        ui.load_image(gpu, mount, &format!("vgui/notices/{n}"));
    }
    // HL2 weapon selection and crosshair: halflife2.ttf glyph fonts, Verdana labels
    ui.register_font(gpu, mount, "WeaponIcons", "HalfLife2", 64.0);
    ui.register_font(gpu, mount, "Crosshairs", "HalfLife2", 40.0);
    let selection_text = if tall { 10.0 } else { 8.0 };
    for (name, px) in [
        ("HudSelectionNumbers", 11.0),
        ("HudSelectionText", selection_text),
    ] {
        if !ui.register_font(gpu, mount, name, "Verdana", px) {
            ui.register_font(gpu, mount, name, "Roboto-Bold", px);
        }
    }
    // base cl_scoreboard.lua / cl_voice.lua fonts: Helvetica 22 / 32 weight 800, Arial 21
    for (name, family, fallback, px) in [
        ("ScoreboardDefault", "Helvetica-Bold", "Roboto-Bold", 22.0),
        (
            "ScoreboardDefaultTitle",
            "Helvetica-Bold",
            "Roboto-Bold",
            32.0,
        ),
        ("GModVoiceNotify", "Arial", "Roboto-Regular", 21.0),
    ] {
        if !ui.register_font(gpu, mount, name, family, px) {
            ui.register_font(gpu, mount, name, fallback, px);
        }
    }
    for img in [
        "vgui/avatar_default",
        "icon32/muted.png",
        "icon32/unmuted.png",
    ] {
        ui.load_image(gpu, mount, img);
    }
    ui.load_image(gpu, mount, "weapons/swep");
    ui.load_image(gpu, mount, "gui/speech_lid");
    // Derma (derma/init.lua): DermaDefault = Tahoma 13 on Windows, DejaVu Sans 14
    // elsewhere (500 / 800); system fonts, Roboto from the content when missing
    let (derma_family, derma_px) = if cfg!(windows) {
        ("Tahoma", 13.0)
    } else {
        ("DejaVu Sans", 14.0)
    };
    if !ui.register_font(gpu, mount, derma::FONT, derma_family, derma_px) {
        ui.register_font(gpu, mount, derma::FONT, "Roboto-Medium", derma_px);
    }
    let derma_bold = format!("{derma_family}-Bold");
    if !ui.register_font(gpu, mount, derma::FONT_BOLD, &derma_bold, derma_px) {
        ui.register_font(gpu, mount, derma::FONT_BOLD, "Roboto-Bold", derma_px);
    }
    ui.load_image(gpu, mount, derma::ATLAS);
    ui.load_image(gpu, mount, derma::HOVER_MAT);
    ui.load_image(gpu, mount, derma::GENERATING_MAT);
    // menu.html: Helvetica / Arial (the system's; Liberation Sans via fontconfig on
    // Linux, as the game's browser gets), options 23 / 16 px bold, body 12
    let html = |ui: &mut UiRenderer, name: &str, bold: bool, px: f32| {
        let (family, fallback) = if bold {
            ("Arial-Bold", "Roboto-Bold")
        } else {
            ("Arial", "Roboto-Regular")
        };
        if !ui.register_font(gpu, mount, name, family, px) {
            ui.register_font(gpu, mount, name, fallback, px);
        }
    };
    html(ui, derma::MENU_FONT_23, true, 23.0);
    html(ui, derma::MENU_FONT_16, true, 16.0);
    html(ui, derma::NAV_FONT, false, 12.0);
    // cl_worldtips.lua: GModWorldtip = Helvetica 20 weight 700
    html(ui, "GModWorldtip", true, 20.0);
    // SourceScheme: Tahoma [$WINDOWS] / Verdana elsewhere, Default 13 / bold / Large 15
    let scheme = if cfg!(windows) { "Tahoma" } else { "Verdana" };
    let scheme_bold = format!("{scheme}-Bold");
    if !ui.register_font(gpu, mount, options::FONT, scheme, 13.0) {
        ui.register_font(gpu, mount, options::FONT, "Roboto-Regular", 13.0);
    }
    for (name, px) in [(options::FONT_BOLD, 13.0), (options::FONT_LARGE, 15.0)] {
        if !ui.register_font(gpu, mount, name, &scheme_bold, px) {
            ui.register_font(gpu, mount, name, "Roboto-Bold", px);
        }
    }
    ui.load_image(gpu, mount, "vgui/logos/garrysmod.png");
    // newgame.html: body 12, labels 11 / 11 bold, category header 32 + 14, button 20
    for (name, bold, size) in [
        (newgame::F10, false, 10.0),
        (newgame::F11, false, 11.0),
        (newgame::F11B, true, 11.0),
        (newgame::F13, false, 13.0),
        (newgame::F14, false, 14.0),
        (newgame::F20, false, 20.0),
        (newgame::F32, false, 32.0),
        (servers::F20B, true, 20.0),
        (servers::F13B, true, 13.0),
        (ugc::F14B, true, 14.0),
        (ugc::F48B, true, 48.0),
    ] {
        html(ui, name, bold, size);
    }
    for name in derma::MENU_IMAGES {
        ui.load_image(gpu, mount, name);
    }
    for name in derma::IMAGES {
        ui.load_image(gpu, mount, name);
    }
    for (name, tall) in [
        ("TargetID", 12.0),
        ("TargetIDSmall", 11.0),
        ("ChatFont", 12.0),
    ] {
        if !ui.register_font(gpu, mount, name, "Verdana", tall) {
            ui.register_font(gpu, mount, name, "Roboto-Bold", tall);
        }
    }
}

#[cfg(test)]
mod fade_tests {
    use super::fade_alpha_at;

    #[test]
    fn screen_fade_alpha_follows_the_flags() {
        assert_eq!(fade_alpha_at(2, 1.0, 2.0, 1.0, 1.0), Some(0.5));
        assert_eq!(fade_alpha_at(2, 1.0, 2.0, 1.0, 2.5), Some(1.0));
        assert_eq!(fade_alpha_at(2, 1.0, 2.0, 1.0, 3.5), None);
        assert_eq!(fade_alpha_at(2 | 8, 1.0, 2.0, 1.0, 9.0), Some(1.0));
        assert_eq!(fade_alpha_at(1, 0.8, 2.0, 1.0, 0.5), Some(0.8));
        assert_eq!(fade_alpha_at(1, 0.8, 2.0, 1.0, 2.0), Some(0.4));
        assert_eq!(fade_alpha_at(1, 0.8, 2.0, 1.0, 3.1), None);
    }
}

/// utilities_menu.lua / undo.lua: the rows of a Utilities option's ControlPanel (Help, then
/// CheckBox / NumSlider per convar).
fn utility_schema(key: &str) -> Vec<gm_server::ToolSetting> {
    use gm_server::{ToolSetting, ToolSettingKind as K};
    let row = |name: &str, kind: K, min: f32, max: f32, default: &str, label: &str| ToolSetting {
        name: name.to_string(),
        kind,
        min,
        max,
        default: default.to_string(),
        label: label.to_string(),
        options: Vec::new(),
    };
    let help = |t: &str| row("", K::Help, 0.0, 0.0, "", t);
    let check = |n: &str, d: &str, l: &str| row(n, K::Bool, 0.0, 1.0, d, l);
    let num = |n: &str, lo: f32, hi: f32, d: &str, l: &str| row(n, K::Number, lo, hi, d, l);
    match key {
        "ServerSettings" => vec![
            help("Server settings"),
            check(
                "sv_kickerrornum",
                "0",
                "Kick players with too many Lua errors",
            ),
            check("sv_allowcslua", "0", "Allow clientside Lua scripts"),
            check("sv_sticktoground", "1", "Stick to ground"),
            check("sv_playerpickupallowed", "1", "Allow picking up entities"),
            check("mp_falldamage", "0", "Realistic fall damage"),
            check("gmod_suit", "0", "Enable HEV suit"),
            num("sv_gravity", -500.0, 1000.0, "600", "Gravity"),
            num("sv_friction", 0.0, 16.0, "8", "Friction"),
            num("phys_timescale", 0.0, 2.0, "1", "Physics timescale"),
            num(
                "sv_defaultdeployspeed",
                0.1,
                10.0,
                "4",
                "Weapon deploy speed",
            ),
            num("sv_noclipspeed", 1.0, 10.0, "5", "Noclip speed"),
            num("gmod_maxammo", 0.0, 9999.0, "9999", "Maximum ammo"),
            num(
                "g_ragdoll_maxcount",
                0.0,
                128.0,
                "32",
                "Maximum client ragdolls",
            ),
            num("gmod_physiterations", 1.0, 10.0, "4", "Physics iterations"),
            num("sv_timeout", 60.0, 300.0, "65", "Connection timeout"),
        ],
        "SandboxSettings" => {
            let mut v = vec![
                help("Sandbox settings"),
                check("sbox_weapons", "1", "Spawn with weapons"),
                check("sbox_godmode", "0", "God mode"),
                check("sbox_playershurtplayers", "1", "Players damage players"),
                check("sbox_noclip", "1", "Allow noclip"),
                check("sbox_bonemanip_npc", "1", "Bone manipulate NPCs"),
                check("sbox_bonemanip_player", "0", "Bone manipulate players"),
                check("sbox_bonemanip_misc", "0", "Bone manipulate others"),
            ];
            for (cvar, max, d, label) in [
                ("sbox_maxprops", 5000.0, "200", "Max props"),
                ("sbox_maxragdolls", 500.0, "10", "Max ragdolls"),
                ("sbox_maxnpcs", 100.0, "10", "Max NPCs"),
                ("sbox_maxvehicles", 100.0, "20", "Max vehicles"),
                ("sbox_maxeffects", 500.0, "200", "Max effects"),
                ("sbox_maxballoons", 500.0, "100", "Max balloons"),
                ("sbox_maxbuttons", 500.0, "50", "Max buttons"),
                ("sbox_maxdynamite", 500.0, "10", "Max dynamite"),
                ("sbox_maxemitters", 500.0, "50", "Max emitters"),
                ("sbox_maxhoverballs", 500.0, "100", "Max hoverballs"),
                ("sbox_maxlamps", 100.0, "3", "Max lamps"),
                ("sbox_maxlights", 100.0, "3", "Max lights"),
                ("sbox_maxsents", 500.0, "20", "Max scripted entities"),
                ("sbox_maxthrusters", 500.0, "30", "Max thrusters"),
                ("sbox_maxwheels", 500.0, "20", "Max wheels"),
                ("sbox_maxcameras", 500.0, "10", "Max cameras"),
            ] {
                v.push(num(cvar, 0.0, max, d, label));
            }
            v
        }
        "PhysgunSVSettings" => vec![
            help("Physgun server settings"),
            check("physgun_limited", "0", "Limited physgun"),
            num("physgun_maxrange", 128.0, 8192.0, "4096", "Max range"),
            num(
                "physgun_teleportdistance",
                0.0,
                10000.0,
                "0",
                "Teleport distance",
            ),
            num("physgun_maxspeed", 0.0, 10000.0, "0", "Max speed"),
            num("physgun_maxangular", 0.0, 10000.0, "0", "Max angular speed"),
            num("physgun_timetoarrive", 0.0, 2.0, "0.05", "Time to arrive"),
            num(
                "physgun_timetoarriveragdoll",
                0.0,
                2.0,
                "0.1",
                "Time to arrive (ragdolls)",
            ),
        ],
        "PhysgunSettings" => vec![
            help("Physgun settings"),
            check("physgun_halo", "1", "Draw halo around held objects"),
            check("physgun_drawbeams", "1", "Draw beams"),
            check("effects_freeze", "1", "Freeze effect"),
            check("effects_unfreeze", "1", "Unfreeze effect"),
            num("gm_snapgrid", 0.0, 128.0, "0", "Snap grid"),
            num("gm_snapangles", 5.0, 90.0, "45", "Snap angles"),
            num(
                "physgun_rotation_sensitivity",
                0.01,
                1.0,
                "0.05",
                "Rotation sensitivity",
            ),
            num("physgun_wheelspeed", 0.0, 50.0, "10", "Wheel speed"),
        ],
        "SandboxClientSettings" => vec![
            help("Sandbox client settings"),
            num(
                "sbox_search_maxresults",
                1024.0,
                8192.0,
                "1024",
                "Max search results",
            ),
            check("cl_drawhud", "1", "Draw HUD"),
            check("gmod_drawhelp", "1", "Draw tool help"),
            check("gmod_drawtooleffects", "1", "Draw tool UI"),
            check("cl_drawworldtooltips", "1", "Draw world tooltips"),
            check("cl_drawspawneffect", "1", "Spawn effect"),
            check("cl_draweffectrings", "1", "Effect rings"),
            check("cl_drawcameras", "1", "Draw cameras"),
            check("cl_drawthrusterseffects", "1", "Draw thruster effects"),
            check("cl_showhints", "1", "Show hints"),
            check("spawnmenu_toggle", "0", "Toggle spawnmenu"),
        ],
        "PlayerModelSelector" => vec![help("Player model: press C and click the icon")],
        "Undo" => vec![help("Undo: the last things you did (Z)")],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod weapon_table_tests {
    use super::*;

    #[test]
    fn slot_constants_match_the_table() {
        assert_eq!(WEAPONS[0].class, "weapon_physgun");
        assert_eq!(WEAPONS[1].class, "gmod_tool");
        assert_eq!(WEAPONS[GRAVGUN_SLOT as usize].class, "weapon_physcannon");
        assert_eq!(WEAPONS[CAMERA_SLOT as usize].class, "gmod_camera");
        // every wire class has a local viewmodel entry (the selector drops those without)
        for c in gm_server::WIRE_WEAPONS {
            assert!(WEAPONS.iter().any(|w| w.class == c), "{c}");
        }
    }
}
