//! Add components to an installation.

use std::env::consts::EXE_SUFFIX;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::bail;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use tempfile::TempDir;

use crate::components::Components;
use crate::dirs::sprocket_bin_dir;
use crate::dirs::sprocketup_bin_dir;
use crate::downloads::Downloader;
use crate::manifest::Component;
use crate::manifest::Manifest;

/// The format of an archive.
enum ArchiveFormat {
    /// .tar.gz
    TarGz,
    /// .zip
    Zip,
}

/// A component pending a download.
struct ComponentToDownload<'a> {
    /// The component to download.
    component: &'a Component,
    /// The name of the component.
    name: String,
    /// The filename of the component on the remote.
    filename: &'a str,
    /// The format of the target archive.
    archive_format: ArchiveFormat,
}

/// Options to control a component installation.
pub(crate) struct ComponentInstallOptions {
    /// The target manifest for the profile.
    pub(crate) manifest: Manifest,
    /// The base of the profile.
    pub(crate) profile_base: PathBuf,
    /// The binary directory of the profile.
    pub(crate) bin_dir: PathBuf,
    /// Whether to link the binaries to the `sprocket` binary directory.
    pub(crate) link: bool,
}

/// Add one or more components to the current installation.
pub async fn add_components(
    mut components: Vec<String>,
    options: Option<ComponentInstallOptions>,
) -> anyhow::Result<()> {
    let mut local_components = Components::load(
        options
            .as_ref()
            .map(|ComponentInstallOptions { profile_base, .. }| profile_base.clone()),
    )?;

    components.retain(|requested| {
        if local_components
            .list()
            .iter()
            .any(|component| component.name == **requested)
        {
            tracing::warn!("component '{requested}' already installed, skipping");
            return false;
        }

        true
    });

    if components.is_empty() {
        tracing::info!("nothing to do");
        return Ok(());
    }

    let (manifest, bin_dir, link) = match options {
        Some(ComponentInstallOptions {
            manifest,
            bin_dir,
            link,
            ..
        }) => (manifest, bin_dir, link),
        None => (Manifest::load()?, sprocketup_bin_dir()?, true),
    };

    let components_to_download = components
        .iter()
        .map(|component_name| {
            let Some(component) = manifest.component(component_name) else {
                bail!("component '{component_name}' does not exist");
            };

            let Some(filename) = component.src.path_segments().and_then(Iterator::last) else {
                bail!("malformed component source");
            };

            let archive_format;
            if filename.ends_with(".tar.gz") {
                archive_format = ArchiveFormat::TarGz;
            } else if filename.ends_with(".zip") {
                archive_format = ArchiveFormat::Zip;
            } else {
                bail!("Unexpected archive format '{filename}'")
            }

            Ok(ComponentToDownload {
                component,
                name: component_name.to_string(),
                filename,
                archive_format,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let tmp = TempDir::with_prefix("sprocketup")?;
    let downloader = Arc::new(Downloader::new());
    let mut download_tasks = FuturesUnordered::new();
    for component in components_to_download {
        let downloader = downloader.clone();
        let bin_dir = bin_dir.clone();
        let tmp_dir = tmp.path().to_path_buf();
        let task = async move {
            let archive_dest = tmp_dir.join(component.filename);

            let install_status = downloader
                .download(component.component.src.clone())
                .destination(&archive_dest)
                .identifier(&component.name)
                .hash(&component.component.hash)
                .start()
                .await
                .context(format!("failed to download component '{}'", component.name))?;

            let unpack = install_status.unpack(BufReader::new(File::open(&archive_dest)?));
            match component.archive_format {
                ArchiveFormat::TarGz => {
                    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(unpack));
                    archive.unpack(&bin_dir)?;
                }
                ArchiveFormat::Zip => {
                    let mut archive = zip::ZipArchive::new(unpack)?;
                    archive.extract(&bin_dir)?;
                }
            }

            install_status.installed();

            Ok::<_, anyhow::Error>(())
        };

        download_tasks.push(task);
    }

    while let Some(result) = download_tasks.next().await {
        result?;
    }

    let mut manifest_tx = local_components.tx();

    if link {
        link_components_to_sprocket(&bin_dir, &components)?;
    }

    for component in components {
        manifest_tx.installed(component);
    }
    manifest_tx.commit()?;

    Ok(())
}

/// Create a symlink at `dest`, pointing to `src`.
fn symlink_file(src: &Path, dest: &Path) -> anyhow::Result<()> {
    let _ = std::fs::remove_file(dest);

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dest)?;
    }

    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(src, dest)?;
    }

    Ok(())
}

/// Link the binaries in the current `sprocketup` profile into the `sprocket` binary directory.
///
/// This will create the links even if the `sprocket` component itself isn't installed.
pub(crate) fn link_components_to_sprocket(
    sprocketup_bin_dir: &Path,
    components: &[String],
) -> anyhow::Result<()> {
    let sprocket_bin_dir = sprocket_bin_dir()?;
    if let Err(e) = std::fs::create_dir_all(&sprocket_bin_dir)
        && e.kind() != std::io::ErrorKind::AlreadyExists
    {
        return Err(anyhow::anyhow!(e));
    }

    for component in components {
        let component_binary = format!("{component}{EXE_SUFFIX}");
        symlink_file(
            &sprocketup_bin_dir.join(&component_binary),
            &sprocket_bin_dir.join(&component_binary),
        )?;
    }

    Ok(())
}
