//! Integration tests against a real Garry's Mod installation. They run only when
//! `GMOD_DIR` points at the game directory (the one containing `garrysmod/`).

use std::path::PathBuf;

fn gmod_dir() -> Option<PathBuf> {
    std::env::var_os("GMOD_DIR")
        .map(PathBuf::from)
        .filter(|p| p.join("garrysmod").is_dir())
}

#[test]
fn garrysmod_vpk_has_materials_and_models() {
    let Some(dir) = gmod_dir() else {
        eprintln!("GMOD_DIR not set, skipping");
        return;
    };
    let ar = gm_formats::vpk::Archive::open(dir.join("garrysmod/garrysmod_dir.vpk")).unwrap();
    let d = &ar.directory;
    assert!(d.len() > 1000, "only {} entries", d.len());
    assert!(d.iter_prefix("materials").count() > 100);
    assert!(d.iter_prefix("models").count() > 100);
    let (path, _) = d
        .iter_prefix("materials")
        .find(|(k, _)| k.ends_with(".vmt"))
        .unwrap();
    let vmt = ar.read(path).unwrap();
    let text = String::from_utf8_lossy(&vmt);
    let doc = gm_formats::keyvalues::parse(&text).unwrap();
    assert!(doc.root().is_some(), "{path} parsed to empty document");
}

#[test]
fn gameinfo_parses() {
    let Some(dir) = gmod_dir() else {
        return;
    };
    let text = std::fs::read_to_string(dir.join("garrysmod/gameinfo.txt")).unwrap();
    let doc = gm_formats::keyvalues::parse(&text).unwrap();
    let root = doc.get("GameInfo").unwrap();
    assert_eq!(root.get_str("game"), Some("Garry's Mod"));
    let fs = root.get("FileSystem").unwrap();
    assert_eq!(fs.get_str("SteamAppId"), Some("4000"));
}

#[test]
fn hl2_vpk_vtf_headers_parse() {
    let Some(dir) = gmod_dir() else {
        return;
    };
    let candidates = [
        "sourceengine/hl2_textures_dir.vpk",
        "sourceengine/content_hl2_dir.vpk",
    ];
    let path = candidates
        .iter()
        .map(|c| dir.join(c))
        .find(|p| p.exists())
        .expect("an HL2 content vpk");
    let ar = gm_formats::vpk::Archive::open(path).unwrap();
    let mut checked = 0;
    for (k, e) in ar.directory.iter_prefix("materials") {
        if !k.ends_with(".vtf") {
            continue;
        }
        let bytes = ar.read_entry(e).unwrap();
        let h = gm_formats::vtf::Header::parse(&bytes).unwrap_or_else(|err| panic!("{k}: {err}"));
        assert!(h.width > 0 && h.height > 0, "{k}");
        assert!(h.header_size as usize <= bytes.len(), "{k}");
        checked += 1;
        if checked >= 200 {
            break;
        }
    }
    assert!(checked > 0);
}
