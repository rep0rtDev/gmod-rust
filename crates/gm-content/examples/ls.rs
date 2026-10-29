//! List mounted content matching a glob: `GMOD_DIR=... cargo run -p gm-content --example ls -- 'models/props_c17/*.mdl'`

fn main() {
    let pattern = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "maps/*.bsp".to_string());
    let inst = gm_content::find_install().expect("set GMOD_DIR to the Garry's Mod directory");
    let mount = gm_content::Mount::for_install(&inst).expect("mount");
    let found = mount.find(&pattern);
    for p in &found {
        let who = mount
            .resolve(p)
            .map(|i| mount.providers()[i].label())
            .unwrap_or_default();
        println!("{p}\t{who}");
    }
    eprintln!(
        "{} matches, build {:?} branch {:?}",
        found.len(),
        inst.build_id,
        inst.branch
    );
}
