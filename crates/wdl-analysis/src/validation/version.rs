//! Validation of supported syntax for WDL versions.

use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::v1;
use wdl_ast::v1::Exponentiation;
use wdl_ast::v1::Expr;
use wdl_ast::v1::HintsKeyword;
use wdl_ast::v1::InputKeyword;
use wdl_ast::v1::MetaKeyword;
use wdl_ast::v1::ParameterMetaKeyword;
use wdl_ast::v1::RequirementsKeyword;
use wdl_ast::version::V1;

use crate::Config;
use crate::Diagnostics;
use crate::VisitReason;
use crate::Visitor;
use crate::document::Document;

/// Creates an "exponentiation requirement" diagnostic.
fn exponentiation_requirement(span: Span) -> Diagnostic {
    Diagnostic::error("use of the exponentiation operator requires WDL version 1.2")
        .with_highlight(span)
}

/// Creates a "requirements section requirement" diagnostic.
fn requirements_section(span: Span) -> Diagnostic {
    Diagnostic::error("use of the `requirements` section requires WDL version 1.2")
        .with_highlight(span)
}

/// Creates a "hints section requirement" diagnostic.
fn hints_section(span: Span) -> Diagnostic {
    Diagnostic::error("use of the `hints` section requires WDL version 1.2").with_highlight(span)
}

/// Creates a "multi-line string requirement" diagnostic.
fn multiline_string_requirement(span: Span) -> Diagnostic {
    Diagnostic::error("use of multi-line strings requires WDL version 1.2").with_highlight(span)
}

/// Creates a "directory type" requirement diagnostic.
fn directory_type_requirement(span: Span) -> Diagnostic {
    Diagnostic::error("use of the `Directory` type requires WDL version 1.2").with_highlight(span)
}

/// Creates an "input keyword" requirement diagnostic.
fn input_keyword_requirement(span: Span) -> Diagnostic {
    Diagnostic::error("omitting the `input` keyword in a call statement requires WDL version 1.2")
        .with_label("missing an `input` keyword before this input", span)
        .with_fix("add an `input` keyword followed by a colon before any call inputs")
}

/// Creates a "struct metadata requirement" diagnostic.
fn struct_metadata_requirement(kind: &str, span: Span) -> Diagnostic {
    Diagnostic::error(format!(
        "use of a `{kind}` section in a struct definition requires WDL version 1.2"
    ))
    .with_highlight(span)
}

/// Creates an "env var" requirement diagnostic.
fn env_var_requirement(span: Span) -> Diagnostic {
    Diagnostic::error("use of environment variable declarations requires WDL version 1.2")
        .with_highlight(span)
}

/// Creates a deprecation warning for a deprecated version feature flag.
fn deprecated_version_feature_flag(flag_name: &str, span: Span) -> Diagnostic {
    Diagnostic::warning(format!(
        "the `{flag_name}` feature flag is deprecated; please remove this feature flag from your \
         configuration file"
    ))
    .with_highlight(span)
}

/// Creates an "unsupported version" diagnostic.
fn unsupported_version(version: SupportedVersion, span: Span) -> Diagnostic {
    Diagnostic::error(format!("unsupported version {version}")).with_highlight(span)
}

/// Tracks the state of a deprecated version feature flag.
#[derive(Clone, Copy, Debug, Default)]
struct DeprecatedVersionFeatureFlag {
    /// Whether the user explicitly disabled the feature flag.
    explicitly_disabled: bool,
    /// Whether the deprecation warning has been emitted.
    warning_emitted: bool,
}

/// An AST visitor that ensures the syntax present in the document matches the
/// document's declared version.
#[derive(Debug, Default)]
pub struct VersionVisitor {
    /// The state of the deprecated `wdl_1_3` feature flag.
    wdl_1_3_ff: DeprecatedVersionFeatureFlag,
    /// Stores the supported version of the WDL document we're visiting.
    version: Option<SupportedVersion>,
}

impl Visitor for VersionVisitor {
    fn register(&mut self, config: &Config) {
        self.wdl_1_3_ff.explicitly_disabled = !config.feature_flags().wdl_1_3();
    }

    fn reset(&mut self) {
        let wdl_1_3_ff = self.wdl_1_3_ff;
        *self = Default::default();
        self.wdl_1_3_ff = wdl_1_3_ff;
    }

    fn document(
        &mut self,
        _: &mut Diagnostics,
        reason: VisitReason,
        _: &Document,
        version: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        self.version = Some(version);
    }

