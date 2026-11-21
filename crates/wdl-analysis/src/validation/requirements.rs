//! Validation of requirements section keys.

use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
use wdl_ast::SupportedVersion;
use wdl_ast::v1;
use wdl_ast::v1::TASK_REQUIREMENT_CONTAINER;
use wdl_ast::v1::TASK_REQUIREMENT_CONTAINER_ALIAS;
use wdl_ast::v1::TASK_REQUIREMENT_CPU;
use wdl_ast::v1::TASK_REQUIREMENT_DISKS;
use wdl_ast::v1::TASK_REQUIREMENT_FPGA;
use wdl_ast::v1::TASK_REQUIREMENT_GPU;
use wdl_ast::v1::TASK_REQUIREMENT_MAX_RETRIES;
use wdl_ast::v1::TASK_REQUIREMENT_MAX_RETRIES_ALIAS;
use wdl_ast::v1::TASK_REQUIREMENT_MEMORY;
use wdl_ast::v1::TASK_REQUIREMENT_PREEMPTIBLE_ONLY;
use wdl_ast::v1::TASK_REQUIREMENT_RETURN_CODES;
use wdl_ast::v1::TASK_REQUIREMENT_RETURN_CODES_ALIAS;
use wdl_ast::version::V1;

use crate::Diagnostics;
use crate::VisitReason;
use crate::Visitor;
use crate::document::Document;

/// Creates an "unsupported requirements key" diagnostic.
fn unsupported_requirements_key(name: &Ident) -> Diagnostic {
    Diagnostic::error(format!(
        "unsupported requirements key `{name}`",
        name = name.text()
    ))
    .with_highlight(name.span())
}

/// Creates a "`preemptible_only` requires WDL 1.4" diagnostic.
fn preemptible_only_requirement(name: &Ident) -> Diagnostic {
    Diagnostic::error("use of the `preemptible_only` requirement requires WDL version 1.4")
        .with_highlight(name.span())
}

/// An AST visitor that ensures the keys of a requirements section are
/// supported.
#[derive(Debug, Default)]
pub struct RequirementsVisitor {
    /// Stores the supported version of the WDL document we're visiting.
    version: Option<SupportedVersion>,
}

impl Visitor for RequirementsVisitor {
    fn reset(&mut self) {
        *self = Default::default();
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

    fn requirements_section(
        &mut self,
        diagnostics: &mut Diagnostics,
        reason: VisitReason,
        section: &v1::RequirementsSection,
    ) {
        /// The supported set of requirement keys as of 1.2
        const SUPPORTED_KEYS: &[&str] = &[
            TASK_REQUIREMENT_CONTAINER,
            TASK_REQUIREMENT_CONTAINER_ALIAS,
            TASK_REQUIREMENT_CPU,
            TASK_REQUIREMENT_MEMORY,
            TASK_REQUIREMENT_GPU,
            TASK_REQUIREMENT_FPGA,
            TASK_REQUIREMENT_DISKS,
            TASK_REQUIREMENT_MAX_RETRIES,
            TASK_REQUIREMENT_MAX_RETRIES_ALIAS,
            TASK_REQUIREMENT_RETURN_CODES,
            TASK_REQUIREMENT_RETURN_CODES_ALIAS,
        ];

        if reason == VisitReason::Exit {
            return;
        }

        for item in section.items() {
            let name = item.name();
            if name.text() == TASK_REQUIREMENT_PREEMPTIBLE_ONLY {
                if self.version < Some(SupportedVersion::V1(V1::Four)) {
                    diagnostics.add(preemptible_only_requirement(&name));
                }
            } else if !SUPPORTED_KEYS.contains(&name.text()) {
                diagnostics.add(unsupported_requirements_key(&name))
            }
        }
    }
}
