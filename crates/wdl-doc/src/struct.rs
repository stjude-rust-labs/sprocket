//! Create HTML documentation for WDL structs.

use std::path::Path;

use maud::Markup;
use maud::html;
use wdl_ast::AstToken;
use wdl_ast::Documented;
use wdl_ast::SupportedVersion;
use wdl_ast::v1::Decl;
use wdl_ast::v1::MetadataValue;
use wdl_ast::v1::StructDefinition;

use crate::VersionBadge;
use crate::docs_tree::PageSections;
use crate::meta::DESCRIPTION_KEY;
use crate::meta::DefinitionMeta;
use crate::meta::MetaMap;
use crate::meta::MetaMapExt;
use crate::meta::MetaMapValueSource;
use crate::meta::doc_comments;
use crate::meta::parse_metadata_items;

/// A member in a struct.
#[derive(Debug)]
struct Member {
    /// The declaration of the parameter.
    decl: Decl,
    /// Any meta entries associated with the parameter.
    meta: MetaMap,
}

impl Member {
    /// Create a new struct member.
    fn new(decl: Decl, meta: MetaMap) -> Self {
        Self { decl, meta }
    }
}

impl DefinitionMeta for Member {
    fn meta(&self) -> &MetaMap {
        &self.meta
    }
}

/// A struct in a WDL document.
#[derive(Debug)]
pub struct Struct {
    /// The meta of the struct.
    meta: MetaMap,
    /// The struct's members.
    members: Vec<Member>,
    /// The AST definition of the struct.
    definition: StructDefinition,
    /// The version of WDL this struct is defined in.
    version: VersionBadge,
}

impl DefinitionMeta for Struct {
    fn meta(&self) -> &MetaMap {
        &self.meta
    }
}

impl Struct {
    /// Create a new struct.
    pub fn new(
        definition: StructDefinition,
        version: SupportedVersion,
        enable_doc_comments: bool,
    ) -> Self {
        let mut meta = definition
            .metadata()
            .map(|meta| parse_metadata_items(meta.items()))
            .fold(MetaMap::new(), |mut acc, mut meta| {
                acc.append(&mut meta);
                acc
            });

        if enable_doc_comments && let Some(comments) = definition.doc_comments() {
            // Doc comments take precedence
            meta.append(&mut doc_comments(comments));
        }

        let parameter_meta = definition
            .parameter_metadata()
            .map(|meta| parse_metadata_items(meta.items()))
            .fold(MetaMap::new(), |mut acc, mut meta| {
                acc.append(&mut meta);
                acc
            });

        let members = parse_member_meta(&definition, &parameter_meta, enable_doc_comments);
        Self {
            meta,
            members,
            definition,
            version: VersionBadge::new(version),
        }
    }

    /// Render the struct as HTML.
    pub fn render(&self, assets: &Path) -> (Markup, PageSections) {
        let name = self.definition.name();
        let name = name.text();

        let members = html! {
            div class="main__section" {
                h2 id="struct-members" class="main__section-header" { "Members" }
                div class="main__grid-container" {
                    div class="main__grid-struct-member-container" {
                        div class="main__grid-header-cell" { "Name" }
                        div class="main__grid-header-cell" { "Type" }
                        div class="main__grid-header-cell" { "Description" }
                        div class="main__grid-header-separator" {}
                        @for member in self.members.iter() {
                            @let member_name = member.decl.name();
                            @let member_id = format!("member.{}", member_name.text());
                            div id=(member_id) class="main__grid-row" x-data="{ description_expanded: false }" {
                                div class="main__grid-cell" {
                                    code { (member_name.text()) }
                                }

                                div class="main__grid-cell" {
                                    code { (member.decl.ty()) }
                                }
                                div class="main__grid-cell" {
                                    (member.meta().render_description(true))
                                }
                                div x-show="description_expanded" class="main__grid-full-width-cell" {
                                    (member.meta().render_description(false))
                                }
                            }
                            div class="main__grid-row-separator" {}
                        }
                    }
                }
            }
        };

        let meta_markup = self
            .meta
            .render_remaining(&[DESCRIPTION_KEY], assets)
            .map_or_else(|| html! {}, |markup| html! { (markup) });

        let markup = html! {
            div class="main__container" {
                p class="text-brand-pink-400" { "Struct" }
                h1 id="title" class="main__title" { code { (name) } }
                div class="markdown-body mb-4" {
                    (self.meta.render_description(false))
                }
                div class="main__badge-container" {
                    (self.version.render())
                }
                div class="main__section" {
                    sprocket-code language="wdl" {
                        (self.definition)
                    }
                }
                div class="main__section" {
                    (meta_markup)
                }
                (members)
            }
        };
        (markup, PageSections::default())
    }
}

/// Parse the `meta`/`parameter_meta` and doc comments on the struct members.
fn parse_member_meta(
    definition: &StructDefinition,
    parameter_meta: &MetaMap,
    enable_doc_comments: bool,
) -> Vec<Member> {
    definition
        .members()
        .map(|decl| {
            let name = decl.name().text().to_owned();
            let mut meta_map = MetaMap::default();
            if let Some(MetaMapValueSource::MetaValue(meta)) = parameter_meta.get(&name) {
                match meta {
                    MetadataValue::Object(o) => {
                        for item in o.items() {
                            meta_map.insert(
                                item.name().text().to_string(),
                                MetaMapValueSource::MetaValue(item.value().clone()),
                            );
                        }
                    }
                    MetadataValue::String(_s) => {
                        meta_map.insert(
                            DESCRIPTION_KEY.to_string(),
                            MetaMapValueSource::MetaValue(meta.clone()),
                        );
                    }
                    _ => {}
                }
            }

            if enable_doc_comments && let Some(comments) = decl.doc_comments() {
                // Doc comments take precedence
                meta_map.append(&mut doc_comments(comments));
            }

            Member::new(Decl::Unbound(decl), meta_map)
        })
        .collect()
}
