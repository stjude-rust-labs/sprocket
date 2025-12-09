//! Implementation of the `test` subcommand.

use std::collections::HashSet;
use std::fs::read;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use clap::Parser;
use nonempty::NonEmpty;
use path_clean::PathClean;
use serde_json::Value as JsonValue;
use tokio::fs::remove_dir_all;
use tracing::debug;
use tracing::info;
use tracing::warn;
use wdl::engine::CancellationContext;
use wdl::engine::Events;
use wdl::engine::Inputs as EngineInputs;

use crate::analysis::Analysis;
use crate::analysis::Source;
use crate::commands::CommandError;
use crate::commands::CommandResult;
use crate::commands::run::EVENTS_CHANNEL_CAPACITY;
use crate::eval::Evaluator;
use crate::inputs::OriginPaths;
use crate::test::Assertion;
use crate::test::InputMatrix;
use crate::test::TestDefinition;

const TEST_DIR: &str = "test";
const FIXTURES_DIR: &str = "fixtures";
const RUNS_DIR: &str = "runs";

/// Arguments for the `test` subcommand.
#[derive(Parser, Debug)]
pub struct Args {
    /// Local path to a WDL document or workspace to unit test.
    pub source: Option<Source>,
    /// Root workspace where test fixtures are relative to.
    #[clap(short, long)]
    pub workspace: Option<PathBuf>,
    /// Specific test tag that should be run.
    ///
    /// Can be repeated multiple times.
    #[clap(short='t', long, value_name = "TAG",
        action = clap::ArgAction::Append,
        num_args = 1,
        conflicts_with="filter_tag",
    )]
    pub include_tag: Vec<String>,
    /// Filter out any tests with a matching tag.
    ///
    /// Can be repeated multiple times.
    #[clap(short, long, value_name = "TAG",
        action = clap::ArgAction::Append,
        num_args = 1,
    )]
    pub filter_tag: Vec<String>,
    /// The engine configuration to use.
    ///
    /// This is not exposed via [`clap`] and is not settable by users.
    /// It will always be overwritten by the engine config provided by the user
    /// (which will be set with `Default::default()` if the user does not
    /// explicitly set `run` config values).
    #[clap(skip)]
    pub engine: wdl::engine::config::Config,
}

impl Args {
    pub fn apply(mut self, config: crate::config::Config) -> Self {
        self.engine = config.run.engine;
        self
    }
}

const NESTED_TEST_DIR_NAME: &str = "test";

fn find_yaml(wdl_path: &Path) -> Result<Option<PathBuf>> {
    let mut result: Option<PathBuf> = None;
    let mut inner = |path: &Path| {
        for ext in ["yaml", "yml"] {
            let yaml = path.with_extension(ext);
            match (yaml.exists(), &result) {
                (true, Some(previous)) => bail!(
                    "test file `{path}` conflicts with test file `{previous}`",
                    path = yaml.display(),
                    previous = previous.display()
                ),
                (true, None) => {
                    result = Some(yaml);
                }
                _ => {}
            }
        }

        Ok(())
    };
    inner(wdl_path)?;

    let parent = wdl_path.parent().expect("should have parent");
    let nested = parent
        .join(NESTED_TEST_DIR_NAME)
        .join(wdl_path.file_name().expect("should have filename"));
    inner(&nested)?;
    Ok(result)
}

fn filter_test(
    test: &TestDefinition,
    include_tags: &HashSet<String>,
    filter_tags: &HashSet<String>,
) -> bool {
    if !include_tags.is_empty() && !test.tags.iter().any(|t| include_tags.contains(t)) {
        return true;
    }
    if test.tags.iter().any(|t| filter_tags.contains(t)) {
        return true;
    }
    false
}

fn parse_test(
    test: &TestDefinition,
) -> Result<(IndexMap<String, serde_yaml_ng::Value>, InputMatrix)> {
    let assertions = test
        .parse_assertions()
        .with_context(|| format!("parsing assertions of `{}`", test.name))?;
    let matrix = test
        .parse_inputs()
        .with_context(|| format!("parsing inputs of `{}`", test.name))?;
    Ok((assertions, matrix))
}

