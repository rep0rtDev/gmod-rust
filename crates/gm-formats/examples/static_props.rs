//! List a map's static props inside a box: `static_props <map> x0 y0 z0 x1 y1 z1`.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = std::env::var("GMOD_DIR").expect("GMOD_DIR");
    let data = std::fs::read(format!("{dir}/garrysmod/maps/{}.bsp", args[0])).unwrap();
    let bsp = gm_formats::bsp::Bsp::parse(&data).unwrap();
    let props = gm_formats::bsp::extra::StaticProps::parse(&bsp).unwrap();
    let b: Vec<f32> = args[1..7].iter().map(|s| s.parse().unwrap()).collect();
    println!(
        "version {} props {} models {}",
        props.version,
        props.props.len(),
        props.models.len()
    );
    for p in &props.props {
        let o = p.origin;
        if o[0] >= b[0]
            && o[0] <= b[3]
            && o[1] >= b[1]
            && o[1] <= b[4]
            && o[2] >= b[2]
            && o[2] <= b[5]
        {
            println!(
                "{:?} ang {:?} solid {} flags {} {}",
                o, p.angles, p.solid, p.flags, props.models[p.model as usize]
            );
        }
    }
}
