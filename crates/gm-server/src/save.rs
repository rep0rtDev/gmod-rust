//! `.rdup`: the sandbox world (props and constraints) as JSON, our own duplicator / save format
//! (design: not the game's `.dupe`). Props are indexed by position in `props`; a constraint's
//! `a` / `b` of `None` is the world.

use crate::Server;
use glam::Vec3;
use gm_sandbox::{ConstraintKind, Device, LengthController, NumpadBind};

pub const EXTENSION: &str = "rdup";

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Save {
    pub format: u32,
    pub map: String,
    pub props: Vec<SavedProp>,
    pub constraints: Vec<SavedConstraint>,
    /// Powered stool entities on props (by prop index); absent in older files.
    #[serde(default)]
    pub devices: Vec<SavedDevice>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SavedDevice {
    pub prop: usize,
    pub kind: SavedDeviceKind,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SavedDeviceKind {
    Thruster {
        force: f32,
        mul: f32,
        toggle: bool,
        top_z: f32,
        bottom_z: f32,
        key: i32,
        key_back: i32,
    },
    Hoverball {
        target_z: f32,
        speed: f32,
        resistance: f32,
        strength: f32,
        enabled: bool,
        key_up: i32,
        key_down: i32,
        key_toggle: i32,
    },
    Balloon {
        force: f32,
    },
}

impl From<&Device> for SavedDeviceKind {
    fn from(d: &Device) -> Self {
        match d.clone() {
            Device::Thruster {
                force,
                mul,
                toggle,
                top_z,
                bottom_z,
                key,
                key_back,
                ..
            } => SavedDeviceKind::Thruster {
                force,
                mul,
                toggle,
                top_z,
                bottom_z,
                key,
                key_back,
            },
            Device::Hoverball {
                target_z,
                speed,
                resistance,
                strength,
                enabled,
                key_up,
                key_down,
                key_toggle,
                ..
            } => SavedDeviceKind::Hoverball {
                target_z,
                speed,
                resistance,
                strength,
                enabled,
                key_up,
                key_down,
                key_toggle,
            },
            Device::Balloon { force } => SavedDeviceKind::Balloon { force },
        }
    }
}

impl SavedDeviceKind {
    /// The device and the numpad bindings it needs on prop `id`.
    pub fn restore(&self, id: u32) -> (Device, Vec<(i32, NumpadBind)>) {
        match self.clone() {
            SavedDeviceKind::Thruster {
                force,
                mul,
                toggle,
                top_z,
                bottom_z,
                key,
                key_back,
            } => (
                Device::Thruster {
                    force,
                    mul,
                    toggle,
                    on: mul != 0.0,
                    top_z,
                    bottom_z,
                    key,
                    key_back,
                },
                vec![
                    (key, NumpadBind::Thruster(id, 1.0)),
                    (key_back, NumpadBind::Thruster(id, -1.0)),
                ],
            ),
            SavedDeviceKind::Hoverball {
                target_z,
                speed,
                resistance,
                strength,
                enabled,
                key_up,
                key_down,
                key_toggle,
            } => (
                Device::Hoverball {
                    target_z,
                    zvel: 0.0,
                    speed,
                    resistance,
                    strength,
                    enabled,
                    key_up,
                    key_down,
                    key_toggle,
                },
                vec![
                    (key_up, NumpadBind::HoverballMove(id, 1.0)),
                    (key_down, NumpadBind::HoverballMove(id, -1.0)),
                    (key_toggle, NumpadBind::HoverballToggle(id)),
                ],
            ),
            SavedDeviceKind::Balloon { force } => (Device::Balloon { force }, Vec::new()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SavedProp {
    pub model: String,
    pub pos: [f32; 3],
    pub angles: [f32; 3],
    pub frozen: bool,
    /// Skin family; absent in files written before skins existed.
    #[serde(default)]
    pub skin: u8,
    #[serde(default)]
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SavedConstraint {
    pub a: Option<usize>,
    pub b: Option<usize>,
    pub kind: SavedKind,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SavedKind {
    Weld {
        forcelimit: f32,
        nocollide: bool,
    },
    NoCollide,
    Axis {
        lpos1: [f32; 3],
        lpos2: [f32; 3],
        local_axis: [f32; 3],
        forcelimit: f32,
        torquelimit: f32,
        friction: f32,
        nocollide: bool,
    },
    Rope {
        lpos1: [f32; 3],
        lpos2: [f32; 3],
        length: f32,
        addlength: f32,
        width: f32,
        material: String,
        rigid: bool,
    },
    Ballsocket {
        lpos: [f32; 3],
        forcelimit: f32,
        torquelimit: f32,
        nocollide: bool,
    },
    Slider {
        lpos1: [f32; 3],
        lpos2: [f32; 3],
        width: f32,
        material: String,
        #[serde(default)]
        part_of: Option<u32>,
    },
    Hydraulic {
        lpos1: [f32; 3],
        lpos2: [f32; 3],
        length1: f32,
        length2: f32,
        width: f32,
        material: String,
        key: i32,
        fixed: bool,
        speed: f32,
        toggle: bool,
        constant: f32,
        damping: f32,
        controller: SavedController,
    },
    Winch {
        lpos1: [f32; 3],
        lpos2: [f32; 3],
        width: f32,
        material: String,
        fwd_key: i32,
        bwd_key: i32,
        fwd_speed: f32,
        bwd_speed: f32,
        toggle: bool,
        constant: f32,
        damping: f32,
        controller: SavedController,
    },
    Muscle {
        lpos1: [f32; 3],
        lpos2: [f32; 3],
        length1: f32,
        length2: f32,
        width: f32,
        material: String,
        key: i32,
        fixed: bool,
        period: f32,
        amplitude: f32,
        starton: bool,
        constant: f32,
        damping: f32,
        controller: SavedController,
    },
    Pulley {
        lpos1: [f32; 3],
        lpos4: [f32; 3],
        wpos2: [f32; 3],
        wpos3: [f32; 3],
        forcelimit: f32,
        rigid: bool,
        width: f32,
        material: String,
        addlength: f32,
    },
    Elastic {
        lpos1: [f32; 3],
        lpos2: [f32; 3],
        constant: f32,
        damping: f32,
        rdamping: f32,
        width: f32,
        material: String,
        length: f32,
        stretchonly: bool,
    },
}

/// A `gmod_winch_controller` mid-stroke.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SavedController {
    pub muscle: bool,
    pub length: f32,
    pub min: f32,
    pub max: Option<f32>,
    pub fwd_speed: f32,
    pub bwd_speed: f32,
    pub toggle: bool,
    pub direction: i8,
    pub expanded: bool,
    pub ctime: f32,
    pub period: f32,
    pub amplitude: f32,
}

impl SavedController {
    fn from(c: &LengthController) -> Self {
        SavedController {
            muscle: c.muscle,
            length: c.length,
            min: c.min,
            max: c.max,
            fwd_speed: c.fwd_speed,
            bwd_speed: c.bwd_speed,
            toggle: c.toggle,
            direction: c.direction,
            expanded: c.expanded,
            ctime: c.ctime,
            period: c.period,
            amplitude: c.amplitude,
        }
    }

    fn controller(&self) -> LengthController {
        LengthController {
            muscle: self.muscle,
            length: self.length,
            min: self.min,
            max: self.max,
            fwd_speed: self.fwd_speed,
            bwd_speed: self.bwd_speed,
            toggle: self.toggle,
            direction: self.direction,
            expanded: self.expanded,
            ctime: self.ctime,
            period: self.period,
            amplitude: self.amplitude,
        }
    }
}

fn arr(v: Vec3) -> [f32; 3] {
    v.to_array()
}

impl From<&ConstraintKind> for SavedKind {
    fn from(k: &ConstraintKind) -> Self {
        match k {
            ConstraintKind::Weld {
                forcelimit,
                nocollide,
            } => SavedKind::Weld {
                forcelimit: *forcelimit,
                nocollide: *nocollide,
            },
            ConstraintKind::NoCollide => SavedKind::NoCollide,
            ConstraintKind::Axis {
                lpos1,
                lpos2,
                local_axis,
                forcelimit,
                torquelimit,
                friction,
                nocollide,
            } => SavedKind::Axis {
                lpos1: arr(*lpos1),
                lpos2: arr(*lpos2),
                local_axis: arr(*local_axis),
                forcelimit: *forcelimit,
                torquelimit: *torquelimit,
                friction: *friction,
                nocollide: *nocollide,
            },
            ConstraintKind::Rope {
                lpos1,
                lpos2,
                length,
                addlength,
                width,
                material,
                rigid,
            } => SavedKind::Rope {
                lpos1: arr(*lpos1),
                lpos2: arr(*lpos2),
                length: *length,
                addlength: *addlength,
                width: *width,
                material: material.clone(),
                rigid: *rigid,
            },
            ConstraintKind::Ballsocket {
                lpos,
                forcelimit,
                torquelimit,
                nocollide,
            } => SavedKind::Ballsocket {
                lpos: arr(*lpos),
                forcelimit: *forcelimit,
                torquelimit: *torquelimit,
                nocollide: *nocollide,
            },
            ConstraintKind::Slider {
                lpos1,
                lpos2,
                width,
                material,
                part_of,
            } => SavedKind::Slider {
                lpos1: arr(*lpos1),
                lpos2: arr(*lpos2),
                width: *width,
                material: material.clone(),
                part_of: *part_of,
            },
            ConstraintKind::Hydraulic {
                lpos1,
                lpos2,
                length1,
                length2,
                width,
                material,
                key,
                fixed,
                speed,
                toggle,
                constant,
                damping,
                controller,
            } => SavedKind::Hydraulic {
                lpos1: arr(*lpos1),
                lpos2: arr(*lpos2),
                length1: *length1,
                length2: *length2,
                width: *width,
                material: material.clone(),
                key: *key,
                fixed: *fixed,
                speed: *speed,
                toggle: *toggle,
                constant: *constant,
                damping: *damping,
                controller: SavedController::from(controller),
            },
            ConstraintKind::Winch {
                lpos1,
                lpos2,
                width,
                material,
                fwd_key,
                bwd_key,
                fwd_speed,
                bwd_speed,
                toggle,
                constant,
                damping,
                controller,
            } => SavedKind::Winch {
                lpos1: arr(*lpos1),
                lpos2: arr(*lpos2),
                width: *width,
                material: material.clone(),
                fwd_key: *fwd_key,
                bwd_key: *bwd_key,
                fwd_speed: *fwd_speed,
                bwd_speed: *bwd_speed,
                toggle: *toggle,
                constant: *constant,
                damping: *damping,
                controller: SavedController::from(controller),
            },
            ConstraintKind::Muscle {
                lpos1,
                lpos2,
                length1,
                length2,
                width,
                material,
                key,
                fixed,
                period,
                amplitude,
                starton,
                constant,
                damping,
                controller,
            } => SavedKind::Muscle {
                lpos1: arr(*lpos1),
                lpos2: arr(*lpos2),
                length1: *length1,
                length2: *length2,
                width: *width,
                material: material.clone(),
                key: *key,
                fixed: *fixed,
                period: *period,
                amplitude: *amplitude,
                starton: *starton,
                constant: *constant,
                damping: *damping,
                controller: SavedController::from(controller),
            },
            ConstraintKind::Pulley {
                lpos1,
                lpos4,
                wpos2,
                wpos3,
                forcelimit,
                rigid,
                width,
                material,
                addlength,
            } => SavedKind::Pulley {
                lpos1: arr(*lpos1),
                lpos4: arr(*lpos4),
                wpos2: arr(*wpos2),
                wpos3: arr(*wpos3),
                forcelimit: *forcelimit,
                rigid: *rigid,
                width: *width,
                material: material.clone(),
                addlength: *addlength,
            },
            ConstraintKind::Elastic {
                lpos1,
                lpos2,
                constant,
                damping,
                rdamping,
                width,
                material,
                length,
                stretchonly,
            } => SavedKind::Elastic {
                lpos1: arr(*lpos1),
                lpos2: arr(*lpos2),
                constant: *constant,
                damping: *damping,
                rdamping: *rdamping,
                width: *width,
                material: material.clone(),
                length: *length,
                stretchonly: *stretchonly,
            },
        }
    }
}

impl From<&SavedKind> for ConstraintKind {
    fn from(k: &SavedKind) -> Self {
        match k {
            SavedKind::Weld {
                forcelimit,
                nocollide,
            } => ConstraintKind::Weld {
                forcelimit: *forcelimit,
                nocollide: *nocollide,
            },
            SavedKind::NoCollide => ConstraintKind::NoCollide,
            SavedKind::Axis {
                lpos1,
                lpos2,
                local_axis,
                forcelimit,
                torquelimit,
                friction,
                nocollide,
            } => ConstraintKind::Axis {
                lpos1: Vec3::from(*lpos1),
                lpos2: Vec3::from(*lpos2),
                local_axis: Vec3::from(*local_axis),
                forcelimit: *forcelimit,
                torquelimit: *torquelimit,
                friction: *friction,
                nocollide: *nocollide,
            },
            SavedKind::Rope {
                lpos1,
                lpos2,
                length,
                addlength,
                width,
                material,
                rigid,
            } => ConstraintKind::Rope {
                lpos1: Vec3::from(*lpos1),
                lpos2: Vec3::from(*lpos2),
                length: *length,
                addlength: *addlength,
                width: *width,
                material: material.clone(),
                rigid: *rigid,
            },
            SavedKind::Ballsocket {
                lpos,
                forcelimit,
                torquelimit,
                nocollide,
            } => ConstraintKind::Ballsocket {
                lpos: Vec3::from(*lpos),
                forcelimit: *forcelimit,
                torquelimit: *torquelimit,
                nocollide: *nocollide,
            },
            SavedKind::Slider {
                lpos1,
                lpos2,
                width,
                material,
                part_of,
            } => ConstraintKind::Slider {
                lpos1: Vec3::from(*lpos1),
                lpos2: Vec3::from(*lpos2),
                width: *width,
                material: material.clone(),
                part_of: *part_of,
            },
            SavedKind::Hydraulic {
                lpos1,
                lpos2,
                length1,
                length2,
                width,
                material,
                key,
                fixed,
                speed,
                toggle,
                constant,
                damping,
                controller,
            } => ConstraintKind::Hydraulic {
                lpos1: Vec3::from(*lpos1),
                lpos2: Vec3::from(*lpos2),
                length1: *length1,
                length2: *length2,
                width: *width,
                material: material.clone(),
                key: *key,
                fixed: *fixed,
                speed: *speed,
                toggle: *toggle,
                constant: *constant,
                damping: *damping,
                controller: controller.controller(),
            },
            SavedKind::Winch {
                lpos1,
                lpos2,
                width,
                material,
                fwd_key,
                bwd_key,
                fwd_speed,
                bwd_speed,
                toggle,
                constant,
                damping,
                controller,
            } => ConstraintKind::Winch {
                lpos1: Vec3::from(*lpos1),
                lpos2: Vec3::from(*lpos2),
                width: *width,
                material: material.clone(),
                fwd_key: *fwd_key,
                bwd_key: *bwd_key,
                fwd_speed: *fwd_speed,
                bwd_speed: *bwd_speed,
                toggle: *toggle,
                constant: *constant,
                damping: *damping,
                controller: controller.controller(),
            },
            SavedKind::Muscle {
                lpos1,
                lpos2,
                length1,
                length2,
                width,
                material,
                key,
                fixed,
                period,
                amplitude,
                starton,
                constant,
                damping,
                controller,
            } => ConstraintKind::Muscle {
                lpos1: Vec3::from(*lpos1),
                lpos2: Vec3::from(*lpos2),
                length1: *length1,
                length2: *length2,
                width: *width,
                material: material.clone(),
                key: *key,
                fixed: *fixed,
                period: *period,
                amplitude: *amplitude,
                starton: *starton,
                constant: *constant,
                damping: *damping,
                controller: controller.controller(),
            },
            SavedKind::Pulley {
                lpos1,
                lpos4,
                wpos2,
                wpos3,
                forcelimit,
                rigid,
                width,
                material,
                addlength,
            } => ConstraintKind::Pulley {
                lpos1: Vec3::from(*lpos1),
                lpos4: Vec3::from(*lpos4),
                wpos2: Vec3::from(*wpos2),
                wpos3: Vec3::from(*wpos3),
                forcelimit: *forcelimit,
                rigid: *rigid,
                width: *width,
                material: material.clone(),
                addlength: *addlength,
            },
            SavedKind::Elastic {
                lpos1,
                lpos2,
                constant,
                damping,
                rdamping,
                width,
                material,
                length,
                stretchonly,
            } => ConstraintKind::Elastic {
                lpos1: Vec3::from(*lpos1),
                lpos2: Vec3::from(*lpos2),
                constant: *constant,
                damping: *damping,
                rdamping: *rdamping,
                width: *width,
                material: material.clone(),
                length: *length,
                stretchonly: *stretchonly,
            },
        }
    }
}

impl Server<'_> {
    /// Every prop and constraint of the sandbox (ownership is not saved: a load gives them to
    /// the loading player, like a duplicator paste).
    pub fn save_world(&self, map: &str) -> Save {
        let props = self.sandbox.props();
        let index = |e: Option<u32>| e.and_then(|id| props.iter().position(|p| p.id == id));
        Save {
            format: 1,
            map: map.to_string(),
            props: props
                .iter()
                .map(|p| SavedProp {
                    model: p.model.clone(),
                    pos: arr(p.pos),
                    angles: arr(p.angles),
                    frozen: p.frozen,
                    skin: p.skin,
                    body: p.body.clone(),
                })
                .collect(),
            constraints: self
                .sandbox
                .constraints()
                .iter()
                .filter(|c| {
                    (c.ent1.is_none() || index(c.ent1).is_some())
                        && (c.ent2.is_none() || index(c.ent2).is_some())
                })
                .map(|c| SavedConstraint {
                    a: index(c.ent1),
                    b: index(c.ent2),
                    kind: SavedKind::from(&c.kind),
                })
                .collect(),
            devices: self
                .sandbox
                .devices()
                .iter()
                .filter_map(|(id, d)| {
                    Some(SavedDevice {
                        prop: index(Some(*id))?,
                        kind: SavedDeviceKind::from(d),
                    })
                })
                .collect(),
        }
    }

    /// Adds the saved props and constraints to the world as `owner`'s; bodies and joints are
    /// built by the physics sync. Returns (props, constraints) added; a prop whose model does
    /// not load is skipped with its constraints.
    pub fn load_world(&mut self, save: &Save, owner: u32) -> (usize, usize) {
        let mut ids: Vec<Option<u32>> = Vec::with_capacity(save.props.len());
        for p in &save.props {
            if !self.models.contains_key(&p.model) {
                match self.model_loader.as_mut().and_then(|f| f(&p.model)) {
                    Some(info) => self.register_model(&p.model, info),
                    None => {
                        ids.push(None);
                        continue;
                    }
                }
            }
            let id = self.sandbox.restore_prop(
                owner,
                &p.model,
                Vec3::from(p.pos),
                Vec3::from(p.angles),
                p.frozen,
            );
            self.sandbox.set_skin(id, p.skin);
            self.sandbox.set_body(id, p.body.clone());
            ids.push(Some(id));
        }
        let mut constraints = 0;
        for c in &save.constraints {
            let ent = |i: Option<usize>| match i {
                None => Some(None),
                Some(i) => ids.get(i).copied().flatten().map(Some),
            };
            let (Some(a), Some(b)) = (ent(c.a), ent(c.b)) else {
                continue;
            };
            self.sandbox
                .restore_constraint(owner, a, b, ConstraintKind::from(&c.kind));
            constraints += 1;
        }
        for d in &save.devices {
            let Some(Some(id)) = ids.get(d.prop).copied() else {
                continue;
            };
            let (device, binds) = d.kind.restore(id);
            self.sandbox.set_gravity(id, device.gravity());
            self.sandbox.restore_device(id, device, &binds);
        }
        self.sync_after_script();
        (ids.iter().filter(|i| i.is_some()).count(), constraints)
    }
}
