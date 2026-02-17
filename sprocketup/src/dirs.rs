//! Utilities for interacting with `sprocket`/`sprocketup` directories.

use std::path::PathBuf;

use anyhow::bail;

/// The name of the directory holding the active profile.
pub(crate) const CURRENT_PROFILE_NAME: &str = "current";
/// The name of the directory holding the installed binaries.
pub(crate) const BIN_DIR_NAME: &str = "bin";

/// Get the `sprocketup` data directory.
pub(crate) fn sprocketup_data_dir() -> anyhow::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("SPROCKETUP_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }

    if let Some(data_dir) = dirs::data_dir() {
        return Ok(data_dir.join("sprocketup"));
    }

    if let Some(home_dir) = dirs::home_dir() {
        return Ok(home_dir.join(".sprocketup"));
    }

    bail!("Unable to determine sprocketup data directory");
}

/// Get the path to the current profile.
pub(crate) fn current_profile_dir() -> anyhow::Result<PathBuf> {
    Ok(sprocketup_data_dir()?.join(CURRENT_PROFILE_NAME))
}

/// Get the `sprocketup` binary components directory.
pub(crate) fn sprocketup_bin_dir() -> anyhow::Result<PathBuf> {
    Ok(current_profile_dir()?.join(BIN_DIR_NAME))
}

/// Get the `sprocket` data directory.
pub(crate) fn sprocket_data_dir() -> anyhow::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("SPROCKET_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }

    if let Some(data_dir) = dirs::data_dir() {
        return Ok(data_dir.join("sprocket"));
    }

    if let Some(home_dir) = dirs::home_dir() {
        return Ok(home_dir.join(".sprocket"));
    }

    bail!("Unable to determine sprocket data directory");
}

/// Get the `sprocket` binary components directory.
pub(crate) fn sprocket_bin_dir() -> anyhow::Result<PathBuf> {
    Ok(sprocket_data_dir()?.join(BIN_DIR_NAME))
}
