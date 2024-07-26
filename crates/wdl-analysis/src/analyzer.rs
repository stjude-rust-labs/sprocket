//! Implementation of the analyzer.

use std::fmt;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::ops::Range;
use std::path::absolute;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;

use line_index::LineCol;
use line_index::LineIndex;
use line_index::WideEncoding;
use line_index::WideLineCol;
use path_clean::clean;
use rowan::GreenNode;
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use url::Url;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::SyntaxNode;
use wdl_ast::Validator;

use crate::graph::DocumentGraphNode;
use crate::graph::ParseState;
use crate::queue::AnalysisQueue;
use crate::queue::AnalyzeRequest;
use crate::queue::RemoveRequest;
use crate::queue::Request;
use crate::rayon::RayonHandle;
use crate::DocumentScope;

/// Represents the kind of analysis progress being reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressKind {
    /// The progress is for parsing documents.
    Parsing,
    /// The progress is for analyzing documents.
    Analyzing,
}

impl fmt::Display for ProgressKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parsing => write!(f, "parsing"),
            Self::Analyzing => write!(f, "analyzing"),
        }
    }
}

/// Converts a local path to a file schemed URI.
pub fn path_to_uri(path: &Path) -> Option<Url> {
    Url::from_file_path(clean(absolute(path).ok()?)).ok()
}

/// Represents the result of a parse.
#[derive(Debug, Clone)]
pub enum ParseResult {
    /// There was an error parsing the document.
    Error(Arc<anyhow::Error>),
    /// The document was parsed.
    Parsed {
        /// The root node of the document.
        root: GreenNode,
        /// The line index used to map line/column offsets to byte offsets and
        /// vice versa.
        lines: Arc<LineIndex>,
    },
}

impl ParseResult {
    /// Gets the root from the parse result.
    ///
    /// Returns `None` if there was an error parsing the document.
    pub fn root(&self) -> Option<&GreenNode> {
        match self {
            Self::Error(_) => None,
            Self::Parsed { root, .. } => Some(root),
        }
    }

    /// Gets the line index from the parse result.
    ///
    /// Returns `None` if there was an error parsing the document.
    pub fn lines(&self) -> Option<&Arc<LineIndex>> {
        match self {
            Self::Error(_) => None,
            Self::Parsed { lines, .. } => Some(lines),
        }
    }

    /// Gets the AST document of the parse result.
    ///
    /// Returns `None` if there was an error parsing the document.
    pub fn document(&self) -> Option<wdl_ast::Document> {
        match &self {
            ParseResult::Error(_) => None,
            ParseResult::Parsed { root, .. } => Some(
                wdl_ast::Document::cast(SyntaxNode::new_root(root.clone()))
                    .expect("node should cast"),
            ),
        }
    }

    /// Gets the error parsing the document.
    ///
    /// Returns` None` if the document was parsed.
    pub fn error(&self) -> Option<&Arc<anyhow::Error>> {
        match self {
            Self::Error(e) => Some(e),
            ParseResult::Parsed { .. } => None,
        }
    }
}

impl From<&ParseState> for ParseResult {
    fn from(state: &ParseState) -> Self {
        match state {
            ParseState::NotParsed => {
                panic!("cannot create a result for an file that hasn't been parsed")
            }
            ParseState::Error(e) => Self::Error(e.clone()),
            ParseState::Parsed { root, lines, .. } => Self::Parsed {
                root: root.clone(),
                lines: lines.clone(),
            },
        }
    }
}

/// Represents the result of an analysis.
///
/// Analysis results are cheap to clone.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    /// The URI of the analyzed document.
    uri: Arc<Url>,
    /// The result from parsing the file.
    parse_result: ParseResult,
    /// The diagnostics for the document.
    diagnostics: Arc<[Diagnostic]>,
    /// The scope of the analyzed document.
    scope: Arc<DocumentScope>,
}

impl AnalysisResult {
    /// Constructs a new analysis result for the given graph node.
    pub(crate) fn new(node: &DocumentGraphNode) -> Self {
        let analysis = node.analysis().expect("analysis not completed");

        Self {
            uri: node.uri().clone(),
            parse_result: node.parse_state().into(),
            diagnostics: analysis.diagnostics().clone(),
            scope: analysis.scope().clone(),
        }
    }

    /// Gets the URI of the document that was analyzed.
    pub fn uri(&self) -> &Arc<Url> {
        &self.uri
    }

    /// Gets the result of the parse.
    pub fn parse_result(&self) -> &ParseResult {
        &self.parse_result
    }

