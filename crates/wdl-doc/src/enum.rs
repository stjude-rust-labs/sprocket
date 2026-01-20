//! Create HTML documentation for WDL enums.

use std::path::Path;

use maud::Markup;
use maud::html;
use wdl_ast::AstToken;
use wdl_ast::SupportedVersion;
use wdl_ast::v1::EnumDefinition;
use wdl_ast::v1::EnumVariant;

use crate::VersionBadge;
use crate::docs_tree::PageSections;
use crate::meta::DESCRIPTION_KEY;
use crate::meta::MetaMap;
use crate::meta::MetaMapExt;
use crate::meta::doc_comments;

/// An [`EnumVariant`] with an associated [`MetaMap`]
#[derive(Debug)]
pub struct DocumentedEnumVariant {
    /// The enum variant's `meta`, derived from its doc comments.
    meta: MetaMap,
    /// The AST definition of the enum variant.
    variant: EnumVariant,
}

impl DocumentedEnumVariant {
    /// Get the [full description] of the variant
    ///
    /// [full description]: MetaMap::full_description()
    pub fn full_description(&self) -> String {
        self.meta
            .full_description()
            .unwrap_or_else(|| String::from("No description provided"))
    }
}

/// An enum in a WDL document.
#[derive(Debug)]
pub struct Enum {
    /// The enum's `meta`, derived from its doc comments.
    meta: MetaMap,
    /// The enum's variants.
    variants: Vec<DocumentedEnumVariant>,
    /// The AST definition of the enum.
    definition: EnumDefinition,
    /// The version of WDL this enum is defined in.
    version: VersionBadge,
}

impl Enum {
    /// Create a new enum.
    pub fn new(definition: EnumDefinition, version: SupportedVersion) -> Self {
        let (meta, variants) = parse_meta(&definition);

        Self {
            meta,
            variants,
            definition,
            version: VersionBadge::new(version),
        }
    }

    /// Render the enum as HTML.
    pub fn render(&self, assets: &Path) -> (Markup, PageSections) {
        let name = self.definition.name();
        let name = name.text();

        let variants = html! {
            div class="main__section" {
                h2 id="variants" class="main__section-header" { "Variants" }
                @for variant in self.variants.iter() {
                    @let variant_name = variant.variant.name();
                    @let variant_id = format!("variant.{}", variant_name.text());
                    @let variant_anchor = format!("#{variant_id}");
                    section id=(variant_id) {
                        div class="main__meta-item-member" {
                            a href=(variant_anchor) {}
                            h3 class="main__section-subheader" { (variant_name.text()) }
                        }

                        div class="main__meta-item-member-description" {
                            @for paragraph in variant.full_description().split('\n') {
                                p class="main__meta-item-member-description-para" { (paragraph) }
                            }
                        }
                    }
                }
            }
        };

        let meta_markup = self
            .meta
            .render_remaining(&[DESCRIPTION_KEY], assets)
            .map_or_else(|| html! {}, |markup| html! { (markup) });

        let mut definition = String::new();
        self.definition
            .fmt(&mut definition, None)
            .expect("writing to strings should never fail");

        let markup = html! {
            div class="main__container" {
                p class="text-brand-yellow-400" { "Enum" }
                h1 id="title" class="main__title" { code { (name) } }
                div class="markdown-body mb-4" {
                    (self.meta.render_description(false))
                }
                div class="main__badge-container" {
                    (self.version.render())
                }
                div class="main__section" {
                    sprocket-code language="wdl" {
                        (definition)
                    }
                }
                div class="main__section" {
                    (meta_markup)
                }
                (variants)
            }
        };
        (markup, PageSections::default())
    }
}

fn parse_meta(definition: &EnumDefinition) -> (MetaMap, Vec<DocumentedEnumVariant>) {
    let enum_docs = doc_comments(definition.keyword().inner());

    let mut variant_docs = Vec::new();
    for variant in definition.variants() {
        variant_docs.push(DocumentedEnumVariant {
            meta: doc_comments(variant.name().inner()),
            variant: variant.clone(),
        });
    }

    (enum_docs, variant_docs)
}
