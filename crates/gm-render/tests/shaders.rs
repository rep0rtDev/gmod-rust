//! The WGSL sources parse and validate under naga: the pipelines are only built on a GPU,
//! which the test runs do not have, so a shader edit is checked here.

fn check(name: &str, src: &str) {
    let module = naga::front::wgsl::parse_str(src)
        .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("{name}: {e:?}"));
}

#[test]
fn world_wgsl_validates() {
    check("world.wgsl", include_str!("../src/shaders/world.wgsl"));
}

#[test]
fn post_wgsl_validates() {
    check("post.wgsl", include_str!("../src/shaders/post.wgsl"));
}

#[test]
fn ui_wgsl_validates() {
    check("ui.wgsl", include_str!("../src/shaders/ui.wgsl"));
}
