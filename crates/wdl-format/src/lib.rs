//! Formatting facilities for WDL.

pub mod config;
pub mod element;
mod token;
pub mod v1;

use std::fmt::Write;

pub use config::*;
pub use token::*;
use wdl_ast::Element;
use wdl_ast::Node as AstNode;

use crate::element::FormatElement;

/// Newline constant used for formatting on windows platforms.
#[cfg(windows)]
pub const NEWLINE: &str = "\r\n";
/// Newline constant used for formatting on non-windows platforms.
#[cfg(not(windows))]
pub const NEWLINE: &str = "\n";
/// A space.
pub const SPACE: &str = " ";
/// A tab.
pub const TAB: &str = "\t";

/// An element that can be written to a token stream.
pub trait Writable {
    /// Writes the element to the token stream.
    fn write(&self, stream: &mut TokenStream<PreToken>, config: Option<&Config>);
}

impl Writable for &FormatElement {
    fn write(&self, stream: &mut TokenStream<PreToken>, config: Option<&Config>) {
        match self.element() {
            Element::Node(node) => match node {
                AstNode::AccessExpr(_) => v1::expr::format_access_expr(self, stream),
                AstNode::AdditionExpr(_) => v1::expr::format_addition_expr(self, stream),
                AstNode::ArrayType(_) => v1::decl::format_array_type(self, stream),
                AstNode::Ast(_) => v1::format_ast(self, stream),
                AstNode::BoundDecl(_) => v1::decl::format_bound_decl(self, stream),
                AstNode::CallAfter(_) => v1::workflow::call::format_call_after(self, stream),
                AstNode::CallAlias(_) => v1::workflow::call::format_call_alias(self, stream),
                AstNode::CallExpr(_) => v1::expr::format_call_expr(self, stream),
                AstNode::CallInputItem(_) => {
                    v1::workflow::call::format_call_input_item(self, stream)
                }
                AstNode::CallStatement(_) => {
                    v1::workflow::call::format_call_statement(self, stream)
                }
                AstNode::CallTarget(_) => v1::workflow::call::format_call_target(self, stream),
                AstNode::CommandSection(_) => v1::task::format_command_section(self, stream),
                AstNode::ConditionalStatement(_) => {
                    v1::workflow::format_conditional_statement(self, stream)
                }
                AstNode::ConditionalStatementClause(_) => {
                    v1::workflow::format_conditional_statement_clause(self, stream)
                }
                AstNode::DefaultOption(_) => v1::expr::format_default_option(self, stream),
                AstNode::DivisionExpr(_) => v1::expr::format_division_expr(self, stream),
                AstNode::EqualityExpr(_) => v1::expr::format_equality_expr(self, stream),
                AstNode::EnumDefinition(_) => v1::r#enum::format_enum_definition(self, stream),
                AstNode::EnumTypeParameter(_) => {
                    v1::r#enum::format_enum_type_parameter(self, stream)
                }
                AstNode::EnumVariant(_) => v1::r#enum::format_enum_variant(self, stream),
                AstNode::ExponentiationExpr(_) => {
                    v1::expr::format_exponentiation_expr(self, stream)
                }
                AstNode::GreaterEqualExpr(_) => v1::expr::format_greater_equal_expr(self, stream),
                AstNode::GreaterExpr(_) => v1::expr::format_greater_expr(self, stream),
                AstNode::IfExpr(_) => v1::expr::format_if_expr(self, stream),
                AstNode::ImportAlias(_) => v1::import::format_import_alias(self, stream),
                AstNode::ImportStatement(_) => v1::import::format_import_statement(self, stream),
                AstNode::IndexExpr(_) => v1::expr::format_index_expr(self, stream),
                AstNode::InequalityExpr(_) => v1::expr::format_inequality_expr(self, stream),
                AstNode::InputSection(_) => v1::format_input_section(self, stream, config),
                AstNode::LessEqualExpr(_) => v1::expr::format_less_equal_expr(self, stream),
                AstNode::LessExpr(_) => v1::expr::format_less_expr(self, stream),
                AstNode::LiteralArray(_) => v1::expr::format_literal_array(self, stream),
                AstNode::LiteralBoolean(_) => v1::expr::format_literal_boolean(self, stream),
                AstNode::LiteralFloat(_) => v1::expr::format_literal_float(self, stream),
                AstNode::LiteralHints(_) => v1::format_literal_hints(self, stream),
                AstNode::LiteralHintsItem(_) => v1::format_literal_hints_item(self, stream),
                AstNode::LiteralInput(_) => v1::format_literal_input(self, stream),
                AstNode::LiteralInputItem(_) => v1::format_literal_input_item(self, stream),
                AstNode::LiteralInteger(_) => v1::expr::format_literal_integer(self, stream),
                AstNode::LiteralMap(_) => v1::expr::format_literal_map(self, stream),
                AstNode::LiteralMapItem(_) => v1::expr::format_literal_map_item(self, stream),
                AstNode::LiteralNone(_) => v1::expr::format_literal_none(self, stream),
                AstNode::LiteralNull(_) => v1::meta::format_literal_null(self, stream),
                AstNode::LiteralObject(_) => v1::expr::format_literal_object(self, stream),
                AstNode::LiteralObjectItem(_) => v1::expr::format_literal_object_item(self, stream),
                AstNode::LiteralOutput(_) => v1::format_literal_output(self, stream),
                AstNode::LiteralOutputItem(_) => v1::format_literal_output_item(self, stream),
                AstNode::LiteralPair(_) => v1::expr::format_literal_pair(self, stream),
                AstNode::LiteralString(_) => v1::expr::format_literal_string(self, stream),
                AstNode::LiteralStruct(_) => v1::r#struct::format_literal_struct(self, stream),
                AstNode::LiteralStructItem(_) => {
                    v1::r#struct::format_literal_struct_item(self, stream)
                }
                AstNode::LogicalAndExpr(_) => v1::expr::format_logical_and_expr(self, stream),
                AstNode::LogicalNotExpr(_) => v1::expr::format_logical_not_expr(self, stream),
                AstNode::LogicalOrExpr(_) => v1::expr::format_logical_or_expr(self, stream),
                AstNode::MapType(_) => v1::decl::format_map_type(self, stream),
                AstNode::MetadataArray(_) => v1::meta::format_metadata_array(self, stream),
                AstNode::MetadataObject(_) => v1::meta::format_metadata_object(self, stream),
                AstNode::MetadataObjectItem(_) => {
                    v1::meta::format_metadata_object_item(self, stream)
                }
                AstNode::MetadataSection(_) => v1::meta::format_metadata_section(self, stream),
                AstNode::ModuloExpr(_) => v1::expr::format_modulo_expr(self, stream),
                AstNode::MultiplicationExpr(_) => {
                    v1::expr::format_multiplication_expr(self, stream)
                }
                AstNode::NameRefExpr(_) => v1::expr::format_name_ref_expr(self, stream),
                AstNode::NegationExpr(_) => v1::expr::format_negation_expr(self, stream),
                AstNode::OutputSection(_) => v1::format_output_section(self, stream),
                AstNode::PairType(_) => v1::decl::format_pair_type(self, stream),
                AstNode::ObjectType(_) => v1::decl::format_object_type(self, stream),
                AstNode::ParameterMetadataSection(_) => {
                    v1::meta::format_parameter_metadata_section(self, stream)
                }
                AstNode::ParenthesizedExpr(_) => v1::expr::format_parenthesized_expr(self, stream),
                AstNode::Placeholder(_) => v1::expr::format_placeholder(self, stream),
                AstNode::PrimitiveType(_) => v1::decl::format_primitive_type(self, stream),
                AstNode::RequirementsItem(_) => v1::task::format_requirements_item(self, stream),
                AstNode::RequirementsSection(_) => {
                    v1::task::format_requirements_section(self, stream)
                }
                AstNode::RuntimeItem(_) => v1::task::format_runtime_item(self, stream),
                AstNode::RuntimeSection(_) => v1::task::format_runtime_section(self, stream),
                AstNode::ScatterStatement(_) => {
                    v1::workflow::format_scatter_statement(self, stream)
                }
                AstNode::SepOption(_) => v1::expr::format_sep_option(self, stream),
                AstNode::StructDefinition(_) => {
                    v1::r#struct::format_struct_definition(self, stream)
                }
                AstNode::SubtractionExpr(_) => v1::expr::format_subtraction_expr(self, stream),
                AstNode::TaskDefinition(_) => v1::task::format_task_definition(self, stream),
                AstNode::TaskHintsItem(_) => v1::task::format_task_hints_item(self, stream),
                AstNode::TaskHintsSection(_) => v1::task::format_task_hints_section(self, stream),
                AstNode::TrueFalseOption(_) => v1::expr::format_true_false_option(self, stream),
                AstNode::TypeRef(_) => v1::decl::format_type_ref(self, stream),
                AstNode::UnboundDecl(_) => v1::decl::format_unbound_decl(self, stream),
                AstNode::VersionStatement(_) => v1::format_version_statement(self, stream),
                AstNode::WorkflowDefinition(_) => {
                    v1::workflow::format_workflow_definition(self, stream)
                }
                AstNode::WorkflowHintsArray(_) => {
                    v1::workflow::format_workflow_hints_array(self, stream)
                }
                AstNode::WorkflowHintsItem(_) => {
                    v1::workflow::format_workflow_hints_item(self, stream)
                }
                AstNode::WorkflowHintsObject(_) => {
                    v1::workflow::format_workflow_hints_object(self, stream)
                }
                AstNode::WorkflowHintsObjectItem(_) => {
                    v1::workflow::format_workflow_hints_object_item(self, stream)
                }
                AstNode::WorkflowHintsSection(_) => {
                    v1::workflow::format_workflow_hints_section(self, stream)
                }
            },
            Element::Token(token) => {
                stream.push_ast_token(token);
            }
        }
    }
}

