//! Create HTML documentation for WDL structs.
// TODO: handle >=v1.2 structs

use maud::Markup;
use maud::html;
use wdl_ast::AstToken;
use wdl_ast::SupportedVersion;
use wdl_ast::v1::StructDefinition;

use crate::VersionBadge;
use crate::docs_tree::PageSections;

/// A struct in a WDL document.
#[derive(Debug)]
pub struct Struct {
    /// The AST definition of the struct.
    definition: StructDefinition,
    /// The version of WDL this struct is defined in.
    version: VersionBadge,
}

impl Struct {
    /// Create a new struct.
    pub fn new(definition: StructDefinition, version: SupportedVersion) -> Self {
        Self {
            definition,
            version: VersionBadge::new(version),
        }
    }

    /// Render the struct as HTML.
    pub fn render(&self) -> (Markup, PageSections) {
        let name = self.definition.name();
        let name = name.text();
        let markup = html! {
            div class="main__container" {
                p class="text-brand-pink-400" { "Struct" }
                h1 id="title" class="main__title" { code { (name) } }
                div class="main__badge-container" {
                    (self.version.render())
                }
                div class="main__section" {
                    sprocket-code language="wdl" {
                        (self.definition)
                    }
                }
            }
        };
        (markup, PageSections::default())
    }
}
