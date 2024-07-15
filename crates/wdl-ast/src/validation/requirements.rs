//! Validation of requirements section keys.

use crate::v1;
use crate::AstToken;
use crate::Diagnostic;
use crate::Diagnostics;
use crate::Document;
use crate::Ident;
use crate::SupportedVersion;
use crate::VisitReason;
use crate::Visitor;

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
            "container",
            "docker", // alias of `container` to be removed in 2.0
            "cpu",
            "memory",
            "gpu",
            "fpga",
            "disks",
            "max_retries",
            "maxRetries", // alias of `max_retries`
            "return_codes",
            "returnCodes", // alias of `return_codes`
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
