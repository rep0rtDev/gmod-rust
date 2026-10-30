//! `gmr` end to end: new -> pack -> keygen/sign/verify -> publish -> resolve, as a user would.
use std::process::Command;

fn gmr(args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_gmr"))
        .args(args)
        .output()
        .expect("gmr runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

#[test]
fn gmr_new_pack_sign_publish_resolve() {
    let dir = std::env::temp_dir().join(format!("gmr-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let addon = dir.join("my_addon");
    let tool = dir.join("my-tool");
    let s = |p: &std::path::Path| p.to_str().unwrap().to_string();
    let (ok, text) = gmr(&["new", "addon", &s(&addon)]);
    assert!(ok, "{text}");
    let (ok, text) = gmr(&["new", "tool", &s(&tool)]);
    assert!(ok, "{text}");
    let (ok, text) = gmr(&["new", "addon", &s(&addon)]);
    assert!(!ok && text.contains("exists"), "{text}");
    let (ok, text) = gmr(&["new", "addon", &s(&dir.join("bad id!"))]);
    assert!(!ok, "{text}");

    // the templates load in both realms and the tool registers
    let mut server = gm_script::Runtime::new();
    server.load_addons(&[addon.clone(), tool.clone()]).unwrap();
    assert_eq!(server.actors().len(), 2);
    let mut client = gm_script::Runtime::client();
    client.load_addon(&addon).unwrap();
    assert_eq!(client.actors().len(), 1);

    let key = dir.join("me.key");
    let (ok, text) = gmr(&["keygen", &s(&key)]);
    assert!(ok && text.starts_with("public key "), "{text}");
    let pk = text.trim().rsplit(' ').next().unwrap().to_string();
    let (ok, text) = gmr(&["verify", &s(&addon)]);
    assert!(!ok && text.contains("not signed"), "{text}");
    let (ok, text) = gmr(&["sign", &s(&addon), &s(&key)]);
    assert!(ok && text.contains(&pk), "{text}");
    let gmr_file = dir.join("my_addon.gmr");
    let (ok, text) = gmr(&["pack", &s(&addon), &s(&gmr_file)]);
    assert!(ok && text.contains("sha256"), "{text}");
    let (ok, text) = gmr(&["verify", &s(&gmr_file)]);
    assert!(ok && text.contains(&pk), "{text}");
    let (ok, text) = gmr(&["inspect", &s(&gmr_file)]);
    assert!(
        ok && text.contains("my_addon 0.1.0") && text.contains("depends []"),
        "{text}"
    );

    let reg = dir.join("registry");
    let (ok, text) = gmr(&["publish", &s(&gmr_file), &s(&reg)]);
    assert!(ok && text.contains("published my_addon 0.1.0"), "{text}");
    let (ok, text) = gmr(&["publish", &s(&tool), &s(&reg)]);
    assert!(!ok && text.contains("not signed"), "{text}");
    let (ok, text) = gmr(&["resolve", &s(&reg), "my_addon@^0.1"]);
    assert!(ok && text.starts_with("my_addon 0.1.0 sha256 "), "{text}");
    let (ok, text) = gmr(&["resolve", &s(&reg), "my_addon@>=1"]);
    assert!(!ok && text.contains("no package satisfies"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}
