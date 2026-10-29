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

#[test]
fn gm_construct_lumps_parse() {
    let Some(dir) = gmod_dir() else {
        return;
    };
    let data = std::fs::read(dir.join("garrysmod/maps/gm_construct.bsp")).unwrap();
    let bsp = gm_formats::bsp::Bsp::parse(&data).unwrap();
    assert!(bsp.faces.len() > 1000, "{} faces", bsp.faces.len());
    assert!(!bsp.models.is_empty() && !bsp.leafs.is_empty() && !bsp.nodes.is_empty());
    assert_eq!(bsp.models[0].first_face, 0);
    let ws = bsp.worldspawn().expect("worldspawn");
    assert!(ws.get("skyname").is_some());
    assert!(bsp
        .entities
        .iter()
        .any(|e| e.classname() == "info_player_start"));
    let mut lit = 0;
    for f in bsp.faces.iter().take(500) {
        let verts = bsp.face_vertices(f);
        assert!(
            verts.len() >= 3 || f.num_edges == 0,
            "face with {} verts",
            verts.len()
        );
        let m = bsp.face_material(f).expect("material");
        assert!(!m.is_empty());
        if f.light_ofs >= 0 {
            let lm = bsp.face_lightmap(f).unwrap();
            let (w, h) = f.lightmap_dims();
            assert_eq!(lm.len(), (w * h) as usize * f.light_styles());
            lit += 1;
        }
    }
    assert!(lit > 100, "{lit} lit faces");
    assert!(bsp.tex_names.iter().any(|n| n.starts_with("tools/")));
}
