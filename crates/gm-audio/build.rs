//! Copies Steam Audio's shared library from `STEAMAUDIO_LIB_DIR` (the SDK's
//! `lib/<platform>` directory; `audionimbus-sys` links against the same variable) next to
//! the built binaries, where the workspace's rpath finds it at run time. The SDK is not
//! in the tree: it is Valve's Apache 2.0 download, fetched by the developer or CI.

use std::path::PathBuf;

fn main() {
    println!("cargo::rerun-if-env-changed=STEAMAUDIO_LIB_DIR");
    let Ok(lib_dir) = std::env::var("STEAMAUDIO_LIB_DIR") else {
        return;
    };
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    // target/<profile>/build/gm-audio-<hash>/out -> target/<profile>
    let Some(profile_dir) = out.ancestors().nth(3) else {
        return;
    };
    for name in ["libphonon.so", "libphonon.dylib", "phonon.dll"] {
        let src = PathBuf::from(&lib_dir).join(name);
        if src.is_file() {
            let dst = profile_dir.join(name);
            let stale = dst
                .metadata()
                .and_then(|d| Ok((d.len(), src.metadata()?.len())))
                .map(|(a, b)| a != b)
                .unwrap_or(true);
            if stale {
                if let Err(e) = std::fs::copy(&src, &dst) {
                    println!("cargo::warning=copy {name} to {}: {e}", dst.display());
                }
            }
        }
    }
}
