/// Registry reference parsing and normalization.
mod reference;

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use path_clean::PathClean;

pub use reference::RegistryReference;
pub use reference::RegistryTransport;
use reference::validate_sha256;

use crate::Inputs;
use crate::Object;
use crate::v1::requirements;
use crate::v1::requirements::ContainerSource;

/// An immutable runtime container lock policy.
#[derive(Debug)]
pub struct ContainerLock {
    /// The absolute path to the lock file.
    path: PathBuf,
    /// Canonical mutable image references mapped to immutable sources.
    images: BTreeMap<String, ContainerSource>,
    /// Normalized SIF file paths mapped to expected sha256 digests.
    sif_files: BTreeMap<String, String>,
    /// Verified SIF file paths cached by normalized key.
    verified_sif_files: tokio::sync::Mutex<HashMap<String, PathBuf>>,
}

impl ContainerLock {
    /// Creates a container lock policy from image and SIF entries.
    pub fn try_new(
        path: impl Into<PathBuf>,
        images: impl IntoIterator<Item = (String, String)>,
        sif_files: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self> {
        let path = std::path::absolute(path.into())
            .context("failed to make the container lock path absolute")?
            .clean();
        let mut normalized_images = BTreeMap::new();
        for (key, value) in images {
            // SAFETY: `FromStr` for `ContainerSource` is infallible.
            let key_source: ContainerSource = key.parse().unwrap();
            // SAFETY: `FromStr` for `ContainerSource` is infallible.
            let value_source: ContainerSource = value.parse().unwrap();
            let key = RegistryReference::try_from_source(&key_source)?;
            let value = RegistryReference::try_from_source(&value_source)?;
            anyhow::ensure!(!key.is_immutable(), "lock key `{key}` must be mutable");
            anyhow::ensure!(
                value.is_immutable(),
                "lock value `{value}` must use a digest"
            );
            anyhow::ensure!(
                key.transport() == value.transport()
                    && key.registry() == value.registry()
                    && key.repository() == value.repository(),
                "locked image `{key}` changes transport, registry, or repository to `{value}`"
            );
            let canonical = key.canonical();
            anyhow::ensure!(
                normalized_images
                    .insert(canonical.clone(), value.to_container_source())
                    .is_none(),
                "duplicate canonical image key `{canonical}`"
            );
        }

        let mut normalized_sifs = BTreeMap::new();
        for (path, digest) in sif_files {
            validate_sha256(&digest)?;
            let path = normalize_sif_key(Path::new(&path));
            anyhow::ensure!(
                normalized_sifs.insert(path.clone(), digest).is_none(),
                "duplicate normalized SIF path `{path}`"
            );
        }

        Ok(Self {
            path,
            images: normalized_images,
            sif_files: normalized_sifs,
            verified_sif_files: Default::default(),
        })
    }

    /// Gets the absolute path to the lock file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Resolves a container source through the lock policy.
    pub async fn resolve(&self, source: &ContainerSource) -> Result<ContainerSource> {
        match source {
            ContainerSource::Docker(_) | ContainerSource::Oras(_) => {
                let reference = RegistryReference::try_from_source(source)?;
                if reference.is_immutable() {
                    return Ok(reference.to_container_source());
                }
                self.images
                    .get(&reference.canonical())
                    .cloned()
                    .with_context(|| {
                        format!(
                            "container `{reference}` is not present in `{}`",
                            self.path.display()
                        )
                    })
            }
            ContainerSource::SifFile(path) => self.resolve_sif(path).await,
            ContainerSource::Library(_) | ContainerSource::Unknown(_) => {
                anyhow::bail!(
                    "unsupported mutable container source `{source:#}` while using `{}`",
                    self.path.display()
                )
            }
        }
    }

    /// Resolves all container source candidates through the lock policy.
    pub async fn preflight(&self, sources: &[ContainerSource]) -> Result<Vec<ContainerSource>> {
        let mut resolved = Vec::with_capacity(sources.len());
        for source in sources {
            resolved.push(self.resolve(source).await?);
        }
        Ok(resolved)
    }

    /// Applies the lock policy to container overrides in nested inputs.
    pub async fn preflight_inputs(
        &self,
        inputs: &Inputs,
        default: &str,
        target: &str,
    ) -> Result<()> {
        let mut pending = vec![(target.to_string(), inputs)];
        while let Some((path, inputs)) = pending.pop() {
            match inputs {
                Inputs::Task(task) => {
                    if task.requirement("container").is_some()
                        || task.requirement("docker").is_some()
                    {
                        let sources = requirements::container(task, &Object::empty(), default);
                        self.preflight(&sources).await.with_context(|| {
                            format!("failed to apply container lock to input override for `{path}`")
                        })?;
                    }
                }
                Inputs::Workflow(workflow) => pending.extend(
                    workflow
                        .calls()
                        .iter()
                        .map(|(name, inputs)| (format!("{path}.{name}"), inputs)),
                ),
            }
        }
        Ok(())
    }

    /// Resolves and verifies a SIF file through the lock policy.
    async fn resolve_sif(&self, path: &Path) -> Result<ContainerSource> {
        let key = normalize_sif_key(path);
        if let Some(path) = self.verified_sif_files.lock().await.get(&key).cloned() {
            return Ok(ContainerSource::SifFile(path));
        }

        let expected = self
            .sif_files
            .get(&key)
            .with_context(|| {
                format!(
                    "SIF file `{key}` is not present in `{}`",
                    self.path.display()
                )
            })?
            .clone();
        let path = if path.is_absolute() {
            path.clean()
        } else {
            self.path
                .parent()
                .context("an absolute lock path has no parent")?
                .join(path)
                .clean()
        };
        let actual = sha256_file(&path).await?;
        anyhow::ensure!(
            actual == expected,
            "SIF file `{}` checksum does not match `{}`",
            path.display(),
            self.path.display()
        );
        self.verified_sif_files
            .lock()
            .await
            .insert(key, path.clone());
        Ok(ContainerSource::SifFile(path))
    }
}

impl PartialEq for ContainerLock {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && self.images == other.images && self.sif_files == other.sif_files
    }
}

