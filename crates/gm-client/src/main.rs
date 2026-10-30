//! Playable client: a window with the live world render, keyboard and mouse input, and an
//! in-process server reached through the loopback transport (the same protocol as over UDP).
//!
//! `GMOD_DIR=... cargo run --release -p gm-client -- [map] [yaw] [pitch]`
//! WASD move, mouse look, space jump, ctrl duck, shift run, LMB/RMB fire, 1/2/3 weapon
//! (none, physgun, tool gun), Q spawn a prop, Z undo, Esc release the mouse / quit.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Result};
use glam::Vec3;
use gm_move::buttons;
use gm_net::{ClientNet, Command, Loopback, LoopbackEndpoint, NetCmd, ServerNet};
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
    grabbed: bool,
    last: Instant,
    acc: f32,
    frames: u32,
    fps_at: Instant,
    error: Option<anyhow::Error>,
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
            }
        }
    }

    fn render(&mut self) {
        let alpha = self.acc / gm_core::tick::DEFAULT_TICK_INTERVAL;
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
        for e in ents.values() {
            if !g.models.contains_key(&e.model) {
                let m = g.world.load_model(&g.gpu, &self.mount, &e.model);
                g.models.insert(e.model.clone(), m);
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
                Some((
                    g.models.get(&e.model)?.as_ref()?,
                    gm_render::world::model_transform(pos, ang),
                ))
            })
            .collect();
        let me = self.sim.client.player_id;
        for p in &self.sim.client.world.players {
            if Some(p.id) == me || !p.alive {
                continue;
            }
            let h = if p.ducked { 36.0 } else { 72.0 };
            items.push((
                &g.player_box,
                glam::Mat4::from_translation(p.origin + Vec3::Z * (h / 2.0))
                    * glam::Mat4::from_rotation_z(p.view_angles.y.to_radians())
                    * glam::Mat4::from_scale(Vec3::new(32.0, 32.0, h)),
            ));
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
                items.push((
                    &g.beam,
                    glam::Mat4::from_scale_rotation_translation(
                        Vec3::new(len, 1.0, 1.0),
                        rot,
                        start + d * 0.5,
                    ),
                ));
            }
        }
        g.world
            .draw_models(&g.gpu, &camera, &view, &g.depth, aspect, &items);
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
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                let down = event.state == ElementState::Pressed;
                if down && !event.repeat {
                    match code {
                        KeyCode::Escape if self.grabbed => self.set_grab(false),
                        KeyCode::Escape => el.exit(),
                        KeyCode::Digit1 => self.weapon = 0,
                        KeyCode::Digit2 => self.weapon = 1,
                        KeyCode::Digit3 => self.weapon = 2,
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
                    self.mouse.insert(b);
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
        grabbed: false,
        last: Instant::now(),
        acc: 0.0,
        frames: 0,
        fps_at: Instant::now(),
        error: None,
    };
    let el = EventLoop::new()?;
    el.set_control_flow(ControlFlow::Poll);
    el.run_app(&mut app)?;
    match app.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