    fn version_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &wdl_ast::VersionStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Emit a deprecation warning if the user explicitly disabled WDL 1.3 and we
        // encounter a WDL 1.3 document.
        if let Some(version) = self.version {
            match version {
                SupportedVersion::V1(V1::Three)
                    if self.wdl_1_3_ff.explicitly_disabled && !self.wdl_1_3_ff.warning_emitted =>
                {
                    diagnostics.add(deprecated_version_feature_flag(
                        "wdl_1_3",
                        stmt.version().span(),
                    ));
                    self.wdl_1_3_ff.warning_emitted = true;
                }
                // TODO ACF 2025-10-21: This is an unfortunate consequence of using
                // `#[non_exhaustive]` on the version enums. We should consider removing that
                // attribute in the future to get static assurance that downstream consumers of
                // versions comprehensively handle the possible cases.
                SupportedVersion::V1(V1::Zero | V1::One | V1::Two | V1::Three) => {}
                other => diagnostics.add(unsupported_version(other, stmt.version().span())),
            }
        }
    }

    fn requirements_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::RequirementsSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(version) = self.version
            && version < SupportedVersion::V1(V1::Two)
        {
            diagnostics.add(requirements_section(
                section
                    .token::<RequirementsKeyword<_>>()
                    .expect("should have keyword")
                    .span(),
            ));
        }
    }

    fn task_hints_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::TaskHintsSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(version) = self.version
            && version < SupportedVersion::V1(V1::Two)
        {
            diagnostics.add(hints_section(
                section
                    .token::<HintsKeyword<_>>()
                    .expect("should have keyword")
                    .span(),
            ));
        }
    }

    fn workflow_hints_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::WorkflowHintsSection,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(version) = self.version
            && version < SupportedVersion::V1(V1::Two)
        {
            diagnostics.add(hints_section(
                section
                    .token::<HintsKeyword<_>>()
                    .expect("should have keyword")
                    .span(),
            ));
        }
    }

    fn expr(&mut self, diagnostics: &mut Diagnostics, reason: VisitReason, expr: &v1::Expr) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(version) = self.version {
            match expr {
                Expr::Exponentiation(e) if version < SupportedVersion::V1(V1::Two) => {
                    diagnostics.add(exponentiation_requirement(
                        e.token::<Exponentiation<_>>()
                            .expect("should have operator")
                            .span(),
                    ));
                }
                v1::Expr::Literal(v1::LiteralExpr::String(s))
                    if version < SupportedVersion::V1(V1::Two)
                        && s.kind() == v1::LiteralStringKind::Multiline =>
                {
                    diagnostics.add(multiline_string_requirement(s.span()));
                }
                _ => {}
            }
        }
    }

    fn bound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &v1::BoundDecl,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(version) = self.version {
            if let Some(env) = decl.env()
                && version < SupportedVersion::V1(V1::Two)
            {
                diagnostics.add(env_var_requirement(env.span()));
            }

            if let v1::Type::Primitive(ty) = decl.ty()
                && version < SupportedVersion::V1(V1::Two)
                && ty.kind() == v1::PrimitiveTypeKind::Directory
            {
                diagnostics.add(directory_type_requirement(ty.span()));
            }
        }
    }

    fn unbound_decl(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        decl: &v1::UnboundDecl,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(version) = self.version {
            if let Some(env) = decl.env()
                && version < SupportedVersion::V1(V1::Two)
            {
                diagnostics.add(env_var_requirement(env.span()));
            }

            if let v1::Type::Primitive(ty) = decl.ty()
                && version < SupportedVersion::V1(V1::Two)
                && ty.kind() == v1::PrimitiveTypeKind::Directory
            {
                diagnostics.add(directory_type_requirement(ty.span()));
            }
        }
    }

    fn call_statement(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        stmt: &v1::CallStatement,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(version) = self.version
            && version < SupportedVersion::V1(V1::Two)
        {
            // Ensure there is a input keyword child token if there are inputs
            if let Some(input) = stmt.inputs().next()
                && stmt.token::<InputKeyword<_>>().is_none()
            {
                diagnostics.add(input_keyword_requirement(input.span()));
            }
        }
    }

    fn struct_definition(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        def: &v1::StructDefinition,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        if let Some(version) = self.version
            && version < SupportedVersion::V1(V1::Two)
        {
            if let Some(section) = def.metadata().next() {
                diagnostics.add(struct_metadata_requirement(
                    "meta",
                    section
                        .token::<MetaKeyword<_>>()
                        .expect("should have keyword")
                        .span(),
                ));
            }

            if let Some(section) = def.parameter_metadata().next() {
                diagnostics.add(struct_metadata_requirement(
                    "parameter_meta",
                    section
                        .token::<ParameterMetaKeyword<_>>()
                        .expect("should have keyword")
                        .span(),
                ));
            }
        }
    }
}
