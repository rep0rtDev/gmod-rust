//! The coverage manifest may not claim more than an actor VM really provides.
use gm_script::{ActorConfig, ActorSpec, Runtime};
use std::collections::HashSet;

#[test]
fn manifest_claims_only_what_the_vm_provides() {
    // the manifest covers both realms: implemented when either realm provides it, a stub
    // only when it is a stub in both (how tools/glua/build_manifest.py merges the lists)
    let mut present = HashSet::new();
    let mut stubs: Option<HashSet<String>> = None;
    for mut rt in [Runtime::new(), Runtime::client()] {
        rt.load(ActorSpec {
            addon: "glua".into(),
            name: "coverage".into(),
            priority: 0,
            source: String::new(),
            config: ActorConfig::default(),
        })
        .unwrap();
        let (p, s) = rt.actors()[0].glua_list().unwrap();
        present.extend(p);
        let s: HashSet<String> = s.into_iter().collect();
        stubs = Some(match stubs {
            Some(prev) => prev.intersection(&s).cloned().collect(),
            None => s,
        });
    }
    let stubs = stubs.unwrap_or_default();
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/glua/manifest.json"
    );
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut claimed = 0;
    let mut wrong = Vec::new();
    let mut unlisted = 0;
    let mut addresses = HashSet::new();
    for e in manifest["functions"].as_array().unwrap() {
        let addr = e["address"].as_str().unwrap();
        addresses.insert(addr.to_owned());
        match e["status"].as_str().unwrap() {
            "implemented" => {
                claimed += 1;
                if !present.contains(addr) {
                    wrong.push(format!("{addr}: claimed implemented, absent"));
                }
            }
            "stub" => {
                if !stubs.contains(addr) {
                    wrong.push(format!("{addr}: claimed stub, not a stub"));
                }
            }
            _ => {
                if present.contains(addr) {
                    unlisted += 1;
                }
            }
        }
    }
    eprintln!(
        "manifest: {} entries, {claimed} implemented, {} present in the VM, {unlisted} present but not recorded (rerun tools/glua/build_manifest.py --present)",
        addresses.len(),
        present.iter().filter(|a| addresses.contains(*a)).count()
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
