//! cargo run -p gm-dedicated --example hull_probe <model>: a model's physics hull extents
//! around its origin (from the hull planes), to compare resting heights with the game.
fn main() -> anyhow::Result<()> {
    let model = std::env::args()
        .nth(1)
        .unwrap_or("models/props_borealis/bluebarrel001.mdl".into());
    let inst = gm_content::find_install().expect("GMOD_DIR");
    let mount = gm_content::Mount::for_install(&inst)?;
    let info = gm_dedicated::model_info(&mount, &model)?;
    println!(
        "script phys {:?}..{:?}",
        info.script.phys_mins, info.script.phys_maxs
    );
    for (i, hull) in info.hull_planes.iter().enumerate() {
        let mut lo = glam::Vec3::splat(f32::INFINITY);
        let mut hi = glam::Vec3::splat(f32::NEG_INFINITY);
        for (n, d) in hull {
            for a in 0..3 {
                if n[a] > 0.99 {
                    hi[a] = hi[a].min(*d);
                }
                if n[a] < -0.99 {
                    lo[a] = lo[a].min(-*d);
                }
            }
        }
        println!(
            "hull {i}: {} planes, axis extents lo {lo:?} hi {hi:?}",
            hull.len()
        );
    }
    Ok(())
}
