//! Create HTML documentation for WDL tasks.

use std::path::PathBuf;

use maud::Markup;
use maud::html;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::SupportedVersion;
use wdl_ast::v1::CommandSection;
use wdl_ast::v1::RuntimeSection;
use wdl_ast::v1::TaskDefinition;

use super::*;
use crate::command_section::CommandSectionExt;
use crate::docs_tree::Header;
use crate::docs_tree::PageSections;
use crate::meta::DESCRIPTION_KEY;
use crate::parameter::Parameter;

/// A task in a WDL document.
#[derive(Debug)]
pub struct Task {
    /// The name of the task.
    name: String,
    /// The [`VersionBadge`] which displays the WDL version of the task.
    version: VersionBadge,
    /// The meta of the task.
    meta: MetaMap,
    /// The input parameters of the task.
    inputs: Vec<Parameter>,
    /// The output parameters of the task.
    outputs: Vec<Parameter>,
    /// The runtime section of the task.
    runtime_section: Option<RuntimeSection>,
    /// The command section of the task.
    command_section: Option<CommandSection>,
    /// The path from the root of the WDL workspace to the WDL document which
    /// contains this task.
    ///
    /// Used to render the "run with" component.
    wdl_path: Option<PathBuf>,
}

impl Task {
    /// Create a new task.
    ///
    /// If `wdl_path` is omitted, no "run with" component will be
    /// rendered.
    pub fn new(
        name: String,
        version: SupportedVersion,
        definition: TaskDefinition,
        wdl_path: Option<PathBuf>,
    ) -> Self {
        let meta = match definition.metadata() {
            Some(mds) => parse_meta(&mds),
            _ => MetaMap::default(),
        };
        let parameter_meta = match definition.parameter_metadata() {
            Some(pmds) => parse_parameter_meta(&pmds),
            _ => MetaMap::default(),
        };
        let inputs = match definition.input() {
            Some(is) => parse_inputs(&is, &parameter_meta),
            _ => Vec::new(),
        };
        let outputs = match definition.output() {
            Some(os) => parse_outputs(&os, &meta, &parameter_meta),
            _ => Vec::new(),
        };

        Self {
            name,
            version: VersionBadge::new(version),
            meta,
            inputs,
            outputs,
            runtime_section: definition.runtime(),
            command_section: definition.command(),
            wdl_path,
        }
    }

    /// Render the meta section of the task as HTML.
    ///
    /// This will render all metadata key-value pairs except for `description`
    /// and `outputs`.
    pub fn render_meta(&self, assets: &Path) -> Option<Markup> {
        self.meta()
            .render_remaining(&[DESCRIPTION_KEY, "outputs"], assets)
    }

    /// Render the runtime section of the task as HTML.
    pub fn render_runtime_section(&self) -> Markup {
        match &self.runtime_section {
            Some(runtime_section) => {
                let rows = runtime_section
                    .items()
                    .map(|entry| {
                        {
                            html! {
                                div class="main__grid-row" {
                                    div class="main__grid-cell" {
                                        code { (entry.name().text()) }
                                    }
                                    div class="main__grid-cell" {
                                        code { ({let e = entry.expr(); e.text().to_string()}) }
                                    }
                                }
                            }
                        }
                        .into_string()
                    })
                    .collect::<Vec<_>>()
                    .join(&html! { div class="main__grid-row-separator" {} }.into_string());

                html! {
                    div class="main__section" {
                        h2 id="runtime" class="main__section-header" { "Default Runtime Attributes" }
                        div class="main__grid-container" {
                            div class="main__grid-runtime-container" {
                                div class="main__grid-header-cell" { "Attribute" }
                                div class="main__grid-header-cell" { "Value" }
                                div class="main__grid-header-separator" {}
                                (PreEscaped(rows))
                            }
                        }
                    }
                }
            }
            _ => {
                html! {}
            }
        }
    }

    /// Render the command section of the task as HTML.
    pub fn render_command_section(&self) -> Markup {
        match &self.command_section {
            Some(command_section) => {
                html! {
                    div class="main__section" {
                        h2 id="command" class="main__section-header" { "Command" }
                        sprocket-code language="wdl" class="pt-8" {
                            (command_section.script())
                        }
                    }
                }
            }
            _ => {
                html! {}
            }
        }
    }

    /// Render the task as HTML.
    pub fn render(&self, assets: &Path) -> (Markup, PageSections) {
        let mut headers = PageSections::default();

        let (input_markup, inner_headers) = self.render_inputs(assets);
        headers.extend(inner_headers);

        let markup = html! {
            div class="main__container" {
                span class="text-violet-400" { "Task" }
                h1 id="title" class="main__title" { code { (self.name()) } }
                div class="markdown-body mb-4" {
                    (self.render_description(false))
                }
                div class="main__badge-container" {
                    (self.render_version())
                }
                (self.render_run_with(assets))
                @if let Some(meta) = self.render_meta(assets) {
                    div class="main__section" {
                        (meta)
                    }
                }
                (input_markup)
                (self.render_outputs(assets))
                (self.render_runtime_section())
                (self.render_command_section())
            }
        };
        headers.push(Header::Header("Outputs".to_string(), "outputs".to_string()));
        headers.push(Header::Header("Runtime".to_string(), "runtime".to_string()));
        headers.push(Header::Header("Command".to_string(), "command".to_string()));

        (markup, headers)
    }
}

impl Runnable for Task {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &VersionBadge {
        &self.version
    }

    fn meta(&self) -> &MetaMap {
        &self.meta
    }

    fn inputs(&self) -> &[Parameter] {
        &self.inputs
    }

    fn outputs(&self) -> &[Parameter] {
        &self.outputs
    }

    fn wdl_path(&self) -> Option<&Path> {
        self.wdl_path.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use wdl_ast::Document;
    use wdl_ast::version::V1;

    use super::*;

    #[test]
    fn test_task() {
        let (doc, _) = Document::parse(
            r#"
            version 1.0

            task my_task {
                input {
                    String name
                }
                output {
                    String greeting = "Hello, ${name}!"
                }
                runtime {
                    docker: "ubuntu:latest"
                }
                meta {
                    description: "A simple task"
                }
            }
            "#,
        );

        let doc_item = doc.ast().into_v1().unwrap().items().next().unwrap();
        let ast_task = doc_item.into_task_definition().unwrap();

        let task = Task::new(
            ast_task.name().text().to_owned(),
            SupportedVersion::V1(V1::Zero),
            ast_task,
            None,
        );

        assert_eq!(task.name(), "my_task");
        assert_eq!(
            task.meta()
                .get("description")
                .unwrap()
                .clone()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "A simple task"
        );
        assert_eq!(task.inputs().len(), 1);
        assert_eq!(task.outputs().len(), 1);
    }
}
