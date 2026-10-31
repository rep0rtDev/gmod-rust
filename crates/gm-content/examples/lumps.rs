fn main() {
    let install = gm_content::find_install().unwrap();
    let mount = gm_content::Mount::for_install(&install).unwrap();
    let map = std::env::args().nth(1).unwrap_or("gm_construct".into());
    let data = mount.read(&format!("maps/{map}.bsp")).unwrap();
    let bsp = gm_formats::bsp::Bsp::parse(&data).unwrap();
    for i in 0..64u32 {
        let l = bsp.header.lumps[i as usize];
        if l.length > 0 {
            println!(
                "lump {i:2}: off {:9} len {:9} ver {}",
                l.offset, l.length, l.version
            );
        }
    }
}
