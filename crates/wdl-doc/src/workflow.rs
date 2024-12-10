//! Create HTML documentation for WDL tasks.

use std::fmt::Display;

use html::content;
use html::text_content;
use wdl_ast::AstNode;
use wdl_ast::v1::MetadataSection;

use crate::parameter::Parameter;

/// A task in a WDL document.
#[derive(Debug)]
pub struct Workflow {
    /// The name of the task.
    name: String,
    /// The meta section of the task.
    meta_section: Option<MetadataSection>,
    /// The input parameters of the task.
    inputs: Vec<Parameter>,
    /// The output parameters of the task.
    outputs: Vec<Parameter>,
}

impl Workflow {
    /// Create a new task.
    pub fn new(
        name: String,
        meta_section: Option<MetadataSection>,
        inputs: Vec<Parameter>,
        outputs: Vec<Parameter>,
    ) -> Self {
        Self {
            name,
            meta_section,
            inputs,
            outputs,
        }
    }

    /// Get the name of the task.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the meta section of the task.
    pub fn meta_section(&self) -> Option<&MetadataSection> {
        self.meta_section.as_ref()
    }

    /// Get the input parameters of the task.
    pub fn inputs(&self) -> &[Parameter] {
        &self.inputs
    }

    /// Get the output parameters of the task.
    pub fn outputs(&self) -> &[Parameter] {
        &self.outputs
    }
}

impl Display for Workflow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let task_name = content::Heading1::builder()
            .text(self.name().to_owned())
            .build();

        let mut content = text_content::UnorderedList::builder();
        if let Some(meta_section) = self.meta_section() {
            content.push(
                text_content::ListItem::builder()
                    .text("Meta:")
                    .push(meta_section.syntax().to_string())
                    .build(),
            );
        }
        content.push(
            text_content::ListItem::builder()
                .text("Inputs:")
                .push(
                    text_content::UnorderedList::builder()
                        .extend(self.inputs().iter().map(|param| {
                            text_content::ListItem::builder()
                                .push(param.to_string())
                                .build()
                        }))
                        .build(),
                )
                .build(),
        );
        content.push(
            text_content::ListItem::builder()
                .text("Outputs:")
                .push(
                    text_content::UnorderedList::builder()
                        .extend(self.outputs().iter().map(|param| {
                            text_content::ListItem::builder()
                                .push(param.to_string())
                                .build()
                        }))
                        .build(),
                )
                .build(),
        );

        write!(f, "{}", task_name)?;
        write!(f, "{}", content.build())
    }
}
