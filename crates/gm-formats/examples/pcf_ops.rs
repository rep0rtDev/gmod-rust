//! Lists every particle system and operator function of the `.pcf` files in a directory:
//! `cargo run -p gm-formats --example pcf_ops -- $GMOD_DIR/garrysmod/particles`
use std::collections::BTreeMap;

fn main() {
    let dir = std::env::args().nth(1).expect("particles directory");
    let mut funcs: BTreeMap<String, usize> = BTreeMap::new();
    let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("pcf") {
            continue;
        }
        let data = std::fs::read(&path).unwrap();
        let pcf = match gm_formats::pcf::Pcf::parse(&data) {
            Ok(p) => p,
            Err(e) => {
                println!("{}: {e}", path.display());
                continue;
            }
        };
        let names: Vec<&str> = pcf.system_names().collect();
        println!(
            "{}: {} systems {names:?}",
            path.file_name().unwrap().to_string_lossy(),
            names.len()
        );
        for &i in &pcf.systems {
            let s = &pcf.dmx.elements[i];
            for list in gm_formats::pcf::OPERATOR_LISTS {
                for op in pcf.operators(s, list) {
                    let f = if op.type_name == "DmeParticleChild" {
                        "child".to_string()
                    } else {
                        gm_formats::pcf::function_name(op).to_string()
                    };
                    *funcs.entry(format!("{list}/{f}")).or_default() += 1;
                }
            }
        }
    }
    for (f, n) in funcs {
        println!("{n:4} {f}");
    }
}
