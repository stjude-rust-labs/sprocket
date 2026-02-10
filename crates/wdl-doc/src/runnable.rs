//! HTML generation for WDL runnables (workflows and tasks).

pub mod task;
pub mod workflow;

use std::collections::BTreeSet;
use std::path::MAIN_SEPARATOR;
use std::path::Path;

use maud::Markup;
use maud::PreEscaped;
use maud::html;
use wdl_ast::AstToken;
use wdl_ast::v1::InputSection;
use wdl_ast::v1::MetadataValue;
use wdl_ast::v1::OutputSection;

use crate::VersionBadge;
use crate::docs_tree::Header;
use crate::docs_tree::PageSections;
use crate::meta::DefinitionMeta;
use crate::meta::MetaMap;
use crate::meta::MetaMapExt;
use crate::meta::MetaMapValueSource;
use crate::parameter::Group;
use crate::parameter::InputOutput;
use crate::parameter::Parameter;
use crate::parameter::render_non_required_parameters_table;

/// A runnable (workflow or task) in a WDL document.
pub(crate) trait Runnable: DefinitionMeta {
    /// Get the name of the runnable.
    fn name(&self) -> &str;

    /// Get the inputs of the runnable.
    fn inputs(&self) -> &[Parameter];

    /// Get the outputs of the runnable.
    fn outputs(&self) -> &[Parameter];

    /// Get the [`VersionBadge`] of the runnable.
    fn version(&self) -> &VersionBadge;

    /// Get the path from the root of the WDL workspace to the WDL document
    /// which contains this runnable.
    fn wdl_path(&self) -> Option<&Path>;

    /// Get the required input parameters of the runnable.
    fn required_inputs(&self) -> impl Iterator<Item = &Parameter> {
        self.inputs().iter().filter(|param| {
            param
                .required()
                .expect("inputs should return Some(required)")
        })
    }

    /// Get the sorted set of unique `group` values of the inputs.
    ///
    /// The `Common` group, if present, will always be first in the set,
    /// followed by any other groups in alphabetical order, and lastly
    /// the `Resources` group.
    fn input_groups(&self) -> BTreeSet<Group> {
        self.inputs()
            .iter()
            .filter_map(|param| param.group())
            .map(|arg0: Group| Group(arg0.0.clone()))
            .collect()
    }

    /// Get the inputs of the runnable that are part of `group`.
    fn inputs_in_group<'a>(&'a self, group: &'a Group) -> impl Iterator<Item = &'a Parameter> {
        self.inputs().iter().filter(move |param| {
            if let Some(param_group) = param.group()
                && param_group == *group
            {
                return true;
            }
            false
        })
    }

    /// Get the inputs of the runnable that are neither required nor part of a
    /// group.
    fn other_inputs(&self) -> impl Iterator<Item = &Parameter> {
        self.inputs().iter().filter(|param| {
            !param
                .required()
                .expect("inputs should return Some(required)")
                && param.group().is_none()
        })
    }

    /// Render the version of the runnable as a badge.
    fn render_version(&self) -> Markup {
        self.version().render()
    }

    /// Render the "run with" component of the runnable.
    fn render_run_with(&self, _assets: &Path) -> Markup {
        if let Some(wdl_path) = self.wdl_path() {
            let unix_path = wdl_path.to_string_lossy().replace(MAIN_SEPARATOR, "/");
            let windows_path = wdl_path.to_string_lossy().replace(MAIN_SEPARATOR, "\\");
            html! {
                div x-data="{ unix: true }" class="main__run-with-container" data-pagefind-ignore="all" {
                    div class="main__run-with-label" {
                        span class="main__run-with-label-text" {
                            "RUN WITH"
                        }
                        button x-on:click="unix = !unix" class="main__run-with-toggle" {
                            div x-bind:class="unix ? 'main__run-with-toggle-label--active' : 'main__run-with-toggle-label--inactive'" {
                                "Unix"
                            }
                            div x-bind:class="!unix ? 'main__run-with-toggle-label--active' : 'main__run-with-toggle-label--inactive'" {
                                "Windows"
                            }
                        }
                    }
                    div class="main__run-with-content" {
                        p class="main__run-with-content-text" {
                            "sprocket run --target "
                            (self.name())
                            " "
                            span x-show="unix" {
                                (unix_path)
                            }
                            span x-show="!unix" {
                                (windows_path)
                            }
                            " [INPUTS]..."
                        }
                    }
                }
            }
        } else {
            html! {}
        }
    }

