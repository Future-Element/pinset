fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            println!("cargo:rustc-link-arg-bin=pinset=/STACK:4194304");
        } else {
            println!("cargo:rustc-link-arg-bin=pinset=-Wl,--stack,4194304");
        }
    }
}
