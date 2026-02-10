//! Create HTML documentation for WDL workflows.

use std::path::PathBuf;

use maud::Markup;
use wdl_ast::SupportedVersion;
use wdl_ast::v1::MetadataValue;
use wdl_ast::v1::WorkflowDefinition;

use super::*;
use crate::docs_tree::Header;
use crate::docs_tree::PageSections;
use crate::meta::DESCRIPTION_KEY;
use crate::meta::MetaMapValueSource;
use crate::meta::parse_metadata_items;
use crate::parameter::Parameter;

/// The key used to override the name of the workflow in the meta section.
const NAME_KEY: &str = "name";
/// The key used to specify the category of the workflow in the meta section.
const CATEGORY_KEY: &str = "category";

/// A workflow in a WDL document.
#[derive(Debug)]
pub(crate) struct Workflow {
    /// The name of the workflow.
    name: String,
    /// The [`VersionBadge`] which displays the WDL version of the workflow.
    version: VersionBadge,
    /// The meta of the workflow.
    meta: MetaMap,
    /// The inputs of the workflow.
    inputs: Vec<Parameter>,
    /// The outputs of the workflow.
    outputs: Vec<Parameter>,
    /// The path to the WDL file.
    wdl_path: Option<PathBuf>,
}

impl DefinitionMeta for Workflow {
    fn meta(&self) -> &MetaMap {
        &self.meta
    }
}

impl Workflow {
    /// Create a new workflow.
    pub fn new(
        name: String,
        version: SupportedVersion,
        definition: WorkflowDefinition,
        wdl_path: Option<PathBuf>,
    ) -> Self {
        let meta = match definition.metadata() {
            Some(mds) => parse_metadata_items(mds.items()),
            _ => MetaMap::default(),
        };
        let parameter_meta = match definition.parameter_metadata() {
            Some(pmds) => parse_metadata_items(pmds.items()),
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
            wdl_path,
        }
    }

    /// Returns the [`NAME_KEY`] meta entry, if it exists and is a String.
    pub fn name_override(&self) -> Option<String> {
        self.meta.get(NAME_KEY).and_then(MetaMapValueSource::text)
    }

    /// Returns the [`CATEGORY_KEY`] meta entry, if it exists and is a String.
    pub fn category(&self) -> Option<String> {
        self.meta
            .get(CATEGORY_KEY)
            .and_then(MetaMapValueSource::text)
    }

    /// Returns the name of the workflow as HTML.
    ///
    /// If the `name` meta entry exists and is a String, it will be used instead
    /// of the `name` struct member.
    pub fn render_name(&self) -> Markup {
        if let Some(name) = self.name_override() {
            html! { (name) }
        } else {
            html! { (self.name) }
        }
    }

    /// Renders the meta section of the workflow as HTML.
    ///
    /// This will render all metadata key-value pairs except for `description`,
    /// `name`, `category`, `allowNestedInputs`/`allow_nested_inputs`,
    /// and `outputs`.
    pub fn render_meta(&self, assets: &Path) -> Option<Markup> {
        self.meta().render_remaining(
            &[
                DESCRIPTION_KEY,
                NAME_KEY,
                CATEGORY_KEY,
                "allowNestedInputs",
                "allow_nested_inputs",
                "outputs",
            ],
            assets,
        )
    }

    /// Render the `allowNestedInputs`/`allow_nested_inputs` meta entry as a
    /// badge.
    ///
    /// If the value is `true`, it renders an "allowed badge", in all other
    /// cases it renders a "disabled badge".
    pub fn render_allow_nested_inputs(&self) -> Markup {
        if let Some(MetaMapValueSource::MetaValue(MetadataValue::Boolean(b))) = self
            .meta
            .get("allowNestedInputs")
            .or(self.meta.get("allow_nested_inputs"))
            && b.value()
        {
            return html! {
                div class="main__badge main__badge--success" {
                    span class="main__badge-text" {
                        "Nested Inputs Allowed"
                    }
                }
            };
        }
        html! {
            div class="main__badge main__badge--disabled" {
                span class="main__badge-text" {
                    "Nested Inputs Not Allowed"
                }
            }
        }
    }

    /// Render the `category` meta entry as a badge, if it exists and is a
    /// String.
    pub fn render_category(&self) -> Option<Markup> {
        self.category().map(|category| {
            html! {
                div class="main__badge" {
                    span class="main__badge-text" {
                        "Category"
                    }
                    div class="main__badge-inner" {
                        span class="main__badge-inner-text" {
                            (category)
                        }
                    }
                }
            }
        })
    }

    /// Render the workflow as HTML.
    pub fn render(&self, assets: &Path) -> (Markup, PageSections) {
        let mut headers = PageSections::default();

        let meta_markup = if let Some(meta) = self.render_meta(assets) {
            html! { (meta) }
        } else {
            html! {}
        };

        let (input_markup, inner_headers) = self.render_inputs(assets);

        headers.extend(inner_headers);

        // Invisible image for search results
        let search_image = html! {
            img src=(assets.join("workflow-selected.svg").to_string_lossy()) class="hidden" data-pagefind-meta="image[src]" {}
        };

        let markup = html! {
            div class="main__container" data-pagefind-body {
                (search_image)
                span class="text-brand-emerald-400" data-pagefind-ignore { "Workflow" }
                h1 id="title" class="main__title" data-pagefind-meta="title" { (self.render_name()) }
                div class="markdown-body mb-4" {
                    (self.render_description(false))
                }
                div class="main__badge-container" {
                    (self.render_version())
                    @if let Some(badge) = self.render_category() {
                        (badge)
                    }
                    (self.render_allow_nested_inputs())
                }
                (self.render_run_with(assets))
                div class="main__section" {
                    (meta_markup)
                }
                (input_markup)
                (self.render_outputs(assets))
            }
        };

        headers.push(Header::Header("Outputs".to_string(), "outputs".to_string()));

        (markup, headers)
    }
}

impl Runnable for Workflow {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &VersionBadge {
        &self.version
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
    fn test_workflow() {
        let (doc, _) = Document::parse(
            r#"
            version 1.0
            workflow test {
                input {
                    String name
                }
                output {
                    String greeting = "Hello, ${name}!"
                }
            }
            "#,
        );

        let doc_item = doc.ast().into_v1().unwrap().items().next().unwrap();
        let ast_workflow = doc_item.into_workflow_definition().unwrap();

        let workflow = Workflow::new(
            ast_workflow.name().text().to_string(),
            SupportedVersion::V1(V1::Zero),
            ast_workflow,
            None,
        );

        assert_eq!(workflow.name(), "test");
        assert_eq!(workflow.inputs.len(), 1);
        assert_eq!(workflow.outputs.len(), 1);
    }
}
