mod app;
mod args;
mod presentation;
mod progress;
fn main() {
    // Return from the application first so its guards and sensitive values are
    // dropped before exiting. Windows native statuses require all 32 bits.
    let code = app::start();
    std::process::exit(code);
}
