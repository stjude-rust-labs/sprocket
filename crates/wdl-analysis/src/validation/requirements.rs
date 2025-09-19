//! Validation of requirements section keys.

use wdl_ast::AstToken;
use wdl_ast::Diagnostic;
use wdl_ast::Ident;
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
use wdl_ast::v1::TASK_REQUIREMENT_RETURN_CODES;
use wdl_ast::v1::TASK_REQUIREMENT_RETURN_CODES_ALIAS;

use crate::Diagnostics;
use crate::VisitReason;
use crate::Visitor;

/// Creates an "unsupported requirements key" diagnostic.
fn unsupported_requirements_key(name: &Ident) -> Diagnostic {
    Diagnostic::error(format!(
        "unsupported requirements key `{name}`",
        name = name.text()
    ))
    .with_highlight(name.span())
}

/// An AST visitor that ensures the keys of a requirements section are
/// supported.
#[derive(Debug, Default)]
pub struct RequirementsVisitor;

impl Visitor for RequirementsVisitor {
    fn reset(&mut self) {
        *self = Default::default();
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
            if !SUPPORTED_KEYS.contains(&name.text()) {
                diagnostics.add(unsupported_requirements_key(&name))
            }
        }
    }
}
