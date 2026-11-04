fn main() {
    let mut args = std::env::args().skip(1);
    let vpk = gm_formats::vpk::Archive::open(args.next().unwrap()).unwrap();
    for path in args {
        let phy = gm_formats::phy::Phy::parse(&vpk.read(&path).unwrap()).unwrap();
        for p in &phy.props {
            println!(
                "{path} props {}: mass {} surfaceprop {}",
                p.index, p.mass, p.surfaceprop
            );
        }
        for (i, s) in phy.solids.iter().enumerate() {
            println!(
                "{path} solid {i}: mass_center {:?} inertia {:?}",
                s.mass_center, s.rotation_inertia
            );
            for (j, c) in s.convexes.iter().enumerate() {
                println!("convex {j}: {} points", c.points.len());
                for p in &c.points {
                    println!("  {:.3} {:.3} {:.3}", p[0], p[1], p[2]);
                }
            }
        }
    }
}