/// A formatter.
#[derive(Debug, Default)]
pub struct Formatter {
    /// The configuration.
    config: Config,
}

impl Formatter {
    /// Creates a new formatter.
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Gets the configuration for this formatter.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Formats an element.
    pub fn format<W: Writable>(&self, element: W) -> std::result::Result<String, std::fmt::Error> {
        let mut result = String::new();

        for token in self.to_stream(element) {
            write!(result, "{token}", token = token.display(self.config()))?;
        }

        Ok(result)
    }

    /// Gets the [`PostToken`] stream.
    fn to_stream<W: Writable>(&self, element: W) -> TokenStream<PostToken> {
        let mut stream = TokenStream::default();
        element.write(&mut stream, Some(self.config()));

        let mut postprocessor = Postprocessor::default();
        postprocessor.run(stream, self.config())
    }
}

#[cfg(test)]
mod tests {
    use wdl_ast::Document;
    use wdl_ast::Node;

    use crate::Formatter;
    use crate::element::node::AstNodeFormatExt as _;

    #[test]
    fn smoke() {
        let (document, diagnostics) = Document::parse(
            "## WDL
version 1.2  # This is a comment attached to the version.

# This is a comment attached to the task keyword.
task foo # This is an inline comment on the task ident.
{

} # This is an inline comment on the task close brace.

# This is a comment attached to the workflow keyword.
workflow bar # This is an inline comment on the workflow ident.
{
  # This is attached to the call keyword.
  call foo {}
} # This is an inline comment on the workflow close brace.",
        );

        assert!(diagnostics.is_empty());
        let document = Node::Ast(document.ast().into_v1().unwrap()).into_format_element();
        let formatter = Formatter::default();
        let result = formatter.format(&document);
        match result {
            Ok(s) => {
                print!("{s}");
            }
            Err(err) => {
                panic!("failed to format document: {err}");
            }
        }
    }
}
