//! Lists the documented GLua addresses an actor VM provides, one per line: `glua_list` prints
//! the implemented ones, `glua_list --stubs` the stubs. Feed both to
//! `tools/glua/build_manifest.py --present ... --stubs ...`.
use gm_script::{ActorConfig, ActorSpec, Runtime};

fn main() {
    let stubs = std::env::args().any(|a| a == "--stubs");
    let mut rt = Runtime::new();
    // `--realm client`: list the client realm's surface (the realm globals pick the
    // implementations / stubs of `glua/client.luau`)
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--realm") {
        rt.realm = args[i + 1].clone();
    }
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "list".into(),
        priority: 0,
        source: String::new(),
        config: ActorConfig::default(),
    })
    .expect("actor");
    let (present, stub_list) = rt.actors()[0].glua_list().expect("list");
    for a in if stubs { stub_list } else { present } {
        println!("{a}");
    }
}
