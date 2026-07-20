//! Static container extraction from analyzed WDL documents.

use anyhow::Context as _;
use anyhow::Result;
use wdl::ast::AstToken as _;
use wdl::ast::v1::Expr;
use wdl::ast::v1::LiteralExpr;
use wdl::engine::v1::requirements::ContainerSource;

use crate::analysis::AnalysisResults;

/// Selects how dynamic container expressions are handled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtractionMode {
    /// Rejects any container expression that is not fully static.
    Generate,
    /// Skips dynamic container expression portions for runtime enforcement.
    Preflight,
}

/// A statically discoverable container candidate used by a task.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerUse {
    /// The task name that declared or inherited the container.
    pub task: String,
    /// The analyzed document path that contains the task.
    pub document: String,
    /// The parsed container source candidate.
    pub source: ContainerSource,
}

/// Extracts static container candidates from analyzed WDL documents.
pub fn extract(
    results: &AnalysisResults,
    default: &str,
    mode: ExtractionMode,
) -> Result<Vec<ContainerUse>> {
    let mut uses = Vec::new();
    for result in results.as_slice() {
        let document = result.document();
        let root = document.root();
        let ast = root.ast_with_version_fallback(document.config().fallback_version());
        let ast = ast
            .as_v1()
            .context("container locking requires a supported wdl 1.x document")?;
        let document_path = document.path().to_string();

        for task in ast.tasks() {
            let task_name = task.name().text().to_string();
            let expression = if let Some(runtime) = task.runtime() {
                runtime
                    .items()
                    .find(|item| matches!(item.name().text(), "container" | "docker"))
                    .map(|item| item.expr())
            } else {
                task.requirements()
                    .and_then(|section| {
                        section
                            .items()
                            .find(|item| matches!(item.name().text(), "container" | "docker"))
                    })
                    .map(|item| item.expr())
            };

            let Some(expression) = expression else {
                push_container(&mut uses, &task_name, &document_path, default, default);
                continue;
            };

            extract_expression(
                &expression,
                default,
                mode,
                &task_name,
                &document_path,
                &mut uses,
            )?;
        }
    }

    Ok(uses)
}

fn extract_expression(
    expression: &Expr,
    default: &str,
    mode: ExtractionMode,
    task: &str,
    document: &str,
    uses: &mut Vec<ContainerUse>,
) -> Result<()> {
    let strings = match expression {
        Expr::Literal(LiteralExpr::String(value)) => vec![value.clone()],
        Expr::Literal(LiteralExpr::Array(array)) => {
            let mut values = Vec::new();
            for element in array.elements() {
                match element {
                    Expr::Literal(LiteralExpr::String(value)) => values.push(value),
                    Expr::Literal(_) => anyhow::bail!(
                        "container array in task `{task}` in `{document}` contains a non-string \
                         literal"
                    ),
                    _ if mode == ExtractionMode::Preflight => {}
                    _ => anyhow::bail!(
                        "container array in task `{task}` in `{document}` must contain only \
                         static string literals"
                    ),
                }
            }
            values
        }
        Expr::Literal(_) => anyhow::bail!(
            "container in task `{task}` in `{document}` must be a string or array of strings"
        ),
        _ if mode == ExtractionMode::Preflight => return Ok(()),
        _ => anyhow::bail!(
            "container in task `{task}` in `{document}` must be a static string literal or static \
             array of string literals"
        ),
    };

    for literal in strings {
        let Some(text) = literal.text() else {
            if mode == ExtractionMode::Preflight {
                continue;
            }

            anyhow::bail!(
                "container in task `{task}` in `{document}` must be static and must not contain \
                 interpolation"
            );
        };
        let mut value = String::new();
        text.unescape_to(&mut value);
        push_container(uses, task, document, &value, default);
    }

    Ok(())
}

fn push_container(
    uses: &mut Vec<ContainerUse>,
    task: &str,
    document: &str,
    value: &str,
    default: &str,
) {
    let value = if value == "*" { default } else { value };
    uses.push(ContainerUse {
        task: task.to_string(),
        document: document.to_string(),
        source: parse_container_source(value),
    });
}

