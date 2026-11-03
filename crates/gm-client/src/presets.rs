//! Tool presets (`includes/modules/presets.lua`): `settings/presets/<group>.txt`, the
//! `util.TableToKeyValues` of `{ name = { convar = value } }`, under the client's home dir.

use std::collections::BTreeMap;
use std::path::PathBuf;

pub type Group = BTreeMap<String, BTreeMap<String, String>>;

fn path(group: &str) -> PathBuf {
    crate::settings::home_dir()
        .join("settings")
        .join("presets")
        .join(format!("{group}.txt"))
}

/// The group's presets by name (empty when there is no file).
pub fn load(group: &str) -> Group {
    let mut out = Group::new();
    let Ok(text) = std::fs::read_to_string(path(group)) else {
        return out;
    };
    let Ok(doc) = gm_formats::keyvalues::parse(&text) else {
        return out;
    };
    let Some(root) = doc.root() else {
        return out;
    };
    for preset in root.children() {
        let vars: BTreeMap<String, String> = preset
            .children()
            .iter()
            .filter_map(|n| n.as_str().map(|v| (n.key.clone(), v.to_string())))
            .collect();
        out.insert(preset.key.clone(), vars);
    }
    out
}

/// Writes the group's presets (`presets.Add` / `Remove` save the whole group).
pub fn save(group: &str, presets: &Group) -> std::io::Result<()> {
    let p = path(group);
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let clean = |s: &str| s.replace(['"', '\\'], "");
    let mut out = String::from("\"TableToKeyValues\"\n{\n");
    for (name, vars) in presets {
        out.push_str(&format!("\t\"{}\"\n\t{{\n", clean(name)));
        for (k, v) in vars {
            out.push_str(&format!("\t\t\"{}\"\t\t\"{}\"\n", clean(k), clean(v)));
        }
        out.push_str("\t}\n");
    }
    out.push_str("}\n");
    std::fs::write(p, out)
}
