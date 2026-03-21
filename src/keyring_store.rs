#![allow(dead_code)]

use log::{error, info};

const SERVICE: &str = "arai";
const ACCOUNT: &str = "openai_api_key";

/// Retrieves the OpenAI API key from the OS keyring.
/// Returns `None` if the credential doesn't exist or the keyring is unavailable.
pub fn get_api_key() -> Option<String> {
    match keyring::Entry::new(SERVICE, ACCOUNT) {
        Ok(entry) => match entry.get_password() {
            Ok(key) if !key.is_empty() => Some(key),
            Ok(_) => None,
            Err(keyring::Error::NoEntry) => None,
            Err(e) => {
                error!("Failed to read API key from keyring: {e}");
                None
            }
        },
        Err(e) => {
            error!("Failed to create keyring entry: {e}");
            None
        }
    }
}

/// Stores the OpenAI API key in the OS keyring.
pub fn set_api_key(key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;
    entry
        .set_password(key)
        .map_err(|e| format!("Failed to store API key in keyring: {e}"))?;
    info!("API key stored in keyring");
    Ok(())
}

/// Deletes the OpenAI API key from the OS keyring.
pub fn delete_api_key() -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT)
        .map_err(|e| format!("Failed to create keyring entry: {e}"))?;
    match entry.delete_credential() {
        Ok(()) => {
            info!("API key deleted from keyring");
            Ok(())
        }
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Failed to delete API key from keyring: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_set() {
        assert_eq!(SERVICE, "arai");
        assert_eq!(ACCOUNT, "openai_api_key");
    }
}
