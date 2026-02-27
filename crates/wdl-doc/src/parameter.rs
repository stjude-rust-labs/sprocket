//! Create HTML documentation for WDL parameters.

use std::path::Path;

use maud::Markup;
use maud::PreEscaped;
use maud::html;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::v1::Decl;

use crate::meta::DESCRIPTION_KEY;
use crate::meta::DefinitionMeta;
use crate::meta::MaybeSummarized;
use crate::meta::MetaMap;
use crate::meta::MetaMapExt;
use crate::meta::MetaMapValueSource;
use crate::meta::summarize_if_needed;

/// The maximum length of an expression before it is summarized.
const EXPR_MAX_LENGTH: usize = 80;
/// The length of an expression when summarized.
const EXPR_CLIP_LENGTH: usize = 50;

/// A group of inputs.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Group(pub String);

impl Group {
    /// Get the display name of the group.
    pub fn display_name(&self) -> &str {
        &self.0
    }

    /// Get the id of the group.
    pub fn id(&self) -> String {
        self.0.replace(" ", "-").to_lowercase()
    }
}

impl PartialOrd for Group {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Group {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.0 == other.0 {
            return std::cmp::Ordering::Equal;
        }
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

/// Whether a parameter is an input or output.
#[derive(Debug, Clone, Copy)]
pub(crate) enum InputOutput {
    /// An input parameter.
    Input,
    /// An output parameter.
    Output,
}

/// A parameter (input or output) in a workflow or task.
#[derive(Debug)]
pub(crate) struct Parameter {
    /// The declaration of the parameter.
    decl: Decl,
    /// Any meta entries associated with the parameter.
    meta: MetaMap,
    /// Whether the parameter is an input or output.
    io: InputOutput,
}

impl DefinitionMeta for Parameter {
    fn meta(&self) -> &MetaMap {
        &self.meta
    }
}

impl Parameter {
    /// Create a new parameter.
    pub fn new(decl: Decl, meta: MetaMap, io: InputOutput) -> Self {
        Self { decl, meta, io }
    }

    /// Get the name of the parameter.
    pub fn name(&self) -> String {
        self.decl.name().text().to_owned()
    }

    /// Get the meta of the parameter.
    pub fn meta(&self) -> &MetaMap {
        &self.meta
    }

    /// Get the type of the parameter as a string.
    pub fn ty(&self) -> String {
        self.decl.ty().to_string()
    }