impl Eq for ContainerLock {}

/// Calculates a sha256 digest for a file.
pub async fn sha256_file(path: &Path) -> Result<String> {
    use sha2::Digest;
    use tokio::io::AsyncReadExt;

    let mut file = tokio::fs::File::open(path)
        .await
        .with_context(|| format!("failed to open SIF file `{}`", path.display()))?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .with_context(|| format!("failed to read SIF file `{}`", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest: [u8; 32] = hasher.finalize().into();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity("sha256:".len() + digest.len() * 2);
    encoded.push_str("sha256:");
    for byte in digest {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(encoded)
}

/// Normalizes a SIF path for lock lookup.
fn normalize_sif_key(path: &Path) -> String {
    path.clean().to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TaskInputs;
    use crate::WorkflowInputs;
    use crate::v1::requirements::ContainerSource;

    #[tokio::test]
    async fn resolves_mutable_image_to_locked_digest() {
        let digest = format!("sha256:{}", "a".repeat(64));
        let lock = ContainerLock::try_new(
            "/workspace/sprocket.lock",
            [(
                "docker://docker.io/library/ubuntu:24.04".to_string(),
                format!("docker://docker.io/library/ubuntu@{digest}"),
            )],
            [],
        )
        .unwrap();

        let resolved = lock
            .resolve(&ContainerSource::Docker("ubuntu:24.04".into()))
            .await
            .unwrap();
        assert_eq!(
            resolved,
            ContainerSource::Docker(format!("docker.io/library/ubuntu@{digest}"))
        );
    }

    #[tokio::test]
    async fn rejects_missing_mutable_image() {
        let lock = ContainerLock::try_new("/workspace/sprocket.lock", [], []).unwrap();
        let error = lock
            .resolve(&ContainerSource::Docker("ubuntu:24.04".into()))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("is not present in"));
    }

    #[test]
    fn rejects_mapping_to_another_repository() {
        let digest = format!("sha256:{}", "a".repeat(64));
        let error = ContainerLock::try_new(
            "/workspace/sprocket.lock",
            [(
                "docker://docker.io/library/ubuntu:24.04".to_string(),
                format!("docker://ghcr.io/example/other@{digest}"),
            )],
            [],
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("changes transport, registry, or repository")
        );
    }

    #[tokio::test]
    async fn verifies_relative_sif_against_lock_directory() {
        let root = tempfile::tempdir().unwrap();
        tokio::fs::write(root.path().join("tool.sif"), b"sif contents")
            .await
            .unwrap();
        let digest = sha256_file(&root.path().join("tool.sif")).await.unwrap();
        let lock = ContainerLock::try_new(
            root.path().join("sprocket.lock"),
            [],
            [("tool.sif".to_string(), digest)],
        )
        .unwrap();

        let resolved = lock
            .resolve(&ContainerSource::SifFile("tool.sif".into()))
            .await
            .unwrap();
        assert_eq!(
            resolved,
            ContainerSource::SifFile(root.path().join("tool.sif"))
        );
    }

    #[tokio::test]
    async fn rejects_changed_sif() {
        let root = tempfile::tempdir().unwrap();
        tokio::fs::write(root.path().join("tool.sif"), b"before")
            .await
            .unwrap();
        let digest = sha256_file(&root.path().join("tool.sif")).await.unwrap();
        let lock = ContainerLock::try_new(
            root.path().join("sprocket.lock"),
            [],
            [("tool.sif".to_string(), digest)],
        )
        .unwrap();
        tokio::fs::write(root.path().join("tool.sif"), b"after")
            .await
            .unwrap();

        let error = lock
            .resolve(&ContainerSource::SifFile("tool.sif".into()))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("checksum does not match"));
    }

    #[tokio::test]
    async fn rejects_unsupported_sources() {
        let lock = ContainerLock::try_new("/workspace/sprocket.lock", [], []).unwrap();
        let error = lock
            .resolve(&ContainerSource::Library("sylabs/default/alpine".into()))
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unsupported mutable container source")
        );
    }

    #[tokio::test]
    async fn preflights_nested_input_overrides() {
        let digest = format!("sha256:{}", "a".repeat(64));
        let lock = ContainerLock::try_new(
            "/workspace/sprocket.lock",
            [(
                "docker://docker.io/library/ubuntu:24.04".to_string(),
                format!("docker://docker.io/library/ubuntu@{digest}"),
            )],
            [],
        )
        .unwrap();
        let mut task = TaskInputs::default();
        task.override_requirement("container", "ubuntu:24.04".to_string());
        let mut workflow = WorkflowInputs::default();
        workflow
            .calls_mut()
            .insert("call".into(), crate::Inputs::Task(task));

        lock.preflight_inputs(
            &crate::Inputs::Workflow(workflow),
            "ubuntu:latest",
            "workflow",
        )
        .await
        .unwrap();
    }
}
