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
    fn write(&self, stream: &mut TokenStream<PreToken>, config: &Config);
}

impl Writable for &FormatElement {
    fn write(&self, stream: &mut TokenStream<PreToken>, config: &Config) {
        match self.element() {
            Element::Node(node) => match node {
                AstNode::AccessExpr(_) => v1::expr::format_access_expr(self, stream, config),
                AstNode::AdditionExpr(_) => v1::expr::format_addition_expr(self, stream, config),
                AstNode::ArrayType(_) => v1::decl::format_array_type(self, stream, config),
                AstNode::Ast(_) => v1::format_ast(self, stream, config),
                AstNode::BoundDecl(_) => v1::decl::format_bound_decl(self, stream, config),
                AstNode::CallAfter(_) => {
                    v1::workflow::call::format_call_after(self, stream, config)
                }
                AstNode::CallAlias(_) => {
                    v1::workflow::call::format_call_alias(self, stream, config)
                }
                AstNode::CallExpr(_) => v1::expr::format_call_expr(self, stream, config),
                AstNode::CallInputItem(_) => {
                    v1::workflow::call::format_call_input_item(self, stream, config)
                }
                AstNode::CallStatement(_) => {
                    v1::workflow::call::format_call_statement(self, stream, config)
                }
                AstNode::CallTarget(_) => {
                    v1::workflow::call::format_call_target(self, stream, config)
                }
                AstNode::CommandSection(_) => {
                    v1::task::format_command_section(self, stream, config)
                }
                AstNode::ConditionalStatement(_) => {
                    v1::workflow::format_conditional_statement(self, stream, config)
                }
                AstNode::ConditionalStatementClause(_) => {
                    v1::workflow::format_conditional_statement_clause(self, stream, config)
                }
                AstNode::DefaultOption(_) => v1::expr::format_default_option(self, stream, config),
                AstNode::DivisionExpr(_) => v1::expr::format_division_expr(self, stream, config),
                AstNode::EqualityExpr(_) => v1::expr::format_equality_expr(self, stream, config),
                AstNode::EnumDefinition(_) => {
                    v1::r#enum::format_enum_definition(self, stream, config)
                }
                AstNode::EnumTypeParameter(_) => {
                    v1::r#enum::format_enum_type_parameter(self, stream, config)
                }
              AstNode::EnumChoice(_) => v1::r#enum::format_enum_choice(self, stream, config),
              main
                AstNode::ExponentiationExpr(_) => {
                    v1::expr::format_exponentiation_expr(self, stream, config)
                }
                AstNode::GreaterEqualExpr(_) => {
                    v1::expr::format_greater_equal_expr(self, stream, config)
                }
                AstNode::GreaterExpr(_) => v1::expr::format_greater_expr(self, stream, config),
                AstNode::IfExpr(_) => v1::expr::format_if_expr(self, stream, config),
                AstNode::ImportAlias(_) => v1::import::format_import_alias(self, stream, config),
                AstNode::ImportStatement(_) => {
                    v1::import::format_import_statement(self, stream, config)
                }
                AstNode::IndexExpr(_) => v1::expr::format_index_expr(self, stream, config),
                AstNode::InequalityExpr(_) => {
                    v1::expr::format_inequality_expr(self, stream, config)
                }
                AstNode::InputSection(_) => v1::format_input_section(self, stream, config),
                AstNode::LessEqualExpr(_) => v1::expr::format_less_equal_expr(self, stream, config),
                AstNode::LessExpr(_) => v1::expr::format_less_expr(self, stream, config),
                AstNode::LiteralArray(_) => v1::expr::format_literal_array(self, stream, config),
                AstNode::LiteralBoolean(_) => {
                    v1::expr::format_literal_boolean(self, stream, config)
                }
                AstNode::LiteralFloat(_) => v1::expr::format_literal_float(self, stream, config),
                AstNode::LiteralHints(_) => v1::format_literal_hints(self, stream, config),
                AstNode::LiteralHintsItem(_) => v1::format_literal_hints_item(self, stream, config),
                AstNode::LiteralInput(_) => v1::format_literal_input(self, stream, config),
                AstNode::LiteralInputItem(_) => v1::format_literal_input_item(self, stream, config),
                AstNode::LiteralInteger(_) => {
                    v1::expr::format_literal_integer(self, stream, config)
                }
                AstNode::LiteralMap(_) => v1::expr::format_literal_map(self, stream, config),
                AstNode::LiteralMapItem(_) => {
                    v1::expr::format_literal_map_item(self, stream, config)
                }
                AstNode::LiteralNone(_) => v1::expr::format_literal_none(self, stream, config),
                AstNode::LiteralNull(_) => v1::meta::format_literal_null(self, stream, config),
                AstNode::LiteralObject(_) => v1::expr::format_literal_object(self, stream, config),
                AstNode::LiteralObjectItem(_) => {
                    v1::expr::format_literal_object_item(self, stream, config)
                }
                AstNode::LiteralOutput(_) => v1::format_literal_output(self, stream, config),
                AstNode::LiteralOutputItem(_) => {
                    v1::format_literal_output_item(self, stream, config)
                }
                AstNode::LiteralPair(_) => v1::expr::format_literal_pair(self, stream, config),
                AstNode::LiteralString(_) => v1::expr::format_literal_string(self, stream, config),
                AstNode::LiteralStruct(_) => {
                    v1::r#struct::format_literal_struct(self, stream, config)
                }
                AstNode::LiteralStructItem(_) => {
                    v1::r#struct::format_literal_struct_item(self, stream, config)
                }
                AstNode::LogicalAndExpr(_) => {
                    v1::expr::format_logical_and_expr(self, stream, config)
                }
                AstNode::LogicalNotExpr(_) => {
                    v1::expr::format_logical_not_expr(self, stream, config)
                }
                AstNode::LogicalOrExpr(_) => v1::expr::format_logical_or_expr(self, stream, config),
                AstNode::MapType(_) => v1::decl::format_map_type(self, stream, config),
                AstNode::MetadataArray(_) => v1::meta::format_metadata_array(self, stream, config),
                AstNode::MetadataObject(_) => {
                    v1::meta::format_metadata_object(self, stream, config)
                }
                AstNode::MetadataObjectItem(_) => {
                    v1::meta::format_metadata_object_item(self, stream, config)
                }
                AstNode::MetadataSection(_) => {
                    v1::meta::format_metadata_section(self, stream, config)
                }
                AstNode::ModuloExpr(_) => v1::expr::format_modulo_expr(self, stream, config),
                AstNode::MultiplicationExpr(_) => {
                    v1::expr::format_multiplication_expr(self, stream, config)
                }
                AstNode::NameRefExpr(_) => v1::expr::format_name_ref_expr(self, stream, config),
                AstNode::NegationExpr(_) => v1::expr::format_negation_expr(self, stream, config),
                AstNode::OutputSection(_) => v1::format_output_section(self, stream, config),
                AstNode::PairType(_) => v1::decl::format_pair_type(self, stream, config),
                AstNode::ObjectType(_) => v1::decl::format_object_type(self, stream, config),
                AstNode::ParameterMetadataSection(_) => {
                    v1::meta::format_parameter_metadata_section(self, stream, config)
                }
                AstNode::ParenthesizedExpr(_) => {
                    v1::expr::format_parenthesized_expr(self, stream, config)
                }
                AstNode::Placeholder(_) => v1::expr::format_placeholder(self, stream, config),
                AstNode::PrimitiveType(_) => v1::decl::format_primitive_type(self, stream, config),
                AstNode::RequirementsItem(_) => {
                    v1::task::format_requirements_item(self, stream, config)
                }
                AstNode::RequirementsSection(_) => {
                    v1::task::format_requirements_section(self, stream, config)
                }
                AstNode::RuntimeItem(_) => v1::task::format_runtime_item(self, stream, config),
                AstNode::RuntimeSection(_) => {
                    v1::task::format_runtime_section(self, stream, config)
                }
                AstNode::ScatterStatement(_) => {
                    v1::workflow::format_scatter_statement(self, stream, config)
                }
                AstNode::SepOption(_) => v1::expr::format_sep_option(self, stream, config),
                AstNode::StructDefinition(_) => {
                    v1::r#struct::format_struct_definition(self, stream, config)
                }
                AstNode::SubtractionExpr(_) => {
                    v1::expr::format_subtraction_expr(self, stream, config)
                }
                AstNode::TaskDefinition(_) => {
                    v1::task::format_task_definition(self, stream, config)
                }
                AstNode::TaskHintsItem(_) => v1::task::format_task_hints_item(self, stream, config),
                AstNode::TaskHintsSection(_) => {
                    v1::task::format_task_hints_section(self, stream, config)
                }
                AstNode::TrueFalseOption(_) => {
                    v1::expr::format_true_false_option(self, stream, config)
                }
                AstNode::TypeRef(_) => v1::decl::format_type_ref(self, stream, config),
                AstNode::UnboundDecl(_) => v1::decl::format_unbound_decl(self, stream, config),
                AstNode::VersionStatement(_) => v1::format_version_statement(self, stream, config),
                AstNode::WorkflowDefinition(_) => {
                    v1::workflow::format_workflow_definition(self, stream, config)
                }
                AstNode::WorkflowHintsArray(_) => {
                    v1::workflow::format_workflow_hints_array(self, stream, config)
                }
                AstNode::WorkflowHintsItem(_) => {
                    v1::workflow::format_workflow_hints_item(self, stream, config)
                }
                AstNode::WorkflowHintsObject(_) => {
                    v1::workflow::format_workflow_hints_object(self, stream, config)
                }
                AstNode::WorkflowHintsObjectItem(_) => {
                    v1::workflow::format_workflow_hints_object_item(self, stream, config)
                }
                AstNode::WorkflowHintsSection(_) => {
                    v1::workflow::format_workflow_hints_section(self, stream, config)
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
        element.write(&mut stream, self.config());

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
