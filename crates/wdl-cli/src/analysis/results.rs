//! Results of an analysis.

use std::rc::Rc;
use std::sync::Arc;

use anyhow::Error;
use nonempty::NonEmpty;
use wdl_analysis::AnalysisResult;
use wdl_ast::AstNode as _;
use wdl_ast::Diagnostic;

use crate::analysis::Source;

/// A set of analysis results.
///
/// If successfully created, the set of analysis results are guaranteed not to
/// have any associated errors (but they may contain diagnostics).
#[derive(Debug)]
pub struct AnalysisResults(Vec<AnalysisResult>);

impl AnalysisResults {
    /// Attempts to create a new set of analysis results.
    ///
    /// Returns any errors encountered during analysis. That being said, each
    /// analysis result may have diagnostics.
    pub fn try_new(
        results: Vec<AnalysisResult>,
    ) -> std::result::Result<Self, NonEmpty<Arc<Error>>> {
        let mut errors = results.iter().flat_map(|result| result.error().cloned());

        if let Some(error) = errors.next() {
            let mut results = NonEmpty::new(error);
            results.extend(errors);
            Err(results)
        } else {
            Ok(Self(results))
        }
    }

    /// Consumes `self` and returns the inner vector of analysis results.
    pub fn into_inner(self) -> Vec<AnalysisResult> {
        self.0
    }

    /// Attempts to find all analysis results that match any of the provided
    /// sources.
    pub fn filter(&self, sources: &[&Source]) -> impl Iterator<Item = &AnalysisResult> {
        self.0.iter().filter(|r| {
            let mut path = None;
            sources.iter().any(|s| match s {
                Source::Remote(url) | Source::File(url) => url == r.document().uri().as_ref(),
                Source::Directory(dir) => path
                    .get_or_insert_with(|| r.document().uri().to_file_path())
                    .as_ref()
                    .map(|p| p.starts_with(dir))
                    .unwrap_or(false),
            })
        })
    }

    /// Iterates over the diagnostics within the analysis result set.
    ///
    /// The return type is an iterator that yields tuples that contain the
    /// following:
    ///
    /// - The path to the file containing the diagnostic.
    /// - The source of the file containing the diagnostic.
    /// - A reference to the diagnostic itself.
    pub fn diagnostics(&self) -> impl Iterator<Item = (Rc<String>, Rc<String>, &Diagnostic)> {
        self.0.iter().flat_map(|result| {
            let path = Rc::new(result.document().path().to_string());
            let source = Rc::new(result.document().root().text().to_string());

            result
                .document()
                .diagnostics()
                .map(move |diagnostic| (path.clone(), source.clone(), diagnostic))
        })
    }
}

impl IntoIterator for AnalysisResults {
    type IntoIter = std::vec::IntoIter<AnalysisResult>;
    type Item = AnalysisResult;

    fn into_iter(self) -> Self::IntoIter {
        self.into_inner().into_iter()
    }
}
