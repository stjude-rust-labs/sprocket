/// Can we find an executable of the given name on the global (not counting
/// `cwd`) path?
fn can_find_binary(binary_name: impl AsRef<std::ffi::OsStr>) -> Result<bool, anyhow::Error> {
    match which::which_global(binary_name) {
        Ok(_) => Ok(true),
        Err(which::Error::CannotFindBinaryPath) => Ok(false),
        Err(e) => Err(e)?,
    }
}

/// Are LSF and Apptainer tools available?
pub fn lsf_apptainer_available() -> Result<bool, anyhow::Error> {
    let lsf_available = can_find_binary("bsub")?;
    let apptainer_available = can_find_binary("apptainer")?;
    Ok(lsf_available && apptainer_available)
}
