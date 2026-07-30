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
use crate::meta::doc_comments;
use crate::meta::main_container;
use crate::meta::parse_metadata_items;
use crate::page::DeclarationHero;
use crate::page::TitleKind;
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
        enable_doc_comments: bool,
    ) -> Self {
        let mut meta = match definition.metadata() {
            Some(mds) => parse_metadata_items(mds.items()),
            _ => MetaMap::default(),
        };

        if enable_doc_comments && let Some(comments) = definition.doc_comments() {
            // Doc comments take precedence
            meta.append(&mut doc_comments(comments));
        }

        let parameter_meta = match definition.parameter_metadata() {
            Some(pmds) => parse_metadata_items(pmds.items()),
            _ => MetaMap::default(),
        };
        let inputs = match definition.input() {
            Some(is) => parse_inputs(&is, &parameter_meta, enable_doc_comments),
            _ => Vec::new(),
        };
        let outputs = match definition.output() {
            Some(os) => parse_outputs(&os, &meta, &parameter_meta, enable_doc_comments),
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
            html! { code { (self.name) } }
        }
    }

    /// Renders the meta section of the workflow as HTML.
    ///
    /// This will render all metadata key-value pairs except for `description`,
    /// `name`, `category`, `allowNestedInputs`/`allow_nested_inputs`,
    /// and `outputs`.
    pub fn render_meta(&self, _assets: &Path) -> Option<Markup> {
        self.meta().render_remaining(&[
            DESCRIPTION_KEY,
            NAME_KEY,
            CATEGORY_KEY,
            "allowNestedInputs",
            "allow_nested_inputs",
            "outputs",
        ])
    }

    /// Render the `allowNestedInputs`/`allow_nested_inputs` meta entry as a
    /// badge.
    ///
    /// If the value is `true`, it renders an "allowed badge", in all other
    /// cases it renders a "disabled badge".
    pub fn render_allow_nested_inputs(&self, assets: &Path) -> Markup {
        if let Some(MetaMapValueSource::MetaValue(MetadataValue::Boolean(b))) = self
            .meta
            .get("allowNestedInputs")
            .or(self.meta.get("allow_nested_inputs"))
            && b.value()
        {
            return html! {
                div class="main__badge main__badge--success" {
                    span class="main__badge-status-icon" aria-hidden="true" {
                        img src=(assets.join("check.svg").to_string_lossy()) class="block light:hidden" alt="";
                        img src=(assets.join("check.light.svg").to_string_lossy()) class="hidden light:block" alt="";
                    }
                    span class="main__badge-text" {
                        "Nested Inputs Allowed"
                    }
                }
            };
        }
        html! {
            div class="main__badge main__badge--error" {
                span class="main__badge-status-icon" aria-hidden="true" {
                    img src=(assets.join("x.svg").to_string_lossy()) class="block light:hidden" alt="";
                    img src=(assets.join("x.light.svg").to_string_lossy()) class="hidden light:block" alt="";
                }
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
    pub fn render(
        &self,
        assets: &Path,
        links: &PageLinkIndex,
        page_dir: &Path,
    ) -> (Markup, PageSections) {
        let mut headers = PageSections::default();

        let (input_markup, inner_headers) = self.render_inputs(assets, links, page_dir);

        headers.extend(inner_headers);

        let name_override = self.name_override();
        let (title, title_kind) = match name_override.as_deref() {
            Some(display_name) => (display_name, TitleKind::Plain),
            None => (self.name.as_str(), TitleKind::Identifier),
        };
        let mut hero = DeclarationHero::new("Workflow", title, self.render_description(false))
            .title_kind(title_kind)
            .kind_class("text-brand-emerald-400")
            .pagefind_type("workflow")
            .badge(self.render_version());
        if let Some(badge) = self.render_category() {
            hero = hero.badge(badge);
        }
        hero = hero.badge(self.render_allow_nested_inputs(assets));
        if let Some(path) = self.wdl_path.as_deref() {
            hero = hero.source_path(path);
        }

        let markup = html! {
            (hero.render(assets))
            @if let Some(body) = self.meta().render_authored_body(assets) {
                (body)
            }
            (self.render_run_with(assets))
            @if let Some(meta) = self.render_meta(assets) {
                div class="main__section" {
                    (meta)
                }
            }
            (input_markup)
            (self.render_outputs(assets, links, page_dir))
        };

        headers.push(Header::Header("Outputs".to_string(), "outputs".to_string()));

        (
            main_container("workflow", self.wdl_path.is_none(), markup),
            headers,
        )
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
    use std::path::Path;

    use wdl_ast::Document;
    use wdl_ast::version::V1;

    use super::*;
    use crate::links::PageLinkIndex;

    #[test]
    fn test_workflow() {
        let (doc, _) = Document::parse(
            r#"
            version 1.0

            ## This comment should be ignored.
            workflow test {
                input {
                    String name
                }
                output {
                    String greeting = "Hello, ${name}!"
                }
            }
            "#,
            None,
        );

        let doc_item = doc.ast().into_v1().unwrap().items().next().unwrap();
        let ast_workflow = doc_item.into_workflow_definition().unwrap();

        let workflow = Workflow::new(
            ast_workflow.name().text().to_string(),
            SupportedVersion::V1(V1::Zero),
            ast_workflow,
            None,
            false,
        );

        assert_eq!(workflow.name(), "test");
        assert!(workflow.meta().get("description").is_none());
        assert_eq!(workflow.inputs.len(), 1);
        assert_eq!(workflow.outputs.len(), 1);
    }

    #[test]
    fn workflow_with_doc_comments() {
        let (doc, _) = Document::parse(
            r#"
            version 1.0

            ## This is my workflow. It greets people.
            workflow test {
                input {
                    ## The name to greet.
                    String name
                }
                output {
                    ## The generated greeting.
                    String greeting = "Hello, ${name}!"
                }
            }
            "#,
            None,
        );

        let doc_item = doc.ast().into_v1().unwrap().items().next().unwrap();
        let ast_workflow = doc_item.into_workflow_definition().unwrap();

        let workflow = Workflow::new(
            ast_workflow.name().text().to_string(),
            SupportedVersion::V1(V1::Zero),
            ast_workflow,
            None,
            true,
        );

        assert_eq!(workflow.name(), "test");

        assert_eq!(
            workflow
                .meta()
                .get("description")
                .unwrap()
                .clone()
                .text()
                .unwrap(),
            "This is my workflow. It greets people."
        );
        assert_eq!(workflow.inputs().len(), 1);
        let input = &workflow.inputs()[0];
        assert_eq!(
            input
                .meta()
                .get("description")
                .unwrap()
                .clone()
                .text()
                .unwrap(),
            "The name to greet."
        );

        assert_eq!(workflow.outputs.len(), 1);
        let output = &workflow.outputs()[0];
        assert_eq!(
            output
                .meta()
                .get("description")
                .unwrap()
                .clone()
                .text()
                .unwrap(),
            "The generated greeting."
        );
    }

    #[test]
    fn workflow_hero_uses_plain_title_for_meta_name() {
        let (doc, _) = Document::parse(
            r#"
            version 1.2

            workflow align_reads {
                meta {
                    name: "Align Reads (v2)"
                }
            }
            "#,
            None,
        );

        let doc_item = doc.ast().into_v1().unwrap().items().next().unwrap();
        let ast_workflow = doc_item.into_workflow_definition().unwrap();

        let workflow = Workflow::new(
            ast_workflow.name().text().to_string(),
            SupportedVersion::V1(V1::Two),
            ast_workflow,
            None,
            false,
        );

        let links = PageLinkIndex::default();
        let (markup, _) = workflow.render(Path::new("assets"), &links, Path::new(""));
        let html = markup.into_string();

        // The friendly `meta.name` display name is shown verbatim in the title.
        assert!(html.contains("Align Reads (v2)"));
        // It is a human-facing name, not a WDL identifier, so it must not be
        // wrapped as a code literal.
        assert!(!html.contains("heading-code-literal"));
    }

    #[test]
    fn workflow_hero_uses_code_literal_for_identifier() {
        let (doc, _) = Document::parse(
            r#"
            version 1.2

            workflow align_reads {
            }
            "#,
            None,
        );

        let doc_item = doc.ast().into_v1().unwrap().items().next().unwrap();
        let ast_workflow = doc_item.into_workflow_definition().unwrap();

        let workflow = Workflow::new(
            ast_workflow.name().text().to_string(),
            SupportedVersion::V1(V1::Two),
            ast_workflow,
            None,
            false,
        );

        let links = PageLinkIndex::default();
        let (markup, _) = workflow.render(Path::new("assets"), &links, Path::new(""));
        let html = markup.into_string();

        // Without a `meta.name` override the title is the raw WDL identifier and
        // must be rendered as a code literal.
        assert!(html.contains("<code class=\"heading-code-literal\">align_reads</code>"));
    }

    #[test]
    fn nested_inputs_allowed_badge_is_green_with_checkmark() {
        let (doc, _) = Document::parse(
            r#"
            version 1.2

            workflow nested_inputs {
                meta {
                    allowNestedInputs: true
                }
            }
            "#,
            None,
        );

        // SAFETY: the test document declares WDL version 1.2.
        let ast = doc.ast().into_v1().unwrap();
        // SAFETY: the test document contains one workflow definition.
        let doc_item = ast.items().next().unwrap();
        // SAFETY: the only document item is a workflow definition.
        let ast_workflow = doc_item.into_workflow_definition().unwrap();
        let workflow = Workflow::new(
            ast_workflow.name().text().to_string(),
            SupportedVersion::V1(V1::Two),
            ast_workflow,
            None,
            false,
        );

        let html = workflow
            .render_allow_nested_inputs(std::path::Path::new("assets"))
            .into_string();
        assert!(html.contains("main__badge--success"));
        assert!(html.contains("main__badge-status-icon"));
        assert!(html.contains("assets/check.svg"));
        assert!(html.contains("assets/check.light.svg"));
        assert!(html.contains("Nested Inputs Allowed"));
    }

    #[test]
    fn nested_inputs_disallowed_badge_is_red_with_x() {
        let (doc, _) = Document::parse(
            r#"
            version 1.2

            workflow nested_inputs {
                meta {
                    allowNestedInputs: false
                }
            }
            "#,
            None,
        );

        // SAFETY: the test document declares WDL version 1.2.
        let ast = doc.ast().into_v1().unwrap();
        // SAFETY: the test document contains one workflow definition.
        let doc_item = ast.items().next().unwrap();
        // SAFETY: the only document item is a workflow definition.
        let ast_workflow = doc_item.into_workflow_definition().unwrap();
        let workflow = Workflow::new(
            ast_workflow.name().text().to_string(),
            SupportedVersion::V1(V1::Two),
            ast_workflow,
            None,
            false,
        );

        let html = workflow
            .render_allow_nested_inputs(std::path::Path::new("assets"))
            .into_string();
        assert!(html.contains("main__badge--error"));
        assert!(html.contains("main__badge-status-icon"));
        assert!(html.contains("assets/x.svg"));
        assert!(html.contains("assets/x.light.svg"));
        assert!(html.contains("Nested Inputs Not Allowed"));
    }
}
