fn main() {
    for path in std::env::args().skip(1) {
        let phy = gm_formats::phy::Phy::parse(&std::fs::read(&path).unwrap()).unwrap();
        for (i, s) in phy.solids.iter().enumerate() {
            for (j, c) in s.convexes.iter().enumerate() {
                println!(
                    "{path} solid {i} convex {j}: {} points, {} triangles",
                    c.points.len(),
                    c.triangles.len()
                );
                for (k, p) in c.points.iter().enumerate() {
                    println!("  {k}: {:.3} {:.3} {:.3}", p[0], p[1], p[2]);
                }
                println!(
                    "  first triangles: {:?}",
                    &c.triangles[..c.triangles.len().min(6)]
                );
            }
        }
    }
}
