/// Secure credential storage backed by the OS credential manager.
///
/// On Windows this uses the Windows Credential Manager (DPAPI-encrypted).
/// Sensitive values (API keys) are stored here rather than in plaintext SQLite
/// so they survive a DB wipe and are protected by the user's Windows session.
use keyring::Entry;

const SERVICE: &str = "zanpos";

/// Retrieve a secret. Returns `None` if not set or on any error.
pub fn get_secret(key: &str) -> Option<String> {
    match Entry::new(SERVICE, key) {
        Ok(entry) => match entry.get_password() {
            Ok(v) => Some(v),
            Err(keyring::Error::NoEntry) => None, // Not set — silent is fine
            Err(e) => {
                tracing::warn!("keyring: read failed for '{}': {}", key, e);
                None
            }
        },
        Err(e) => {
            tracing::warn!("keyring: failed to open entry for '{}': {}", key, e);
            None
        }
    }
}

/// Store (or update) a secret in the OS credential store.
/// Returns false if the underlying OS call fails (non-fatal — we log and continue).
pub fn set_secret(key: &str, value: &str) -> bool {
    let entry = match Entry::new(SERVICE, key) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("keyring: failed to open entry for '{}': {}", key, e);
            return false;
        }
    };
    if entry.set_password(value).is_err() {
        tracing::error!("keyring: write failed for '{}'", key);
        return false;
    }
    // Read-back verification
    match entry.get_password() {
        Ok(stored) if stored == value => true,
        Ok(_) => {
            tracing::error!("keyring: readback mismatch for '{}'", key);
            false
        }
        Err(e) => {
            tracing::error!("keyring: readback failed for '{}': {}", key, e);
            false
        }
    }
}

/// Delete a secret from the OS credential store (e.g. when switching provider).
/// Silently ignores errors.
#[allow(dead_code)]
pub fn delete_secret(key: &str) {
    if let Ok(entry) = Entry::new(SERVICE, key) {
        let _ = entry.delete_password();
    }
}
