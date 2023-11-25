//! Representation of ignored errors as stored in the configuration file.

// This compiler attribute is added because `serde_with` generates a struct
// below that does not have any documentation. The only way to silence the
// warning is to allow missing docs for this file.
#![allow(missing_docs)]

use std::convert::Infallible;

use crate::config::inner::ReportableConcerns;
use crate::config::ReportableConcern;

serde_with::serde_conv!(
    pub ReportableConcernsRepr,
    ReportableConcerns,
    |concerns: &ReportableConcerns| {
        let mut result = concerns
        .clone()
        .into_iter()
        .collect::<Vec<_>>();
    result.sort();
    result
    },
    |errors: Vec<ReportableConcern>| -> Result<_, Infallible> {
        Ok(errors
            .into_iter()
            .collect::<ReportableConcerns>())
    }
);
