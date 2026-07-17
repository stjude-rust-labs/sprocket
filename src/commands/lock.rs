//! Implementation of the `lock` subcommand.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context as _;
use anyhow::Result;
use clap::Parser;
use path_clean::PathClean as _;
use wdl::diagnostics::Mode;
use wdl::engine::container_lock::RegistryReference;
use wdl::engine::container_lock::sha256_file;
use wdl::engine::v1::requirements::ContainerSource;

use crate::Config;
use crate::analysis::Analysis;
use crate::analysis::AnalysisResults;
use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::container_lock::ExtractionMode;
use crate::container_lock::LOCK_FILE_NAME;
use crate::container_lock::RegistryResolver;
use crate::container_lock::ResolveRegistryReferences;
use crate::container_lock::extract;
use crate::container_lock::serialize_version_one;
use crate::container_lock::write_atomic;

/// Arguments for the `lock` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
    /// A WDL source document, directory, or URL to scan for static containers.
    #[clap(value_name = "SOURCE")]
    pub source: Option<Source>,

    /// Directory where `sprocket.lock` is written.
    #[clap(short, long, value_name = "DIR")]
    pub output: Option<PathBuf>,

    /// The report mode for any emitted diagnostics.
    #[arg(short = 'm', long, value_name = "MODE", global = true)]
    pub report_mode: Option<Mode>,
}

/// Performs the `lock` command.
pub async fn lock(args: Args, config: Config, colorize: bool) -> CommandResult<()> {
    let report_mode = args.report_mode.unwrap_or(config.common.report_mode);
    let output_path = args
        .output
        .unwrap_or_else(|| PathBuf::from(std::path::Component::CurDir.as_os_str()))
        .join(LOCK_FILE_NAME);

    let s = args.source.unwrap_or_default();
    let results = Analysis::default()
        .add_source(s)
        .fallback_version(config.common.wdl.fallback_version.into())
        .modules_config(config.modules.clone())
        .feature_flags(config.common.wdl.feature_flags)
        .run(report_mode, colorize)
        .await
        .map_err(CommandError::from)?;

    generate_lock(
        &output_path,
        &results,
        &config.run.engine.task.container,
        &RegistryResolver::default(),
    )
    .await
    .map_err(CommandError::from)
}

