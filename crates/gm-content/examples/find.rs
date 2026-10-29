//! GMOD_DIR=... cargo run -p gm-content --example find -- "models/props_c17/*.phy"
fn main() {
    let pat = std::env::args().nth(1).expect("pattern");
    let install = gm_content::find_install().expect("GMOD_DIR");
    let mount = gm_content::Mount::for_install(&install).unwrap();
    let found = mount.find(&pat);
    println!("{} entries", found.len());
    for f in found.iter().take(10) {
        println!("{f}");
    }
}
