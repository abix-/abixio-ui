use keyring::Entry;

const SERVICE: &str = "abixio-ui";

fn access_key_label(cred_name: &str) -> String {
    format!("{}.access-key", cred_name)
}

fn secret_key_label(cred_name: &str) -> String {
    format!("{}.secret-key", cred_name)
}

pub fn store_keys(cred_name: &str, access_key: &str, secret_key: &str) -> Result<(), String> {
    let ak_entry = Entry::new(SERVICE, &access_key_label(cred_name)).map_err(|e| e.to_string())?;
    ak_entry
        .set_password(access_key)
        .map_err(|e| e.to_string())?;

    let sk_entry = Entry::new(SERVICE, &secret_key_label(cred_name)).map_err(|e| e.to_string())?;
    sk_entry
        .set_password(secret_key)
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn get_keys(cred_name: &str) -> Result<Option<(String, String)>, String> {
    let ak_entry = Entry::new(SERVICE, &access_key_label(cred_name)).map_err(|e| e.to_string())?;
    let access_key = match ak_entry.get_password() {
        Ok(v) => v,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };

    let sk_entry = Entry::new(SERVICE, &secret_key_label(cred_name)).map_err(|e| e.to_string())?;
    let secret_key = match sk_entry.get_password() {
        Ok(v) => v,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };

    Ok(Some((access_key, secret_key)))
}

pub fn delete_keys(cred_name: &str) -> Result<(), String> {
    for label in [access_key_label(cred_name), secret_key_label(cred_name)] {
        let entry = Entry::new(SERVICE, &label).map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}
