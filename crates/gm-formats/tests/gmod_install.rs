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

#[test]
fn hl2_vtf_mips_decode() {
    let Some(dir) = gmod_dir() else {
        return;
    };
    let ar = gm_formats::vpk::Archive::open(dir.join("sourceengine/hl2_textures_dir.vpk")).unwrap();
    let mut decoded = 0;
    let mut nonzero = 0;
    let mut formats = std::collections::BTreeMap::new();
    for (k, e) in ar.directory.iter_prefix("materials") {
        if !k.ends_with(".vtf") || decoded >= 300 {
            continue;
        }
        let bytes = ar.read_entry(e).unwrap();
        let h = gm_formats::vtf::Header::parse(&bytes).unwrap();
        if h.depth != 1 || h.high_res_format.bits_per_pixel() == 0 {
            continue;
        }
        let Ok(slice) = gm_formats::vtf::decode::mip_slice(&h, &bytes, 0, 0, 0, 0) else {
            panic!(
                "{k}: mip 0 out of range (format {:?}, {}x{})",
                h.high_res_format, h.width, h.height
            );
        };
        let rgba = gm_formats::vtf::decode::decode_rgba8(
            h.high_res_format,
            h.width as u32,
            h.height as u32,
            slice,
        )
        .unwrap_or_else(|e| panic!("{k}: {e}"));
        assert_eq!(rgba.len(), (h.width as usize) * (h.height as usize) * 4);
        // a few textures are legitimately black (the bottom face of HDR skyboxes)
        if rgba.iter().any(|&b| b != 0) {
            nonzero += 1;
        }
        let last = h.mipmap_count as u32 - 1;
        let (w, hh, _) = gm_formats::vtf::decode::mip_dims(&h, last);
        let s = gm_formats::vtf::decode::mip_slice(&h, &bytes, last, 0, 0, 0).unwrap();
        assert_eq!(s.len(), h.high_res_format.image_size(w, hh) as usize);
        *formats
            .entry(format!("{:?}", h.high_res_format))
            .or_insert(0) += 1;
        decoded += 1;
    }
    eprintln!("decoded {decoded} textures: {formats:?}");
    assert!(decoded >= 100);
    assert!(
        nonzero * 10 >= decoded * 9,
        "{nonzero} of {decoded} textures non-black"
    );
}

#[test]
fn hl2_props_mdl_vvd_vtx_parse() {
    let Some(dir) = gmod_dir() else {
        return;
    };
    let ar = gm_formats::vpk::Archive::open(dir.join("sourceengine/hl2_misc_dir.vpk")).unwrap();
    let mut checked = 0;
    for (k, e) in ar.directory.iter_prefix("models") {
        if !k.ends_with(".mdl") || checked >= 60 {
            continue;
        }
        let stem = k.trim_end_matches(".mdl");
        let (Some(vvd_e), Some(vtx_e)) = (
            ar.directory.get(&format!("{stem}.vvd")),
            ar.directory.get(&format!("{stem}.dx90.vtx")),
        ) else {
            continue;
        };
        let mdl_bytes = ar.read_entry(e).unwrap();
        let mdl =
            gm_formats::mdl::Mdl::parse(&mdl_bytes).unwrap_or_else(|err| panic!("{k}: {err}"));
        assert!(!mdl.bones.is_empty(), "{k}: no bones");
        assert!(!mdl.body_parts.is_empty(), "{k}: no body parts");
        assert!(!mdl.textures.is_empty(), "{k}: no textures");
        assert!(!mdl.surface_prop.is_empty(), "{k}: no surfaceprop");
        let vvd = gm_formats::mdl::Vvd::parse(&ar.read_entry(vvd_e).unwrap(), 0)
            .unwrap_or_else(|err| panic!("{k}: {err}"));
        assert_eq!(vvd.checksum, mdl.header.checksum, "{k}: vvd checksum");
        let total: i32 = mdl
            .body_parts
            .iter()
            .flat_map(|b| b.models.iter())
            .map(|m| m.num_vertices)
            .sum();
        assert!(
            vvd.vertices.len() as i32 >= total,
            "{k}: {} vvd verts < {total}",
            vvd.vertices.len()
        );
        let vtx = gm_formats::mdl::Vtx::parse(&ar.read_entry(vtx_e).unwrap(), &mdl)
            .unwrap_or_else(|err| panic!("{k}: {err}"));
        assert_eq!(vtx.checksum, mdl.header.checksum, "{k}: vtx checksum");
        let mut tris = 0;
        for (bi, bp) in vtx.body_parts.iter().enumerate() {
            for (mi, m) in bp.models.iter().enumerate() {
                let mdl_model = &mdl.body_parts[bi].models[mi];
                let base = (mdl_model.vertex_index / 48) as u32;
                for lod in m.lods.iter().take(1) {
                    for mesh in &lod.meshes {
                        assert_eq!(mesh.indices.len() % 3, 0, "{k}: index count");
                        for &i in &mesh.indices {
                            assert!(
                                ((base + i) as usize) < vvd.vertices.len(),
                                "{k}: index {i} + base {base} out of {}",
                                vvd.vertices.len()
                            );
                        }
                        tris += mesh.indices.len() / 3;
                    }
                }
            }
        }
        assert!(tris > 0, "{k}: no triangles");
        checked += 1;
    }
    assert!(checked >= 20, "checked {checked}");
}

