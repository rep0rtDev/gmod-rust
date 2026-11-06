//! Dump a file from a VPK: `vpk_cat <dir.vpk> <path/in/vpk>` writes it to stdout.
use std::io::Write;
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let data = gm_formats::vpk::Archive::open(&args[0])
        .expect("vpk")
        .read(&args[1])
        .expect("entry");
    std::io::stdout().write_all(&data).unwrap();
}
