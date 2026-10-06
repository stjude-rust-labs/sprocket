//! Validation of deprecated language features.

use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::TreeToken;
use wdl_ast::v1::Placeholder;
use wdl_ast::v1::PlaceholderOption;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::Type;
use wdl_grammar::Diagnostic;
use wdl_grammar::Severity;
use wdl_grammar::Span;
use wdl_grammar::SupportedVersion;
use wdl_grammar::version::V1;

use crate::DeprecatedObjectRule;
use crate::DeprecatedPlaceholderRule;
use crate::DeprecatedRuntimeSectionRule;
use crate::Diagnostics;
use crate::Document;
use crate::VisitReason;
use crate::Visitor;

/// Creates a deprecated object use diagnostic.
fn deprecated_object_use(span: Span) -> Diagnostic {
    Diagnostic::note(String::from("use of a deprecated `Object` type"))
        .with_rule(DeprecatedObjectRule::ID)
        .with_highlight(span)
        .with_fix("replace the `Object` with a `Map` or a `Struct`")
}

/// Creates a diagnostic for the use of the deprecated `default` placeholder
/// option.
fn deprecated_default_placeholder_option(span: Span) -> Diagnostic {
    Diagnostic::note(String::from(
        "use of the deprecated `default` placeholder option",
    ))
    .with_rule(DeprecatedPlaceholderRule::ID)
    .with_highlight(span)
    .with_fix(
        "replace the `default` placeholder option with a call to the `select_first()` standard \
         library function",
    )
}

/// Creates a diagnostic for the use of the deprecated `sep` placeholder option.
fn deprecated_sep_placeholder_option(span: Span) -> Diagnostic {
    Diagnostic::note(String::from(
        "use of the deprecated `sep` placeholder option",
    ))
    .with_rule(DeprecatedPlaceholderRule::ID)
    .with_highlight(span)
    .with_fix(
        "replace the `sep` placeholder option with a call to the `sep()` standard library function",
    )
}

/// Creates a diagnostic for the use of the deprecated `${}` placeholder option.
fn deprecated_interpolation_placeholder_option(span: Span) -> Diagnostic {
    Diagnostic::note(String::from(
        "use of the deprecated `${}` placeholder option",
    ))
    .with_rule(DeprecatedPlaceholderRule::ID)
    .with_highlight(span)
    .with_fix("replace the opening token `$` with `~`")
}

/// Creates a diagnostic for the use of the deprecated `true`/`false`
/// placeholder option.
fn deprecated_true_false_placeholder_option(span: Span) -> Diagnostic {
    Diagnostic::note(String::from(
        "use of the deprecated `true`/`false` placeholder option",
    ))
    .with_rule(DeprecatedPlaceholderRule::ID)
    .with_highlight(span)
    .with_fix("replace the `true`/`false` placeholder option with an `if`/`else` expression")
}

/// Creates a "deprecated runtime section" diagnostic.
fn deprecated_runtime_section(task: &str, span: Span) -> Diagnostic {
    Diagnostic::note(format!(
        "task `{task}` contains a deprecated `runtime` section"
    ))
    .with_rule(DeprecatedRuntimeSectionRule::ID)
    .with_highlight(span)
    .with_fix("replace the `runtime` section with a `requirements` section")
}

/// A visitor for deprecated WDL features.
#[derive(Default)]
pub struct Deprecated {
    /// The document version.
    version: Option<SupportedVersion>,
    /// The severity of the `DeprecatedObject` rule.
    object: Option<Severity>,
    /// The severity of the `DeprecatedPlaceholder` rule.
    placeholder: Option<Severity>,
    /// The severity of the `DeprecatedRuntimeSection` rule.
    runtime_section: Option<Severity>,
}

impl Visitor for Deprecated {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn document(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        document: &Document,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.version = Some(version);
        self.object = document.config().diagnostics_config().deprecated_object;
        self.placeholder = document
            .config()
            .diagnostics_config()
            .deprecated_placeholder;
        self.runtime_section = document
            .config()
            .diagnostics_config()
            .deprecated_runtime_section;
    }

    fn bound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &wdl_ast::v1::BoundDecl,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let Some(severity) = self.object else {
            return;
        };

