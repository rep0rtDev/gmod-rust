use gm_content::{Install, Mount};

fn install() -> Option<Install> {
    let dir = std::env::var_os("GMOD_DIR")?;
    Install::at(dir).ok()
}

#[test]
fn mounts_and_finds_core_files() {
    let Some(inst) = install() else {
        eprintln!("GMOD_DIR not set, skipping");
        return;
    };
    let m = Mount::for_install(&inst).unwrap();
    assert!(
        m.providers().len() >= 6,
        "{} providers",
        m.providers().len()
    );
    for p in [
        "maps/gm_construct.bsp",
        "maps/gm_flatgrass.bsp",
        "scripts/surfaceproperties.txt",
        "models/props_c17/oildrum001.mdl",
        "models/weapons/w_physics.mdl",
        "resource/clientscheme.res",
    ] {
        assert!(m.exists(p), "missing {p}");
    }
    let bsp = m.read("maps/gm_construct.bsp").unwrap();
    let h = gm_formats::bsp::Header::parse(&bsp).unwrap();
    assert!((19..=21).contains(&h.version));
    assert!(!m.find("materials/models/props_c17/*.vmt").is_empty());
    let c17 = m.find("models/props_c17/*.mdl");
    assert!(c17.len() > 100, "{} props_c17 models", c17.len());
    let vmts = m.find("materials/**/*.vmt");
    assert!(vmts.len() > 10_000, "{} vmts", vmts.len());
}
