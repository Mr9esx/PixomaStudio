use crate::Result;

pub(crate) fn load(name: &str) -> Result<Option<String>> {
    let entry = keyring::Entry::new("com.pixoma.studio", name)?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn save(name: &str, value: &str) -> Result<()> {
    keyring::Entry::new("com.pixoma.studio", name)?.set_password(value)?;
    Ok(())
}

pub(crate) fn remove(name: &str) -> Result<()> {
    let entry = keyring::Entry::new("com.pixoma.studio", name)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.into()),
    }
}
