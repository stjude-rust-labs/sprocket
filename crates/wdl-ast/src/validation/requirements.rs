//! Validation of requirements section keys.

use crate::AstToken;
use crate::Diagnostic;
use crate::Diagnostics;
use crate::Document;
use crate::Ident;
use crate::SupportedVersion;
use crate::VisitReason;
use crate::Visitor;
use crate::v1;
use crate::v1::TASK_REQUIREMENT_CONTAINER;
use crate::v1::TASK_REQUIREMENT_CONTAINER_ALIAS;
use crate::v1::TASK_REQUIREMENT_CPU;
use crate::v1::TASK_REQUIREMENT_DISKS;
use crate::v1::TASK_REQUIREMENT_FPGA;
use crate::v1::TASK_REQUIREMENT_GPU;
use crate::v1::TASK_REQUIREMENT_MAX_RETRIES;
use crate::v1::TASK_REQUIREMENT_MAX_RETRIES_ALIAS;
use crate::v1::TASK_REQUIREMENT_MEMORY;
use crate::v1::TASK_REQUIREMENT_RETURN_CODES;
use crate::v1::TASK_REQUIREMENT_RETURN_CODES_ALIAS;

/// Creates an "unsupported requirements key" diagnostic.
fn unsupported_requirements_key(name: &Ident) -> Diagnostic {
    Diagnostic::error(format!(
        "unsupported requirements key `{name}`",
        name = name.as_str()
    ))
    .with_highlight(name.span())
}

/// An AST visitor that ensures the keys of a requirements section are
/// supported.
#[derive(Debug, Default)]
pub struct RequirementsVisitor;

impl Visitor for RequirementsVisitor {
    type State = Diagnostics;

    fn document(
        &mut self,
        _: &mut Self::State,
        reason: VisitReason,
        _: &Document,
        _: SupportedVersion,
    ) {
        if reason == VisitReason::Exit {
            return;
        }

        // Reset the visitor on document entry
        *self = Default::default();
    }

    fn requirements_section(
        &mut self,
        state: &mut Self::State,
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
            if !SUPPORTED_KEYS.contains(&name.as_str()) {
                state.add(unsupported_requirements_key(&name))
            }
        }
    }
}
