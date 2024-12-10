//! Create HTML documentation for WDL structs.

use std::fmt::Display;

use html::content;
use html::text_content;
use wdl_ast::AstToken;
use wdl_ast::v1::StructDefinition;

/// A struct in a WDL document.
#[derive(Debug)]
pub struct Struct {
    /// The AST definition of the struct.
    def: StructDefinition,
}

impl Struct {
    /// Create a new struct.
    pub fn new(def: StructDefinition) -> Self {
        Self { def }
    }

    /// Get the name of the struct.
    pub fn name(&self) -> String {
        self.def.name().as_str().to_owned()
    }

    /// Get the members of the struct.
    pub fn members(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.def.members().map(|decl| {
            let name = decl.name().as_str().to_owned();
            let ty = decl.ty().to_string();
            (name, ty)
        })
    }
}

impl Display for Struct {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let struct_name = content::Heading1::builder().text(self.name()).build();

        let mut members = text_content::UnorderedList::builder();
        for (name, ty) in self.members() {
            members.push(
                text_content::ListItem::builder()
                    .text(format!("{}: {}", name, ty))
                    .build(),
            );
        }
        let members = members.build();

        write!(f, "{}", struct_name)?;
        write!(f, "{}", members)
    }
}
