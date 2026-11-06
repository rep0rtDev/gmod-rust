//! Combat behaviour of the NPC classes the schedule brain has no body for: the floor turret
//! (deploy, fire, retire), the barnacle (tongue, pull, bite), the rollermine (roll at the
//! player, shock), the vortigaunt (zap at hostile NPCs), the stalker (beam) and the birds
//! (take off and flee). Built from the HL2 behaviour, not measured against srcds.

use crate::{npc_hostile_to_players, weapons, Server};
use glam::Vec3;

/// Per-NPC scratch state of these behaviours.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NpcSpecial {
    /// The player (or NPC, for the vortigaunt) the behaviour is on.
    pub target: Option<u32>,
    pub phase: u8,
    /// Tick the current phase ends.
    pub until: u64,
    /// Tick of the next shot / bite / shock.
    pub next_hit: u64,
    /// The barnacle's floor point, a bird's take-off point.
    pub anchor: Option<Vec3>,
    /// A bird's flight direction.
    pub dir: Vec3,
}

/// Classes whose body never walks: the schedule brain leaves them alone.
pub fn npc_stationary_class(class: &str) -> bool {
    matches!(
        class,
        "npc_turret_floor" | "npc_barnacle" | "npc_rollermine"
    )
}

const TICK: f32 = gm_core::tick::DEFAULT_TICK_INTERVAL;

fn ticks(seconds: f32) -> u64 {
    (seconds / TICK).round().max(1.0) as u64
}

/// `sk_npc_dmg_pistol`: the floor turret fires pistol rounds.
const TURRET_DMG: f32 = 3.0;
const TURRET_RANGE: f32 = 1200.0;
const TURRET_MUZZLE: f32 = 40.0;
/// Shots every 0.1 s, deploy 1 s, retire after 10 s without the enemy.
const TURRET_SHOT: f32 = 0.1;
const TURRET_DEPLOY: f32 = 1.0;
const TURRET_RETIRE: f32 = 10.0;

const BARNACLE_RADIUS: f32 = 20.0;
const BARNACLE_PULL: f32 = 80.0;
const BARNACLE_BITE: f32 = 15.0;
const BARNACLE_MOUTH: f32 = 24.0;

const ROLLER_RANGE: f32 = 1500.0;
const ROLLER_SPEED: f32 = 300.0;
const ROLLER_RADIUS: f32 = 20.0;
const ROLLER_SHOCK: f32 = 10.0;
const ROLLER_SHOCK_REACH: f32 = 48.0;

const VORT_RANGE: f32 = 1000.0;
const VORT_ZAP: f32 = 50.0;
const VORT_CHARGE: f32 = 1.5;
const VORT_COOLDOWN: f32 = 3.0;

const STALKER_RANGE: f32 = 1200.0;
const STALKER_DMG: f32 = 2.0;
const STALKER_BEAM: f32 = 0.1;

const BIRD_SCARE: f32 = 200.0;
const BIRD_SPEED: f32 = 200.0;
const BIRD_CLIMB: f32 = 100.0;
const BIRD_FLEE: f32 = 600.0;
const BIRD_RISE: f32 = 150.0;
const BIRD_SOAR: f32 = 3.0;

fn bird_class(class: &str) -> bool {
    matches!(class, "npc_crow" | "npc_seagull" | "npc_pigeon")
}

