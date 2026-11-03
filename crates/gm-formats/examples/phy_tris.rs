fn main() {
    let mut args = std::env::args().skip(1);
    let vpk = gm_formats::vpk::Archive::open(args.next().unwrap()).unwrap();
    for path in args {
        let phy = gm_formats::phy::Phy::parse(&vpk.read(&path).unwrap()).unwrap();
        for (j, c) in phy.solids[0].convexes.iter().enumerate() {
            println!("convex {j} {} {}", c.points.len(), c.triangles.len());
            for p in &c.points {
                println!("p {:.3} {:.3} {:.3}", p[0], p[1], p[2]);
            }
            for t in &c.triangles {
                println!("t {} {} {}", t[0], t[1], t[2]);
            }
        }
    }
}
