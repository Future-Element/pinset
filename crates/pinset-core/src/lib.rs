//! Pinset 3 domain, strict file protocol and read-only execution planning.
mod config;
mod environment_protocol;
mod error;
mod integrity;
mod model;
mod routing;
mod target;
mod work_directory;
pub use config::*;
pub use environment_protocol::*;
pub use error::*;
pub use integrity::*;
pub use model::*;
pub use routing::*;
pub use target::*;
pub use work_directory::*;

pub fn pinset_version() -> &'static str {
    option_env!("PINSET_RELEASE_VERSION")
        .unwrap_or(env!("CARGO_PKG_VERSION"))
        .trim_start_matches('v')
}
