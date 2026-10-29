//! List face centroids for a material substring: GMOD_DIR=... cargo run -p gm-formats --example faces_by_material -- gm_construct stair
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var("GMOD_DIR").expect("GMOD_DIR");
    let data = std::fs::read(format!("{dir}/garrysmod/maps/{}.bsp", args[0])).unwrap();
    let bsp = gm_formats::bsp::Bsp::parse(&data).unwrap();
    for (i, f) in bsp.faces.iter().enumerate() {
        let Some(m) = bsp.face_material(f) else {
            continue;
        };
        if !m.contains(&args[1]) {
            continue;
        }
        let v = bsp.face_vertices(f);
        if v.is_empty() {
            continue;
        }
        let n = v.len() as f32;
        let c = v.iter().fold([0f32; 3], |a, p| {
            [a[0] + p[0] / n, a[1] + p[1] / n, a[2] + p[2] / n]
        });
        let pl = &bsp.planes[f.plane_num as usize];
        println!(
            "{i} {m} centre {:.0?} normal {:.2?} verts {}",
            c,
            pl.normal,
            v.len()
        );
    }
}
