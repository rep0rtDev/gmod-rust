//! cargo run -p gm-formats --example phy_info -- file.phy: header fields of each solid
fn main() {
    for path in std::env::args().skip(1) {
        let phy = gm_formats::phy::Phy::parse(&std::fs::read(&path).unwrap()).unwrap();
        for (i, s) in phy.solids.iter().enumerate() {
            println!(
                "{path} solid {i}: drag_axis_areas {:?} axis_map {} mass_center {:?} inertia {:?} radius {:.2} convexes {}",
                s.drag_axis_areas, s.axis_map_size, s.mass_center, s.rotation_inertia, s.upper_limit_radius, s.convexes.len()
            );
        }
    }
}