impl Server<'_> {
    /// The nearest living player within `range` of `from` with a line of sight to `eye`.
    fn special_nearest_player(
        &self,
        from: Vec3,
        eye: Vec3,
        range: f32,
        skip: Option<u32>,
    ) -> Option<u32> {
        self.players
            .iter()
            .filter(|(_, p)| p.alive && !p.no_target)
            .filter(|(_, p)| p.state.origin.distance(from) <= range)
            .filter(|(id, _)| {
                self.eye(**id)
                    .is_some_and(|(e, _)| self.npc_los_from(eye, e, skip))
            })
            .min_by(|a, b| {
                a.1.state
                    .origin
                    .distance(from)
                    .total_cmp(&b.1.state.origin.distance(from))
            })
            .map(|(id, _)| *id)
    }

    /// Advances every special behaviour one tick (after the players' moves).
    pub(crate) fn tick_npc_special(&mut self) {
        let now = self.tick_count;
        let ids: Vec<u32> = self
            .npcs
            .iter()
            .filter(|(_, n)| {
                npc_stationary_class(&n.class)
                    || bird_class(&n.class)
                    || matches!(n.class.as_str(), "npc_vortigaunt" | "npc_stalker")
            })
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            let Some(n) = self.npcs.get(&id) else {
                continue;
            };
            if n.dead_at.is_some() {
                if n.class == "npc_barnacle" {
                    self.barnacle_release(id);
                }
                continue;
            }
            match n.class.as_str() {
                "npc_turret_floor" => self.tick_turret(id, now),
                "npc_barnacle" => self.tick_barnacle(id, now),
                "npc_rollermine" => self.tick_rollermine(id, now),
                "npc_vortigaunt" => self.tick_vortigaunt(id, now),
                "npc_stalker" => self.tick_stalker(id, now),
                c if bird_class(c) => self.tick_bird(id, now),
                _ => {}
            }
        }
    }

    fn tick_turret(&mut self, id: u32, now: u64) {
        let Some(n) = self.npcs.get(&id) else {
            return;
        };
        if !npc_hostile_to_players(n) {
            return;
        }
        let pos = n.pos;
        let muzzle = pos + Vec3::new(0.0, 0.0, TURRET_MUZZLE);
        let phase = n.special.phase;
        let seen = self.special_nearest_player(pos, muzzle, TURRET_RANGE, n.body);
        match phase {
            // closed: a player in sight deploys
            0 => {
                if let Some(p) = seen {
                    self.set_sequence(id, "deploy");
                    if let Some(n) = self.npcs.get_mut(&id) {
                        n.special.phase = 1;
                        n.special.until = now + ticks(TURRET_DEPLOY);
                        n.special.target = Some(p);
                    }
                }
            }
            1 => {
                if now >= self.npcs[&id].special.until {
                    self.set_sequence(id, "idlealert");
                    if let Some(n) = self.npcs.get_mut(&id) {
                        n.special.phase = 2;
                        n.special.next_hit = now;
                        n.special.until = now + ticks(TURRET_RETIRE);
                    }
                }
            }
            2 => {
                if let Some(p) = seen {
                    let (eye, _) = self.eye(p).unwrap_or((pos, Vec3::ZERO));
                    let aim = eye - Vec3::new(0.0, 0.0, 24.0);
                    let dir = (aim - muzzle).normalize_or_zero();
                    if let Some(n) = self.npcs.get_mut(&id) {
                        n.special.target = Some(p);
                        n.special.until = now + ticks(TURRET_RETIRE);
                        n.angles.y = dir.y.atan2(dir.x).to_degrees();
                    }
                    if now >= self.npcs[&id].special.next_hit {
                        // a 3 degree cone, deterministic on the tick
                        let t = now as f32;
                        let spread =
                            Vec3::new((t * 0.731).sin(), (t * 1.173).cos(), (t * 0.517).sin())
                                * 0.026;
                        // the shot starts outside the turret's own body
                        let from = muzzle + dir * 32.0;
                        self.npc_fire_bullet(id, from, (dir + spread).normalize(), TURRET_DMG);
                        self.set_sequence(id, "fire");
                        if let Some(n) = self.npcs.get_mut(&id) {
                            n.special.next_hit = now + ticks(TURRET_SHOT);
                        }
                    }
                } else if now >= self.npcs[&id].special.until {
                    self.set_sequence(id, "retract");
                    if let Some(n) = self.npcs.get_mut(&id) {
                        n.special.phase = 3;
                        n.special.until = now + ticks(TURRET_DEPLOY);
                        n.special.target = None;
                    }
                } else if self.npcs[&id].sequence != "idlealert" {
                    self.set_sequence(id, "idlealert");
                }
            }
            _ => {
                if now >= self.npcs[&id].special.until {
                    self.set_sequence(id, "idle");
                    if let Some(n) = self.npcs.get_mut(&id) {
                        n.special.phase = 0;
                    }
                }
            }
        }
    }

    fn barnacle_release(&mut self, id: u32) {
        let Some(n) = self.npcs.get_mut(&id) else {
            return;
        };
        if let Some(p) = n.special.target.take() {
            n.special.phase = 0;
            if let Some(pl) = self.players.get_mut(&p) {
                pl.state.phys_lift = false;
            }
            if self.npcs.get(&id).is_some_and(|n| n.dead_at.is_none()) {
                self.set_sequence(id, "idle01");
            }
        }
    }

    fn tick_barnacle(&mut self, id: u32, now: u64) {
        let Some(n) = self.npcs.get(&id) else {
            return;
        };
        if !npc_hostile_to_players(n) {
            return;
        }
        let (pos, anchor, target) = (n.pos, n.special.anchor, n.special.target);
        let floor = match anchor {
            Some(f) => f,
            None => {
                let t = self.collision.trace_ray(
                    pos,
                    pos - Vec3::new(0.0, 0.0, 4096.0),
                    gm_collision::MASK_PLAYERSOLID,
                );
                let f = t.end;
                if let Some(n) = self.npcs.get_mut(&id) {
                    n.special.anchor = Some(f);
                }
                f
            }
        };
        match target {
            None => {
                // a player under the tongue
                let caught = self
                    .players
                    .iter()
                    .filter(|(_, p)| p.alive && !p.no_target)
                    .find(|(_, p)| {
                        let o = p.state.origin;
                        o.truncate().distance(pos.truncate()) <= BARNACLE_RADIUS
                            && o.z >= floor.z - 8.0
                            && o.z + 72.0 <= pos.z
                    })
                    .map(|(id, _)| *id);
                if let Some(p) = caught {
                    self.set_sequence(id, "tongue_wrap");
                    if let Some(n) = self.npcs.get_mut(&id) {
                        n.special.target = Some(p);
                        n.special.phase = 1;
                        n.special.next_hit = now + ticks(1.0);
                    }
                }
            }
            Some(p) => {
                let alive = self.players.get(&p).is_some_and(|pl| pl.alive);
                if !alive {
                    self.barnacle_release(id);
                    return;
                }
                let mouth = pos.z - BARNACLE_MOUTH - 72.0;
                let at_mouth = self.players[&p].state.origin.z >= mouth - 1.0;
                if let Some(pl) = self.players.get_mut(&p) {
                    let o = pl.state.origin;
                    let xy = o.truncate().lerp(pos.truncate(), 0.2);
                    let z = (o.z + BARNACLE_PULL * TICK).min(mouth);
                    pl.state.origin = Vec3::new(xy.x, xy.y, z);
                    pl.state.velocity = Vec3::ZERO;
                    pl.state.on_ground = false;
                    pl.state.phys_lift = true;
                }
                if at_mouth {
                    if self.npcs[&id].special.phase == 1 {
                        self.set_sequence(id, "attack_player");
                        if let Some(n) = self.npcs.get_mut(&id) {
                            n.special.phase = 2;
                        }
                    }
                    if now >= self.npcs[&id].special.next_hit {
                        self.damage_player(p, BARNACLE_BITE, weapons::DMG_SLASH);
                        if let Some(n) = self.npcs.get_mut(&id) {
                            n.special.next_hit = now + ticks(1.0);
                        }
                    }
                }
            }
        }
    }

    fn tick_rollermine(&mut self, id: u32, now: u64) {
        let Some(n) = self.npcs.get(&id) else {
            return;
        };
        if !npc_hostile_to_players(n) {
            return;
        }
        let Some(&body) = n.body.and_then(|prop| self.bodies.get(&prop)) else {
            return;
        };
        let st = self.physics.state(body);
        let centre = st.position;
        let Some(p) =
            self.special_nearest_player(centre, centre + Vec3::Z * 16.0, ROLLER_RANGE, n.body)
        else {
            return;
        };
        let target = self.players[&p].state.origin + Vec3::Z * 36.0;
        let to = (target - centre).truncate();
        let dir = to.normalize_or_zero().extend(0.0);
        let dist = (target - centre).length();
        if dist > ROLLER_SHOCK_REACH {
            let v = dir * ROLLER_SPEED;
            let angular = Vec3::Z.cross(v) / ROLLER_RADIUS;
            self.physics
                .set_velocity(body, Vec3::new(v.x, v.y, st.velocity.z), angular);
        } else if now >= n.special.next_hit {
            if self.damage_player(p, ROLLER_SHOCK, weapons::DMG_SHOCK) > 0.0 {
                if let Some(pl) = self.players.get_mut(&p) {
                    pl.state.velocity += dir * 250.0 + Vec3::Z * 150.0;
                    pl.state.on_ground = false;
                }
            }
            self.physics
                .set_velocity(body, -dir * 150.0 + Vec3::Z * 100.0, Vec3::ZERO);
            if let Some(n) = self.npcs.get_mut(&id) {
                n.special.next_hit = now + ticks(1.0);
            }
        }
        if let Some(n) = self.npcs.get_mut(&id) {
            n.special.target = Some(p);
        }
    }

    fn tick_vortigaunt(&mut self, id: u32, now: u64) {
        let Some(n) = self.npcs.get(&id) else {
            return;
        };
        if npc_hostile_to_players(n) || n.ai.scripted {
            return;
        }
        let pos = n.pos;
        let eye = pos + Vec3::Z * 64.0;
        let phase = n.special.phase;
        if phase == 1 {
            if now >= n.special.until {
                if let Some(t) = n.special.target {
                    let hit = self
                        .npcs
                        .get(&t)
                        .is_some_and(|e| e.dead_at.is_none() && e.pos.distance(pos) <= VORT_RANGE);
                    if hit {
                        self.damage_npc_typed(t, id, VORT_ZAP, 0, false);
                    }
                }
                if let Some(n) = self.npcs.get_mut(&id) {
                    n.special.phase = 0;
                    n.special.next_hit = now + ticks(VORT_COOLDOWN);
                    if let Some(rest) = n.rest_sequence.clone() {
                        n.sequence = rest;
                    }
                }
            }
            return;
        }
        if now < n.special.next_hit || n.goal.is_some() {
            return;
        }
        let enemy = self
            .npcs
            .iter()
            .filter(|(eid, e)| **eid != id && e.dead_at.is_none() && npc_hostile_to_players(e))
            .filter(|(_, e)| e.pos.distance(pos) <= VORT_RANGE)
            .filter(|(_, e)| self.npc_los(eye, e.pos + Vec3::Z * 36.0))
            .min_by(|a, b| a.1.pos.distance(pos).total_cmp(&b.1.pos.distance(pos)))
            .map(|(eid, e)| (*eid, e.pos));
        if let Some((eid, epos)) = enemy {
            self.set_sequence(id, "zapattack1");
            if let Some(n) = self.npcs.get_mut(&id) {
                let d = epos - pos;
                n.angles.y = d.y.atan2(d.x).to_degrees();
                n.special.target = Some(eid);
                n.special.phase = 1;
                n.special.until = now + ticks(VORT_CHARGE);
            }
        }
    }

    fn tick_stalker(&mut self, id: u32, now: u64) {
        let Some(n) = self.npcs.get(&id) else {
            return;
        };
        if !npc_hostile_to_players(n) {
            return;
        }
        let pos = n.pos;
        let eye = pos + Vec3::Z * 60.0;
        let Some(p) = self.special_nearest_player(pos, eye, STALKER_RANGE, None) else {
            if let Some(n) = self.npcs.get_mut(&id) {
                n.special.target = None;
            }
            return;
        };
        let target = self.players[&p].state.origin;
        if let Some(n) = self.npcs.get_mut(&id) {
            let d = target - pos;
            n.angles.y = d.y.atan2(d.x).to_degrees();
            n.special.target = Some(p);
        }
        if now >= self.npcs[&id].special.next_hit {
            self.damage_player(p, STALKER_DMG, crate::DMG_BURN);
            if let Some(n) = self.npcs.get_mut(&id) {
                n.special.next_hit = now + ticks(STALKER_BEAM);
            }
        }
    }

    fn tick_bird(&mut self, id: u32, now: u64) {
        let Some(n) = self.npcs.get(&id) else {
            return;
        };
        let pos = n.pos;
        match n.special.phase {
            0 => {
                let scare = self
                    .players
                    .iter()
                    .filter(|(_, p)| p.alive)
                    .map(|(_, p)| p.state.origin)
                    .filter(|o| o.distance(pos) <= BIRD_SCARE)
                    .min_by(|a, b| a.distance(pos).total_cmp(&b.distance(pos)));
                if let Some(from) = scare {
                    let away = (pos - from).truncate().normalize_or_zero();
                    let away = if away.length_squared() < 0.5 {
                        glam::Vec2::X
                    } else {
                        away
                    };
                    self.set_sequence(id, "Takeoff");
                    if let Some(n) = self.npcs.get_mut(&id) {
                        n.special.phase = 1;
                        n.special.until = now + ticks(0.5);
                        n.special.anchor = Some(pos);
                        n.special.dir = away.extend(0.0);
                        n.angles.y = away.y.atan2(away.x).to_degrees();
                    }
                }
            }
            1 => {
                if now >= n.special.until {
                    self.set_sequence(id, "Fly01");
                    if let Some(n) = self.npcs.get_mut(&id) {
                        n.special.phase = 2;
                    }
                }
            }
            2 => {
                let home = n.special.anchor.unwrap_or(pos);
                let step = n.special.dir * BIRD_SPEED * TICK + Vec3::Z * BIRD_CLIMB * TICK;
                let next = pos + step;
                let lift = Vec3::Z * 8.0;
                let tr = self.collision.trace_ray(
                    pos + lift,
                    next + lift + step.normalize_or_zero() * 16.0,
                    gm_collision::MASK_PLAYERSOLID,
                );
                let blocked = !tr.start_solid && tr.fraction < 1.0;
                let far = next.truncate().distance(home.truncate()) >= BIRD_FLEE
                    || next.z - home.z >= BIRD_RISE;
                let mut soar = far;
                if let Some(n) = self.npcs.get_mut(&id) {
                    if !blocked {
                        n.pos = next;
                    } else if n.special.next_hit < 4 {
                        // a wall in the way: turn a quarter and keep going
                        n.special.next_hit += 1;
                        let d = n.special.dir;
                        n.special.dir = Vec3::new(-d.y, d.x, 0.0);
                        n.angles.y = n.special.dir.y.atan2(n.special.dir.x).to_degrees();
                    } else {
                        soar = true;
                    }
                    if soar {
                        n.special.phase = 3;
                        n.special.until = now + ticks(BIRD_SOAR);
                        n.special.next_hit = 0;
                    }
                }
                if soar {
                    self.set_sequence(id, "Soar");
                }
            }
            3 => {
                if now >= n.special.until {
                    self.set_sequence(id, "Land");
                    if let Some(n) = self.npcs.get_mut(&id) {
                        n.special.phase = 4;
                    }
                }
            }
            _ => {
                let down = self.collision.trace_ray(
                    pos,
                    pos - Vec3::Z * (BIRD_CLIMB * TICK + 2.0),
                    gm_collision::MASK_PLAYERSOLID,
                );
                if down.fraction < 1.0 {
                    self.set_sequence(id, "Idle01");
                    if let Some(n) = self.npcs.get_mut(&id) {
                        n.pos = down.end + Vec3::Z * 0.5;
                        n.special.phase = 0;
                        n.special.anchor = None;
                    }
                } else if let Some(n) = self.npcs.get_mut(&id) {
                    n.pos = pos - Vec3::Z * BIRD_CLIMB * TICK;
                }
            }
        }
    }
}
