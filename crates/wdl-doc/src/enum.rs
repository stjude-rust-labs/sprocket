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
use crate::meta::DEFAULT_DESCRIPTION;
use crate::meta::DESCRIPTION_KEY;
use crate::meta::DefinitionMeta;
use crate::meta::MetaMap;
use crate::meta::MetaMapExt;
use crate::meta::doc_comments;

/// An [`EnumVariant`] with an associated [`MetaMap`].
#[derive(Debug)]
pub(crate) struct DocumentedEnumVariant {
    /// The enum variant's `meta`, derived from its doc comments.
    meta: MetaMap,
    /// The AST definition of the enum variant.
    variant: EnumVariant,
}

impl DocumentedEnumVariant {
    /// Get the [full description] of the variant.
    ///
    /// [full description]: MetaMap::full_description()
    pub fn full_description(&self) -> String {
        self.meta
            .full_description()
            .unwrap_or_else(|| String::from(DEFAULT_DESCRIPTION))
    }
}

/// An enum in a WDL document.
#[derive(Debug)]
pub(crate) struct Enum {
    /// The enum's `meta`, derived from its doc comments.
    meta: MetaMap,
    /// The enum's variants.
    variants: Vec<DocumentedEnumVariant>,
    /// The AST definition of the enum.
    definition: EnumDefinition,
    /// The version of WDL this enum is defined in.
    version: VersionBadge,
}

impl DefinitionMeta for Enum {
    fn meta(&self) -> &MetaMap {
        &self.meta
    }
}

impl Enum {
    /// Create a new enum.
    pub fn new(
        definition: EnumDefinition,
        version: SupportedVersion,
        enable_doc_comments: bool,
    ) -> Self {
        let (meta, variants) = parse_meta(&definition, enable_doc_comments);

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

        let definition = self.definition.display(None);
        let markup = html! {
            div class="main__container" {
                p class="text-brand-lime-300" { "Enum" }
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

/// Parse the doc comments on the enum definition and its variants.
fn parse_meta(
    definition: &EnumDefinition,
    enable_doc_comments: bool,
) -> (MetaMap, Vec<DocumentedEnumVariant>) {
    let enum_docs = if enable_doc_comments {
        doc_comments(definition.keyword().inner())
    } else {
        MetaMap::new()
    };

    let mut variant_docs = Vec::new();
    for variant in definition.variants() {
        let meta = if enable_doc_comments {
            doc_comments(variant.name().inner())
        } else {
            MetaMap::new()
        };

        variant_docs.push(DocumentedEnumVariant {
            meta,
            variant: variant.clone(),
        });
    }

    (enum_docs, variant_docs)
}
