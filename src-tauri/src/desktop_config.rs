use crate::{PlatformClient, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DesktopConfig {
    platform_url: String,
}

pub(crate) fn load_platform(
    directory: &Path,
    runtime_address: Option<&str>,
    bundled_address: Option<&str>,
) -> Result<Option<PlatformClient>> {
    let file = directory.join("desktop.json");
    if runtime_address.is_none() {
        match std::fs::read(&file) {
            Ok(bytes) => {
                let saved: DesktopConfig = serde_json::from_slice(&bytes)?;
                return Ok(Some(PlatformClient::new(&saved.platform_url)?));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let Some(address) = runtime_address.or(bundled_address) else {
        return Ok(None);
    };
    let platform = PlatformClient::new(address)?;
    let configuration = DesktopConfig {
        platform_url: address.to_owned(),
    };
    std::fs::create_dir_all(directory)?;
    let mut output = tempfile::NamedTempFile::new_in(directory)?;
    output.write_all(&serde_json::to_vec_pretty(&configuration)?)?;
    output.as_file().sync_all()?;
    output.persist(file).map_err(|error| error.error)?;
    Ok(Some(platform))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_address_survives_startup_without_environment_variables()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("desktop-config-")
            .tempdir_in(root)?;
        let address = "http://127.0.0.1:18082";
        assert!(load_platform(directory.path(), Some(address), None)?.is_some());
        assert!(load_platform(directory.path(), None, None)?.is_some());
        assert!(load_platform(directory.path(), None, Some("invalid-address"))?.is_some());
        let file = directory.path().join("desktop.json");
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&file)?)?;
        assert_eq!(saved, serde_json::json!({ "platform_url": address }));
        let bytes = std::fs::read(&file)?;
        assert!(
            load_platform(
                directory.path(),
                Some("https://user:secret@example.com"),
                None
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&file)?, bytes);
        std::fs::write(&file, b"{invalid-json}")?;
        assert!(load_platform(directory.path(), None, Some(address)).is_err());
        Ok(())
    }

    #[test]
    fn bundled_platform_address_initializes_a_separate_configuration_directory()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
        std::fs::create_dir_all(&root)?;
        let directory = tempfile::Builder::new()
            .prefix("bundled-config-")
            .tempdir_in(root)?;
        let configuration = directory.path().join("configuration");
        assert!(load_platform(&configuration, None, None)?.is_none());
        assert!(!configuration.exists());
        assert!(load_platform(&configuration, None, Some("http://127.0.0.1:18082"))?.is_some());
        assert!(configuration.join("desktop.json").is_file());
        assert!(load_platform(&configuration, Some("http://127.0.0.1:18083"), None)?.is_some());
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(configuration.join("desktop.json"))?)?;
        assert_eq!(saved["platform_url"], "http://127.0.0.1:18083");
        Ok(())
    }
}
