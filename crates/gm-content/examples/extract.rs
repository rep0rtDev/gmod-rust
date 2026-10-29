//! GMOD_DIR=... cargo run -p gm-content --example extract -- models/x.phy out.bin
fn main() {
    let mut args = std::env::args().skip(1);
    let (path, out) = (args.next().expect("path"), args.next().expect("out"));
    let install = gm_content::find_install().expect("GMOD_DIR");
    let mount = gm_content::Mount::for_install(&install).unwrap();
    std::fs::write(&out, mount.read(&path).unwrap()).unwrap();
}