    /// Get the expr of the parameter as HTML.
    ///
    /// If `summarize` is `false`, the full expression is rendered in a code
    /// block with WDL syntax highlighting.
    pub fn render_expr(&self, summarize: bool) -> Markup {
        let expr = self
            .decl
            .expr()
            .map(|expr| expr.text().to_string())
            .unwrap_or("None".to_string());
        if !summarize {
            // If we are not summarizing, we need to remove the first
            // line from the leading whitespace calculation as the first line never
            // leads with whitespace.
            let mut lines = expr.lines();
            let first_line = lines.next().expect("expr should have at least one line");

            let common_indent = lines
                .clone()
                .map(|line| line.chars().take_while(|c| c.is_whitespace()).count())
                .min()
                .unwrap_or(0);

            let remaining_expr = lines
                .map(|line| line.chars().skip(common_indent).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");

            let full_expr = if remaining_expr.is_empty() {
                first_line
            } else {
                &format!("{first_line}\n{remaining_expr}")
            };

            return html! {
                sprocket-code language="wdl" {
                    (full_expr)
                }
            };
        }

        match summarize_if_needed(expr, EXPR_MAX_LENGTH, EXPR_CLIP_LENGTH) {
            MaybeSummarized::No(expr) => {
                html! { code { (expr) } }
            }
            MaybeSummarized::Yes(summary) => {
                html! {
                    div class="main__summary-container" {
                        code { (summary) }
                        "..."
                        button type="button" class="main__button" x-on:click="expr_expanded = !expr_expanded" {
                            b x-text="expr_expanded ? 'Hide full expression' : 'Show full expression'" {}
                        }
                    }
                }
            }
        }
    }

    /// Get whether the input parameter is required.
    ///
    /// Returns `None` for outputs.
    pub fn required(&self) -> Option<bool> {
        match self.io {
            InputOutput::Input => match self.decl.as_unbound_decl() {
                Some(d) => Some(!d.ty().is_optional()),
                _ => Some(false),
            },
            InputOutput::Output => None,
        }
    }

    /// Get the `group` meta entry of the parameter as a [`Group`], if the meta
    /// entry exists and is a String.
    pub fn group(&self) -> Option<Group> {
        self.meta()
            .get("group")
            .and_then(MetaMapValueSource::text)
            .map(Group)
    }

    /// Render any remaining metadata as HTML.
    ///
    /// This will render all metadata key-value pairs except for `description`
    /// and `group`.
    pub fn render_remaining_meta(&self, assets: &Path) -> Option<Markup> {
        self.meta()
            .render_remaining(&[DESCRIPTION_KEY, "group"], assets)
    }

    /// Render the parameter as HTML.
    pub fn render(&self, assets: &Path) -> Markup {
        let show_expr = self.required() != Some(true);
        html! {
            div class="main__grid-row" x-data=(
                if show_expr { "{ description_expanded: false, expr_expanded: false }" } else { "{ description_expanded: false }" }
            ) {
                div class="main__grid-cell" {
                    code { (self.name()) }
                }
                div class="main__grid-cell" {
                    code { (self.ty()) }
                }
                @if show_expr {
                    div class="main__grid-cell" { (self.render_expr(true)) }
                }
                div class="main__grid-cell" {
                    (self.meta().render_description(true))
                }
                div x-show="description_expanded" class="main__grid-full-width-cell" {
                    (self.meta().render_description(false))
                }
                @if show_expr {
                    div x-show="expr_expanded" class="main__grid-full-width-cell" {
                        (self.render_expr(false))
                    }
                }
            }
            @if let Some(addl_meta) = self.render_remaining_meta(assets) {
                div class="main__grid-full-width-cell" x-data="{ addl_meta_expanded: false }" {
                    div class="main__addl-meta-outer-container" {
                        button type="button" class="main__button" x-on:click="addl_meta_expanded = !addl_meta_expanded" {
                            b x-text="addl_meta_expanded ? 'Hide Additional Meta' : 'Show Additional Metadata'" {}
                        }
                        div x-show="addl_meta_expanded" class="main__addl-meta-inner-container" {
                            (addl_meta)
                        }
                    }
                }
            }
        }
    }
}

/// Render a table for non-required parameters (both inputs and outputs
/// accepted).
///
/// A separate implementation is used for non-required parameters
/// because they require an extra column for the default value (when inputs)
/// or expression (when outputs). This may seem like a duplication on its
/// surface, but because of the way CSS/HTML grids work, this is the most
/// straightforward way to handle the different shape grids.
///
/// The distinction between inputs and outputs is made by checking if the
/// `required` method returns `None` for any of the provided parameters. If it
/// does, the parameter is an output (and all other parameters will also be
/// treated as outputs), and the third column will be labeled "Expression". If
/// it returns `Some(true)` or `Some(false)` for every parameter, they are all
/// inputs and the third column will be labeled "Default".
pub(crate) fn render_non_required_parameters_table<'a, I>(params: I, assets: &Path) -> Markup
where
    I: Iterator<Item = &'a Parameter>,
{
    let params = params.collect::<Vec<_>>();

    let third_col = if params.iter().any(|p| p.required().is_none()) {
        // If any parameter is an output, we use "Expression" as the third column
        // header.
        "Expression"
    } else {
        // If all parameters are inputs, we use "Default" as the third column header.
        "Default"
    };

    let rows = params
        .iter()
        .map(|param| param.render(assets).into_string())
        .collect::<Vec<_>>()
        .join(&html! { div class="main__grid-row-separator" {} }.into_string());

    html! {
        div class="main__grid-container" {
            div class="main__grid-non-req-param-container" {
                div class="main__grid-header-cell" { "Name" }
                div class="main__grid-header-cell" { "Type" }
                div class="main__grid-header-cell" { (third_col) }
                div class="main__grid-header-cell" { "Description" }
                div class="main__grid-header-separator" {}
                (PreEscaped(rows))
            }
        }
    }
}
