//! HTML generation for WDL callables (workflows and tasks).

pub mod task;
pub mod workflow;

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use maud::Markup;
use maud::html;
use wdl_ast::AstToken;
use wdl_ast::v1::InputSection;
use wdl_ast::v1::MetadataSection;
use wdl_ast::v1::MetadataValue;
use wdl_ast::v1::OutputSection;
use wdl_ast::v1::ParameterMetadataSection;

use crate::meta::render_value;
use crate::parameter::InputOutput;
use crate::parameter::Parameter;

/// A map of metadata key-value pairs, sorted by key.
type MetaMap = BTreeMap<String, MetadataValue>;

/// A group of inputs.
#[derive(Debug, Eq, PartialEq)]
pub struct Group(pub String);

impl PartialOrd for Group {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Group {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.0 == "Common" {
            return std::cmp::Ordering::Less;
        }
        if other.0 == "Common" {
            return std::cmp::Ordering::Greater;
        }
        if self.0 == "Resources" {
            return std::cmp::Ordering::Greater;
        }
        if other.0 == "Resources" {
            return std::cmp::Ordering::Less;
        }
        self.0.cmp(&other.0)
    }
}

/// A callable (workflow or task) in a WDL document.
pub trait Callable {
    /// Get the name of the callable.
    fn name(&self) -> &str;

    /// Get the [`MetaMap`] of the callable.
    fn meta(&self) -> &MetaMap;

    /// Get the inputs of the callable.
    fn inputs(&self) -> &[Parameter];

    /// Get the outputs of the callable.
    fn outputs(&self) -> &[Parameter];

    /// Get the description of the callable.
    fn description(&self) -> Markup {
        self.meta()
            .get("description")
            .map(render_value)
            .unwrap_or_else(|| html! {})
    }

    /// Get the required input parameters of the callable.
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

    /// Get the inputs of the callable that are part of `group`.
    fn inputs_in_group<'a>(&'a self, group: &'a Group) -> impl Iterator<Item = &'a Parameter> {
        self.inputs().iter().filter(move |param| {
            if let Some(param_group) = param.group() {
                if param_group == *group {
                    return true;
                }
            }
            false
        })
    }

    /// Get the inputs of the callable that are neither required nor part of a
    /// group.
    fn other_inputs(&self) -> impl Iterator<Item = &Parameter> {
        self.inputs().iter().filter(|param| {
            !param
                .required()
                .expect("inputs should return Some(required)")
                && param.group().is_none()
        })
    }

    /// Render the required inputs of the callable.
    fn render_required_inputs(&self) -> Markup {
        let mut iter = self.required_inputs().peekable();
        if iter.peek().is_some() {
            return html! {
                h3 { "Required Inputs" }
                table class="border" {
                    thead class="border" { tr {
                        th { "Name" }
                        th { "Type" }
                        th { "Description" }
                        th { "Additional Meta" }
                    }}
                    tbody class="border" {
                        @for param in iter {
                            (param.render())
                        }
                    }
                }
            };
        };
        html! {}
    }

    /// Render the inputs with a group of the callable.
    fn render_group_inputs(&self) -> Markup {
        let group_tables = self.input_groups().into_iter().map(|group| {
            html! {
                h3 { (group.0) }
                table class="border" {
                    thead class="border" { tr {
                        th { "Name" }
                        th { "Type" }
                        th { "Default" }
                        th { "Description" }
                        th { "Additional Meta" }
                    }}
                    tbody class="border" {
                        @for param in self.inputs_in_group(&group) {
                            (param.render())
                        }
                    }
                }
            }
        });
        html! {
            @for group_table in group_tables {
                (group_table)
            }
        }
    }

    /// Render the inputs that are neither required nor part of a group.
    fn render_other_inputs(&self) -> Markup {
        let mut iter = self.other_inputs().peekable();
        if iter.peek().is_some() {
            return html! {
                h3 { "Other Inputs" }
                table class="border" {
                    thead class="border" { tr {
                        th { "Name" }
                        th { "Type" }
                        th { "Default" }
                        th { "Description" }
                        th { "Additional Meta" }
                    }}
                    tbody class="border" {
                        @for param in iter {
                            (param.render())
                        }
                    }
                }
            };
        };
        html! {}
    }

    /// Render the inputs of the callable.
    fn render_inputs(&self) -> Markup {
        html! {
            h2 { "Inputs" }
            (self.render_required_inputs())
            (self.render_group_inputs())
            (self.render_other_inputs())
        }
    }

