//! Prints the attributes of every operator named like the given function in the `.pcf` files
//! of a directory: `pcf_attrs $GMOD_DIR/garrysmod/particles "lock to bone"`
fn main() {
    let dir = std::env::args().nth(1).expect("particles directory");
    let needle = std::env::args().nth(2).unwrap_or_default().to_lowercase();
    let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("pcf") {
            continue;
        }
        let Ok(pcf) = gm_formats::pcf::Pcf::parse(&std::fs::read(&path).unwrap()) else {
            continue;
        };
        for name in pcf.system_names() {
            let sys = pcf.system(name).unwrap();
            for list in [
                "initializers",
                "operators",
                "emitters",
                "renderers",
                "forces",
                "constraints",
            ] {
                for op in pcf.operators(sys, list) {
                    let f = gm_formats::pcf::function_name(op);
                    if !f.to_lowercase().contains(&needle) {
                        continue;
                    }
                    println!(
                        "{} / {} / {}:",
                        path.file_name().unwrap().to_string_lossy(),
                        name,
                        f
                    );
                    for (k, v) in &op.attrs {
                        if k != "functionName" {
                            println!("    {k} = {v:?}");
                        }
                    }
                }
            }
        }
    }
}
