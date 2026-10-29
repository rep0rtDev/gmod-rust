fn main() {
    let install = gm_content::find_install().unwrap();
    let mount = gm_content::Mount::for_install(&install).unwrap();
    let data = mount.read("maps/gm_construct.bsp").unwrap();
    let bsp = gm_formats::bsp::Bsp::parse(&data).unwrap();
    let mut mats = std::collections::BTreeMap::new();
    for f in &bsp.faces {
        *mats
            .entry(bsp.face_material(f).unwrap().to_string())
            .or_insert(0) += 1;
    }
    println!("{} materials", mats.len());
    for (m, n) in mats.iter().take(60) {
        let p = format!("materials/{m}.vmt");
        let txt = mount
            .read(&p)
            .map(|b| {
                String::from_utf8_lossy(&b)
                    .replace('\n', " | ")
                    .chars()
                    .take(160)
                    .collect::<String>()
            })
            .unwrap_or_else(|e| format!("MISSING {e}"));
        println!("{n:5} {m}: {txt}");
    }
}
