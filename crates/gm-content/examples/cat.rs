fn main() {
    let install = gm_content::find_install().unwrap();
    let mount = gm_content::Mount::for_install(&install).unwrap();
    for p in std::env::args().skip(1) {
        match mount.read(&p) {
            Ok(b) => println!(
                "== {p} ({} bytes)\n{}",
                b.len(),
                String::from_utf8_lossy(&b[..b.len().min(4000)])
            ),
            Err(e) => println!("== {p}: MISSING {e}"),
        }
    }
}