#[test]
fn gm_construct_props_disps_pvs() {
    let Some(dir) = gmod_dir() else {
        return;
    };
    let data = std::fs::read(dir.join("garrysmod/maps/gm_construct.bsp")).unwrap();
    let bsp = gm_formats::bsp::Bsp::parse(&data).unwrap();
    let props = gm_formats::bsp::extra::StaticProps::parse(&bsp).unwrap();
    assert_eq!(props.version, 6);
    assert_eq!(props.props.len(), 182);
    assert_eq!(props.models.len(), 17);
    assert!(
        props
            .models
            .iter()
            .all(|m| m.starts_with("models/") && m.ends_with(".mdl")),
        "{:?}",
        props.models
    );
    for p in &props.props {
        assert!((p.model as usize) < props.models.len());
        assert!(p.origin.iter().all(|v| v.abs() < 16384.0));
    }
    let infos = gm_formats::bsp::extra::disp_infos(&bsp).unwrap();
    let verts = gm_formats::bsp::extra::disp_verts(&bsp).unwrap();
    assert_eq!(infos.len(), 110);
    let mut tris = 0;
    for d in &infos {
        let m = gm_formats::bsp::extra::build_disp_mesh(&bsp, d, &verts).unwrap();
        assert_eq!(m.positions.len(), d.vert_count());
        assert_eq!(m.indices.len(), d.tri_count() * 3);
        tris += d.tri_count();
    }
    assert!(tris > 1000);
    let vis = gm_formats::bsp::extra::Visibility::parse(&bsp).unwrap();
    assert!(vis.num_clusters > 100, "{}", vis.num_clusters);
    let spawn = bsp
        .entities
        .iter()
        .find(|e| e.classname() == "info_player_start")
        .and_then(|e| e.origin())
        .unwrap();
    let leaf = bsp.leaf_at([spawn[0], spawn[1], spawn[2] + 32.0]);
    let cluster = bsp.leafs[leaf].cluster;
    assert!(cluster >= 0, "spawn leaf {leaf} has cluster {cluster}");
    let pvs = vis.pvs(&bsp, cluster as usize).unwrap();
    let visible = pvs.iter().filter(|&&v| v).count();
    assert!(pvs[cluster as usize], "cluster sees itself");
    assert!(
        visible > 1 && visible < vis.num_clusters,
        "{visible}/{}",
        vis.num_clusters
    );
}

#[test]
fn hl2_props_phy_parse() {
    let Some(dir) = gmod_dir() else {
        return;
    };
    let ar = gm_formats::vpk::Archive::open(dir.join("sourceengine/hl2_misc_dir.vpk")).unwrap();
    let crate_phy = ar.read("models/props_junk/wood_crate001a.phy").unwrap();
    let phy = gm_formats::phy::Phy::parse(&crate_phy).unwrap();
    assert_eq!(phy.solids.len(), 1);
    assert_eq!(phy.solids[0].convexes.len(), 1);
    let hull = &phy.solids[0].convexes[0];
    assert_eq!(hull.points.len(), 8);
    assert_eq!(hull.triangles.len(), 12);
    for p in &hull.points {
        for c in p {
            assert!((c.abs() - 20.0).abs() < 1.0, "crate point {p:?}");
        }
    }
    assert_eq!(phy.props.len(), 1);
    assert_eq!(phy.props[0].surfaceprop, "Wood_Crate");
    assert!((phy.props[0].mass - 30.0).abs() < 1e-3);

    let (mut files, mut solids, mut convexes) = (0, 0, 0);
    for (k, e) in ar.directory.iter_prefix("models") {
        if !k.ends_with(".phy") {
            continue;
        }
        let bytes = ar.read_entry(e).unwrap();
        let phy = gm_formats::phy::Phy::parse(&bytes).unwrap_or_else(|e| panic!("{k}: {e}"));
        assert!(!phy.solids.is_empty(), "{k}");
        for s in &phy.solids {
            for c in &s.convexes {
                assert!(c.points.len() >= 3 && !c.triangles.is_empty(), "{k}");
                for t in &c.triangles {
                    assert!(t.iter().all(|&i| (i as usize) < c.points.len()), "{k}");
                }
                for p in &c.points {
                    assert!(p.iter().all(|c| c.abs() < 16384.0), "{k} {p:?}");
                }
            }
            convexes += s.convexes.len();
        }
        solids += phy.solids.len();
        files += 1;
    }
    eprintln!("{files} hl2_misc PHY files, {solids} solids, {convexes} convexes");
    assert!(files > 1000);
}
