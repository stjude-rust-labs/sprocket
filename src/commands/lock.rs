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
use wdl::ast::v1::Expr;
use wdl::ast::v1::LiteralExpr;
use wdl::cli::Analysis;
use wdl::cli::analysis::Source;

/// Name for the lock file.
const LOCK_FILE: &str = "sprocket.lock";

/// Arguments for the `lock` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
    /// A source WDL file or URL.
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
    let mut buffer = String::new();
    for result in results {
        let doc = result.document().root();

        for task in doc.ast().as_v1().expect("should be a v1 document").tasks() {
            if let Some(runtime) = task.runtime() {
                if let Some(container) = runtime.container() {
                    if let Ok(image) = container.value() {
                        if let Expr::Literal(LiteralExpr::String(s)) = image.expr() {
                            if let Some(text) = s.text() {
                                text.unescape_to(&mut buffer);
                                images.insert(buffer.clone());
                            }
                        }
                    }
                }
            }
            buffer.clear();
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

    if !map.is_empty() {
        let lock = Lock {
            timestamp: time.to_string(),
            images: map,
        };
        let data = toml::to_string_pretty(&lock)?;
        std::fs::write(output_path, data)?;
    } else {
        let lock = Lock {
            timestamp: time.to_string(),
            images: HashMap::new(),
        };

        let data = toml::to_string_pretty(&lock)?;
        std::fs::write(output_path, data)?;
    }

    Ok(())
}