fn parse_container_source(value: &str) -> ContainerSource {
    match value.parse() {
        Ok(source) => source,
        Err(error) => match error {},
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use wdl::diagnostics::Mode;
    use wdl::engine::v1::requirements::ContainerSource;

    use super::ExtractionMode;
    use super::extract;
    use crate::analysis::Analysis;
    use crate::analysis::AnalysisResults;

    async fn analyze(name: &str, source: &str) -> Result<(tempfile::TempDir, AnalysisResults)> {
        analyze_files(name, &[(name, source)]).await
    }

    async fn analyze_files(
        root_name: &str,
        files: &[(&str, &str)],
    ) -> Result<(tempfile::TempDir, AnalysisResults)> {
        let root = tempfile::tempdir()?;
        for (name, source) in files {
            let path = root.path().join(name);
            std::fs::write(&path, source)?;
        }
        let path = root.path().join(root_name);
        let source = path.to_string_lossy().parse()?;
        let results = Analysis::default()
            .add_source(source)
            .run(Mode::default(), false)
            .await
            .map_err(|errors| anyhow::anyhow!("{errors:#?}"))?;

        Ok((root, results))
    }

    #[tokio::test]
    async fn extracts_runtime_alias_and_requirements_array() -> Result<()> {
        let (_root, runtime) = analyze(
            "runtime.wdl",
            r#"version 1.0
task legacy {
    command { echo legacy }
    runtime {
        docker: "ubuntu:20.04"
    }
}"#,
        )
        .await?;
        let runtime_uses = extract(&runtime, "ubuntu:24.04", ExtractionMode::Generate)?;
        assert_eq!(
            runtime_uses[0].source,
            ContainerSource::Docker("ubuntu:20.04".into())
        );

        let (_root, requirements) = analyze(
            "requirements.wdl",
            r#"version 1.2
task modern {
    command { echo modern }
    requirements {
        container: ["ubuntu:24.04", "oras://ghcr.io/x/y:v1"]
    }
}"#,
        )
        .await?;
        let requirement_uses = extract(&requirements, "ubuntu:24.04", ExtractionMode::Generate)?;
        assert_eq!(
            requirement_uses
                .into_iter()
                .map(|usage| usage.source)
                .collect::<Vec<_>>(),
            vec![
                ContainerSource::Docker("ubuntu:24.04".into()),
                ContainerSource::Oras("ghcr.io/x/y:v1".into()),
            ]
        );

        Ok(())
    }

    #[tokio::test]
    async fn extracts_requirements_docker_alias() -> Result<()> {
        let (_root, results) = analyze(
            "requirements-docker.wdl",
            r#"version 1.2
task alias {
    command { echo alias }
    requirements {
        docker: "ubuntu:22.04"
    }
}"#,
        )
        .await?;

        let uses = extract(&results, "ubuntu:24.04", ExtractionMode::Generate)?;

        assert_eq!(
            uses.into_iter()
                .map(|usage| usage.source)
                .collect::<Vec<_>>(),
            vec![ContainerSource::Docker("ubuntu:22.04".into())]
        );

        Ok(())
    }

    #[tokio::test]
    async fn uses_default_for_missing_and_wildcard_containers() -> Result<()> {
        let (_root, results) = analyze(
            "defaults.wdl",
            r#"version 1.2
task missing {
    command { echo missing }
}
task wildcard {
    command { echo wildcard }
    requirements {
        container: "*"
    }
}"#,
        )
        .await?;
        let uses = extract(&results, "ubuntu:24.04", ExtractionMode::Generate)?;
        assert_eq!(uses.len(), 2);
        assert!(
            uses.iter()
                .all(|usage| usage.source == ContainerSource::Docker("ubuntu:24.04".into()))
        );

        Ok(())
    }

    #[tokio::test]
    async fn generation_rejects_interpolation_with_task_and_document_context() -> Result<()> {
        let (_root, results) = analyze(
            "dynamic.wdl",
            r#"version 1.2
task dynamic {
    input { String version }
    command { echo dynamic }
    requirements {
        container: "ghcr.io/x/y:~{version}"
    }
}"#,
        )
        .await?;
        let error = match extract(&results, "ubuntu:24.04", ExtractionMode::Generate) {
            Ok(_) => anyhow::bail!("dynamic container should fail extraction"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("dynamic"));
        assert!(error.contains("dynamic.wdl"));
        assert!(error.contains("static"));

        Ok(())
    }

    #[tokio::test]
    async fn preflight_skips_dynamic_container_for_runtime_enforcement() -> Result<()> {
        let (_root, results) = analyze(
            "dynamic.wdl",
            r#"version 1.2
task dynamic {
    input { String version }
    command { echo dynamic }
    requirements {
        container: "ghcr.io/x/y:~{version}"
    }
}"#,
        )
        .await?;
        assert!(extract(&results, "ubuntu:24.04", ExtractionMode::Preflight)?.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn preflight_keeps_static_array_members_and_skips_dynamic_members() -> Result<()> {
        let (_root, results) = analyze(
            "mixed.wdl",
            r#"version 1.2
task mixed {
    input { String version }
    command { echo mixed }
    requirements {
        container: ["ubuntu:24.04", "ghcr.io/x/y:~{version}", "*"]
    }
}"#,
        )
        .await?;
        let uses = extract(&results, "alpine:3.18", ExtractionMode::Preflight)?;
        assert_eq!(
            uses.into_iter()
                .map(|usage| usage.source)
                .collect::<Vec<_>>(),
            vec![
                ContainerSource::Docker("ubuntu:24.04".into()),
                ContainerSource::Docker("alpine:3.18".into()),
            ]
        );

        Ok(())
    }

    #[tokio::test]
    async fn preflight_rejects_static_non_string_container_literals() -> Result<()> {
        let (_root, results) = analyze(
            "invalid.wdl",
            r#"version 1.2
task invalid {
    command { echo invalid }
    requirements {
        container: ["ubuntu:24.04", 1]
    }
}"#,
        )
        .await?;
        let error = match extract(&results, "ubuntu:24.04", ExtractionMode::Preflight) {
            Ok(_) => anyhow::bail!("static non-string container should fail extraction"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("invalid"));
        assert!(error.contains("invalid.wdl"));
        assert!(error.contains("non-string"));

        Ok(())
    }

    #[tokio::test]
    async fn extracts_tasks_from_imported_documents() -> Result<()> {
        let (_root, results) = analyze_files(
            "main.wdl",
            &[
                (
                    "main.wdl",
                    r#"version 1.2
import "tasks.wdl" as tasks

workflow main {}
"#,
                ),
                (
                    "tasks.wdl",
                    r#"version 1.2
task imported {
    command { echo imported }
    requirements {
        container: "ubuntu:22.04"
    }
}
"#,
                ),
            ],
        )
        .await?;
        let uses = extract(&results, "ubuntu:24.04", ExtractionMode::Generate)?;
        assert!(uses.iter().any(|usage| usage.task == "imported"
            && usage.source == ContainerSource::Docker("ubuntu:22.04".into())));

        Ok(())
    }
}
