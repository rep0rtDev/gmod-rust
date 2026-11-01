//! Acoustics off the audio thread: the map as a Steam Audio scene, a simulator that
//! ray-casts every live voice against it (volumetric occlusion, frequency-dependent
//! transmission through the hit materials, air absorption over the distance) and hands the
//! results back as a [`Path`] per voice, plus a listener-centric reverb (parametric
//! reflections traced from the listener's position: decay time per band and an EQ, the
//! engine renders it over the wet sum of the voices). The audio thread publishes where the
//! voices and the listener are ([`Snapshot`]) and picks up the results with `try_lock`,
//! never blocking.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use audionimbus::effect::{Parametric, ReflectionEffectParams, Transmission};
use audionimbus::geometry::{CoordinateSystem, Scene, StaticMesh, StaticMeshSettings};
use audionimbus::model::AirAbsorptionModel;
use audionimbus::simulation::{
    Direct, DirectSimulationParameters, DirectSimulationSettings, Occlusion, OcclusionAlgorithm,
    ParametricParameters, ParametricSettings, Reflections, ReflectionsSharedInputs,
    SimulationInputs, SimulationParameters, SimulationSettings, SimulationSharedInputs, Simulator,
    Source, TransmissionParameters,
};
use audionimbus::{AudioSettings, Context};
use glam::Vec3;

use crate::engine::{dir_sa, to_sa, Path, FRAME};
use crate::mixer::Listener;
use crate::scene::Geometry;

/// Where the voices and the listener are, as of the last rendered frame.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub listener: Option<Listener>,
    pub voices: Vec<(u32, Vec3)>,
}

pub type Paths = HashMap<u32, Path>;

/// The mailboxes between the audio thread and the simulation thread.
#[derive(Default)]
pub struct Shared {
    pub snapshot: Mutex<Snapshot>,
    pub paths: Mutex<Paths>,
    /// The latest reverb at the listener; the engine takes it and keeps it until the next.
    pub reverb: Mutex<Option<ReflectionEffectParams<Parametric>>>,
    pub stop: AtomicBool,
}

/// Source size for volumetric occlusion (meters): a prop's worth, so a post does not mute
/// a crate behind it the way a single ray would.
pub const SOURCE_RADIUS: f32 = 0.4;
pub const OCCLUSION_SAMPLES: u32 = 16;
pub const TRANSMISSION_RAYS: u32 = 4;
/// Simulation rate.
pub const INTERVAL: Duration = Duration::from_millis(33);
/// Reverb: rays from the listener, bounces, longest tail (seconds), ambisonics order.
pub const REVERB_RAYS: u32 = 2048;
pub const REVERB_BOUNCES: u32 = 8;
pub const REVERB_DURATION: f32 = 1.5;
pub const REVERB_ORDER: u32 = 1;
/// The reverb is traced every this many direct passes (it costs most of the thread).
pub const REVERB_EVERY: u32 = 3;

fn frame(listener: &Listener) -> CoordinateSystem {
    CoordinateSystem {
        right: dir_sa(listener.right),
        up: dir_sa(listener.up),
        ahead: dir_sa(listener.ahead),
        origin: to_sa(listener.position),
    }
}

type Sim =
    Simulator<audionimbus::ray_tracing::DefaultRayTracer, Direct, Reflections, (), Parametric>;

pub struct Acoustics {
    simulator: Sim,
    _scene: Scene,
    sources: HashMap<u32, Source<Direct>>,
    reverb: Source<(), Reflections, (), Parametric>,
    passes: u32,
    pub triangles: usize,
}

impl Acoustics {
    /// Build the scene from `geometry` and a direct simulator over it.
    pub fn new(context: &Context, rate: u32, geometry: &Geometry) -> anyhow::Result<Acoustics> {
        let audio = AudioSettings {
            sampling_rate: rate,
            frame_size: FRAME as u32,
        };
        let settings = SimulationSettings::new(&audio)
            .with_direct(DirectSimulationSettings {
                max_num_occlusion_samples: OCCLUSION_SAMPLES,
            })
            .with_reflections(ParametricSettings {
                max_num_rays: REVERB_RAYS,
                num_diffuse_samples: 32,
                max_duration: REVERB_DURATION,
                max_num_sources: 1,
                num_threads: 1,
                max_order: REVERB_ORDER,
            });
        let mut simulator = Simulator::try_new(context, &settings)
            .map_err(|e| anyhow::anyhow!("steam audio simulator: {e}"))?;
        let mut scene =
            Scene::try_new(context).map_err(|e| anyhow::anyhow!("steam audio scene: {e}"))?;
        let triangles = geometry.triangles.len();
        if triangles > 0 {
            let vertices: Vec<_> = geometry.vertices.iter().map(|&v| to_sa(v)).collect();
            let tris: Vec<_> = geometry
                .triangles
                .iter()
                .map(|t| audionimbus::geometry::Triangle {
                    indices: [t[0] as i32, t[1] as i32, t[2] as i32],
                })
                .collect();
            let material_indices: Vec<usize> =
                geometry.material_of.iter().map(|&m| m as usize).collect();
            let mesh = StaticMesh::try_new(
                &scene,
                &StaticMeshSettings {
                    vertices: &vertices,
                    triangles: &tris,
                    material_indices: &material_indices,
                    materials: &geometry.materials,
                },
            )
            .map_err(|e| anyhow::anyhow!("steam audio static mesh: {e}"))?;
            scene.add_static_mesh(mesh);
            scene.commit();
        }
        simulator.set_scene(&scene);
        let reverb = Source::try_new_subset(&simulator)
            .map_err(|e| anyhow::anyhow!("steam audio reverb source: {e}"))?;
        simulator.add_source(&reverb);
        simulator.commit();
        Ok(Acoustics {
            simulator,
            _scene: scene,
            sources: HashMap::new(),
            reverb,
            passes: 0,
            triangles,
        })
    }

