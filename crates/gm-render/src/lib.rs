//! wgpu renderer. First target: a headless still of a BSP world with
//! lightmaps and base textures, so map rendering can be compared against
//! screenshots of the original game.

pub mod gpu;
pub mod lightmap;
pub mod material;
pub mod props;
pub mod world;

pub use gpu::Gpu;
pub use world::{Camera, WorldRenderer};
