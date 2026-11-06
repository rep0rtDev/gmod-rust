//! The game's particle systems (`particles/*.pcf`): definitions are the DMX operator lists, an
//! instance runs its emitters, initializers and operators per frame and draws
//! `render_animated_sprites` through the material's sprite sheet (`render_sprite_trail` as a
//! quad stretched along the velocity). Control point 0 is where the effect was created (or the
//! entity it follows), its forward is the effect's angles.
use glam::Vec3;
use gm_formats::pcf::{function_name, Element, Pcf};
use gm_formats::vtf::sheet::Sheet;
use gm_render::world::Vertex;
use gm_render::{Camera, Gpu, WorldRenderer};
use std::collections::HashMap;

const MAX_CP: usize = 4;
/// An effect nobody stops (an endless emitter without an entity) ends after this long.
const ORPHAN_LIFE: f32 = 30.0;

#[derive(Clone, Debug)]
struct Op {
    func: String,
    el: Element,
}

impl Op {
    fn f(&self, name: &str, default: f32) -> f32 {
        self.el.float(name).unwrap_or(default)
    }
    fn i(&self, name: &str, default: i32) -> i32 {
        self.el.int(name).unwrap_or(default)
    }
    fn b(&self, name: &str, default: bool) -> bool {
        self.el.bool(name).unwrap_or(default)
    }
    fn v(&self, name: &str, default: Vec3) -> Vec3 {
        self.el.vec3(name).map(Vec3::from_array).unwrap_or(default)
    }
    fn c(&self, name: &str, default: [u8; 4]) -> [u8; 4] {
        self.el.color(name).unwrap_or(default)
    }
}

#[derive(Clone, Debug)]
struct SystemDef {
    material: String,
    max_particles: usize,
    initial_particles: usize,
    radius: f32,
    color: [f32; 3],
    alpha: f32,
    rotation: f32,
    rotation_speed: f32,
    sequence: i32,
    initializers: Vec<Op>,
    operators: Vec<Op>,
    emitters: Vec<Op>,
    forces: Vec<Op>,
    constraints: Vec<Op>,
    renderers: Vec<Op>,
    children: Vec<(String, f32)>,
}

#[derive(Clone, Debug)]
struct Particle {
    pos: Vec3,
    prev: Vec3,
    vel: Vec3,
    radius: f32,
    radius0: f32,
    roll: f32,
    roll_speed: f32,
    yaw: f32,
    color: [f32; 3],
    alpha: f32,
    alpha0: f32,
    life: f32,
    born: f32,
    sequence: i32,
    trail: f32,
    seed: u32,
}

struct Instance {
    def: String,
    cps: [(Vec3, Vec3); MAX_CP],
    prev_cps: [Vec3; MAX_CP],
    parent: Option<(u32, Vec3)>,
    t: f32,
    particles: Vec<Particle>,
    emitted: f32,
    fired: Vec<bool>,
    stopped: bool,
    children: Vec<Instance>,
    seed: u32,
}

struct Mat {
    bind: wgpu::BindGroup,
    additive: bool,
    sheet: Option<Sheet>,
}

#[derive(Default)]
pub struct Particles {
    defs: HashMap<String, SystemDef>,
    loaded: bool,
    mats: HashMap<String, Option<Mat>>,
    instances: Vec<Instance>,
    seed: u32,
}

pub type Tracer<'a> = &'a dyn Fn(Vec3, Vec3) -> Option<(Vec3, Vec3)>;

/// A particle effect the game asked for this frame.
pub struct Spawn {
    pub name: String,
    pub pos: Vec3,
    pub angles: Vec3,
    /// Entity the effect follows (0: none); the offset is kept from the spawn position.
    pub parent: u32,
}

fn rng(seed: &mut u32) -> f32 {
    let mut x = *seed;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *seed = x.max(1);
    (x >> 8) as f32 / (1u32 << 24) as f32
}

fn range(seed: &mut u32, lo: f32, hi: f32) -> f32 {
    lo + (hi - lo) * rng(seed)
}

fn range_exp(seed: &mut u32, lo: f32, hi: f32, exponent: f32) -> f32 {
    let r = rng(seed);
    let r = if (exponent - 1.0).abs() > 1e-4 {
        r.powf(exponent.max(1e-3))
    } else {
        r
    };
    lo + (hi - lo) * r
}

pub fn forward_of(angles: Vec3) -> Vec3 {
    let (sp, cp) = angles.x.to_radians().sin_cos();
    let (sy, cy) = angles.y.to_radians().sin_cos();
    Vec3::new(cp * cy, cp * sy, -sp)
}

