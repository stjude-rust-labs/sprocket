//! Create HTML documentation for WDL parameters.

use maud::Markup;
use maud::html;
use wdl_ast::AstToken;
use wdl_ast::v1::Decl;
use wdl_ast::v1::MetadataValue;

use crate::callable::Group;
use crate::meta::render_value;

/// Whether a parameter is an input or output.
#[derive(Debug, Clone, Copy)]
pub enum InputOutput {
    /// An input parameter.
    Input,
    /// An output parameter.
    Output,
}

/// A parameter (input or output) in a workflow or task.
#[derive(Debug)]
pub struct Parameter {
    /// The declaration of the parameter.
    decl: Decl,
    /// Any meta entries associated with the parameter.
    meta: Option<MetadataValue>,
    /// Whether the parameter is an input or output.
    io: InputOutput,
}

impl Parameter {
    /// Create a new parameter.
    pub fn new(decl: Decl, meta: Option<MetadataValue>, io: InputOutput) -> Self {
        Self { decl, meta, io }
    }

    /// Get the name of the parameter.
    pub fn name(&self) -> String {
        self.decl.name().as_str().to_owned()
    }

    /// Get the type of the parameter.
    pub fn ty(&self) -> String {
        self.decl.ty().to_string()
    }

    /// Get whether the parameter is an input or output.
    pub fn io(&self) -> InputOutput {
        self.io
    }

    /// Get the Expr value of the parameter as a String.
    pub fn expr(&self) -> String {
        self.decl
            .expr()
            .map(|expr| expr.syntax().to_string())
            .unwrap_or("None".to_string())
    }

    /// Get whether the input parameter is required.
    ///
    /// Returns `None` for outputs.
    pub fn required(&self) -> Option<bool> {
        match self.io {
            InputOutput::Input => {
                if let Some(d) = self.decl.as_unbound_decl() {
                    Some(!d.ty().is_optional())
                } else {
                    Some(false)
                }
            }
            InputOutput::Output => None,
        }
    }

    /// Get the "group" of the parameter.
    pub fn group(&self) -> Option<Group> {
        if let Some(MetadataValue::Object(o)) = &self.meta {
            for item in o.items() {
                if item.name().as_str() == "group" {
                    if let MetadataValue::String(s) = item.value() {
                        return s.text().map(|t| t.as_str().to_string()).map(Group);
                    }
                }
            }
        }
        None
    }

    /// Get the description of the parameter.
    pub fn description(&self) -> Markup {
        if let Some(meta) = &self.meta {
            if let MetadataValue::String(_) = meta {
                return render_value(meta);
            } else if let MetadataValue::Object(o) = meta {
                for item in o.items() {
                    if item.name().as_str() == "description" {
                        if let MetadataValue::String(_) = item.value() {
                            return render_value(&item.value());
                        }
                    }
                }
            }
        }
        html! {}
    }

    /// Render the remaining metadata as HTML.
    ///
    /// This will render any metadata that is not rendered elsewhere.
    pub fn render_remaining_meta(&self) -> Markup {
        if let Some(MetadataValue::Object(o)) = &self.meta {
            let filtered_items = o.items().filter(|item| {
                item.name().as_str() != "description" && item.name().as_str() != "group"
            });
            return html! {
                ul {
                    @for item in filtered_items {
                        li {
                            b { (item.name().as_str()) ":" } " " (render_value(&item.value()))
                        }
                    }
                }
            };
        }
        html! {}
    }

    /// Render the parameter as HTML.
    pub fn render(&self) -> Markup {
        if self.required() == Some(true) {
            html! {
                tr class="border" {
                    td class="border" { (self.name()) }
                    td class="border" { code { (self.ty()) } }
                    td class="border" { (self.description()) }
                    td class="border" { (self.render_remaining_meta()) }
                }
            }
        } else {
            html! {
                tr class="border" {
                    td class="border" { (self.name()) }
                    td class="border" { code { (self.ty()) } }
                    td class="border" { code { (self.expr()) } }
                    td class="border" { (self.description()) }
                    td class="border" { (self.render_remaining_meta()) }
                }
            }
        }
    }
}
