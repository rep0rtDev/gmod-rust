//! The launcher's non-UI half: where Garry's Mod is, which servers the master knows, and
//! whether a release is the one we signed.

pub mod minisign;
pub mod release;
pub mod servers;

pub use gm_content::install::{find_install, steam_roots, Install};
