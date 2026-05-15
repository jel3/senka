use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecretsError {
    #[error("keyring error: {0}")]
    Keyring(#[from] keyring::Error),
}

/// Retrieve a secret from the OS keychain.
pub fn get(project_id: &str, env_name: &str, key: &str) -> Result<Option<String>, SecretsError> {
    let service = format!("senka.{project_id}.{env_name}");
    let entry = keyring::Entry::new(&service, key)?;
    match entry.get_password() {
        Ok(val) => Ok(Some(val)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(SecretsError::Keyring(e)),
    }
}

/// Store a secret in the OS keychain.
pub fn set(project_id: &str, env_name: &str, key: &str, value: &str) -> Result<(), SecretsError> {
    let service = format!("senka.{project_id}.{env_name}");
    let entry = keyring::Entry::new(&service, key)?;
    entry.set_password(value)?;
    Ok(())
}

/// Delete a secret from the OS keychain.
pub fn delete(project_id: &str, env_name: &str, key: &str) -> Result<(), SecretsError> {
    let service = format!("senka.{project_id}.{env_name}");
    let entry = keyring::Entry::new(&service, key)?;
    entry.delete_password()?;
    Ok(())
}
