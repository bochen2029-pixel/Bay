//! Thin wrapper around the `keyring` crate for the LLM API key. On
//! Windows this uses Credential Manager, on macOS the Keychain, on
//! Linux libsecret. Key name pinned per SPEC §6 `keychain.rs` role.
//!
//! The API key never crosses IPC after write: `get_settings` exposes
//! only `has_api_key: bool` derived from `has_api_key()`. Only the LLM
//! client reads it internally via `get_api_key()`.

const SERVICE: &str = "bay";
const ACCOUNT_LLM: &str = "llm_api_key";

fn entry() -> Option<keyring::Entry> {
    match keyring::Entry::new(SERVICE, ACCOUNT_LLM) {
        Ok(e) => Some(e),
        Err(e) => {
            eprintln!("keychain: entry construction failed: {e}");
            None
        }
    }
}

pub fn has_api_key() -> bool {
    entry().map_or(false, |e| e.get_password().is_ok())
}

pub fn get_api_key() -> Option<String> {
    entry().and_then(|e| e.get_password().ok())
}

/// Set the key. Empty string deletes the entry.
pub fn set_api_key(key: &str) -> Result<(), String> {
    let entry = entry().ok_or_else(|| "keychain unavailable".to_string())?;
    if key.is_empty() {
        match entry.delete_credential() {
            Ok(_) => Ok(()),
            // Deleting a non-existent entry is not an error for us.
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("delete key: {e}")),
        }
    } else {
        entry
            .set_password(key)
            .map_err(|e| format!("set key: {e}"))
    }
}
