//! Create HTML documentation for WDL structs.

use std::path::Path;

use maud::Markup;
use maud::html;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
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
use crate::meta::parse_meta;
use crate::meta::parse_parameter_meta;

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

    /// Get the [full description] of the member
    ///
    /// [full description]: MetaMap::full_description()
    pub fn full_description(&self) -> String {
        self.meta
            .full_description()
            .unwrap_or_else(|| String::from("No description provided"))
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
        let mut meta = definition.metadata().map(|meta| parse_meta(&meta)).fold(
            MetaMap::new(),
            |mut acc, mut meta| {
                acc.append(&mut meta);
                acc
            },
        );

        if enable_doc_comments {
            // Doc comments take precedence
            meta.append(&mut doc_comments(definition.keyword().inner()));
        }

        let parameter_meta = definition
            .parameter_metadata()
            .map(|meta| parse_parameter_meta(&meta))
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
                @for member in self.members.iter() {
                    @let member_name = member.decl.name();
                    @let member_id = format!("member.{}", member_name.text());
                    @let member_anchor = format!("#{member_id}");
                    section id=(member_id) {
                        div class="main__meta-item-member" {
                            a href=(member_anchor) {}
                            h3 class="main__section-subheader" { (member_name.text()) }
                        }

                        div class="main__meta-item-member-description" {
                            @for paragraph in member.full_description().split('\n') {
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

            if enable_doc_comments {
                // Doc comments take precedence
                if let Some(token) = decl.inner().first_token() {
                    meta_map.append(&mut doc_comments(&token));
                }
            }

            Member::new(Decl::Unbound(decl.clone()), meta_map)
        })
        .collect()
}
