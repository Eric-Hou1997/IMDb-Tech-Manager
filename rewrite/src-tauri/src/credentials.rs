use product_core::{services::CredentialStore, AppError, Result};
pub struct NativeCredentials {
    service: String,
}
impl NativeCredentials {
    pub fn new(service: String) -> Self {
        Self { service }
    }
    fn entry(&self, account: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, account)
            .map_err(|e| AppError::new("credential-store", e))
    }
}
impl CredentialStore for NativeCredentials {
    fn get(&self, account: &str) -> Result<Option<String>> {
        match self.entry(account)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AppError::new("credential-read", e)),
        }
    }
    fn put(&self, account: &str, secret: &str) -> Result<()> {
        self.entry(account)?
            .set_password(secret)
            .map_err(|e| AppError::new("credential-write", e))
    }
    fn delete(&self, account: &str) -> Result<()> {
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AppError::new("credential-delete", e)),
        }
    }
}

/// Imported ITM profiles reference the original Keychain entry without copying
/// or deleting it. New secrets are always owned by the current app namespace.
pub struct AiCredentials {
    current: NativeCredentials,
}
impl AiCredentials {
    pub fn new(service: String) -> Self {
        Self {
            current: NativeCredentials::new(service),
        }
    }
}
impl CredentialStore for AiCredentials {
    fn get(&self, account: &str) -> Result<Option<String>> {
        if account == product_core::migration::ai_profile::LEGACY_ACCOUNT {
            #[cfg(target_os = "macos")]
            {
                return NativeCredentials::new("local.imdb-tech-manager.ai".into()).get("api-key");
            }
            #[cfg(not(target_os = "macos"))]
            {
                return Ok(None);
            }
        }
        self.current.get(account)
    }
    fn put(&self, account: &str, secret: &str) -> Result<()> {
        if account == product_core::migration::ai_profile::LEGACY_ACCOUNT {
            return Err(AppError::new(
                "legacy-credential-protected",
                "Existing legacy credential must not be overwritten",
            ));
        }
        self.current.put(account, secret)
    }
    fn delete(&self, account: &str) -> Result<()> {
        if account == product_core::migration::ai_profile::LEGACY_ACCOUNT {
            return Err(AppError::new(
                "legacy-credential-protected",
                "Existing legacy credential must not be removed",
            ));
        }
        self.current.delete(account)
    }
}
