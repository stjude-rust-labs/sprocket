//! Generic handling of lint rules from all sources.

use std::fmt::Write;

use maud::PreEscaped;
use maud::html;
use pulldown_cmark::Options;
use pulldown_cmark::Parser;
use serde_json::Value;
use serde_json::json;
use wdl_lint::Config;
use wdl_lint::ConfigField;
use wdl_lint::Rule;

/// The source of a lint rule.
pub enum LintRuleSource {
    /// A lint rule from `wdl-lint`.
    WdlLint,
    /// A lint rule from `wdl-analysis`.
    WdlAnalysis,
}

/// A lint rule from any supported source.
pub enum LintRule {
    /// A lint rule from `wdl-lint`.
    WdlLint(Box<dyn Rule>),
    /// A lint rule from `wdl-analysis`.
    WdlAnalysis(Box<dyn wdl_analysis::Rule>),
}

impl LintRule {
    /// Render the rule's Markdown documentation as HTML.
    fn render(&self) -> PreEscaped<String> {
        let mut markdown = format!(
            r#"### What it Does
{description}

### Why is this Bad?
{explanation}
"#,
            description = self.description(),
            explanation = self.explanation()
        );

        let examples = self.examples();
        if !examples.is_empty() {
            writeln!(&mut markdown).unwrap();
            writeln!(&mut markdown, "### Examples").unwrap();
            for example in examples {
                writeln!(&mut markdown, "{example}").unwrap();
            }
            writeln!(&mut markdown).unwrap();
        }

        if let Some(config_fields) = self.applicable_config_fields() {
            writeln!(&mut markdown, "### Configuration").unwrap();
            for field in config_fields {
                writeln!(
                    &mut markdown,
                    "#### `{}` (Default: `{}`)",
                    field.name, field.default
                )
                .unwrap();
                writeln!(&mut markdown).unwrap();
                writeln!(&mut markdown, "{}", field.description).unwrap();
            }
        }

        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_GFM);
        options.insert(Options::ENABLE_DEFINITION_LIST);
        let parser = Parser::new_ext(&markdown, options);

        let mut html = String::new();
        pulldown_cmark::html::push_html(&mut html, parser);

        PreEscaped(html)
    }

    /// Get the lint's ID.
    pub fn id(&self) -> &'static str {
        match self {
            LintRule::WdlLint(rule) => rule.id(),
            LintRule::WdlAnalysis(rule) => rule.id(),
        }
    }

    /// Get the version (crate-dependent) in which the lint was added.
    pub fn version(&self) -> &'static str {
        match self {
            LintRule::WdlLint(rule) => rule.version(),
            LintRule::WdlAnalysis(rule) => rule.version(),
        }
    }

    /// Get the rule's short description.
    pub fn description(&self) -> &'static str {
        match self {
            LintRule::WdlLint(rule) => rule.description(),
            LintRule::WdlAnalysis(rule) => rule.description(),
        }
    }

    /// Get the rule's extended description.
    pub fn explanation(&self) -> &'static str {
        match self {
            LintRule::WdlLint(rule) => rule.explanation(),
            LintRule::WdlAnalysis(rule) => rule.explanation(),
        }
    }

    /// Get the rule's examples.
    pub fn examples(&self) -> &'static [&'static str] {
        match self {
            LintRule::WdlLint(rule) => rule.examples(),
            LintRule::WdlAnalysis(rule) => rule.examples(),
        }
    }

    /// Encode the rule to JSON.
    pub fn to_json(&self) -> Value {
        let tags = match self {
            LintRule::WdlLint(rule) => rule.tags().iter().map(|tag| tag.to_string()).collect(),
            // wdl-analysis rules have no tags, so they'll always be displayed
            LintRule::WdlAnalysis(_) => Vec::new(),
        };

        let source = match self {
            LintRule::WdlLint(_) => "wdlLint",
            LintRule::WdlAnalysis(_) => "wdlAnalysis",
        };

        json!({
            "source": source,
            "id": self.id(),
            "tags": tags,
            "descriptionHtml": self.render().0,
            "addedIn": self.version(),
        })
    }

    /// All config fields that apply to this lint rule.
    pub fn applicable_config_fields(&self) -> Option<Vec<ConfigField>> {
        // `wdl-analysis` rules have no configuration
        let Self::WdlLint(rule) = self else {
            return None;
        };

        let applicable_fields = Config::fields()
            .into_iter()
            .filter(|field| field.applicable_lints.contains(&rule.id()))
            .collect::<Vec<_>>();

        if applicable_fields.is_empty() {
            None
        } else {
            Some(applicable_fields)
        }
    }
}

/// A list of lint rules.
pub fn lint_rule_list(source: LintRuleSource) -> PreEscaped<String> {
    let source = match source {
        LintRuleSource::WdlLint => "wdlLint",
        LintRuleSource::WdlAnalysis => "wdlAnalysis",
    };

    html! {
        div class="rule__container" {
            template "x-for"=(format!("lint in {source}.allLints")) ":key"="lint.id" {
                article
                    ":id"="lint.id"
                    x-show="isVisible(lint)"
                    class="group bg-slate-900/50 border border-slate-800 rounded-xl overflow-hidden hover:border-slate-700 transition-colors mb-4"
                {
                    input ":id"="`label-${lint.id}`" type="checkbox" class="accordion-check";
                    label x-bind:for="`label-${lint.id}`" class="flex items-center justify-between p-5 cursor-pointer select-none w-full" {
                        h2 class="text-base font-semibold text-slate-100 font-mono" x-text="lint.id" {}

                        div class="chevron transition-transform duration-200 text-slate-500 group-hover:text-slate-300" {
                            svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" {
                                path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" {}
                            }
                        }
                    }

                    div class="accordion-content overflow-hidden" {
                        div class="min-h-0" {
                            div class="py-0 text-sm text-slate-400 leading-relaxed border-t border-slate-800/50 mt-2" {
                                div class="w-full px-5 pt-4 rule-description" x-html="lint.descriptionHtml" {}
                                div class="w-full flex flex-row flex-nowrap border-t border-slate-800/50 px-5 mt-2 rule-extras" {
                                    div class="inline-flex grow my-auto" {
                                        "Added in: "
                                        div class="main__badge-inner mx-[5px] my-auto" {
                                            span class="main__badge-inner-text" x-text="lint.addedIn" {}
                                        }
                                    }
                                    div class="inline-flex grow my-auto border-l border-slate-800/50" x-show="lint.tags.length > 0" {
                                        "Tags: "
                                        div class="inline-flex gap-2 mx-[5px] my-auto" {
                                            template "x-for"=("tag in lint.tags") {
                                                div class="main__badge-inner" {
                                                    span class="main__badge-inner-text" x-text="tag" {}
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
