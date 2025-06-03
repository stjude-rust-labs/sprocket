//! Implementation of the `lock` subcommand.

use std::collections::HashMap;
use std::collections::HashSet;

use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use crankshaft_docker::Docker as crankshaft_docker;
use wdl::ast::AstNode;
use wdl::ast::AstToken;
use wdl::cli::Analysis;
use wdl::cli::analysis::Source;

use crate::Mode;

/// Arguments for the `lock` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
    /// A source WDL file or URL.
    #[clap(value_name = "PATH or URL")]
    pub source: Source,

    /// Disables color output.
    #[arg(long)]
    pub no_color: bool,

    /// The report mode.
    #[arg(short = 'm', long, value_name = "MODE")]
    pub report_mode: Option<Mode>,
}

impl Args {
    /// Applies the configuration to the command arguments.
    pub fn apply(mut self, config: crate::config::Config) -> Self {
        self.no_color = self.no_color || !config.common.color;
        self.report_mode = match self.report_mode {
            Some(mode) => Some(mode),
            None => Some(config.common.report_mode),
        };
        self
    }
}

/// Performs the `lock` command.
pub async fn lock(args: Args) -> Result<()> {
    let results = match Analysis::default().add_source(args.source).run().await {
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
        for task in result.document().tasks() {
            doc.ast()
                .as_v1()
                .expect("should be a v1 document")
                .tasks()
                .filter(|t| t.name().text() == task.name())
                .for_each(|t| {
                    if let Some(runtime) = t.runtime() {
                        if let Some(container) = runtime.container() {
                            if let Ok(image) = container.value() {
                                if let Ok(text) =
                                    serde_json::from_str(image.expr().text().to_string().as_str())
                                {
                                    images.insert(text);
                                }
                            }
                        }
                    }
                })
        }
    }

    let mut map: HashMap<String, String> = HashMap::new();
    for image in images {
        let prefix = image.split(':').next().unwrap_or("");
        let docker = crankshaft_docker::with_defaults()?;
        docker
            .ensure_image(&image)
            .await
            .expect("should ensure image");

        let image_info = docker
            .inner()
            .inspect_image(image.as_str())
            .await
            .expect("should inspect image");
        if let Some(digests) = image_info.repo_digests {
            for d in digests {
                if !d.starts_with(prefix) {
                    continue;
                }
                map.insert(image.clone(), d.clone());
            }
        }
    }

    if !map.is_empty() {
        let data = toml::to_string_pretty(&map)?;
        let path = "sprocket.lock";
        std::fs::write(path, data)?;
    }

    Ok(())
}
