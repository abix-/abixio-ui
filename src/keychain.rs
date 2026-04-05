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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cred_name() -> String {
        format!(
            "__test_abixio_ui_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        )
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let name = test_cred_name();
        let result = get_keys(&name).unwrap();
        assert!(result.is_none());
    }

    // keychain store/get tests require an interactive desktop session.
    // on CI or headless environments, the keyring crate may silently
    // fail to persist credentials. these tests are ignored by default
    // and can be run with `cargo test -- --ignored`.

    #[test]
    #[ignore]
    fn store_and_get_round_trip() {
        let name = test_cred_name();
        store_keys(&name, "AKID_TEST", "SECRET_TEST_12345678").unwrap();
        let keys = get_keys(&name).unwrap();
        assert_eq!(
            keys,
            Some(("AKID_TEST".to_string(), "SECRET_TEST_12345678".to_string()))
        );
        delete_keys(&name).unwrap();
    }

    #[test]
    fn delete_nonexistent_is_ok() {
        let name = test_cred_name();
        let result = delete_keys(&name);
        assert!(result.is_ok());
    }

    #[test]
    #[ignore]
    fn delete_removes_keys() {
        let name = test_cred_name();
        store_keys(&name, "AKID_DEL", "SECRET_DEL_12345678").unwrap();
        delete_keys(&name).unwrap();
        let keys = get_keys(&name).unwrap();
        assert!(keys.is_none());
    }

    #[test]
    #[ignore]
    fn overwrite_existing_keys() {
        let name = test_cred_name();
        store_keys(&name, "OLD_AK", "OLD_SECRET_12345678").unwrap();
        store_keys(&name, "NEW_AK", "NEW_SECRET_12345678").unwrap();
        let keys = get_keys(&name).unwrap();
        assert_eq!(
            keys,
            Some(("NEW_AK".to_string(), "NEW_SECRET_12345678".to_string()))
        );
        delete_keys(&name).unwrap();
    }

    #[test]
    fn label_format() {
        assert_eq!(access_key_label("myconn"), "myconn.access-key");
        assert_eq!(secret_key_label("myconn"), "myconn.secret-key");
    }
}
