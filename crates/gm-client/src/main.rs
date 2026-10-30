//! Playable client: a window with the live world render, keyboard and mouse input, and an
//! in-process server reached through the loopback transport (the same protocol as over UDP).
//!
//! `GMOD_DIR=... cargo run --release -p gm-client -- [map] [yaw] [pitch]`
//! WASD move, mouse look, space jump, ctrl duck, shift run, LMB/RMB fire, 1/2/3 weapon
//! (none, physgun, tool gun), Q spawn a prop, Z undo, Esc release the mouse / quit,
//! ` (grave) console: `help`, `spawn [model]`, `undo`, `cleanup`, `clear`, `quit`, server
//! console lines (`status`, `cvar`, `save`, `load`, ...).

use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Result};
use glam::Vec3;
use gm_move::buttons;
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
    prev_origin: Vec3,
    origin: Vec3,
    ducked: bool,
}

impl Sim {
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
        let w = &self.client.world;
        if w.tick != seen {
            self.interp.push(w.tick, &w.entities);
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
/// GMod's default `viewmodel_fov`.
const VIEWMODEL_FOV: f32 = 54.0;
/// Hands drawn on the `c_` viewmodels' arm bones (GMod's default player hands).
const HANDS_MODEL: &str = "models/weapons/c_arms_citizen.mdl";

/// Viewmodel of a weapon slot: model, idle sequence, sequence on attack, sequence while a
/// prop is held.
struct Viewmodel {
    path: &'static str,
    idle: &'static str,
    fire: &'static str,
    hold: Option<&'static str>,
}

fn viewmodel(weapon: u8) -> Option<Viewmodel> {
    match weapon {
        1 => Some(Viewmodel {
            path: "models/weapons/c_superphyscannon.mdl",
            idle: "idle",
            fire: "fire",
            hold: Some("hold_idle"),
        }),
        2 => Some(Viewmodel {
            path: "models/weapons/c_toolgun.mdl",
            idle: "idle01",
            fire: "fire01",
            hold: None,
        }),
        _ => None,
    }
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
    /// One-shot viewmodel sequence playing (draw, fire) and when it started.
    vm_anim: Option<(&'static str, Instant)>,
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

    fn select_weapon(&mut self, weapon: u8) {
        if self.weapon != weapon {
            self.weapon = weapon;
            self.vm_anim = Some(("draw", Instant::now()));
        }
    }

    /// Sequence and time for the viewmodel this frame: the one-shot sequence while it lasts,
    /// then the hold sequence while a prop is held, else idle.
    fn vm_sequence(&mut self, vm: &Viewmodel) -> (&'static str, f32) {
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
            weapon: self.weapon,
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
            for e in self.sim.client.events.drain(..) {
                println!("{e}");
                self.console.log(e);
            }
        }
    }

    fn render(&mut self) {
        let alpha = self.acc / gm_core::tick::DEFAULT_TICK_INTERVAL;
        let vm = viewmodel(self.weapon);
        let vm_seq = vm.as_ref().map(|vm| self.vm_sequence(vm));
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
        let aspect = g.config.width as f32 / g.config.height as f32;
        g.world.draw(&g.gpu, &camera, &view, &g.depth, aspect);
        let ents = &self.sim.client.world.entities;
        let others = self.sim.client.world.players.len() > 1;
        for model in ents
            .values()
            .map(|e| e.model.as_str())
            .chain(others.then_some(gm_server::PLAYER_MODEL))
        {
            if !g.models.contains_key(model) {
                let m = g.world.load_model(&g.gpu, &self.mount, model);
                g.models.insert(model.to_string(), m);
            }
        }
        let rt = self.sim.interp.render_tick(self.sim.client.world.tick) + alpha.clamp(0.0, 1.0);
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
        let me = self.sim.client.player_id;
        let player_model = g
            .models
            .get(gm_server::PLAYER_MODEL)
            .and_then(|m| m.as_ref());
        for p in &self.sim.client.world.players {
            if Some(p.id) == me || !p.alive {
                continue;
            }
            // bind pose until animations: standing model, the hull box when the model is missing
            match player_model {
                Some(m) => items.push(DrawItem {
                    model: m,
                    transform: gm_render::world::model_transform(
                        p.origin,
                        Vec3::new(0.0, p.view_angles.y, 0.0),
                    ),
                    bones: None,
                }),
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
        // physgun beam: from just below and right of the eye to the held prop, or along the aim
        if self.weapon == 1 && self.mouse.contains(&0) {
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
        };
        g.world.draw_models(&g.gpu, &camera, &target, &items);
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
                let world = m.bone_world(seq, t, None);
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
                ["none", "physgun", "toolgun"][self.weapon as usize % 3]
            ));
            self.frames = 0;
            self.fps_at = Instant::now();
        }
    }
}

impl App {
    fn ui_items(&self, o: Vec3) -> Vec<UiItem> {
        let Some(g) = &self.gfx else {
            return Vec::new();
        };
        let (w, h) = (g.config.width as f32, g.config.height as f32);
        let lh = g.ui.line_height();
        let mut items = Vec::new();
        let white = [0.92, 0.92, 0.92, 1.0];
        if !self.console.open {
            let hud = format!(
                "props {}  weapon {}  pos {:.0} {:.0} {:.0}",
                self.sim.client.world.entities.len(),
                ["none", "physgun", "toolgun"][self.weapon as usize % 3],
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
            "bot" => {
                let id = 1000 + self.sim.server.players.len() as u32;
                let yaw = self.view.y.to_radians();
                let fwd = Vec3::new(yaw.cos(), yaw.sin(), 0.0);
                self.sim.server.add_player(
                    id,
                    self.sim.origin + fwd * 96.0,
                    Vec3::new(0.0, self.view.y + 180.0, 0.0),
                );
                self.console.log(format!("bot {id} added"));
            }
            "cleanup" => self.sim.client.command(Command::Cleanup),
            "help" => {
                self.console
                    .log("client: spawn [model] | bot | undo | cleanup | clear | quit; server:");
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
            WindowEvent::KeyboardInput { event, .. } => {
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
                        KeyCode::KeyQ => self.sim.client.command(Command::Spawn {
                            model: SPAWN_MODEL.to_string(),
                        }),
                        KeyCode::KeyZ => self.sim.client.command(Command::Undo),
                        _ => {}
                    }
                }
                if down {
                    self.keys.insert(code);
                } else {
                    self.keys.remove(&code);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if self.console.open {
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
                        if let Some(vm) = viewmodel(self.weapon) {
                            // the physgun's right click only acts while holding
                            if b == 0 || vm.hold.is_none() {
                                self.vm_anim = Some((vm.fire, Instant::now()));
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
    let mut net = ServerNet::new(&map, spawn);
    net.admins.extend(1..=16);
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
        prev_origin: spawn,
        origin: spawn,
        ducked: false,
    };
    let mut app = App {
        mount,
        bsp_data,
        sim,
        gfx: None,
        keys: BTreeSet::new(),
        mouse: BTreeSet::new(),
        view: Vec3::new(arg_f(3).unwrap_or(0.0), arg_f(2).unwrap_or(yaw), 0.0),
        weapon: 1,
        vm_anim: None,
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