/// Performs the `test` command.
pub async fn test(args: Args) -> CommandResult<()> {
    let source = args.source.unwrap_or_default();
    let (source, workspace) = match (&source, args.workspace) {
        (Source::Remote(_), _) => {
            return Err(anyhow!("the `test` subcommand does not accept remote sources").into());
        }
        (Source::Directory(_), Some(_)) => {
            return Err(anyhow!("arg conflict").into());
        }
        (Source::Directory(source_dir), None) => (source.clone(), source_dir.to_path_buf()),
        (Source::File(_), Some(supplied_dir)) => (source, supplied_dir),
        (Source::File(_), None) => (
            source,
            std::env::current_dir().context("failed to get current directory")?,
        ),
    };
    let workspace = absolute(&workspace)
        .with_context(|| {
            format!(
                "resolving absolute path to workspace: `{}`",
                workspace.display()
            )
        })?
        .clean();

    let results = Analysis::default()
        .add_source(source.clone())
        .run()
        .await
        .map_err(CommandError::from)?;

    // Find and parse all YAML before beginning any executions.
    // This is so that any totally invalid YAML is caught up-front before we start
    // testing. Smaller issues with test definitions will later be collected and
    // reported on after all tests execute.
    let mut documents = Vec::new();
    for result in results.filter(&[&source]) {
        let document = result.document();
        let wdl_path = PathBuf::from(document.path().as_ref());
        let yaml_path = match find_yaml(&wdl_path)? {
            Some(p) => p,
            None => {
                debug!(
                    "no test YAML found for WDL document `{}`",
                    wdl_path.display()
                );
                continue;
            }
        };
        let document_tests: crate::test::DocumentTests = serde_yaml_ng::from_slice(
            &read(&yaml_path)
                .with_context(|| format!("reading file: `{}`", yaml_path.display()))?,
        )
        .with_context(|| format!("parsing YAML: `{}`", yaml_path.display()))?;
        documents.push((result, document_tests));
        info!(
            "found tests for WDL `{}` in `{}`",
            wdl_path.display(),
            yaml_path.display()
        )
    }

    let test_dir = workspace.join(TEST_DIR);
    let fixture_origins = OriginPaths::Single(wdl::engine::path::EvaluationPath::Local(
        test_dir.join(FIXTURES_DIR),
    ));
    let cancellation = CancellationContext::new(args.engine.failure_mode);
    let events = Events::new(EVENTS_CHANNEL_CAPACITY);

    let include_tags = HashSet::from_iter(args.include_tag.into_iter());
    let filter_tags = HashSet::from_iter(args.filter_tag.into_iter());
    let mut errors = Vec::new();
    let mut success_counter = 0;
    let mut fail_counter = 0;
    for (analysis, test_definitions) in documents {
        let wdl_document = analysis.document();
        info!("testing WDL document `{}`", wdl_document.path());
        for (entrypoint, tests) in test_definitions.entrypoints.iter() {
            let found_entrypoint = match (
                wdl_document.task_by_name(entrypoint),
                wdl_document.workflow(),
            ) {
                (Some(_), _) => true,
                (None, Some(wf)) if wf.name() == entrypoint => true,
                (..) => false,
            };
            if !found_entrypoint {
                errors.push(Arc::new(anyhow!(
                    "no entrypoint named `{}` in `{}`",
                    entrypoint,
                    wdl_document.path()
                )));
                continue;
            }
            info!("testing entrypoint `{}`", entrypoint);
            for test in tests {
                if filter_test(test, &include_tags, &filter_tags) {
                    info!("skipping `{}`", test.name);
                    continue;
                }
                let (_assertions, matrix) = match parse_test(test)
                    .with_context(|| format!("parsing test definition of `{}`", test.name))
                {
                    Ok(res) => res,
                    Err(e) => {
                        errors.push(Arc::new(e));
                        warn!(
                            "skipping test `{}` due to problem with definition",
                            test.name
                        );
                        continue;
                    }
                };
                info!("running `{}`", test.name);
                for run_inputs in matrix.cartesian_product() {
                    let inputs = match run_inputs
                        .map(|(key, yaml_val)| match serde_json::to_value(yaml_val) {
                            Ok(json_val) => Ok((format!("{entrypoint}.{key}"), json_val)),
                            Err(e) => Err(anyhow!(e)),
                        })
                        .collect::<Result<serde_json::Map<String, JsonValue>>>()
                        .with_context(|| "converting YAML inputs to a JSON map")
                    {
                        Ok(res) => res,
                        Err(e) => {
                            errors.push(Arc::new(e));
                            warn!(
                                "skipping test `{}` due to problem with input matrix",
                                test.name
                            );
                            continue;
                        }
                    };

                    let engine_inputs = EngineInputs::parse_object(wdl_document, inputs)
                        .with_context(|| "converting to WDL inputs")?;

                    let Some((_derived_ep, wdl_inputs)) = engine_inputs else {
                        todo!("handle empty inputs");
                    };
                    let run_dir = test_dir.join(RUNS_DIR).join(&entrypoint).join(&test.name);
                    if run_dir.exists() {
                        remove_dir_all(&run_dir).await.with_context(|| "removing prior run dir")?;
                    }
                    let evaluator = Evaluator::new(
                        wdl_document,
                        &entrypoint,
                        wdl_inputs,
                        fixture_origins.clone(),
                        args.engine.clone(),
                        &run_dir,
                    );
                    let result = evaluator.run(cancellation.clone(), &events).await;
                    match result {
                        Ok(outputs) => {
                            dbg!("success!");
                            success_counter += 1;
                        }
                        Err(e) => {
                            let e_str = e.to_string();
                            dbg!("fail: ", e_str);
                            fail_counter += 1;
                        }
                    }
                }
            }
        }
    }
    
    println!("successful tests: {success_counter}");
    println!("failed tests: {fail_counter}");

    if let Some(errors) = NonEmpty::from_vec(errors) {
        return Err(CommandError::from(errors));
    };

    Ok(())
}