    /// Render the required inputs of the runnable if present.
    fn render_required_inputs(&self, assets: &Path) -> Option<Markup> {
        let mut iter = self.required_inputs().peekable();
        iter.peek()?;

        let rows = iter
            .map(|param| param.render(assets).into_string())
            .collect::<Vec<_>>()
            .join(&html! { div class="main__grid-row-separator" {} }.into_string());

        Some(html! {
            h3 id="required-inputs" class="main__section-subheader" { "Required Inputs" }
            div class="main__grid-container" {
                div class="main__grid-req-inputs-container" {
                    div class="main__grid-header-cell" { "Name" }
                    div class="main__grid-header-cell" { "Type" }
                    div class="main__grid-header-cell" { "Description" }
                    div class="main__grid-header-separator" {}
                    (PreEscaped(rows))
                }
            }
        })
    }

    /// Render the inputs with a group of the runnable if present.
    ///
    /// This will render each group with a subheader and a table
    /// of parameters that are part of that group.
    fn render_group_inputs(&self, assets: &Path) -> Option<Markup> {
        let mut group_tables = self
            .input_groups()
            .into_iter()
            .map(|group| {
                html! {
                    h3 id=(group.id()) class="main__section-subheader" { (group.display_name()) }
                    (render_non_required_parameters_table(self.inputs_in_group(&group), assets))
                }
            })
            .peekable();
        group_tables.peek()?;

        Some(html! {
            @for group_table in group_tables {
                (group_table)
            }
        })
    }

    /// Render the inputs that are neither required nor part of a group if
    /// present.
    fn render_other_inputs(&self, assets: &Path) -> Option<Markup> {
        let mut iter = self.other_inputs().peekable();
        iter.peek()?;

        Some(html! {
            h3 id="other-inputs" class="main__section-subheader" { "Other Inputs" }
            (render_non_required_parameters_table(iter, assets))
        })
    }

    /// Render the inputs of the runnable.
    fn render_inputs(&self, assets: &Path) -> (Markup, PageSections) {
        let mut inner_markup = Vec::new();
        let mut headers = PageSections::default();
        headers.push(Header::Header("Inputs".to_string(), "inputs".to_string()));
        if let Some(req) = self.render_required_inputs(assets) {
            inner_markup.push(req);
            headers.push(Header::SubHeader(
                "Required Inputs".to_string(),
                "required-inputs".to_string(),
            ));
        }
        if let Some(group) = self.render_group_inputs(assets) {
            inner_markup.push(group);
            for group in self.input_groups() {
                headers.push(Header::SubHeader(
                    group.display_name().to_string(),
                    group.id(),
                ));
            }
        }
        if let Some(other) = self.render_other_inputs(assets) {
            inner_markup.push(other);
            headers.push(Header::SubHeader(
                "Other Inputs".to_string(),
                "other-inputs".to_string(),
            ));
        }
        let markup = html! {
            div class="main__section" {
                h2 id="inputs" class="main__section-header" { "Inputs" }
                @for section in inner_markup {
                    (section)
                }
            }
        };

        (markup, headers)
    }

    /// Render the outputs of the runnable.
    fn render_outputs(&self, assets: &Path) -> Markup {
        html! {
            div class="main__section" {
                h2 id="outputs" class="main__section-header" { "Outputs" }
                (render_non_required_parameters_table(self.outputs().iter(), assets))
            }
        }
    }
}

/// Parse the [`InputSection`] into a vector of [`Parameter`]s.
fn parse_inputs(input_section: &InputSection, parameter_meta: &MetaMap) -> Vec<Parameter> {
    input_section
        .declarations()
        .map(|decl| {
            let name = decl.name().text().to_owned();
            let meta = parameter_meta.get(&name);
            Parameter::new(decl.clone(), meta.cloned(), InputOutput::Input)
        })
        .collect()
}

