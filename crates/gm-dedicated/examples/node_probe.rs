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
    if std::env::var_os("NODE_BOX").is_some() {
        let (lo, hi) = pts.iter().fold(
            (glam::Vec2::splat(f32::MAX), glam::Vec2::splat(f32::MIN)),
            |(lo, hi), p| (lo.min(p.truncate()), hi.max(p.truncate())),
        );
        let (lo, hi) = (lo - 64.0, hi + 64.0);
        let inside = |p: [f32; 3]| (lo.x..=hi.x).contains(&p[0]) && (lo.y..=hi.y).contains(&p[1]);
        for (i, n) in ain.nodes.iter().enumerate() {
            if inside(n.pos) {
                let links: Vec<String> = ain
                    .links
                    .iter()
                    .filter_map(|l| {
                        let o = if l.src as usize == i {
                            l.dst
                        } else if l.dst as usize == i {
                            l.src
                        } else {
                            return None;
                        };
                        Some(format!("{o}:{:x}", l.moves[hull]))
                    })
                    .collect();
                println!(
                    "node #{i} wc{} {:?} type {} -> {}",
                    ain.wc_ids[i],
                    n.pos,
                    n.node_type,
                    links.join(" ")
                );
            }
        }
        if pts.len() >= 2 {
            let ext = glam::Vec3::new(13.0, 13.0, 72.0);
            let d = world.nav.find_path(pts[0], pts[pts.len() - 1], ext);
            println!("nav direct: {:?}", d.map(|p| (p.partial, p.points)));
        }
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
