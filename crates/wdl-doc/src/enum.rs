//! Create HTML documentation for WDL enums.

use maud::html;
use maud::Markup;
use std::path::Path;
use wdl_ast::v1::EnumDefinition;
use wdl_ast::AstToken;
use wdl_ast::SupportedVersion;

use crate::docs_tree::PageSections;
use crate::VersionBadge;

/// An enum in a WDL document.
#[derive(Debug)]
pub struct Enum {
    /// The AST definition of the enum.
    definition: EnumDefinition,
    /// The version of WDL this enum is defined in.
    version: VersionBadge,
}

impl Enum {
    /// Create a new enum.
    pub fn new(definition: EnumDefinition, version: SupportedVersion) -> Self {
        Self {
            definition,
            version: VersionBadge::new(version),
        }
    }

    /// Render the enum as HTML.
    pub fn render(&self, _assets: &Path) -> (Markup, PageSections) {
        let name = self.definition.name();
        let name = name.text();

        let mut definition = String::new();
        self.definition.fmt(&mut definition, None).expect("writing to strings should never fail");

        let markup = html! {
            div class="main__container" {
                p class="text-brand-yellow-400" { "Enum" }
                h1 id="title" class="main__title" { code { (name) } }
                div class="main__badge-container" {
                    (self.version.render())
                }
                div class="main__section" {
                    sprocket-code language="wdl" {
                        (definition)
                    }
                }
            }
        };
        (markup, PageSections::default())
    }
}
