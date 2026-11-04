//! cargo run -p gm-dedicated --example node_probe [x y ...]: gm_construct graph nodes within 40 u
//! of each given point, and the graph route between the nodes nearest the first and last point.
fn main() -> anyhow::Result<()> {
    let inst = gm_content::find_install().expect("GMOD_DIR");
    let mount = gm_content::Mount::for_install(&inst)?;
    let world = gm_dedicated::load_world(&mount, "gm_construct")?;
    let ain = world.ai_graph.as_ref().expect("ain");
    let g = gm_server::AiGraph::from_ain(ain);
    let hull = gm_server::npc_hull_index("npc_citizen");
    let args: Vec<f32> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();
    let pts: Vec<glam::Vec3> = args
        .chunks(2)
        .map(|c| glam::Vec3::new(c[0], c[1], -143.0))
        .collect();
    for p in &pts {
        let near: Vec<String> = (0..g.len())
            .filter(|&i| g.node_pos(i, hull).truncate().distance(p.truncate()) < 40.0)
            .map(|i| format!("#{i} {:?}", g.node_pos(i, hull).truncate()))
            .collect();
        println!("near ({},{}): {}", p.x, p.y, near.join(", "));
    }
    if pts.len() >= 2 {
        let s = g.ground_near(pts[0], hull, 200.0);
        let e = g.ground_near(pts[pts.len() - 1], hull, 200.0);
        println!(
            "start candidates {:?} end candidates {:?}",
            &s[..s.len().min(4)],
            &e[..e.len().min(4)]
        );
        if let (Some(&s0), Some(&e0)) = (s.first(), e.first()) {
            let chain = g.route_caps(s0, e0, hull, gm_formats::ain::CAP_MOVE_GROUND);
            println!(
                "route {s0}->{e0}: {:?}",
                chain.map(|c| c
                    .iter()
                    .map(|&(i, _)| (i, g.node_pos(i, hull).truncate()))
                    .collect::<Vec<_>>())
            );
        }
    }
    Ok(())
}
