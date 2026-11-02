//! Navigation mesh over the real gm_construct; runs only with a mounted game (`GMOD_DIR`).

use glam::Vec3;
use gm_nav::{NavMesh, NavParams};

fn load_construct() -> Option<Vec<u8>> {
    let install = gm_content::find_install()?;
    let mount = gm_content::Mount::for_install(&install).ok()?;
    mount.read("maps/gm_construct.bsp").ok()
}

#[test]
fn construct_navmesh_paths_across_the_open_ground() {
    let Some(data) = load_construct() else {
        eprintln!("no GMod install, skipping");
        return;
    };
    let bsp = gm_formats::bsp::Bsp::parse(&data).unwrap();
    let g = gm_nav::world_geometry(&bsp);
    eprintln!(
        "geometry: {} verts, {} tris, bounds {} .. {}",
        g.verts.len(),
        g.tris.len(),
        g.mins,
        g.maxs
    );
    let nav = NavMesh::build(&bsp, &NavParams::default()).unwrap();
    eprintln!(
        "gm_construct: {} polys, {} verts",
        nav.poly_count(),
        nav.vertices().len()
    );
    assert!(nav.poly_count() > 200, "only {} polys", nav.poly_count());
    // Flat concrete at the sandbox spawn area (floor z = -143.97) to the far side.
    let a = Vec3::new(823.0, -600.0, -140.0);
    let b = Vec3::new(-300.0, -300.0, -140.0);
    let path = nav.find_path(a, b, Vec3::new(32.0, 32.0, 64.0)).unwrap();
    eprintln!("path: {:?}", path.points);
    assert!(!path.partial);
    assert!(path.points.first().unwrap().distance(a) < 8.0);
    assert!(path.points.last().unwrap().distance(b) < 8.0);
}

#[test]
fn construct_polygons_are_closed_and_their_adjacency_is_symmetric() {
    let Some(data) = load_construct() else {
        return;
    };
    let bsp = gm_formats::bsp::Bsp::parse(&data).unwrap();
    let nav = NavMesh::build(&bsp, &NavParams::default()).unwrap();
    let polys = nav.polys();
    assert_eq!(polys.len(), nav.poly_count());
    let nverts = nav.vertices().len() as u32;
    let mut links = 0;
    for (i, p) in polys.iter().enumerate() {
        assert!(
            p.verts.len() >= 3 && p.verts.len() == p.neis.len(),
            "poly {i}"
        );
        assert!(p.verts.iter().all(|&v| v < nverts));
        for n in p.neis.iter().flatten() {
            links += 1;
            assert!(
                polys[*n as usize].neis.contains(&Some(i as u32)),
                "poly {i} -> {n} is not mirrored"
            );
        }
    }
    assert!(
        links > polys.len(),
        "{links} links over {} polygons",
        polys.len()
    );
}