    /// Gets the diagnostics associated with the document.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Gets the scope of the analyzed document.
    pub fn scope(&self) -> &Arc<DocumentScope> {
        &self.scope
    }
}

/// Represents a position in a document's source.
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Default)]
pub struct SourcePosition {
    /// Line position in a document (zero-based).
    // NOTE: this field must come before `character` to maintain a correct sort order.
    pub line: u32,
    /// Character offset on a line in a document (zero-based). The meaning of
    /// this offset is determined by the position encoding.
    pub character: u32,
}

impl SourcePosition {
    /// Constructs a new source position from a line and character offset.
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// Represents the encoding of a source position.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum SourcePositionEncoding {
    /// The position is UTF8 encoded.
    ///
    /// A position's character is the UTF-8 offset from the start of the line.
    UTF8,
    /// The position is UTF16 encoded.
    ///
    /// A position's character is the UTF-16 offset from the start of the line.
    UTF16,
}

/// Represents an edit to a document's source.
#[derive(Debug, Clone)]
pub struct SourceEdit {
    /// The range of the edit.
    range: Range<SourcePosition>,
    /// The encoding of the edit positions.
    encoding: SourcePositionEncoding,
    /// The replacement text.
    text: String,
}

impl SourceEdit {
    /// Creates a new source edit for the given range and replacement text.
    pub fn new(
        range: Range<SourcePosition>,
        encoding: SourcePositionEncoding,
        text: impl Into<String>,
    ) -> Self {
        Self {
            range,
            encoding,
            text: text.into(),
        }
    }

    /// Gets the range of the edit.
    pub(crate) fn range(&self) -> Range<SourcePosition> {
        self.range.start..self.range.end
    }

    /// Applies the edit to the given string.
    pub(crate) fn apply(&self, source: &mut String, lines: &LineIndex) {
        let (start, end) = match self.encoding {
            SourcePositionEncoding::UTF8 => (
                LineCol {
                    line: self.range.start.line,
                    col: self.range.start.character,
                },
                LineCol {
                    line: self.range.end.line,
                    col: self.range.end.character,
                },
            ),
            SourcePositionEncoding::UTF16 => (
                lines
                    .to_utf8(
                        WideEncoding::Utf16,
                        WideLineCol {
                            line: self.range.start.line,
                            col: self.range.start.character,
                        },
                    )
                    .expect("expected a valid start position"),
                lines
                    .to_utf8(
                        WideEncoding::Utf16,
                        WideLineCol {
                            line: self.range.end.line,
                            col: self.range.end.character,
                        },
                    )
                    .expect("expected a valid end position"),
            ),
        };

        let range: Range<usize> = lines
            .offset(start)
            .expect("expected a valid start position")
            .into()
            ..lines
                .offset(end)
                .expect("expected a valid end position")
                .into();
        source.replace_range(range, &self.text);
    }
}

/// Represents a change to a document.
#[derive(Clone, Debug)]
pub enum DocumentChange {
    /// The document has changed and should be fetched again from its URI.
    Refetch,
    /// The document has incrementally changed and the specified edits should be
    /// applied.
    Incremental {
        /// The source to start from for applying edits.
        ///
        /// If this is `Some`, a full reparse will occur after applying edits to
        /// this string.
        ///
        /// If this is `None`, edits will be applied to the existing CST.
        start: Option<Arc<String>>,
        /// The source edits to apply.
        edits: Vec<SourceEdit>,
    },
}

/// Represents a Workflow Description Language (WDL) document analyzer.
///
/// By default, analysis parses documents, performs validation checks, resolves
/// imports, and performs type checking.
///
/// Each analysis operation is processed in order of request; however, the
/// individual parsing, resolution, and analysis of documents is performed
/// across a thread pool.
///
/// Note that dropping the analyzer is a blocking operation as it will wait for
/// the queue thread to join.
#[derive(Debug)]
pub struct Analyzer {
    /// The sender for sending analysis requests to the queue.
    sender: ManuallyDrop<mpsc::UnboundedSender<Request>>,
    /// The join handle for the queue task.
    handle: Option<JoinHandle<()>>,
}

impl Analyzer {
    /// Constructs a new analyzer.
    ///
    /// The provided progress callback will be invoked during analysis.
    ///
    /// The analyzer will use a default validator for validation.
    ///
    /// The analyzer must be constructed from the context of a Tokio runtime.
    pub fn new<P, R>(progress: P) -> Self
    where
        P: Fn(ProgressKind, usize, usize, Option<String>) -> R + Send + Sync + 'static,
        R: Future<Output = ()>,
    {
        Self::new_with_validator(progress, Validator::default)
    }

