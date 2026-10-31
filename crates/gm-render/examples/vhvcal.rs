use glam::Vec3;
fn main() {
    let install = gm_content::find_install().unwrap();
    let mount = gm_content::Mount::for_install(&install).unwrap();
    let data = mount.read("maps/gm_construct.bsp").unwrap();
    let bsp = gm_formats::bsp::Bsp::parse(&data).unwrap();
    let light = gm_render::light::ModelLight::new(&bsp);
    let props = gm_formats::bsp::extra::StaticProps::parse(&bsp).unwrap();
    let mut ratios = Vec::new();
    for (i, p) in props.props.iter().enumerate().step_by(7).take(30) {
        let Some(v) = gm_formats::bsp::light::static_prop_vertex_light(&bsp, i, false).unwrap()
        else {
            continue;
        };
        let n: usize = v.meshes.iter().map(Vec::len).sum();
        if n == 0 {
            continue;
        }
        let mean: f32 = v
            .meshes
            .iter()
            .flatten()
            .map(|c| (c[0] as f32 + c[1] as f32 + c[2] as f32) / (3.0 * 255.0))
            .sum::<f32>()
            / n as f32;
        let l = light.at(Vec3::from(p.origin), 4);
        let dirs = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z];
        let ours: f32 = dirs
            .iter()
            .map(|d| l.shade(*d, false).element_sum() / 3.0)
            .sum::<f32>()
            / 6.0;
        let model = &props.models[p.model as usize];
        println!(
            "sp_{i:3} {:40} vhv {mean:.3} (lin {:.3}) ours {ours:.3} lights {}",
            &model[model.len().saturating_sub(40)..],
            mean.powf(2.2),
            l.lights.len()
        );
        ratios.push((mean / ours.max(1e-4), mean.powf(2.2) / ours.max(1e-4)));
    }
    let n = ratios.len() as f32;
    println!(
        "mean ratio vhv/ours gamma-as-is {:.3}, linearised {:.3}",
        ratios.iter().map(|r| r.0).sum::<f32>() / n,
        ratios.iter().map(|r| r.1).sum::<f32>() / n
    );
}
