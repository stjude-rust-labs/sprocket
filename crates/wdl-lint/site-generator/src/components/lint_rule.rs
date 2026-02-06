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

pub enum LintRuleSource {
    WdlLint,
    WdlAnalysis,
}

pub enum LintRule {
    WdlLint(Box<dyn Rule>),
    WdlAnalysis(Box<dyn wdl_analysis::Rule>),
}

impl LintRule {
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

        if let Some(config_fields) = self.applicable_config_fields() {
            writeln!(&mut markdown).unwrap();
            writeln!(&mut markdown, "### Configuration").unwrap();
            for field in config_fields {
                writeln!(&mut markdown, "* {}", field.name).unwrap();
            }
        }

        let examples = self.examples();
        if !examples.is_empty() {
            writeln!(&mut markdown).unwrap();
            writeln!(&mut markdown, "### Examples").unwrap();
            for example in examples {
                writeln!(&mut markdown, "{example}").unwrap();
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

    pub fn id(&self) -> &'static str {
        match self {
            LintRule::WdlLint(rule) => rule.id(),
            LintRule::WdlAnalysis(rule) => rule.id(),
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            LintRule::WdlLint(rule) => rule.description(),
            LintRule::WdlAnalysis(rule) => rule.description(),
        }
    }

    pub fn explanation(&self) -> &'static str {
        match self {
            LintRule::WdlLint(rule) => rule.explanation(),
            LintRule::WdlAnalysis(rule) => rule.explanation(),
        }
    }

    pub fn examples(&self) -> &'static [&'static str] {
        match self {
            LintRule::WdlLint(rule) => rule.examples(),
            LintRule::WdlAnalysis(rule) => rule.examples(),
        }
    }

    pub fn to_json(&self) -> Value {
        let tags = match self {
            LintRule::WdlLint(rule) => rule.tags().iter().map(|tag| tag.to_string()).collect(),
            // wdl-analysis rules have no tags, so they'll always be displayed
            LintRule::WdlAnalysis(_) => Vec::new(),
        };

        json!({
            "id": self.id(),
            "tags": tags,
            "descriptionHtml": self.render().0,
        })
    }

    pub fn applicable_config_fields(&self) -> Option<Vec<&'static ConfigField>> {
        // `wdl-analysis` rules have no configuration
        let Self::WdlLint(rule) = self else {
            return None;
        };

        let applicable_fields = Config::fields()
            .iter()
            .filter(|field| field.applicable_lints.contains(&rule.id()))
            .collect::<Vec<_>>();

        if applicable_fields.is_empty() {
            None
        } else {
            Some(applicable_fields)
        }
    }
}

pub fn lint_rule_list(source: LintRuleSource) -> PreEscaped<String> {
    let list_name = match source {
        LintRuleSource::WdlLint => "allLints",
        LintRuleSource::WdlAnalysis => "allAnalysisLints",
    };

    html! {
        div class="rule__container" {
            template "x-for"=(format!("lint in $store.{list_name}")) ":key"="lint.id" {
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
                            div class="p-5 pt-0 text-sm text-slate-400 leading-relaxed border-t border-slate-800/50 mt-2" {
                                div class="w-full pt-4 rule-description" x-html="lint.descriptionHtml" {}
                            }
                        }
                    }
                }
            }
        }
    }
}