    /// One simulation pass over the voices in `snap` (voices not seen are dropped) and,
    /// every [`REVERB_EVERY`] passes, the reverb at the listener.
    pub fn step(
        &mut self,
        snap: &Snapshot,
    ) -> anyhow::Result<(Paths, Option<ReflectionEffectParams<Parametric>>)> {
        let Some(listener) = snap.listener else {
            return Ok((Paths::new(), None));
        };
        let listener_frame = frame(&listener);
        let live: std::collections::HashSet<u32> = snap.voices.iter().map(|v| v.0).collect();
        let dead: Vec<u32> = self
            .sources
            .keys()
            .filter(|id| !live.contains(id))
            .copied()
            .collect();
        for id in dead {
            if let Some(s) = self.sources.remove(&id) {
                self.simulator.remove_source(&s);
            }
        }
        for &(id, position) in &snap.voices {
            if !self.sources.contains_key(&id) {
                let s = Source::try_new_subset(&self.simulator)
                    .map_err(|e| anyhow::anyhow!("steam audio source: {e}"))?;
                self.simulator.add_source(&s);
                self.sources.insert(id, s);
            }
            let s = &self.sources[&id];
            let inputs = SimulationInputs {
                source: CoordinateSystem {
                    origin: to_sa(position),
                    ..listener_frame
                },
                parameters: SimulationParameters::new().with_direct(
                    DirectSimulationParameters::new()
                        .with_air_absorption(AirAbsorptionModel::default())
                        .with_occlusion(
                            Occlusion::new(OcclusionAlgorithm::Volumetric {
                                radius: SOURCE_RADIUS,
                                num_occlusion_samples: OCCLUSION_SAMPLES,
                            })
                            .with_transmission(
                                TransmissionParameters {
                                    num_transmission_rays: TRANSMISSION_RAYS,
                                },
                            ),
                        ),
                ),
            };
            s.set_direct_inputs(&inputs)
                .map_err(|e| anyhow::anyhow!("steam audio source inputs: {e}"))?;
        }
        self.reverb
            .set_reflections_inputs(&SimulationInputs {
                source: listener_frame,
                parameters: SimulationParameters::new().with_reflections(ParametricParameters {
                    reverb_scale: [1.0; 3],
                    baked_data_identifier: None,
                }),
            })
            .map_err(|e| anyhow::anyhow!("steam audio reverb inputs: {e}"))?;
        self.simulator
            .set_shared_inputs(
                &SimulationSharedInputs::new(listener_frame).with_reflections(
                    ReflectionsSharedInputs {
                        num_rays: REVERB_RAYS,
                        num_bounces: REVERB_BOUNCES,
                        duration: REVERB_DURATION,
                        order: REVERB_ORDER,
                        irradiance_min_distance: 1.0,
                    },
                ),
            )
            .map_err(|e| anyhow::anyhow!("steam audio shared inputs: {e}"))?;
        self.simulator.commit();
        self.simulator.run_direct();
        let reverb = if self.passes.is_multiple_of(REVERB_EVERY) && self.triangles > 0 {
            self.simulator
                .run_reflections()
                .map_err(|e| anyhow::anyhow!("steam audio reflections: {e}"))?;
            self.reverb.get_reflections_outputs().ok()
        } else {
            None
        };
        self.passes = self.passes.wrapping_add(1);
        let mut paths = Paths::with_capacity(self.sources.len());
        for (&id, s) in &self.sources {
            let Ok(out) = s.get_direct_outputs() else {
                continue;
            };
            let mut path = Path::default();
            if let Some(o) = out.occlusion {
                path.occlusion = o.clamp(0.0, 1.0);
            }
            if let Some(t) = out.transmission {
                let eq = match t {
                    Transmission::FrequencyIndependent(e) | Transmission::FrequencyDependent(e) => {
                        e
                    }
                };
                path.transmission = eq.0;
            }
            if let Some(a) = out.air_absorption {
                path.air_absorption = a.0;
            }
            paths.insert(id, path);
        }
        Ok((paths, reverb))
    }
}

/// Run the simulation in its own thread until `shared.stop`.
pub fn spawn(
    context: Context,
    rate: u32,
    geometry: Geometry,
    shared: Arc<Shared>,
) -> anyhow::Result<std::thread::JoinHandle<()>> {
    let mut acoustics = Acoustics::new(&context, rate, &geometry)?;
    Ok(std::thread::Builder::new()
        .name("gm-acoustics".into())
        .spawn(move || {
            while !shared.stop.load(Ordering::Relaxed) {
                let snap = shared
                    .snapshot
                    .lock()
                    .map(|s| s.clone())
                    .unwrap_or_default();
                match acoustics.step(&snap) {
                    Ok((paths, reverb)) => {
                        if let Ok(mut p) = shared.paths.lock() {
                            *p = paths;
                        }
                        if reverb.is_some() {
                            if let Ok(mut r) = shared.reverb.lock() {
                                *r = reverb;
                            }
                        }
                    }
                    Err(e) => eprintln!("acoustics: {e}"),
                }
                std::thread::sleep(INTERVAL);
            }
        })?)
}
