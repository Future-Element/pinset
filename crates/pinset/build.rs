fn main() {
    // clap materializes the complete multilingual, nested command graph while parsing. The
    // Windows PE default stack is only 1 MiB and schema 4 adds several nested command families.
    // Reserve 4 MiB for the CLI main thread; runtime child processes are unaffected.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg-bin=pinset=/STACK:4194304");
    }
}