    /// Render the outputs of the callable.
    fn render_outputs(&self) -> Markup {
        html! {
            h2 { "Outputs" }
            table  {
                thead class="border" { tr {
                    th { "Name" }
                    th { "Type" }
                    th { "Expression" }
                    th { "Description" }
                    th { "Additional Meta" }
                }}
                tbody class="border" {
                    @for param in self.outputs() {
                        (param.render())
                    }
                }
            }
        }
    }
}

/// Parse a [`MetadataSection`] into a [`MetaMap`].
fn parse_meta(meta: &MetadataSection) -> MetaMap {
    meta.items()
        .map(|m| {
            let name = m.name().as_str().to_owned();
            let item = m.value();
            (name, item)
        })
        .collect()
}

/// Parse a [`ParameterMetadataSection`] into a [`MetaMap`].
fn parse_parameter_meta(parameter_meta: &ParameterMetadataSection) -> MetaMap {
    parameter_meta
        .items()
        .map(|m| {
            let name = m.name().as_str().to_owned();
            let item = m.value();
            (name, item)
        })
        .collect()
}

/// Parse the [`InputSection`] into a vector of [`Parameter`]s.
fn parse_inputs(input_section: &InputSection, parameter_meta: &MetaMap) -> Vec<Parameter> {
    input_section
        .declarations()
        .map(|decl| {
            let name = decl.name().as_str().to_owned();
            let meta = parameter_meta.get(&name);
            Parameter::new(decl.clone(), meta.cloned(), InputOutput::Input)
        })
        .collect()
}

/// Parse the [`OutputSection`] into a vector of [`Parameter`]s.
fn parse_outputs(
    output_section: &OutputSection,
    meta: &MetaMap,
    parameter_meta: &MetaMap,
) -> Vec<Parameter> {
    let output_meta: MetaMap = meta
        .get("outputs")
        .and_then(|v| match v {
            MetadataValue::Object(o) => Some(o),
            _ => None,
        })
        .map(|o| {
            o.items()
                .map(|i| (i.name().as_str().to_owned(), i.value().clone()))
                .collect()
        })
        .unwrap_or_default();

    output_section
        .declarations()
        .map(|decl| {
            let name = decl.name().as_str().to_owned();
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
        let meta_map = parse_meta(
            &doc_item
                .as_workflow_definition()
                .unwrap()
                .metadata()
                .unwrap(),
        );
        assert_eq!(meta_map.len(), 2);
        assert_eq!(
            meta_map
                .get("name")
                .unwrap()
                .clone()
                .unwrap_string()
                .text()
                .unwrap()
                .as_str(),
            "Workflow"
        );
        assert_eq!(
            meta_map
                .get("description")
                .unwrap()
                .clone()
                .unwrap_string()
                .text()
                .unwrap()
                .as_str(),
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
        let meta_map = parse_parameter_meta(
            &doc_item
                .as_workflow_definition()
                .unwrap()
                .parameter_metadata()
                .unwrap(),
        );
        assert_eq!(meta_map.len(), 1);
        assert_eq!(
            meta_map
                .get("a")
                .unwrap()
                .clone()
                .unwrap_object()
                .items()
                .next()
                .unwrap()
                .value()
                .clone()
                .unwrap_string()
                .text()
                .unwrap()
                .as_str(),
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
        let meta_map = parse_parameter_meta(
            &doc_item
                .as_workflow_definition()
                .unwrap()
                .parameter_metadata()
                .unwrap(),
        );
        let inputs = parse_inputs(
            &doc_item.as_workflow_definition().unwrap().input().unwrap(),
            &meta_map,
        );
        assert_eq!(inputs.len(), 3);
        assert_eq!(inputs[0].name(), "a");
        assert_eq!(inputs[0].description().into_string(), "An integer");
        assert_eq!(inputs[1].name(), "b");
        assert_eq!(inputs[1].description().into_string(), "");
        assert_eq!(inputs[2].name(), "c");
        assert_eq!(inputs[2].description().into_string(), "Another integer");
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
        let meta_map = parse_meta(
            &doc_item
                .as_workflow_definition()
                .unwrap()
                .metadata()
                .unwrap(),
        );
        let parameter_meta = parse_parameter_meta(
            &doc_item
                .as_workflow_definition()
                .unwrap()
                .parameter_metadata()
                .unwrap(),
        );
        let outputs = parse_outputs(
            &doc_item.as_workflow_definition().unwrap().output().unwrap(),
            &meta_map,
            &parameter_meta,
        );
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].name(), "a");
        assert_eq!(outputs[0].description().into_string(), "An integer");
        assert_eq!(outputs[1].name(), "b");
        assert_eq!(outputs[1].description().into_string(), "A different place!");
        assert_eq!(outputs[2].name(), "c");
        assert_eq!(outputs[2].description().into_string(), "Another integer");
    }
}