    /// Constructs a new analyzer with the given validator function.
    ///
    /// The provided progress callback will be invoked during analysis.
    ///
    /// This validator function will be called once per worker thread to
    /// initialize a thread-local validator.
    ///
    /// The analyzer must be constructed from the context of a Tokio runtime.
    pub fn new_with_validator<P, R, V>(progress: P, validator: V) -> Self
    where
        P: Fn(ProgressKind, usize, usize, Option<String>) -> R + Send + Sync + 'static,
        R: Future<Output = ()>,
        V: Fn() -> Validator + Send + Sync + 'static,
    {
        Self::new_with_changes(progress, validator, |_| None)
    }

    /// Constructs a new analyzer with the given validator function and changes
    /// function.
    ///
    /// The provided progress callback will be invoked during analysis.
    ///
    /// This validator function will be called once per worker thread to
    /// initialize a thread-local validator.
    ///
    /// The change function will be called when the analyzer needs to take the
    /// changes for the provided document.
    ///
    /// The analyzer must be constructed from the context of a Tokio runtime.
    pub fn new_with_changes<P, R, V, C>(progress: P, validator: V, changes: C) -> Self
    where
        P: Fn(ProgressKind, usize, usize, Option<String>) -> R + Send + Sync + 'static,
        R: Future<Output = ()>,
        V: Fn() -> Validator + Send + Sync + 'static,
        C: Fn(&Url) -> Option<DocumentChange> + Send + Sync + 'static,
    {
        let (tx, rx) = mpsc::unbounded_channel();
        let tokio = Handle::current();
        let handle = std::thread::spawn(move || {
            let queue = AnalysisQueue::new(tokio, progress, validator, changes);
            queue.run(rx);
        });

        Self {
            sender: ManuallyDrop::new(tx),
            handle: Some(handle),
        }
    }

    /// Finds documents to analyze.
    ///
    /// If a specified path is a directory, it is recursively searched for WDL
    /// documents.
    ///
    /// Returns the URIs of the discovered documents.
    pub async fn find_documents(paths: Vec<PathBuf>) -> Vec<Arc<Url>> {
        RayonHandle::spawn(move || {
            let mut documents = Vec::new();
            for path in paths {
                if path.is_file() {
                    if let Some(uri) = path_to_uri(&path) {
                        documents.push(uri.into());
                    }

                    continue;
                }

                let pattern = format!("{path}/**/*.wdl", path = path.display());
                let options = glob::MatchOptions {
                    case_sensitive: true,
                    require_literal_separator: false,
                    require_literal_leading_dot: true,
                };

                match glob::glob_with(&pattern, options) {
                    Ok(paths) => {
                        for uri in paths.filter_map(|p| match p {
                            Ok(path) => path_to_uri(&path),
                            Err(e) => {
                                log::error!("error while searching for WDL documents: {e}");
                                None
                            }
                        }) {
                            documents.push(uri.into());
                        }
                    }
                    Err(e) => {
                        log::error!("error while searching for WDL documents: {e}");
                    }
                }
            }

            documents
        })
        .await
    }

    /// Removes the specified documents from the analyzer.
    ///
    /// If a specified URI is a prefix (i.e. directory) of documents known to
    /// the analyzer, those documents will be removed.
    ///
    /// Documents are only removed when not referenced from importing documents.
    pub async fn remove_documents<'a>(&self, uris: Vec<Url>) {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::Remove(RemoveRequest {
                uris,
                completed: tx,
            }))
            .expect("failed to send remove request");

        rx.await.unwrap_or_default()
    }

    /// Analyzes the given set of documents.
    ///
    /// The provided context is passed to the progress callback.
    ///
    /// Returns a set of analysis results for each file that was analyzed; note
    /// that the set may contain related files that were analyzed.
    pub async fn analyze(
        &self,
        documents: Vec<Arc<Url>>,
        context: Option<String>,
    ) -> Vec<AnalysisResult> {
        if documents.is_empty() {
            return Default::default();
        }

        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::Analyze(AnalyzeRequest {
                documents,
                context,
                completed: tx,
            }))
            .expect("failed to send analyze request");

        rx.await.unwrap_or_default()
    }
}

impl Drop for Analyzer {
    fn drop(&mut self) {
        unsafe { ManuallyDrop::drop(&mut self.sender) };
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap();
        }
    }
}

/// Constant that asserts `Analyzer` is `Send + Sync`; if not, it fails to
/// compile.
const _: () = {
    /// Helper that will fail to compile if T is not `Send + Sync`.
    const fn _assert<T: Send + Sync>() {}
    _assert::<Analyzer>();
};
