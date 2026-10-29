use std::path::PathBuf;

/// joltc-sys has no `links` key, so its source (Jolt headers) is found in the registry.
fn joltc_src() -> Option<PathBuf> {
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")))?;
    let registry = home.join("registry").join("src");
    let mut found: Vec<PathBuf> = Vec::new();
    for index in std::fs::read_dir(registry).ok()? {
        let Ok(index) = index else { continue };
        for pkg in std::fs::read_dir(index.path()).ok()? {
            let Ok(pkg) = pkg else { continue };
            let name = pkg.file_name();
            if name.to_string_lossy().starts_with("joltc-sys-0.3.1") {
                found.push(pkg.path());
            }
        }
    }
    found.sort();
    found.pop()
}

fn main() {
    println!("cargo:rerun-if-changed=src/shim.cpp");
    let src = joltc_src().expect("joltc-sys 0.3.1 sources in the cargo registry");
    let jolt = src.join("JoltC").join("JoltPhysics");
    // the same configuration joltc-sys builds Jolt with (Release, debug renderer and profiler
    // on, 16 bit object layers, AVX2 feature set), so the PhysicsSystem layout matches
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .file("src/shim.cpp")
        .include(&jolt)
        .define("NDEBUG", None)
        .define("JPH_DEBUG_RENDERER", None)
        .define("JPH_PROFILE_ENABLED", None)
        .define("JPH_OBJECT_LAYER_BITS", "16");
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("x86_64") {
        for d in [
            "JPH_USE_SSE4_1",
            "JPH_USE_SSE4_2",
            "JPH_USE_AVX",
            "JPH_USE_AVX2",
            "JPH_USE_LZCNT",
            "JPH_USE_TZCNT",
            "JPH_USE_F16C",
            "JPH_USE_FMADD",
        ] {
            build.define(d, None);
        }
        for f in ["-mavx2", "-mbmi", "-mpopcnt", "-mlzcnt", "-mf16c", "-mfma"] {
            build.flag(f);
        }
    }
    build.warnings(false).compile("gm_physics_shim");
    // joltc-sys builds Jolt as a static C++ library but does not link the C++ runtime
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=stdc++");
    }
}
