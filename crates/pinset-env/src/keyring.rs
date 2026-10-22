//! Initialize the same platform credential stores used by keyring v4's v1 API.
//! Both Secret Service transports identify existing items by service and username.

use std::sync::LazyLock;

pub(crate) use keyring_core::Error;

pub(crate) struct Entry(keyring_core::Entry);

impl Entry {
    pub(crate) fn new(service: &str, username: &str) -> keyring_core::Result<Self> {
        if STORE.is_err() {
            return Err(Error::NoDefaultStore);
        }
        keyring_core::Entry::new(service, username).map(Self)
    }

    pub(crate) fn set_secret(&self, secret: &[u8]) -> keyring_core::Result<()> {
        self.0.set_secret(secret)
    }

    pub(crate) fn get_secret(&self) -> keyring_core::Result<Vec<u8>> {
        self.0.get_secret()
    }
}

static STORE: LazyLock<keyring_core::Result<()>> = LazyLock::new(initialize);

fn initialize() -> keyring_core::Result<()> {
    #[cfg(target_os = "macos")]
    let store = apple_native_keyring_store::keychain::Store::new()?;
    #[cfg(target_os = "windows")]
    let store = windows_native_keyring_store::Store::new()?;
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    let store = dbus_secret_service_keyring_store::Store::new()?;
    #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
    {
        keyring_core::set_default_store(store);
        Ok(())
    }
    #[cfg(not(all(any(unix, windows), not(any(target_os = "ios", target_os = "android")))))]
    Err(Error::Invalid(
        "platform".to_owned(),
        "requires macOS, Windows, or a supported Unix Secret Service".to_owned(),
    ))
}