        if let Type::Object(ty) = decl.ty() {
            diagnostics.exceptable_add(
                deprecated_object_use(ty.span()).with_severity(severity),
                decl.inner(),
                &DeprecatedObjectRule::EXCEPTABLE_NODES,
            )
        }
    }

    fn unbound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &wdl_ast::v1::UnboundDecl,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let Some(severity) = self.object else {
            return;
        };

        if let Type::Object(ty) = decl.ty() {
            diagnostics.exceptable_add(
                deprecated_object_use(ty.span()).with_severity(severity),
                decl.inner(),
                &DeprecatedObjectRule::EXCEPTABLE_NODES,
            )
        }
    }

    fn task_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        task: &TaskDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let Some(severity) = self.runtime_section else {
            return;
        };

        // This rule should only be present for WDL v1.2 or later, where the
        // `runtime` section has been deprecated in favor of `requirements`.
        if let SupportedVersion::V1(minor_version) =
            self.version.expect("version should exist here")
            && minor_version >= V1::Two
            && let Some(runtime) = task.runtime()
        {
            let name = task.name();

            diagnostics.exceptable_add(
                deprecated_runtime_section(
                    name.text(),
                    runtime
                        .inner()
                        .first_token()
                        .expect("runtime section should have tokens")
                        .text_range()
                        .into(),
                )
                .with_severity(severity),
                runtime.inner(),
                &DeprecatedRuntimeSectionRule::EXCEPTABLE_NODES,
            );
        }
    }

    fn placeholder(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        placeholder: &Placeholder,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        let Some(severity) = self.placeholder else {
            return;
        };

        if !placeholder.has_tilde() {
            diagnostics.exceptable_add(
                deprecated_interpolation_placeholder_option(Span::new(
                    placeholder.open().span().start(),
                    1,
                ))
                .with_severity(severity),
                placeholder.inner(),
                &DeprecatedPlaceholderRule::EXCEPTABLE_NODES,
            );
        }

        // This rule only executes for WDL documents that have v1.1 or greater.
        //
        // SAFETY: the version must always be set before we get to this point,
        // as document is the root node of the tree.
        match self.version.unwrap() {
            SupportedVersion::V1(v) if v >= V1::One => {}
            _ => return,
        };

        if let Some(option) = placeholder.option() {
            let diagnostic = match option {
                PlaceholderOption::Sep(option) => deprecated_sep_placeholder_option(option.span()),
                PlaceholderOption::Default(option) => {
                    deprecated_default_placeholder_option(option.span())
                }
                PlaceholderOption::TrueFalse(option) => {
                    deprecated_true_false_placeholder_option(option.span())
                }
            };
            diagnostics.exceptable_add(
                diagnostic.with_severity(severity),
                placeholder.inner(),
                &DeprecatedPlaceholderRule::EXCEPTABLE_NODES,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use wdl_ast::Document;
    use wdl_ast::SupportedVersion;
    use wdl_ast::v1::Placeholder;
    use wdl_ast::version::V1;

    use super::*;

    /// Parses a WDL document and collects all placeholders.
    fn parse_placeholders(source: &str) -> (Vec<Placeholder>, Document) {
        let (document, diagnostics) = Document::parse(source, None);
        assert!(
            diagnostics.is_empty(),
            "document should parse without errors: {diagnostics:?}"
        );
        let placeholders: Vec<_> = document.descendants::<Placeholder>().collect();
        (placeholders, document)
    }

    /// Runs the visitor's `placeholder()` method on a given placeholder with
    /// the specified version and returns whether any diagnostics were emitted.
    fn has_diagnostics(placeholder: &Placeholder, version: SupportedVersion) -> bool {
        let mut rule = Deprecated {
            version: Some(version),
            placeholder: Some(Severity::Warning),
            ..Deprecated::default()
        };
        let mut diagnostics = Diagnostics::default();
        rule.placeholder(&mut diagnostics, VisitReason::Enter, placeholder);
        !diagnostics.is_empty()
    }

    #[test]
    fn dollar_placeholder_fires_on_v1_0() {
        let (placeholders, _) = parse_placeholders(
            r#"
version 1.0

task test {
    meta {}
    String x = "${bar}"
    command <<< >>>
    output {}
    runtime {}
}
"#,
        );
        assert_eq!(placeholders.len(), 1);
        assert!(has_diagnostics(
            &placeholders[0],
            SupportedVersion::V1(V1::Zero)
        ));
    }

    #[test]
    fn dollar_placeholder_fires_on_v1_1() {
        let (placeholders, _) = parse_placeholders(
            r#"
version 1.1

task test {
    meta {}
    String x = "${bar}"
    command <<< >>>
    output {}
    runtime {}
}
"#,
        );
        assert_eq!(placeholders.len(), 1);
        assert!(has_diagnostics(
            &placeholders[0],
            SupportedVersion::V1(V1::One)
        ));
    }

    #[test]
    fn tilde_placeholder_does_not_fire() {
        let (placeholders, _) = parse_placeholders(
            r#"
version 1.1

task test {
    meta {}
    String x = "~{bar}"
    command <<< >>>
    output {}
    runtime {}
}
"#,
        );
        assert_eq!(placeholders.len(), 1);
        assert!(!has_diagnostics(
            &placeholders[0],
            SupportedVersion::V1(V1::One)
        ));
    }

    #[test]
    fn sep_option_does_not_fire_on_v1_0() {
        let (placeholders, _) = parse_placeholders(
            r#"
version 1.0

task test {
    meta {}
    Array[String] xs = ["a"]
    String x = "~{sep="," xs}"
    command <<< >>>
    output {}
    runtime {}
}
"#,
        );
        assert_eq!(placeholders.len(), 1);
        assert!(!has_diagnostics(
            &placeholders[0],
            SupportedVersion::V1(V1::Zero)
        ));
    }

    #[test]
    fn sep_option_fires_on_v1_1() {
        let (placeholders, _) = parse_placeholders(
            r#"
version 1.1

task test {
    meta {}
    Array[String] xs = ["a"]
    String x = "~{sep="," xs}"
    command <<< >>>
    output {}
    runtime {}
}
"#,
        );
        assert_eq!(placeholders.len(), 1);
        assert!(has_diagnostics(
            &placeholders[0],
            SupportedVersion::V1(V1::One)
        ));
    }

    #[test]
    fn dollar_with_sep_option_fires_on_v1_0() {
        let (placeholders, _) = parse_placeholders(
            r#"
version 1.0

task test {
    meta {}
    Array[String] xs = ["a"]
    String x = "${sep="," xs}"
    command <<< >>>
    output {}
    runtime {}
}
"#,
        );
        assert_eq!(placeholders.len(), 1);
        assert!(has_diagnostics(
            &placeholders[0],
            SupportedVersion::V1(V1::Zero)
        ));
    }

    #[test]
    fn dollar_with_sep_option_fires_on_v1_1() {
        let (placeholders, _) = parse_placeholders(
            r#"
version 1.1

task test {
    meta {}
    Array[String] xs = ["a"]
    String x = "${sep="," xs}"
    command <<< >>>
    output {}
    runtime {}
}
"#,
        );
        assert_eq!(placeholders.len(), 1);
        assert!(has_diagnostics(
            &placeholders[0],
            SupportedVersion::V1(V1::One)
        ));
    }

    #[test]
    fn default_option_fires_on_v1_1() {
        let (placeholders, _) = parse_placeholders(
            r#"
version 1.1

task test {
    meta {}
    String bar = "bar"
    String x = "~{default="baz" bar}"
    command <<< >>>
    output {}
    runtime {}
}
"#,
        );
        assert_eq!(placeholders.len(), 1);
        assert!(has_diagnostics(
            &placeholders[0],
            SupportedVersion::V1(V1::One)
        ));
    }

    #[test]
    fn true_false_option_fires_on_v1_1() {
        let (placeholders, _) = parse_placeholders(
            r#"
version 1.1

task test {
    meta {}
    Boolean flag = true
    String x = "~{true="yes" false="no" flag}"
    command <<< >>>
    output {}
    runtime {}
}
"#,
        );
        assert_eq!(placeholders.len(), 1);
        assert!(has_diagnostics(
            &placeholders[0],
            SupportedVersion::V1(V1::One)
        ));
    }

    #[test]
    fn interpolation_diagnostic_highlights_single_character() {
        let (placeholders, _) = parse_placeholders(
            r#"
version 1.0

task test {
    meta {}
    String x = "${bar}"
    command <<< >>>
    output {}
    runtime {}
}
"#,
        );
        assert_eq!(placeholders.len(), 1);

        let diagnostic = deprecated_interpolation_placeholder_option(Span::new(
            placeholders[0].open().span().start(),
            1,
        ));
        let labels: Vec<_> = diagnostic.labels().collect();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].span().len(), 1);
    }
}
