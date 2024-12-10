//! Parameter module

use std::fmt::Display;

use html::content;
use html::text_content;
use wdl_ast::AstToken;
use wdl_ast::v1::Decl;
use wdl_ast::v1::MetadataValue;

/// A parameter in a workflow or task.
#[derive(Debug)]
pub struct Parameter {
    /// The declaration of the parameter.
    def: Decl,
    /// Any meta entries associated with the parameter.
    meta: Option<MetadataValue>,
}

impl Parameter {
    /// Create a new parameter.
    pub fn new(def: Decl, meta: Option<MetadataValue>) -> Self {
        Self { def, meta }
    }

    /// Get the name of the parameter.
    pub fn name(&self) -> String {
        self.def.name().as_str().to_owned()
    }

    /// Get the type of the parameter.
    pub fn ty(&self) -> String {
        self.def.ty().to_string()
    }

    /// Get the Expr value of the parameter.
    pub fn expr(&self) -> Option<String> {
        self.def.expr().map(|expr| expr.syntax().to_string())
    }

    /// Get the meta entries associated with the parameter.
    pub fn meta(&self) -> Option<&MetadataValue> {
        self.meta.as_ref()
    }
}

impl Display for Parameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let parameter_name = content::Heading2::builder().text(self.name()).build();
        let parameter_type = text_content::Paragraph::builder()
            .text(format!("Type: {}", self.ty()))
            .build();
        let parameter_expr = if let Some(expr) = self.expr() {
            text_content::Paragraph::builder()
                .text(format!("Expr: {}", expr))
                .build()
        } else {
            text_content::Paragraph::builder()
                .text("Expr: None")
                .build()
        };

        write!(f, "{}", parameter_name)?;
        write!(f, "{}", parameter_type)?;
        write!(f, "{}", parameter_expr)
    }
}
