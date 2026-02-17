//! `sprocketup` profile initialization.

use std::fmt::Display;
use std::path::Path;
use std::path::PathBuf;

use anyhow::bail;
use bytes::Bytes;
use clap::ValueEnum;
use tempfile::TempDir;

use crate::components::ComponentInstallOptions;
use crate::dirs::BIN_DIR_NAME;
use crate::dirs::CURRENT_PROFILE_NAME;
use crate::dirs::current_profile_dir;
use crate::manifest::MANIFEST_FILE_NAME;
use crate::manifest::Manifest;

/// The installation profile.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, ValueEnum)]
pub enum Profile {
    /// A "bare minimum" installation.
    Minimal,
    /// The default installation, with most components.
    #[default]
    Default,
    /// Full installation with all available components.
    Full,
}

impl Profile {
    /// Get the profile name as a string.
    fn as_str(self) -> &'static str {
        match self {
            Profile::Minimal => "minimal",
            Profile::Default => "default",
            Profile::Full => "full",
        }
    }
}

impl Display for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Initialize a new `sprocket` profile.
pub async fn init(target_profile: Profile) -> anyhow::Result<()> {
    let output = current_profile_dir()?;
    if output.exists() {
        bail!("profile already exists at '{}'", output.display());
    }

    let (manifest_bytes, manifest) = Manifest::latest().await?;
    let Some(profile) = manifest.profile(target_profile.as_str()) else {
        bail!("profile '{target_profile}' does not exist in the manifest");
    };

    tracing::info!("Installing profile '{target_profile}'...");
    let profile_dir = setup_bare_profile(manifest_bytes)?;

    let components_to_install = profile.components().to_vec();
    crate::components::add_components(
        components_to_install.clone(),
        Some(ComponentInstallOptions {
            manifest,
            bin_dir: profile_dir.bin_dir.clone(),
            profile_base: profile_dir.profile_base.clone(),
            // Need to link manually, since we're installing to a temp directory
            link: false,
        }),
    )
    .await?;

    copy_dir(&profile_dir.profile_base, &output)?;

    let profile_bin_dir = output.join(BIN_DIR_NAME);
    crate::components::link_components_to_sprocket(&profile_bin_dir, &components_to_install)?;

    tracing::info!(
        "Profile '{target_profile}' installed to '{}'",
        output.display()
    );

    Ok(())
}

/// A temporary profile directory.
struct ProfileDir {
    /// The backing tempdir, held to prevent it closing.
    _dir: TempDir,
    /// The binary directory.
    bin_dir: PathBuf,
    /// The base directory of the profile.
    profile_base: PathBuf,
}

/// Setup a new, empty profile directory.
fn setup_bare_profile(manifest_bytes: Bytes) -> anyhow::Result<ProfileDir> {
    let tmp = TempDir::with_prefix("sprocketup")?;

    let profile_base = tmp.path().join(CURRENT_PROFILE_NAME);
    std::fs::create_dir_all(&profile_base)?;
    std::fs::write(profile_base.join(MANIFEST_FILE_NAME), manifest_bytes)?;

    let bin_dir = profile_base.join(BIN_DIR_NAME);
    std::fs::create_dir_all(&bin_dir)?;
    std::fs::File::create(profile_base.join("components"))?;

    Ok(ProfileDir {
        _dir: tmp,
        bin_dir,
        profile_base,
    })
}

/// Copy the contents of `src` into `dest`, recursively.
fn copy_dir(src: &Path, dest: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in src.read_dir()? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let src = entry.path();
        let dest = dest.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&src, &dest)?;
        } else {
            std::fs::copy(&src, &dest)?;
        }
    }
    Ok(())
}
