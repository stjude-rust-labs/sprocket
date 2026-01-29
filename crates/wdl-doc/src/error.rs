//! Error type definitions.

use std::error::Error;
use std::fmt::Display;
use std::io::Error as IoError;
use std::path::PathBuf;

use wdl_analysis::AnalysisResult;

/// Result type for documentation operations.
pub type DocResult<T> = Result<T, DocError>;

/// Extensions for documentation results.
pub(crate) trait ResultContextExt {
    /// Apply additional context to a [`DocResult`] if it contains an error.
    fn with_context<F, C>(self, context: F) -> Self
    where
        F: FnOnce() -> C,
        C: Display;
}

impl<T> ResultContextExt for DocResult<T> {
    fn with_context<F, C>(self, context: F) -> Self
    where
        F: FnOnce() -> C,
        C: Display,
    {
        self.map_err(|e| e.with_context(context()))
    }
}

/// Errors that can occur while running `npm` commands.
#[derive(Debug)]
pub enum NpmError {
    /// Failed to run `npm run build` in the theme directory.
    Build(IoError),
    /// Failed to run `npm install` in the theme directory.
    Install(IoError),
    /// Failed to run `npx @tailwindcss/cli`.
    Tailwind(IoError),
}

impl Display for NpmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NpmError::Build(e) => write!(f, "failed to run `npm run build`: {e}"),
            NpmError::Install(e) => write!(f, "failed to run `npm install`: {e}"),
            NpmError::Tailwind(e) => write!(f, "failed to run `npx @tailwindcss/cli`: {e}"),
        }
    }
}

/// The kinds of errors that can occur
#[derive(Debug)]
pub enum DocErrorKind {
    /// The expected workspace was not found.
    WorkspaceNotFound(PathBuf),
    /// No WDL documents were found in the workspace
    NoDocuments,
    /// Failed to run analysis on the workspace.
    Analyzer(anyhow::Error),
    /// One or more documents failed analysis.
    ///
    /// This contains the analysis results of all failed documents.
    AnalysisFailed(Vec<AnalysisResult>),
    /// Failed to run an `npm` command.
    Npm(NpmError),
    /// An I/O operation failed.
    Io(IoError),
}

/// Errors that can occur while generating documentation.
#[derive(Debug)]
pub struct DocError {
    /// An additional context message, if applicable.
    context: Option<String>,
    /// The kind of error that occurred.
    kind: DocErrorKind,
}

impl DocError {
    /// Create a new `DocError`.
    pub fn new(kind: DocErrorKind) -> Self {
        Self {
            kind,
            context: None,
        }
    }

    /// The kind of error that occured.
    pub fn kind(&self) -> &DocErrorKind {
        &self.kind
    }

    /// Add a context message to this error.
    pub fn with_context(self, context: impl Display) -> Self {
        Self {
            context: Some(context.to_string()),
            kind: self.kind,
        }
    }
}

impl Display for DocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref ctx) = self.context {
            write!(f, "{ctx}: ")?;
        }

        match &self.kind {
            DocErrorKind::WorkspaceNotFound(root) => {
                write!(
                    f,
                    "workspace root `{}` not found in analysis results",
                    root.display()
                )
            }
            DocErrorKind::NoDocuments => write!(f, "no WDL documents found in analysis"),
            DocErrorKind::Analyzer(e) => write!(f, "{e}"),
            DocErrorKind::AnalysisFailed(_) => {
                write!(f, "a WDL document in the workspace has analysis errors")
            }
            DocErrorKind::Npm(e) => write!(f, "{e}"),
            DocErrorKind::Io(e) => write!(f, "{e}"),
        }
    }
}

impl Error for DocError {}

impl From<NpmError> for DocError {
    fn from(e: NpmError) -> Self {
        DocError::new(DocErrorKind::Npm(e))
    }
}

impl From<IoError> for DocError {
    fn from(e: IoError) -> Self {
        DocError::new(DocErrorKind::Io(e))
    }
}

impl From<DocErrorKind> for DocError {
    fn from(e: DocErrorKind) -> Self {
        DocError::new(e)
    }
}
