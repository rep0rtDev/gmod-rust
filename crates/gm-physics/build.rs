fn main() {
    // joltc-sys builds Jolt as a static C++ library but does not link the C++ runtime
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-lib=stdc++");
    }
}
