//! cargo run -p gm-dedicated --example corner_probe [x y z nx ny]: sweeps a human hull from a
//! path corner towards the next one (server_npc_assault_citizen's (1008,16) -> (1064,24) by
//! default) and maps the solids and nav polygons around the corner.
use glam::Vec3;

fn main() -> anyhow::Result<()> {
    let nums: Vec<f32> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();
    let (from, to) = if nums.len() == 5 {
        (
            Vec3::new(nums[0], nums[1], nums[2]),
            Vec3::new(nums[3], nums[4], nums[2]),
        )
    } else {
        (Vec3::new(1008.0, 16.0, -144.0), Vec3::new(1064.0, 24.0, -144.0))
    };
    let inst = gm_content::find_install().expect("GMOD_DIR");
    let mount = gm_content::Mount::for_install(&inst)?;
    let world = gm_dedicated::load_world(&mount, "gm_construct")?;
    let (mins, maxs) = (Vec3::new(-13.0, -13.0, 0.0), Vec3::new(13.0, 13.0, 72.0));
    let lift = Vec3::new(0.0, 0.0, 18.0);
    let dir = (to - from).normalize();
    for step in [22.0f32, 11.0, 5.0, 3.0] {
        let want = from + dir * step;
        let tr = world.collision.trace_box(
            from + lift,
            want + lift,
            mins,
            maxs,
            gm_collision::MASK_PLAYERSOLID,
        );
        println!(
            "sweep {step:5.1}: frac {:.3} end ({:.1},{:.1}) n ({:.2},{:.2},{:.2}) start_solid {}",
            tr.fraction, tr.end.x, tr.end.y, tr.normal.x, tr.normal.y, tr.normal.z, tr.start_solid
        );
    }
    // solid map at hull height: '#' point solid, '.' free (x across, y down from north)
    let (x0, x1, y0, y1) = (from.x - 20.0, from.x + 50.0, from.y - 30.0, from.y + 60.0);
    println!("point solids z {:.0}..{:.0}, x {x0}..{x1} step 2, y {y1}..{y0}", from.z + 18.0, from.z + 60.0);
    let mut y = y1;
    while y >= y0 {
        let mut line = String::new();
        let mut x = x0;
        while x <= x1 {
            let p = Vec3::new(x, y, from.z + 40.0);
            let tr = world.collision.trace_box(
                p,
                p,
                Vec3::ZERO,
                Vec3::ZERO,
                gm_collision::MASK_PLAYERSOLID,
            );
            line.push(if tr.start_solid || tr.fraction < 1.0 { '#' } else { '.' });
            x += 2.0;
        }
        println!("y {y:6.1} {line}");
        y -= 4.0;
    }
    let verts = world.nav.vertices();
    for (i, p) in world.nav.polys().iter().enumerate() {
        let vs: Vec<Vec3> = p.verts.iter().map(|&v| verts[v as usize]).collect();
        if vs.iter().any(|v| (x0..=x1).contains(&v.x) && (y0..=y1).contains(&v.y)) {
            let s: Vec<String> = vs.iter().map(|v| format!("({:.0},{:.0})", v.x, v.y)).collect();
            println!("poly {i}: {}", s.join(" "));
        }
    }
    Ok(())
}