fn color_of(c: [u8; 4]) -> [f32; 3] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
    ]
}

fn field_get(p: &Particle, field: i32) -> f32 {
    match field {
        1 => p.life,
        3 => p.radius,
        4 => p.roll,
        5 => p.roll_speed,
        7 => p.alpha,
        10 => p.trail,
        12 => p.yaw,
        _ => 0.0,
    }
}

fn field_set(p: &mut Particle, field: i32, v: f32) {
    match field {
        1 => p.life = v.max(1e-3),
        3 => p.radius = v,
        4 => p.roll = v,
        5 => p.roll_speed = v,
        7 => p.alpha = v,
        10 => p.trail = v,
        12 => p.yaw = v,
        _ => {}
    }
}

impl Particles {
    /// Loads every `particles/*.pcf` of the mount once.
    pub fn ensure_loaded(&mut self, mount: &gm_content::Mount) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.seed = 0x9e37_79b9;
        let mut files: Vec<String> = mount.find("particles/*.pcf").into_iter().collect();
        if let Ok(manifest) = mount.read_string("particles/particles_manifest.txt") {
            for line in manifest.lines() {
                if let Some(i) = line.find("particles/") {
                    let rest = &line[i..];
                    let end = rest.find('"').unwrap_or(rest.len());
                    let path = rest[..end].trim_start_matches('!').to_string();
                    if path.ends_with(".pcf") && !files.contains(&path) {
                        files.push(path);
                    }
                }
            }
        }
        for path in files {
            let Ok(data) = mount.read(&path) else {
                continue;
            };
            let Ok(pcf) = Pcf::parse(&data) else {
                eprintln!("particles: {path} does not parse");
                continue;
            };
            for &i in &pcf.systems {
                let s = &pcf.dmx.elements[i];
                let ops = |list: &str| -> Vec<Op> {
                    pcf.operators(s, list)
                        .into_iter()
                        .map(|op| Op {
                            func: function_name(op).to_ascii_lowercase(),
                            el: op.clone(),
                        })
                        .collect()
                };
                let children = pcf
                    .operators(s, "children")
                    .into_iter()
                    .filter_map(|c| {
                        let def = pcf.child_definition(c)?;
                        Some((
                            def.name.to_ascii_lowercase(),
                            c.float("delay").unwrap_or(0.0),
                        ))
                    })
                    .collect();
                let color = s.color("color").unwrap_or([255, 255, 255, 255]);
                self.defs.insert(
                    s.name.to_ascii_lowercase(),
                    SystemDef {
                        material: s.str("material").unwrap_or("").to_string(),
                        max_particles: s.int("max_particles").unwrap_or(1000).max(1) as usize,
                        initial_particles: s.int("initial_particles").unwrap_or(0).max(0) as usize,
                        radius: s.float("radius").unwrap_or(5.0),
                        color: color_of(color),
                        alpha: color[3] as f32 / 255.0,
                        rotation: s.float("rotation").unwrap_or(0.0),
                        rotation_speed: s.float("rotation_speed").unwrap_or(0.0),
                        sequence: s.int("sequence_number").unwrap_or(0),
                        initializers: ops("initializers"),
                        operators: ops("operators"),
                        emitters: ops("emitters"),
                        forces: ops("forces"),
                        constraints: ops("constraints"),
                        renderers: ops("renderers"),
                        children,
                    },
                );
            }
        }
        eprintln!("particles: {} systems", self.defs.len());
    }

    pub fn spawn(&mut self, s: &Spawn) -> bool {
        let name = s.name.to_ascii_lowercase();
        if !self.defs.contains_key(&name) {
            return false;
        }
        let seed = {
            let v = rng(&mut self.seed);
            (v * 4.0e9) as u32 | 1
        };
        let inst = self.instance(&name, s.pos, forward_of(s.angles), s.parent, seed, 0);
        self.instances.push(inst);
        true
    }

    fn instance(
        &self,
        name: &str,
        pos: Vec3,
        fwd: Vec3,
        parent: u32,
        seed: u32,
        depth: u32,
    ) -> Instance {
        let def = &self.defs[name];
        let children = if depth < 4 {
            def.children
                .iter()
                .filter(|(c, _)| self.defs.contains_key(c))
                .enumerate()
                .map(|(i, (c, delay))| {
                    let mut child = self.instance(
                        c,
                        pos,
                        fwd,
                        parent,
                        seed.wrapping_mul(31 + i as u32),
                        depth + 1,
                    );
                    child.t = -delay;
                    child
                })
                .collect()
        } else {
            Vec::new()
        };
        Instance {
            def: name.to_string(),
            cps: [(pos, fwd); MAX_CP],
            prev_cps: [pos; MAX_CP],
            parent: (parent != 0).then_some((parent, Vec3::ZERO)),
            t: 0.0,
            particles: Vec::new(),
            emitted: 0.0,
            fired: vec![false; def.emitters.len()],
            stopped: false,
            children,
            seed,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Advances every effect by `dt`; `pose(ent)` is an entity's (origin, forward) when it
    /// still exists, `trace(start, end)` the world hit (point, normal) on that segment.
    pub fn tick(&mut self, dt: f32, pose: &dyn Fn(u32) -> Option<(Vec3, Vec3)>, trace: Tracer<'_>) {
        let dt = dt.clamp(0.0, 0.1);
        let defs = std::mem::take(&mut self.defs);
        for inst in &mut self.instances {
            tick_instance(inst, &defs, dt, pose, trace, None);
        }
        self.defs = defs;
        self.instances.retain(|i| !finished(i));
    }

    /// Prepares the materials the live effects need.
    pub fn prepare(&mut self, gpu: &Gpu, mount: &gm_content::Mount, world: &WorldRenderer) {
        let mut names: Vec<String> = Vec::new();
        fn collect(i: &Instance, defs: &HashMap<String, SystemDef>, out: &mut Vec<String>) {
            if let Some(d) = defs.get(&i.def) {
                if !out.contains(&d.material) {
                    out.push(d.material.clone());
                }
            }
            for c in &i.children {
                collect(c, defs, out);
            }
        }
        for i in &self.instances {
            collect(i, &self.defs, &mut names);
        }
        for name in names {
            if self.mats.contains_key(&name) {
                continue;
            }
            let mat = world
                .sprite_material_sized(gpu, mount, &name)
                .map(|(bind, _)| {
                    let m = gm_render::material::load(mount, &name).ok();
                    let additive = m.as_ref().is_some_and(|m| m.additive);
                    let sheet = m
                        .and_then(|m| m.base_texture)
                        .and_then(|tex| {
                            let tex = tex.trim_end_matches(".vtf");
                            mount.read(&format!("materials/{tex}.vtf")).ok()
                        })
                        .and_then(|bytes| gm_formats::vtf::sheet::sheet_of(&bytes).ok().flatten());
                    Mat {
                        bind,
                        additive,
                        sheet,
                    }
                });
            if mat.is_none() {
                eprintln!("particles: material {name} missing");
            }
            self.mats.insert(name, mat);
        }
    }

    /// Appends the sprites of every effect to the additive / alpha batches.
    pub fn draw<'a>(
        &'a self,
        world: &WorldRenderer,
        camera: &Camera,
        additive: &mut Vec<(&'a wgpu::BindGroup, Vec<Vertex>, Vec<u32>)>,
        alpha: &mut Vec<(&'a wgpu::BindGroup, Vec<Vertex>, Vec<u32>)>,
    ) {
        for inst in &self.instances {
            self.draw_instance(inst, world, camera, additive, alpha);
        }
    }

    fn draw_instance<'a>(
        &'a self,
        inst: &Instance,
        world: &WorldRenderer,
        camera: &Camera,
        additive: &mut Vec<(&'a wgpu::BindGroup, Vec<Vertex>, Vec<u32>)>,
        alpha: &mut Vec<(&'a wgpu::BindGroup, Vec<Vertex>, Vec<u32>)>,
    ) {
        for c in &inst.children {
            self.draw_instance(c, world, camera, additive, alpha);
        }
        let Some(def) = self.defs.get(&inst.def) else {
            return;
        };
        let Some(Some(mat)) = self.mats.get(&def.material) else {
            return;
        };
        let Some(renderer) = def.renderers.first() else {
            return;
        };
        let trail = renderer.func == "render_sprite_trail" || renderer.func == "render_rope";
        let anim_rate = renderer.f("animation rate", 0.1);
        let fit = renderer.b("animation_fit_lifetime", false);
        let orientation = renderer.i("orientation_type", 0);
        let trail_scale = renderer.f("length fade in time", 0.0);
        let min_len = renderer.f("min length", 0.0);
        let max_len = renderer.f("max length", 2000.0);
        let (mut verts, mut idx) = (Vec::new(), Vec::new());
        for p in &inst.particles {
            let age = inst.t - p.born;
            let a = p.alpha.clamp(0.0, 1.0);
            if a <= 0.0 || p.radius <= 0.0 {
                continue;
            }
            let rgba = if mat.additive {
                [p.color[0] * a, p.color[1] * a, p.color[2] * a, 0.0]
            } else {
                [p.color[0], p.color[1], p.color[2], a.max(1e-3)]
            };
            let uv = match &mat.sheet {
                Some(sheet) => {
                    let seq = sheet.sequence(p.sequence.max(0) as u32);
                    match seq {
                        Some(seq) => {
                            let t = if fit {
                                age / p.life.max(1e-3) * seq.total_time
                            } else {
                                age * anim_rate.max(1e-4) * seq.total_time.max(1e-3)
                                    / seq.total_time.max(1e-3)
                            };
                            seq.sample(t).0
                        }
                        None => [0.0, 0.0, 1.0, 1.0],
                    }
                }
                None => [0.0, 0.0, 1.0, 1.0],
            };
            let (v, i) = if trail {
                let dir = p.pos - p.prev;
                let len = (dir.length() * p.trail.max(0.0) + trail_scale * 0.0)
                    .clamp(min_len.max(p.radius), max_len);
                let axis = if dir.length_squared() > 1e-6 {
                    Some(dir.normalize())
                } else {
                    None
                };
                let (v, i) = world.sprite_quad_uv(
                    p.pos - dir.normalize_or_zero() * (len * 0.5),
                    p.radius * 2.0,
                    rgba,
                    0.0,
                    None,
                    uv,
                    camera,
                );
                // stretch the quad along its direction: move the two trailing corners back
                let mut v = v;
                if let Some(ax) = axis {
                    for vert in v.iter_mut() {
                        let pos = Vec3::from_array(vert.position);
                        let along = (pos - p.pos).dot(ax);
                        if along < 0.0 {
                            vert.position = (pos - ax * (len - p.radius * 2.0).max(0.0)).to_array();
                        }
                    }
                }
                (v, i)
            } else {
                let axis = match orientation {
                    1 => Some(Vec3::Z),
                    _ => None,
                };
                world.sprite_quad_uv(
                    p.pos,
                    p.radius * 2.0,
                    rgba,
                    p.roll.to_radians(),
                    axis,
                    uv,
                    camera,
                )
            };
            let base = verts.len() as u32;
            verts.extend(v);
            idx.extend(i.into_iter().map(|k| k + base));
        }
        if verts.is_empty() {
            return;
        }
        if mat.additive {
            additive.push((&mat.bind, verts, idx));
        } else {
            alpha.push((&mat.bind, verts, idx));
        }
    }
}

fn finished(i: &Instance) -> bool {
    i.particles.is_empty() && emitters_done(i) && i.children.iter().all(finished)
}

fn emitters_done(i: &Instance) -> bool {
    i.stopped || i.t > ORPHAN_LIFE || i.fired.iter().all(|f| *f)
}

fn tick_instance(
    inst: &mut Instance,
    defs: &HashMap<String, SystemDef>,
    dt: f32,
    pose: &dyn Fn(u32) -> Option<(Vec3, Vec3)>,
    trace: Tracer<'_>,
    parent_cps: Option<&[(Vec3, Vec3); MAX_CP]>,
) {
    let Some(def) = defs.get(&inst.def) else {
        inst.stopped = true;
        inst.particles.clear();
        return;
    };
    // control point 0 follows the entity (or the parent system)
    if let Some(cps) = parent_cps {
        inst.cps = *cps;
    } else if let Some((ent, offset)) = inst.parent {
        match pose(ent) {
            Some((origin, fwd)) => inst.cps[0] = (origin + offset, fwd),
            None => inst.stopped = true,
        }
    }
    let was_negative = inst.t < 0.0;
    inst.t += dt;
    if inst.t < 0.0 {
        return;
    }
    if was_negative {
        inst.t = 0.0;
    }
    let t = inst.t;
    // emission
    let mut to_emit = 0usize;
    if was_negative || inst.t - dt <= 0.0 {
        to_emit += def.initial_particles;
    }
    if !inst.stopped && inst.t <= ORPHAN_LIFE {
        for (k, e) in def.emitters.iter().enumerate() {
            let start = e.f("emission_start_time", 0.0);
            match e.func.as_str() {
                "emit_continuously" => {
                    let rate = e.f("emission_rate", 100.0);
                    let duration = e.f("emission_duration", 0.0);
                    if t < start {
                        continue;
                    }
                    if duration > 0.0 && t > start + duration {
                        inst.fired[k] = true;
                        continue;
                    }
                    inst.emitted += rate * dt;
                    let n = inst.emitted.floor();
                    inst.emitted -= n;
                    to_emit += n as usize;
                }
                "emit_instantaneously" => {
                    if !inst.fired[k] && t >= start {
                        inst.fired[k] = true;
                        let n = e.i("num_to_emit", 100).max(0) as usize;
                        let lo = e.i("num_to_emit_minimum", 0).max(0) as usize;
                        to_emit += if lo > 0 && lo < n {
                            lo + ((n - lo) as f32 * rng(&mut inst.seed)) as usize
                        } else {
                            n
                        };
                    }
                }
                _ => inst.fired[k] = true,
            }
        }
    }
    let room = def.max_particles.saturating_sub(inst.particles.len());
    for _ in 0..to_emit.min(room) {
        let p = spawn_particle(def, inst, t);
        inst.particles.push(p);
    }
    // operators
    let cps = inst.cps;
    let mut cp_delta = [Vec3::ZERO; MAX_CP];
    for k in 0..MAX_CP {
        cp_delta[k] = cps[k].0 - inst.prev_cps[k];
        inst.prev_cps[k] = cps[k].0;
    }
    let seed = &mut inst.seed;
    for p in &mut inst.particles {
        let age = t - p.born;
        let mut moved = false;
        for f in &def.forces {
            match f.func.as_str() {
                // C_OP_TwistAroundAxis: push around the axis through the control point
                "twist around axis" => {
                    let amount = f.f("amount of force", 0.0);
                    let cp = f.i("control point number", 0).clamp(0, MAX_CP as i32 - 1) as usize;
                    let mut axis = f.v("twist axis", Vec3::Z).normalize_or_zero();
                    if f.b("object local space axis 0/1", false) {
                        let fwd = cps[cp].1;
                        let right = fwd.cross(Vec3::Z).normalize_or_zero();
                        let up = right.cross(fwd).normalize_or_zero();
                        axis = (fwd * axis.x + right * axis.y + up * axis.z).normalize_or_zero();
                    }
                    let r = p.pos - cps[cp].0;
                    let tangent = axis.cross(r).normalize_or_zero();
                    p.vel += tangent * amount * dt;
                }
                // C_OP_RandomForce: a fresh random acceleration every frame
                "random force" => {
                    let lo = f.v("min force", Vec3::ZERO);
                    let hi = f.v("max force", Vec3::ZERO);
                    p.vel += Vec3::new(
                        range(seed, lo.x, hi.x),
                        range(seed, lo.y, hi.y),
                        range(seed, lo.z, hi.z),
                    ) * dt;
                }
                _ => {}
            }
        }
        for op in &def.operators {
            match op.func.as_str() {
                "movement basic" => {
                    let gravity = op.v("gravity", Vec3::ZERO);
                    let drag = op.f("drag", 0.0);
                    p.vel += gravity * dt;
                    p.vel *= (1.0 - drag).clamp(0.0, 1.0).powf(dt * 30.0);
                    p.prev = p.pos;
                    p.pos += p.vel * dt;
                    moved = true;
                }
                "lifespan decay" => {}
                "alpha fade in random" => {
                    let lo = op.f("fade in time min", 0.25);
                    let hi = op.f("fade in time max", 0.25);
                    let prop = op.b("proportional 0/1", true);
                    let mut fade = lo + (hi - lo) * (p.seed as f32 / u32::MAX as f32);
                    if prop {
                        fade *= p.life;
                    }
                    if fade > 0.0 && age < fade {
                        p.alpha = p.alpha.min(p.alpha0 * (age / fade));
                    }
                }
                "alpha fade out random" => {
                    let lo = op.f("fade out time min", 0.25);
                    let hi = op.f("fade out time max", 0.25);
                    let prop = op.b("proportional 0/1", true);
                    let mut fade = lo + (hi - lo) * (p.seed as f32 / u32::MAX as f32);
                    if prop {
                        fade *= p.life;
                    }
                    let start = p.life - fade;
                    if fade > 0.0 && age > start {
                        p.alpha = p
                            .alpha
                            .min(p.alpha0 * (1.0 - (age - start) / fade).max(0.0));
                    }
                }
                "alpha fade and decay" => {
                    let a0 = op.f("start_alpha", 1.0);
                    let a1 = op.f("end_alpha", 0.0);
                    let fi0 = op.f("start_fade_in_time", 0.0);
                    let fi1 = op.f("end_fade_in_time", 0.5);
                    let fo0 = op.f("start_fade_out_time", 0.5);
                    let fo1 = op.f("end_fade_out_time", 1.0);
                    let f = age / p.life.max(1e-3);
                    let a = if f < fi1 {
                        a0 + (p.alpha0 - a0) * ((f - fi0) / (fi1 - fi0).max(1e-4)).clamp(0.0, 1.0)
                    } else if f > fo0 {
                        p.alpha0
                            + (a1 - p.alpha0) * ((f - fo0) / (fo1 - fo0).max(1e-4)).clamp(0.0, 1.0)
                    } else {
                        p.alpha0
                    };
                    p.alpha = a;
                }
                "color fade" => {
                    let target = color_of(op.c("color_fade", [255, 255, 255, 255]));
                    let s = op.f("fade_start_time", 0.0);
                    let e = op.f("fade_end_time", 1.0);
                    let f = ((age / p.life.max(1e-3) - s) / (e - s).max(1e-4)).clamp(0.0, 1.0);
                    let base = def.color;
                    for k in 0..3 {
                        p.color[k] = base[k] + (target[k] - base[k]) * f;
                    }
                }
                "radius scale" => {
                    let s = op.f("start_time", 0.0);
                    let e = op.f("end_time", 1.0);
                    let r0 = op.f("radius_start_scale", 1.0);
                    let r1 = op.f("radius_end_scale", 1.0);
                    let ease = op.b("ease_in_and_out", false);
                    let mut f = ((age / p.life.max(1e-3) - s) / (e - s).max(1e-4)).clamp(0.0, 1.0);
                    if ease {
                        f = f * f * (3.0 - 2.0 * f);
                    }
                    p.radius = p.radius0 * (r0 + (r1 - r0) * f);
                }
                "oscillate scalar" => {
                    let field = op.i("oscillation field", 7);
                    let rate = range(
                        seed,
                        op.f("oscillation rate min", 0.0),
                        op.f("oscillation rate max", 0.0),
                    );
                    let freq = op.f("oscillation frequency min", 1.0);
                    let s = op.f("start time min", 0.0);
                    let e = op.f("end time min", 1.0);
                    let prop = op.b("proportional 0/1", true);
                    let f = if prop { age / p.life.max(1e-3) } else { age };
                    if f >= s && f <= e {
                        let v = field_get(p, field)
                            + rate * (std::f32::consts::TAU * freq * age).cos() * dt;
                        field_set(p, field, v);
                    }
                }
                "rotation spin roll" => {
                    let rate = op.f("spin_rate_degrees", 0.0);
                    let min = op.f("spin_rate_min", 0.0);
                    let stop = op.f("spin_stop_time", 0.0);
                    if stop <= 0.0 || age < stop {
                        p.roll += rate.max(min) * dt;
                    }
                }
                "movement lock to control point" => {
                    let cp = op.i("control_point_number", 0).clamp(0, MAX_CP as i32 - 1) as usize;
                    let s = op.f("start_time", 0.0);
                    let e = op.f("end_time", 1.0);
                    let f = age / p.life.max(1e-3);
                    if f >= s && f <= e {
                        p.pos += cp_delta[cp];
                    }
                }
                "movement dampen relative to control point" => {
                    let cp = op.i("control_point_number", 0).clamp(0, MAX_CP as i32 - 1) as usize;
                    let r = op.f("falloff range", 100.0);
                    let scale = op.f("dampen scale", 1.0);
                    let d = (p.pos - cps[cp].0).length();
                    if d < r {
                        p.vel *=
                            (1.0 - scale * (1.0 - d / r.max(1e-3)) * dt * 10.0).clamp(0.0, 1.0);
                    }
                }
                _ => {}
            }
        }
        if !moved {
            p.prev = p.pos;
        }
        p.roll += p.roll_speed * dt;
        // C_OP_WorldTraceConstraint: bounce / slide on the world along this frame's motion
        for c in &def.constraints {
            if c.func != "collision via traces" || p.pos == p.prev {
                continue;
            }
            let bounce = c.f("amount of bounce", 0.0);
            let slide = c.f("amount of slide", 0.0);
            let radius = p.radius * c.f("radius scale", 1.0);
            let dir = (p.pos - p.prev).normalize_or_zero();
            if let Some((hit, n)) = trace(p.prev, p.pos + dir * radius) {
                p.pos = hit + n * radius.max(0.5) - dir * 0.0;
                let into = p.vel.dot(n);
                if into < 0.0 {
                    let normal = n * into;
                    let tangent = p.vel - normal;
                    p.vel = tangent * slide.clamp(0.0, 1.0) - normal * bounce.clamp(0.0, 1.0);
                    if bounce <= 0.0 && slide <= 0.0 {
                        p.vel = Vec3::ZERO;
                    }
                }
            }
        }
    }
    inst.particles.retain(|p| t - p.born < p.life);
    let cps = inst.cps;
    for c in &mut inst.children {
        if inst.stopped {
            c.stopped = true;
        }
        tick_instance(c, defs, dt, pose, trace, Some(&cps));
    }
}

fn spawn_particle(def: &SystemDef, inst: &mut Instance, t: f32) -> Particle {
    let seed = &mut inst.seed;
    let cps = inst.cps;
    let cp0 = cps[0];
    let mut p = Particle {
        pos: cp0.0,
        prev: cp0.0,
        vel: Vec3::ZERO,
        radius: def.radius,
        radius0: def.radius,
        roll: def.rotation,
        roll_speed: def.rotation_speed,
        yaw: 0.0,
        color: def.color,
        alpha: def.alpha,
        alpha0: def.alpha,
        life: 1.0,
        born: t,
        sequence: def.sequence,
        trail: 1.0,
        seed: (rng(seed) * u32::MAX as f32) as u32,
    };
    for op in &def.initializers {
        match op.func.as_str() {
            "lifetime random" => {
                let e = op.f("lifetime_random_exponent", 1.0);
                p.life = range_exp(
                    seed,
                    op.f("lifetime_min", 1.0),
                    op.f("lifetime_max", 1.0),
                    e,
                )
                .max(1e-3);
            }
            "radius random" => {
                let e = op.f("radius_random_exponent", 1.0);
                p.radius = range_exp(seed, op.f("radius_min", 1.0), op.f("radius_max", 1.0), e);
                p.radius0 = p.radius;
            }
            "alpha random" => {
                let a = range(seed, op.f("alpha_min", 255.0), op.f("alpha_max", 255.0)) / 255.0;
                p.alpha = a;
                p.alpha0 = a;
            }
            "color random" => {
                let c1 = color_of(op.c("color1", [255, 255, 255, 255]));
                let c2 = color_of(op.c("color2", [255, 255, 255, 255]));
                let f = rng(seed);
                p.color = [
                    c1[0] + (c2[0] - c1[0]) * f,
                    c1[1] + (c2[1] - c1[1]) * f,
                    c1[2] + (c2[2] - c1[2]) * f,
                ];
            }
            "rotation random" => {
                let e = op.f("rotation_random_exponent", 1.0);
                p.roll = op.f("rotation_initial", 0.0)
                    + range_exp(
                        seed,
                        op.f("rotation_offset_min", 0.0),
                        op.f("rotation_offset_max", 360.0),
                        e,
                    );
            }
            "rotation yaw random" => {
                p.yaw = op.f("yaw_initial", 0.0)
                    + range(
                        seed,
                        op.f("yaw_offset_min", 0.0),
                        op.f("yaw_offset_max", 360.0),
                    );
            }
            "rotation yaw flip random" => {
                if rng(seed) < op.f("flip percentage", 0.5) {
                    p.yaw += 180.0;
                }
            }
            "sequence random" => {
                let lo = op.i("sequence_min", 0);
                let hi = op.i("sequence_max", 0);
                p.sequence = lo + ((hi - lo + 1) as f32 * rng(seed)) as i32;
                p.sequence = p.sequence.min(hi);
            }
            "sequence two random" => {}
            "trail length random" => {
                p.trail = range(seed, op.f("length_min", 0.1), op.f("length_max", 0.1));
            }
            "position within sphere random" => {
                let cp = op.i("control_point_number", 0).clamp(0, MAX_CP as i32 - 1) as usize;
                let (c, fwd) = cps[cp];
                let dmin = op.f("distance_min", 0.0);
                let dmax = op.f("distance_max", 0.0);
                let bias = op.v("distance_bias", Vec3::ONE);
                let bias_abs = op.v("distance_bias_absolute_value", Vec3::ZERO);
                let smin = op.f("speed_min", 0.0);
                let smax = op.f("speed_max", 0.0);
                let lmin = op.v("speed_in_local_coordinate_system_min", Vec3::ZERO);
                let lmax = op.v("speed_in_local_coordinate_system_max", Vec3::ZERO);
                let random_exp = op.f("speed_random_exponent", 1.0);
                let mut dir = loop {
                    let d = Vec3::new(
                        range(seed, -1.0, 1.0),
                        range(seed, -1.0, 1.0),
                        range(seed, -1.0, 1.0),
                    );
                    let l = d.length_squared();
                    if l > 1e-4 && l <= 1.0 {
                        break d.normalize();
                    }
                };
                dir *= bias;
                if bias_abs.x != 0.0 {
                    dir.x = dir.x.abs();
                }
                if bias_abs.y != 0.0 {
                    dir.y = dir.y.abs();
                }
                if bias_abs.z != 0.0 {
                    dir.z = dir.z.abs();
                }
                let dist = range(seed, dmin, dmax);
                p.pos = c + dir * dist;
                let speed = range_exp(seed, smin, smax, random_exp);
                let local = Vec3::new(
                    range(seed, lmin.x, lmax.x),
                    range(seed, lmin.y, lmax.y),
                    range(seed, lmin.z, lmax.z),
                );
                let right = fwd.cross(Vec3::Z).normalize_or_zero();
                let up = right.cross(fwd).normalize_or_zero();
                p.vel += dir.normalize_or_zero() * speed
                    + fwd * local.x
                    + right * local.y
                    + up * local.z;
                p.prev = p.pos;
            }
            "position modify offset random" => {
                let lo = op.v("offset min", Vec3::ZERO);
                let hi = op.v("offset max", Vec3::ZERO);
                let local = op.b("offset in local space 0/1", false);
                let off = Vec3::new(
                    range(seed, lo.x, hi.x),
                    range(seed, lo.y, hi.y),
                    range(seed, lo.z, hi.z),
                );
                if local {
                    let fwd = cp0.1;
                    let right = fwd.cross(Vec3::Z).normalize_or_zero();
                    let up = right.cross(fwd).normalize_or_zero();
                    p.pos += fwd * off.x + right * off.y + up * off.z;
                } else {
                    p.pos += off;
                }
                p.prev = p.pos;
            }
            "position within box random" => {
                let lo = op.v("min", Vec3::ZERO);
                let hi = op.v("max", Vec3::ZERO);
                let cp = op.i("control point number", 0).clamp(0, MAX_CP as i32 - 1) as usize;
                p.pos = cps[cp].0
                    + Vec3::new(
                        range(seed, lo.x, hi.x),
                        range(seed, lo.y, hi.y),
                        range(seed, lo.z, hi.z),
                    );
                p.prev = p.pos;
            }
            "velocity noise" => {
                let lo = op.v("output minimum", Vec3::ZERO);
                let hi = op.v("output maximum", Vec3::ZERO);
                let local = op.b("apply velocity in local space (0/1)", false);
                let n = Vec3::new(
                    range(seed, lo.x, hi.x),
                    range(seed, lo.y, hi.y),
                    range(seed, lo.z, hi.z),
                );
                if local {
                    let fwd = cp0.1;
                    let right = fwd.cross(Vec3::Z).normalize_or_zero();
                    let up = right.cross(fwd).normalize_or_zero();
                    p.vel += fwd * n.x + right * n.y + up * n.z;
                } else {
                    p.vel += n;
                }
            }
            "lifetime from sequence" => {
                let fps = op.f("frames per second", 30.0);
                p.life = (10.0 / fps.max(1e-3)).max(1e-3);
            }
            "remap initial scalar" => {
                let inf = op.i("input field", 8);
                let outf = op.i("output field", 3);
                let imin = op.f("input minimum", 0.0);
                let imax = op.f("input maximum", 1.0);
                let omin = op.f("output minimum", 0.0);
                let omax = op.f("output maximum", 1.0);
                let scale = op.b("output is scalar of initial random range", false);
                let v = field_get(&p, inf);
                let f = ((v - imin) / (imax - imin).max(1e-6)).clamp(0.0, 1.0);
                let out = omin + (omax - omin) * f;
                let cur = field_get(&p, outf);
                field_set(&mut p, outf, if scale { cur * out } else { out });
                if outf == 3 {
                    p.radius0 = p.radius;
                }
                if outf == 7 {
                    p.alpha0 = p.alpha;
                }
            }
            _ => {}
        }
    }
    p
}