// TODO: Collect doc comments on outputs
/// Parse the [`OutputSection`] into a vector of [`Parameter`]s.
fn parse_outputs(
    output_section: &OutputSection,
    meta: &MetaMap,
    parameter_meta: &MetaMap,
) -> Vec<Parameter> {
    let output_meta: MetaMap = meta
        .get("outputs")
        .and_then(|v| match v {
            MetaMapValueSource::MetaValue(MetadataValue::Object(o)) => Some(o),
            _ => None,
        })
        .map(|o| {
            o.items()
                .map(|i| {
                    (
                        i.name().text().to_owned(),
                        MetaMapValueSource::MetaValue(i.value().clone()),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    output_section
        .declarations()
        .map(|decl| {
            let name = decl.name().text().to_owned();
            let meta = parameter_meta.get(&name).or_else(|| output_meta.get(&name));
            Parameter::new(
                wdl_ast::v1::Decl::Bound(decl.clone()),
                meta.cloned(),
                InputOutput::Output,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use wdl_ast::Document;

    use super::*;
    use crate::meta::DEFAULT_DESCRIPTION;
    use crate::meta::parse_metadata_items;
    use crate::parameter::Group;

    #[test]
    fn test_group_cmp() {
        let common = Group("Common".to_string());
        let resources = Group("Resources".to_string());
        let a = Group("A".to_string());
        let b = Group("B".to_string());
        let c = Group("C".to_string());

        let mut groups = vec![c, a, resources, common, b];
        groups.sort();
        assert_eq!(
            groups,
            vec![
                Group("Common".to_string()),
                Group("A".to_string()),
                Group("B".to_string()),
                Group("C".to_string()),
                Group("Resources".to_string()),
            ]
        );
    }

    #[test]
    fn test_parse_meta() {
        let wdl = r#"
        version 1.1

        workflow wf {
            meta {
                name: "Workflow"
                description: "A workflow"
            }
        }
        "#;

        let (doc, _) = Document::parse(wdl);
        let doc_item = doc.ast().into_v1().unwrap().items().next().unwrap();
        let meta_map = parse_metadata_items(
            doc_item
                .as_workflow_definition()
                .unwrap()
                .metadata()
                .unwrap()
                .items(),
        );
        assert_eq!(meta_map.len(), 2);
        assert_eq!(
            meta_map
                .get("name")
                .unwrap()
                .clone()
                .into_meta()
                .unwrap()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "Workflow"
        );
        assert_eq!(
            meta_map
                .get("description")
                .unwrap()
                .clone()
                .into_meta()
                .unwrap()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "A workflow"
        );
    }

    #[test]
    fn test_parse_parameter_meta() {
        let wdl = r#"
        version 1.1

        workflow wf {
            input {
                Int a
            }
            parameter_meta {
                a: {
                    description: "An integer"
                }
            }
        }
        "#;

        let (doc, _) = Document::parse(wdl);
        let doc_item = doc.ast().into_v1().unwrap().items().next().unwrap();
        let meta_map = parse_metadata_items(
            doc_item
                .as_workflow_definition()
                .unwrap()
                .parameter_metadata()
                .unwrap()
                .items(),
        );
        assert_eq!(meta_map.len(), 1);
        assert_eq!(
            meta_map
                .get("a")
                .unwrap()
                .clone()
                .into_meta()
                .unwrap()
                .unwrap_object()
                .items()
                .next()
                .unwrap()
                .value()
                .clone()
                .unwrap_string()
                .text()
                .unwrap()
                .text(),
            "An integer"
        );
    }

    #[test]
    fn test_parse_inputs() {
        let wdl = r#"
        version 1.1

        workflow wf {
            input {
                Int a
                Int b
                Int c
            }
            parameter_meta {
                a: "An integer"
                c: {
                    description: "Another integer"
                }
            }
        }
        "#;

        let (doc, _) = Document::parse(wdl);
        let doc_item = doc.ast().into_v1().unwrap().items().next().unwrap();
        let meta_map = parse_metadata_items(
            doc_item
                .as_workflow_definition()
                .unwrap()
                .parameter_metadata()
                .unwrap()
                .items(),
        );
        let inputs = parse_inputs(
            &doc_item.as_workflow_definition().unwrap().input().unwrap(),
            &meta_map,
        );
        assert_eq!(inputs.len(), 3);
        assert_eq!(inputs[0].name(), "a");
        assert_eq!(inputs[0].description(false).into_string(), "An integer");
        assert_eq!(inputs[1].name(), "b");
        assert_eq!(
            inputs[1].description(false).into_string(),
            DEFAULT_DESCRIPTION
        );
        assert_eq!(inputs[2].name(), "c");
        assert_eq!(
            inputs[2].description(false).into_string(),
            "Another integer"
        );
    }

    #[test]
    fn test_parse_outputs() {
        let wdl = r#"
        version 1.1

        workflow wf {
            output {
                Int a = 1
                Int b = 2
                Int c = 3
            }
            meta {
                outputs: {
                    a: "An integer"
                    c: {
                        description: "Another integer"
                    }
                }
            }
            parameter_meta {
                b: "A different place!"
            }
        }
        "#;

        let (doc, _) = Document::parse(wdl);
        let doc_item = doc.ast().into_v1().unwrap().items().next().unwrap();
        let meta_map = parse_metadata_items(
            doc_item
                .as_workflow_definition()
                .unwrap()
                .metadata()
                .unwrap()
                .items(),
        );
        let parameter_meta = parse_metadata_items(
            doc_item
                .as_workflow_definition()
                .unwrap()
                .parameter_metadata()
                .unwrap()
                .items(),
        );
        let outputs = parse_outputs(
            &doc_item.as_workflow_definition().unwrap().output().unwrap(),
            &meta_map,
            &parameter_meta,
        );
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].name(), "a");
        assert_eq!(outputs[0].description(false).into_string(), "An integer");
        assert_eq!(outputs[1].name(), "b");
        assert_eq!(
            outputs[1].description(false).into_string(),
            "A different place!"
        );
        assert_eq!(outputs[2].name(), "c");
        assert_eq!(
            outputs[2].description(false).into_string(),
            "Another integer"
        );
    }
}
