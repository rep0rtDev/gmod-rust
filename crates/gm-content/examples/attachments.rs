fn main() {
    let install = gm_content::find_install().unwrap();
    let mount = gm_content::Mount::for_install(&install).unwrap();
    for p in std::env::args().skip(1) {
        let mdl = gm_formats::mdl::Mdl::parse(&mount.read(&p).unwrap()).unwrap();
        println!(
            "== {p}: {} attachments, {} bones",
            mdl.attachments.len(),
            mdl.bones.len()
        );
        for a in &mdl.attachments {
            println!(
                "  {:20} bone {:3} {:24} pos {:?}",
                a.name,
                a.bone,
                mdl.bones
                    .get(a.bone.max(0) as usize)
                    .map(|b| b.name.as_str())
                    .unwrap_or("?"),
                a.position().map(|v| (v * 100.0).round() / 100.0)
            );
        }
    }
}
