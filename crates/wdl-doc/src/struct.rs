//! Create HTML documentation for WDL structs.

use std::path::Path;
use std::path::PathBuf;

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
use crate::links::PageLinkIndex;
use crate::meta::DESCRIPTION_KEY;
use crate::meta::DefinitionMeta;
use crate::meta::MetaMap;
use crate::meta::MetaMapExt;
use crate::meta::MetaMapValueSource;
use crate::meta::doc_comments;
use crate::meta::main_container;
use crate::meta::parse_metadata_items;
use crate::page::DeclarationHero;

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
    /// Whether the struct lives outside the workspace.
    external: bool,
    /// The path from the root of the WDL workspace to the WDL document which
    /// contains this struct.
    ///
    /// Used to render the source card.
    wdl_path: Option<PathBuf>,
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
        external: bool,
        wdl_path: Option<PathBuf>,
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
            external,
            wdl_path,
        }
    }

    /// Render the struct as HTML.
    ///
    /// Member types are rendered through `links` so that references to uniquely
    /// generated struct and enum pages become anchors relative to `page_dir`.
    pub fn render(
        &self,
        assets: &Path,
        links: &PageLinkIndex,
        page_dir: &Path,
    ) -> (Markup, PageSections) {
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
                                    (links.render_type(&member.decl.ty().to_string(), page_dir))
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
            .render_remaining(&[DESCRIPTION_KEY])
            .map_or_else(|| html! {}, |markup| html! { (markup) });

        let mut hero = DeclarationHero::new("Struct", name, self.meta.render_description(false))
            .kind_class("text-brand-pink-400")
            .pagefind_type("struct")
            .badge(self.version.render());
        if let Some(path) = self.wdl_path.as_deref() {
            hero = hero.source_path(path);
        }

        let markup = html! {
            (hero.render(assets))
            @if let Some(body) = self.meta.render_authored_body(assets) {
                (body)
            }
            div class="main__section" {
                sprocket-code language="wdl" copyable expandable line-numbers {
                    (self.definition)
                }
            }
            div class="main__section" {
                (meta_markup)
            }
            (members)
        };
        (
            main_container("struct", self.external, markup),
            PageSections::default(),
        )
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

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::path::PathBuf;

    use wdl_ast::Document;
    use wdl_ast::SupportedVersion;
    use wdl_ast::version::V1;

    use super::Struct;
    use crate::links::PageLinkIndex;

    #[test]
    fn links_member_struct_types() {
        let (doc, _) = Document::parse(
            r#"
            version 1.2
            struct Employee {
                Person person
                Int id
            }
            "#,
            None,
        );
        let item = doc.ast().into_v1().unwrap().items().next().unwrap();
        let def = item.into_struct_definition().unwrap();
        let r#struct = Struct::new(def, SupportedVersion::V1(V1::Two), false, None, true);

        let links = PageLinkIndex::from_pages([("Person", PathBuf::from("Person-struct.html"))]);
        let (markup, _) = r#struct.render(Path::new("assets"), &links, Path::new(""));
        let html = markup.into_string();

        assert!(html.contains("href=\"Person-struct.html\""));
        assert!(html.contains(">Person</a>"));
        // The primitive `Int` member type must not be linked.
        assert!(!html.contains("href=\"Int"));
    }
}
