//! Locating the Garry's Mod install through Steam's library manifests.

use std::path::{Path, PathBuf};

use gm_formats::keyvalues;

use crate::{ContentError, Result};

pub const APP_ID: &str = "4000";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Install {
    /// Directory containing `garrysmod/`, `sourceengine/`, `bin/`.
    pub game_dir: PathBuf,
    /// `buildid` from `appmanifest_4000.acf`, if the manifest was found.
    pub build_id: Option<String>,
    /// Beta branch name from the manifest (`x86-64` or `public` for none).
    pub branch: Option<String>,
}

impl Install {
    /// Accept an explicit game directory, checking the files the launcher needs.
    pub fn at(game_dir: impl AsRef<Path>) -> Result<Self> {
        let game_dir = game_dir.as_ref().to_path_buf();
        if !game_dir.join("garrysmod/gameinfo.txt").is_file()
            || !game_dir.join("garrysmod/garrysmod_dir.vpk").is_file()
        {
            return Err(ContentError::NotAnInstall(game_dir));
        }
        let mut inst = Install {
            game_dir: game_dir.clone(),
            build_id: None,
            branch: None,
        };
        let manifest = format!("appmanifest_{APP_ID}.acf");
        let mut candidates = vec![game_dir.join("steamapps").join(&manifest)];
        if let Some(steamapps) = game_dir.parent().and_then(Path::parent) {
            candidates.push(steamapps.join(&manifest));
        }
        for c in candidates {
            if c.is_file() {
                inst.read_manifest(&c);
                break;
            }
        }
        Ok(inst)
    }

    fn read_manifest(&mut self, path: &Path) {
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let Ok(doc) = keyvalues::parse(&text) else {
            return;
        };
        let Some(root) = doc.get("AppState") else {
            return;
        };
        self.build_id = root.get_str("buildid").map(str::to_string);
        self.branch = root
            .get("UserConfig")
            .and_then(|u| u.get_str("BetaKey"))
            .map(str::to_string)
            .or_else(|| Some("public".to_string()));
    }
}

/// Default Steam roots per platform, then every `path` from `libraryfolders.vdf`.
pub fn steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        roots.push(home.join(".steam/steam"));
        roots.push(home.join(".local/share/Steam"));
        roots.push(home.join("Library/Application Support/Steam"));
    }
    if let Some(pf) = std::env::var_os("ProgramFiles(x86)").map(PathBuf::from) {
        roots.push(pf.join("Steam"));
    }
    if let Some(extra) = std::env::var_os("STEAM_ROOT").map(PathBuf::from) {
        roots.insert(0, extra);
    }
    let mut all = roots.clone();
    for r in roots {
        let vdf = r.join("steamapps/libraryfolders.vdf");
        if let Ok(text) = std::fs::read_to_string(&vdf) {
            if let Ok(doc) = keyvalues::parse(&text) {
                if let Some(lf) = doc.get("libraryfolders") {
                    for folder in lf.children() {
                        if let Some(p) = folder.get_str("path") {
                            all.push(PathBuf::from(p.replace("\\\\", "\\")));
                        }
                    }
                }
            }
        }
    }
    all.dedup();
    all
}

/// Find a complete install (`StateFlags` 4) in any Steam library, or honour `GMOD_DIR`.
pub fn find_install() -> Option<Install> {
    if let Some(dir) = std::env::var_os("GMOD_DIR") {
        if let Ok(i) = Install::at(dir) {
            return Some(i);
        }
    }
    for root in steam_roots() {
        let manifest = root.join(format!("steamapps/appmanifest_{APP_ID}.acf"));
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let Ok(doc) = keyvalues::parse(&text) else {
            continue;
        };
        let Some(state) = doc.get("AppState") else {
            continue;
        };
        if state.get_str("StateFlags") != Some("4") {
            continue;
        }
        let installdir = state.get_str("installdir").unwrap_or("GarrysMod");
        if let Ok(i) = Install::at(root.join("steamapps/common").join(installdir)) {
            return Some(i);
        }
    }
    None
}
