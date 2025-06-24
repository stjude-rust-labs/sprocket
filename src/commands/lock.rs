//! Implementation of the `lock` subcommand.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Result;
use anyhow::bail;
use chrono::prelude::*;
use clap::Parser;
use crankshaft_docker::Docker as crankshaft_docker;
use serde::Deserialize;
use serde::Serialize;
use wdl::ast::AstToken;
use wdl::ast::v1::Expr;
use wdl::ast::v1::LiteralExpr;
use wdl::cli::Analysis;
use wdl::cli::analysis::Source;

/// Name for the lock file.
const LOCK_FILE: &str = "sprocket.lock";

/// Arguments for the `lock` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
    /// A source WDL file, directory, or URL.
    #[clap(value_name = "PATH or URL")]
    pub source: Option<Source>,

    /// Output directory for the lock file.
    #[clap(short, long, value_name = "DIR")]
    pub output: Option<PathBuf>,
}

/// Represents the lock file structure.
#[derive(Debug, Serialize, Deserialize)]
struct Lock {
    /// The time when the lock file was created.
    #[serde(rename = "generation_time")]
    timestamp: String,
    /// A mapping of Docker image names to their sha256 digests.
    images: HashMap<String, String>,
}

/// Performs the `lock` command.
pub async fn lock(args: Args) -> Result<()> {
    let output_path = args
        .output
        .unwrap_or_else(|| PathBuf::from(std::path::Component::CurDir.as_os_str()))
        .join(LOCK_FILE);

    // TODO: replace with `Default` once that's upstream
    let s = args.source.unwrap_or(Source::Directory(PathBuf::from(
        std::path::Component::CurDir.as_os_str(),
    )));
    let results = match Analysis::default().add_source(s).run().await {
        Ok(results) => results,
        Err(errors) => {
            // SAFETY: this is a non-empty, so it must always have a first
            // element.
            bail!(errors.into_iter().next().unwrap())
        }
    };

    let mut images: HashSet<String> = HashSet::new();
    for result in results {
        let doc = result.document().root();

        for task in doc.ast().as_v1().expect("should be a v1 document").tasks() {
            let task_name_token = task.name();
            let task_name = task_name_token.inner().text();
            let doc_path = result.document().path();
            let Some(runtime) = task.runtime() else {
                tracing::warn!(
                    "Skipping task {task_name} in document {doc_path} with no runtime section",
                );
                continue;
            };
            let Some(image) = runtime.container().and_then(|c| c.value().ok()) else {
                tracing::warn!(
                    "Skipping task {task_name} in document {doc_path} with no container image",
                );
                continue;
            };
            let Expr::Literal(LiteralExpr::String(s)) = image.expr() else {
                tracing::warn!(
                    "Skipping image with non-literal value in task {task_name} in document \
                     {doc_path}",
                );
                continue;
            };
            let Some(text) = s.text() else {
                tracing::warn!(
                    "Skipping image with placeholder value in task {task_name} in document \
                     {doc_path}",
                );
                continue;
            };
            let mut buffer = String::new();
            text.unescape_to(&mut buffer);
            images.insert(buffer);
        }
    }

    let time = Utc::now();

    let mut map: HashMap<String, String> = HashMap::new();
    let docker = crankshaft_docker::with_defaults()?;

    for image in images {
        let prefix = image.split(':').next().unwrap_or("");

        let i = docker
            .inner()
            .inspect_registry_image(&image, None)
            .await
            .expect("should inspect registry image");

        // Insert the manifest digest into the map.
        map.insert(
            image.clone(),
            prefix.to_owned() + "@" + &i.descriptor.digest.expect("should have a digest"),
        );
    }

    let lock = Lock {
        timestamp: time.to_string(),
        images: if !map.is_empty() { map } else { HashMap::new() },
    };
    let data = toml::to_string_pretty(&lock)?;
    std::fs::write(output_path, data)?;

    Ok(())
}
