//! Trace a ray through a map's brush world: `trace <map> x0 y0 z0 x1 y1 z1`.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let inst = gm_content::find_install().expect("GMOD_DIR");
    let mount = gm_content::Mount::for_install(&inst).unwrap();
    let world = gm_dedicated::load_world(&mount, &args[0]).unwrap();
    let v: Vec<f32> = args[1..7].iter().map(|s| s.parse().unwrap()).collect();
    let a = glam::Vec3::new(v[0], v[1], v[2]);
    let b = glam::Vec3::new(v[3], v[4], v[5]);
    let tr = world.collision.trace_ray(a, b, gm_collision::MASK_SOLID);
    println!(
        "fraction {:.4} end {:?} normal {:?}",
        tr.fraction, tr.end, tr.normal
    );
}
