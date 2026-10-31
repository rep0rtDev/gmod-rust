use gm_formats::bsp::light;
fn main() {
    let install = gm_content::find_install().unwrap();
    let mount = gm_content::Mount::for_install(&install).unwrap();
    let map = std::env::args().nth(1).unwrap_or("gm_construct".into());
    let data = mount.read(&format!("maps/{map}.bsp")).unwrap();
    let bsp = gm_formats::bsp::Bsp::parse(&data).unwrap();
    for hdr in [false, true] {
        let wl = light::world_lights(&bsp, hdr).unwrap();
        let mut by = std::collections::BTreeMap::new();
        for l in &wl {
            *by.entry(format!("{:?}", l.emit)).or_insert(0) += 1;
        }
        println!("hdr={hdr}: {} world lights {:?}", wl.len(), by);
        for l in wl.iter().filter(|l| {
            matches!(
                l.emit,
                light::EmitType::Skylight | light::EmitType::SkyAmbient
            )
        }) {
            println!(
                "  {:?} intensity {:?} normal {:?}",
                l.emit, l.intensity, l.normal
            );
        }
        for l in wl
            .iter()
            .filter(|l| matches!(l.emit, light::EmitType::Point | light::EmitType::Spotlight))
            .take(3)
        {
            println!(
                "  {:?} at {:?} intensity {:?} attn c{} l{} q{} radius {} stopdot {} {} exp {}",
                l.emit,
                l.origin,
                l.intensity,
                l.constant_attn,
                l.linear_attn,
                l.quadratic_attn,
                l.radius,
                l.stopdot,
                l.stopdot2,
                l.exponent
            );
        }
        let amb = light::LeafAmbient::parse(&bsp, hdr).unwrap();
        let total: usize = amb.leaves.iter().map(Vec::len).sum();
        println!("  ambient: {} leaves, {} samples", amb.leaves.len(), total);
        let leaf = bsp.leaf_at([704.0, 132.0, -100.0]);
        for s in amb.leaves[leaf].iter().take(2) {
            println!(
                "  leaf {leaf} sample pos {:?} cube {:?}",
                s.pos,
                s.cube
                    .iter()
                    .map(|c| c.to_linear().map(|v| (v * 1000.0).round() / 1000.0))
                    .collect::<Vec<_>>()
            );
        }
    }
    let names = light::pakfile_names(&bsp).unwrap();
    println!(
        "pakfile {} entries, {} vhv",
        names.len(),
        names.iter().filter(|n| n.ends_with(".vhv")).count()
    );
    for i in [0usize, 1, 5] {
        if let Some(v) = light::static_prop_vertex_light(&bsp, i, false).unwrap() {
            let n: usize = v.meshes.iter().map(Vec::len).sum();
            let avg: Vec<f32> = (0..4)
                .map(|k| {
                    v.meshes.iter().flatten().map(|c| c[k] as f32).sum::<f32>() / n.max(1) as f32
                })
                .collect();
            println!(
                "sp_{i}: {} meshes {} verts avg {:?} first {:?}",
                v.meshes.len(),
                n,
                avg,
                v.meshes.first().and_then(|m| m.first())
            );
        }
    }
}
