use keyring::Entry;

const SERVICE: &str = "abixio-ui";

pub fn store_secret(credential_name: &str, secret_key: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE, credential_name).map_err(|e| e.to_string())?;
    entry.set_password(secret_key).map_err(|e| e.to_string())
}

pub fn get_secret(credential_name: &str) -> Result<Option<String>, String> {
    let entry = Entry::new(SERVICE, credential_name).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(pw) => Ok(Some(pw)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn delete_secret(credential_name: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE, credential_name).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
