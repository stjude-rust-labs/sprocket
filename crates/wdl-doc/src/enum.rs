//! Create HTML documentation for WDL enums.

use std::path::Path;

use maud::Markup;
use maud::html;
use wdl_ast::AstToken;
use wdl_ast::Documented;
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
pub(crate) struct DocumentedEnumChoice {
    /// The enum choice's `meta`, derived from its doc comments.
    meta: MetaMap,
    /// The AST definition of the enum choice.
    choice: EnumVariant,
}

impl DocumentedEnumChoice {
    /// Get the [full description] of the choice.
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
    /// The enum's choices (variants).
    choices: Vec<DocumentedEnumChoice>,
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
        let (meta, choices) = parse_meta(&definition, enable_doc_comments);

        Self {
            meta,
            choices,
            definition,
            version: VersionBadge::new(version),
        }
    }

    /// Render the enum as HTML.
    pub fn render(&self, assets: &Path) -> (Markup, PageSections) {
        let name = self.definition.name();
        let name = name.text();

        let choices = html! {
            div class="main__section" {
                h2 id="choices" class="main__section-header" { "Choices" }
                @for choice in self.choices.iter() {
                    @let choice_name = choice.choice.name();
                    @let choice_id = format!("choice.{}", choice_name.text());
                    @let choice_anchor = format!("#{choice_id}");
                    section id=(choice_id) {
                        div class="main__meta-item-member" {
                            a href=(choice_anchor) {}
                            h3 class="main__section-subheader" { (choice_name.text()) }
                        }

                        div class="main__meta-item-member-description" {
                            @for paragraph in choice.full_description().split('\n') {
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
                (choices)
            }
        };
        (markup, PageSections::default())
    }
}

/// Parse the doc comments on the enum definition and its choices.
fn parse_meta(
    definition: &EnumDefinition,
    enable_doc_comments: bool,
) -> (MetaMap, Vec<DocumentedEnumChoice>) {
    let enum_docs = if enable_doc_comments && let Some(comments) = definition.doc_comments() {
        doc_comments(comments)
    } else {
        MetaMap::new()
    };

    let mut choice_docs = Vec::new();
    for choice in definition.variants() {
        let meta = if enable_doc_comments && let Some(comments) = choice.doc_comments() {
            doc_comments(comments)
        } else {
            MetaMap::new()
        };

        choice_docs.push(DocumentedEnumChoice {
            meta,
            choice: choice.clone(),
        });
    }

    (enum_docs, choice_docs)
}

#[cfg(test)]
mod tests {
    use wdl_ast::AstToken;
    use wdl_ast::Document;
    use wdl_ast::SupportedVersion;
    use wdl_ast::version::V1;

    use crate::r#enum::Enum;
    use crate::meta::DESCRIPTION_KEY;
    use crate::meta::MetaMapExt;

    #[test]
    fn test_enum() {
        let (doc, _) = Document::parse(
            r##"
            version 1.3
            ## An RGB24 color enum
            ##
            ## Each variant is represented as a 24-bit hexadecimal RGB string with exactly one non-zero channel.
            enum Color[String] {
                ## Pure red
                Red = "#FF0000",
                ## Pure green
                Green = "#00FF00",
                Blue = "#0000FF" # No description
            }
            "##,
        );

        let doc_item = doc.ast().into_v1().unwrap().items().next().unwrap();
        let ast_enum = doc_item.into_enum_definition().unwrap();

        let enum_def = Enum::new(ast_enum, SupportedVersion::V1(V1::Three), true);
        assert_eq!(
            enum_def.meta.full_description().as_deref(),
            Some(
                "An RGB24 color enum\nEach variant is represented as a 24-bit hexadecimal RGB \
                 string with exactly one non-zero channel."
            )
        );
        assert_eq!(enum_def.definition.name().text(), "Color");

        let mut found_red = false;
        let mut found_green = false;
        let mut found_blue = false;
        for choice in enum_def.choices {
            match choice.choice.name().text() {
                "Red" => {
                    assert_eq!(
                        choice.meta.get(DESCRIPTION_KEY).unwrap().text().unwrap(),
                        "Pure red"
                    );
                    found_red = true;
                }
                "Green" => {
                    assert_eq!(
                        choice.meta.get(DESCRIPTION_KEY).unwrap().text().unwrap(),
                        "Pure green"
                    );
                    found_green = true;
                }
                "Blue" => {
                    assert!(!choice.meta.contains_key(DESCRIPTION_KEY));
                    found_blue = true;
                }
                other => unreachable!("unexpected choice: {other}"),
            }
        }

        assert!(found_red);
        assert!(found_green);
        assert!(found_blue);
    }
}