async fn generate_lock<R>(
    output_path: &Path,
    results: &AnalysisResults,
    default_container: &str,
    resolver: &R,
) -> Result<()>
where
    R: ResolveRegistryReferences,
{
    let uses = extract(results, default_container, ExtractionMode::Generate)?;
    let mut registry_references = BTreeMap::new();
    let mut sif_files = BTreeMap::new();

    for usage in uses {
        match &usage.source {
            ContainerSource::Docker(_) | ContainerSource::Oras(_) => {
                let reference =
                    RegistryReference::try_from_source(&usage.source).with_context(|| {
                        format!(
                            "invalid container in task `{}` in `{}`",
                            usage.task, usage.document
                        )
                    })?;
                if !reference.is_immutable() {
                    registry_references.insert(reference.canonical(), reference);
                }
            }
            ContainerSource::SifFile(path) => {
                let key = path.clean().to_string_lossy().replace('\\', "/");
                let resolved = if path.is_absolute() {
                    path.clean()
                } else {
                    output_path
                        .parent()
                        .context("lock output path has no parent")?
                        .join(path)
                        .clean()
                };
                let digest = sha256_file(&resolved).await.with_context(|| {
                    format!(
                        "failed to hash SIF for task `{}` in `{}`",
                        usage.task, usage.document
                    )
                })?;
                sif_files.insert(key, digest);
            }
            ContainerSource::Library(_) | ContainerSource::Unknown(_) => anyhow::bail!(
                "unsupported mutable container `{:#}` in task `{}` in `{}`",
                usage.source,
                usage.task,
                usage.document
            ),
        }
    }

    let images = resolver
        .resolve_all(registry_references.into_values().collect())
        .await?;
    let contents = serialize_version_one(images, sif_files)?;
    write_atomic(output_path, &contents)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use anyhow::Result;
    use async_trait::async_trait;
    use tokio::sync::Mutex;
    use wdl::diagnostics::Mode;
    use wdl::engine::container_lock::RegistryReference;

    use super::generate_lock;
    use crate::analysis::Analysis;
    use crate::container_lock::LOCK_FILE_NAME;
    use crate::container_lock::ResolveRegistryReferences;

    struct FailingResolver;

    #[async_trait]
    impl ResolveRegistryReferences for FailingResolver {
        async fn resolve_all(&self, _: Vec<RegistryReference>) -> Result<BTreeMap<String, String>> {
            anyhow::bail!("registry unavailable")
        }
    }

    struct RecordingResolver {
        references: Arc<Mutex<Vec<RegistryReference>>>,
    }

    #[async_trait]
    impl ResolveRegistryReferences for RecordingResolver {
        async fn resolve_all(
            &self,
            references: Vec<RegistryReference>,
        ) -> Result<BTreeMap<String, String>> {
            *self.references.lock().await = references;
            Ok(BTreeMap::new())
        }
    }

    struct SortedResolver;

    #[async_trait]
    impl ResolveRegistryReferences for SortedResolver {
        async fn resolve_all(
            &self,
            references: Vec<RegistryReference>,
        ) -> Result<BTreeMap<String, String>> {
            let digest = format!("sha256:{}", "b".repeat(64));
            references
                .into_iter()
                .map(|reference| {
                    Ok((
                        reference.canonical(),
                        reference.with_digest(&digest)?.canonical(),
                    ))
                })
                .collect()
        }
    }

    async fn analyze(source_path: &std::path::Path) -> crate::analysis::AnalysisResults {
        Analysis::default()
            .add_source(source_path.to_string_lossy().parse().unwrap())
            .run(Mode::default(), false)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn generation_failure_preserves_existing_lock() {
        let root = tempfile::tempdir().unwrap();
        let source_path = root.path().join("source.wdl");
        std::fs::write(
            &source_path,
            r#"version 1.2
task hello {
    command { echo hello }
    requirements {
        container: "ubuntu:24.04"
    }
}"#,
        )
        .unwrap();
        let results = analyze(&source_path).await;
        let lock_path = root.path().join(LOCK_FILE_NAME);
        std::fs::write(&lock_path, b"old").unwrap();

        let error = generate_lock(&lock_path, &results, "ubuntu:latest", &FailingResolver)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("registry unavailable"));
        assert_eq!(std::fs::read(&lock_path).unwrap(), b"old");
    }

    #[tokio::test]
    async fn existing_digest_pins_skip_registry_resolution() {
        let root = tempfile::tempdir().unwrap();
        let source_path = root.path().join("source.wdl");
        let digest = format!("sha256:{}", "a".repeat(64));
        std::fs::write(
            &source_path,
            format!(
                r#"version 1.2
task hello {{
    command {{ echo hello }}
    requirements {{
        container: "ubuntu:24.04@{digest}"
    }}
}}"#
            ),
        )
        .unwrap();
        let results = analyze(&source_path).await;
        let references = Arc::new(Mutex::new(Vec::new()));
        let lock_path = root.path().join(LOCK_FILE_NAME);

        generate_lock(
            &lock_path,
            &results,
            "ubuntu:latest",
            &RecordingResolver {
                references: references.clone(),
            },
        )
        .await
        .unwrap();

        assert!(references.lock().await.is_empty());
        let lock = std::fs::read_to_string(&lock_path).unwrap();
        assert_eq!(
            lock,
            format!(
                "version = 1\ngeneration_time = \"{}\"\nimages = {{}}\nsif_files = {{}}\n",
                lock.lines()
                    .nth(1)
                    .unwrap()
                    .trim_start_matches("generation_time = \"")
                    .trim_end_matches('"')
            )
        );
    }

    #[tokio::test]
    async fn writes_sorted_transport_preserving_registry_pins() {
        let root = tempfile::tempdir().unwrap();
        let source_path = root.path().join("source.wdl");
        std::fs::write(
            &source_path,
            r#"version 1.2
task images {
    command { echo images }
    requirements {
        container: ["oras://z.example/team/tool:v1", "docker://a.example/team/tool:v2"]
    }
}"#,
        )
        .unwrap();
        let lock_path = root.path().join(LOCK_FILE_NAME);

        generate_lock(
            &lock_path,
            &analyze(&source_path).await,
            "ubuntu:latest",
            &SortedResolver,
        )
        .await
        .unwrap();

        let lock = std::fs::read_to_string(&lock_path).unwrap();
        let timestamp = lock
            .lines()
            .nth(1)
            .unwrap()
            .trim_start_matches("generation_time = \"")
            .trim_end_matches('"');
        let digest = format!("sha256:{}", "b".repeat(64));
        assert_eq!(
            lock,
            format!(
                "version = 1\ngeneration_time = \"{timestamp}\"\nsif_files = \
                 {{}}\n\n[images]\n\"docker://a.example/team/tool:v2\" = \
                 \"docker://a.example/team/tool@{digest}\"\n\"oras://z.example/team/tool:v1\" = \
                 \"oras://z.example/team/tool@{digest}\"\n"
            )
        );
    }
}
