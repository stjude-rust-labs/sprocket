//! Error type definitions.

use std::error::Error;
use std::fmt::Display;

/// Errors that can occur while generating documentation.
#[derive(Debug)]
pub enum DocError {
    /// One or more documents failed analysis.
    ///
    /// NOTE: This contains *all* analysis results, including those of valid
    /// documents.
    AnalysisFailed(Vec<wdl_analysis::AnalysisResult>),
    /// All other errors (e.g. I/O)
    Other(anyhow::Error),
}

impl Display for DocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocError::AnalysisFailed(_) => {
                write!(f, "a WDL document in the workspace has analysis errors")
            }
            DocError::Other(e) => e.fmt(f),
        }
    }
}

impl Error for DocError {}

impl From<anyhow::Error> for DocError {
    fn from(e: anyhow::Error) -> Self {
        DocError::Other(e)
    }
}
