//! Implementation of the analyzer.

use std::ffi::OsStr;
use std::fmt;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::ops::Range;
use std::path::Path;
use std::path::PathBuf;
use std::path::absolute;
use std::sync::Arc;
use std::thread::JoinHandle;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use indexmap::IndexSet;
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
use walkdir::WalkDir;
use wdl_ast::AstNode;
use wdl_ast::Diagnostic;
use wdl_ast::SyntaxNode;
use wdl_ast::Validator;

use crate::graph::DocumentGraphNode;
use crate::graph::ParseState;
use crate::queue::AddRequest;
use crate::queue::AnalysisQueue;
use crate::queue::AnalyzeRequest;
use crate::queue::NotifyChangeRequest;
use crate::queue::NotifyIncrementalChangeRequest;
use crate::queue::RemoveRequest;
use crate::queue::Request;
use crate::rayon::RayonHandle;
use crate::scope::DocumentScope;

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
        /// The monotonic version of the document that was parsed.
        ///
        /// This value comes from incremental changes to the file.
        ///
        /// If `None`, the parsed version had no incremental changes.
        version: Option<i32>,
        /// The root node of the document.
        root: GreenNode,
        /// The line index used to map line/column offsets to byte offsets and
        /// vice versa.
        lines: Arc<LineIndex>,
    },
}

impl ParseResult {
    /// Gets the version of the parsed document.
    ///
    /// Returns `None` if there was an error parsing the document or the parsed
    /// document had no incremental changes.
    pub fn version(&self) -> Option<i32> {
        match self {
            Self::Error(_) => None,
            Self::Parsed { version, .. } => *version,
        }
    }

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
            ParseState::Parsed {
                version,
                root,
                lines,
                diagnostics: _,
            } => Self::Parsed {
                version: *version,
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
    /// The analysis result id.
    ///
    /// The identifier changes every time the document is analyzed.
    id: Arc<String>,
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
            id: analysis.id().clone(),
            uri: node.uri().clone(),
            parse_result: node.parse_state().into(),
            diagnostics: analysis.diagnostics().clone(),
            scope: analysis.scope().clone(),
        }
    }

    /// Gets the identifier of the analysis result.
    ///
    /// This value changes when a document is reanalyzed.
    pub fn id(&self) -> &Arc<String> {
        &self.id
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
    ///
    /// Note that invalid ranges will cause the edit to be ignored.
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

    /// Applies the edit to the given string if it's in range.
    pub(crate) fn apply(&self, source: &mut String, lines: &LineIndex) -> Result<()> {
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
                    .to_utf8(WideEncoding::Utf16, WideLineCol {
                        line: self.range.start.line,
                        col: self.range.start.character,
                    })
                    .context("invalid edit start position")?,
                lines
                    .to_utf8(WideEncoding::Utf16, WideLineCol {
                        line: self.range.end.line,
                        col: self.range.end.character,
                    })
                    .context("invalid edit end position")?,
            ),
        };

        let range: Range<usize> = lines
            .offset(start)
            .context("invalid edit start position")?
            .into()
            ..lines
                .offset(end)
                .context("invalid edit end position")?
                .into();

        if !source.is_char_boundary(range.start) {
            bail!("edit start position is not at a character boundary");
        }

        if !source.is_char_boundary(range.end) {
            bail!("edit end position is not at a character boundary");
        }

        source.replace_range(range, &self.text);
        Ok(())
    }
}

/// Represents an incremental change to a document.
#[derive(Clone, Debug)]
pub struct IncrementalChange {
    /// The monotonic version of the document.
    ///
    /// This is expected to increase for each incremental change.
    pub version: i32,
    /// The source to start from for applying edits.
    ///
    /// If this is `Some`, a full reparse will occur after applying edits to
    /// this string.
    ///
    /// If this is `None`, edits will be applied to the existing CST and an
    /// attempt will be made to incrementally parse the file.
    pub start: Option<String>,
    /// The source edits to apply.
    pub edits: Vec<SourceEdit>,
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
///
/// The type parameter is the context type passed to the progress callback.
#[derive(Debug)]
pub struct Analyzer<Context> {
    /// The sender for sending analysis requests to the queue.
    sender: ManuallyDrop<mpsc::UnboundedSender<Request<Context>>>,
    /// The join handle for the queue task.
    handle: Option<JoinHandle<()>>,
}

impl<Context> Analyzer<Context>
where
    Context: Send + Clone + 'static,
{
    /// Constructs a new analyzer.
    ///
    /// The provided progress callback will be invoked during analysis.
    ///
    /// The analyzer will use a default validator for validation.
    ///
    /// The analyzer must be constructed from the context of a Tokio runtime.
    pub fn new<Progress, Return>(progress: Progress) -> Self
    where
        Progress: Fn(Context, ProgressKind, usize, usize) -> Return + Send + 'static,
        Return: Future<Output = ()>,
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
    pub fn new_with_validator<Progress, Return, Validator>(
        progress: Progress,
        validator: Validator,
    ) -> Self
    where
        Progress: Fn(Context, ProgressKind, usize, usize) -> Return + Send + 'static,
        Return: Future<Output = ()>,
        Validator: Fn() -> wdl_ast::Validator + Send + Sync + 'static,
    {
        let (tx, rx) = mpsc::unbounded_channel();
        let tokio = Handle::current();
        let handle = std::thread::spawn(move || {
            let queue = AnalysisQueue::new(tokio, progress, validator);
            queue.run(rx);
        });

        Self {
            sender: ManuallyDrop::new(tx),
            handle: Some(handle),
        }
    }

    /// Adds documents to the analyzer.
    ///
    /// If a specified path is a directory, it is recursively searched for WDL
    /// documents.
    ///
    /// Returns an error if there was a problem discovering documents for the
    /// specified paths.
    pub async fn add_documents(&self, paths: Vec<PathBuf>) -> Result<()> {
        // Start by searching for documents
        let documents = RayonHandle::spawn(move || -> Result<IndexSet<Url>> {
            let mut documents = IndexSet::new();
            for path in paths {
                let metadata = path.metadata().with_context(|| {
                    format!(
                        "failed to read metadata for `{path}`",
                        path = path.display()
                    )
                })?;

                if metadata.is_file() {
                    documents.insert(path_to_uri(&path).with_context(|| {
                        format!(
                            "failed to convert path `{path}` to a URI",
                            path = path.display()
                        )
                    })?);
                    continue;
                }

                for result in WalkDir::new(&path).follow_links(true) {
                    let entry = result.with_context(|| {
                        format!("failed to read directory `{path}`", path = path.display())
                    })?;
                    if !entry.file_type().is_file()
                        || entry.path().extension().and_then(OsStr::to_str) != Some("wdl")
                    {
                        continue;
                    }

                    documents.insert(path_to_uri(entry.path()).with_context(|| {
                        format!(
                            "failed to convert path `{path}` to a URI",
                            path = entry.path().display()
                        )
                    })?);
                }
            }

            Ok(documents)
        })
        .await?;

        if documents.is_empty() {
            return Ok(());
        }

        // Send the add request to the queue
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::Add(AddRequest {
                documents,
                completed: tx,
            }))
            .map_err(|_| {
                anyhow!("failed to send request to analysis queue because the channel has closed")
            })?;

        rx.await.map_err(|_| {
            anyhow!("failed to receive response from analysis queue because the channel has closed")
        })?;

        Ok(())
    }

    /// Removes the specified documents from the analyzer.
    ///
    /// If a specified URI is a prefix (i.e. directory) of documents known to
    /// the analyzer, those documents will be removed.
    ///
    /// Documents are only removed when not referenced from importing documents.
    pub async fn remove_documents<'a>(&self, documents: Vec<Url>) -> Result<()> {
        // Send the remove request to the queue
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::Remove(RemoveRequest {
                documents,
                completed: tx,
            }))
            .map_err(|_| {
                anyhow!("failed to send request to analysis queue because the channel has closed")
            })?;

        rx.await.map_err(|_| {
            anyhow!("failed to receive response from analysis queue because the channel has closed")
        })?;

        Ok(())
    }

    /// Notifies the analyzer that a document has an incremental change.
    ///
    /// Changes to documents that aren't known to the analyzer are ignored.
    pub fn notify_incremental_change(
        &self,
        document: Url,
        change: IncrementalChange,
    ) -> Result<()> {
        self.sender
            .send(Request::NotifyIncrementalChange(
                NotifyIncrementalChangeRequest { document, change },
            ))
            .map_err(|_| {
                anyhow!("failed to send request to analysis queue because the channel has closed")
            })
    }

    /// Notifies the analyzer that a document has fully changed and should be
    /// fetched again.
    ///
    /// Changes to documents that aren't known to the analyzer are ignored.
    ///
    /// If `discard_pending` is true, then any pending incremental changes are
    /// discarded; otherwise, the full change is ignored if there are pending
    /// incremental changes.
    pub fn notify_change(&self, document: Url, discard_pending: bool) -> Result<()> {
        self.sender
            .send(Request::NotifyChange(NotifyChangeRequest {
                document,
                discard_pending,
            }))
            .map_err(|_| {
                anyhow!("failed to send request to analysis queue because the channel has closed")
            })
    }

    /// Analyzes a specific document.
    ///
    /// The provided context is passed to the progress callback.
    ///
    /// If the document is up-to-date and was previously analyzed, the current
    /// analysis result is returned.
    ///
    /// Returns an analysis result for each document that was analyzed.
    pub async fn analyze_document(
        &self,
        context: Context,
        document: Url,
    ) -> Result<Vec<AnalysisResult>> {
        // Send the analyze request to the queue
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::Analyze(AnalyzeRequest {
                document: Some(document),
                context,
                completed: tx,
            }))
            .map_err(|_| {
                anyhow!("failed to send request to analysis queue because the channel has closed")
            })?;

        rx.await.map_err(|_| {
            anyhow!("failed to receive response from analysis queue because the channel has closed")
        })?
    }

    /// Performs analysis of all documents.
    ///
    /// The provided context is passed to the progress callback.
    ///
    /// If a document is up-to-date and was previously analyzed, the current
    /// analysis result is returned.
    ///
    /// Returns an analysis result for each document that was analyzed.
    pub async fn analyze(&self, context: Context) -> Result<Vec<AnalysisResult>> {
        // Send the analyze request to the queue
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::Analyze(AnalyzeRequest {
                document: None,
                context,
                completed: tx,
            }))
            .map_err(|_| {
                anyhow!("failed to send request to analysis queue because the channel has closed")
            })?;

        rx.await.map_err(|_| {
            anyhow!("failed to receive response from analysis queue because the channel has closed")
        })?
    }
}

impl<C> Drop for Analyzer<C> {
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
    _assert::<Analyzer<()>>();
};

#[cfg(test)]
mod test {
    use std::fs;

    use tempfile::TempDir;
    use wdl_ast::Severity;

    use super::*;

    #[tokio::test]
    async fn it_returns_empty_results() {
        let analyzer = Analyzer::new(|_: (), _, _, _| async {});
        let results = analyzer.analyze(()).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn it_analyzes_a_document() {
        let dir = TempDir::new().expect("failed to create temporary directory");
        let path = dir.path().join("foo.wdl");
        fs::write(
            &path,
            r#"version 1.1

task test {
    command <<<>>>
}

workflow test {
}
"#,
        )
        .expect("failed to create test file");

        // Analyze the file and check the resulting diagnostic
        let analyzer = Analyzer::new(|_: (), _, _, _| async {});
        analyzer
            .add_documents(vec![path])
            .await
            .expect("should add document");

        let results = analyzer.analyze(()).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].diagnostics().len(), 1);
        assert_eq!(results[0].diagnostics()[0].rule(), None);
        assert_eq!(results[0].diagnostics()[0].severity(), Severity::Error);
        assert_eq!(
            results[0].diagnostics()[0].message(),
            "conflicting workflow name `test`"
        );

        // Analyze again and ensure the analysis result id is unchanged
        let id = results[0].id().clone();
        let results = analyzer.analyze(()).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id().as_ref(), id.as_ref());
        assert_eq!(results[0].diagnostics().len(), 1);
        assert_eq!(results[0].diagnostics()[0].rule(), None);
        assert_eq!(results[0].diagnostics()[0].severity(), Severity::Error);
        assert_eq!(
            results[0].diagnostics()[0].message(),
            "conflicting workflow name `test`"
        );
    }

    #[tokio::test]
    async fn it_reanalyzes_a_document_on_change() {
        let dir = TempDir::new().expect("failed to create temporary directory");
        let path = dir.path().join("foo.wdl");
        fs::write(
            &path,
            r#"version 1.1

task test {
    command <<<>>>
}

workflow test {
}
"#,
        )
        .expect("failed to create test file");

        // Analyze the file and check the resulting diagnostic
        let analyzer = Analyzer::new(|_: (), _, _, _| async {});
        analyzer
            .add_documents(vec![path.clone()])
            .await
            .expect("should add document");

        let results = analyzer.analyze(()).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].diagnostics().len(), 1);
        assert_eq!(results[0].diagnostics()[0].rule(), None);
        assert_eq!(results[0].diagnostics()[0].severity(), Severity::Error);
        assert_eq!(
            results[0].diagnostics()[0].message(),
            "conflicting workflow name `test`"
        );

        // Rewrite the file to correct the issue
        fs::write(
            &path,
            r#"version 1.1

task test {
    command <<<>>>
}

workflow something_else {
}
"#,
        )
        .expect("failed to create test file");

        let uri = path_to_uri(&path).expect("should convert to URI");
        analyzer.notify_change(uri.clone(), false).unwrap();

        // Analyze again and ensure the analysis result id is changed and the issue
        // fixed
        let id = results[0].id().clone();
        let results = analyzer.analyze(()).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].id().as_ref() != id.as_ref());
        assert_eq!(results[0].diagnostics().len(), 0);

        // Analyze again and ensure the analysis result id is unchanged
        let id = results[0].id().clone();
        let results = analyzer.analyze_document((), uri).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].id().as_ref() == id.as_ref());
        assert_eq!(results[0].diagnostics().len(), 0);
    }

    #[tokio::test]
    async fn it_reanalyzes_a_document_on_incremental_change() {
        let dir = TempDir::new().expect("failed to create temporary directory");
        let path = dir.path().join("foo.wdl");
        fs::write(
            &path,
            r#"version 1.1

task test {
    command <<<>>>
}

workflow test {
}
"#,
        )
        .expect("failed to create test file");

        // Analyze the file and check the resulting diagnostic
        let analyzer = Analyzer::new(|_: (), _, _, _| async {});
        analyzer
            .add_documents(vec![path.clone()])
            .await
            .expect("should add document");

        let results = analyzer.analyze(()).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].diagnostics().len(), 1);
        assert_eq!(results[0].diagnostics()[0].rule(), None);
        assert_eq!(results[0].diagnostics()[0].severity(), Severity::Error);
        assert_eq!(
            results[0].diagnostics()[0].message(),
            "conflicting workflow name `test`"
        );

        // Edit the file to correct the issue
        let uri = path_to_uri(&path).expect("should convert to URI");
        analyzer
            .notify_incremental_change(uri.clone(), IncrementalChange {
                version: 2,
                start: None,
                edits: vec![SourceEdit {
                    range: SourcePosition::new(6, 9)..SourcePosition::new(6, 13),
                    encoding: SourcePositionEncoding::UTF8,
                    text: "something_else".to_string(),
                }],
            })
            .unwrap();

        // Analyze again and ensure the analysis result id is changed and the issue was
        // fixed
        let id = results[0].id().clone();
        let results = analyzer.analyze_document((), uri).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].id().as_ref() != id.as_ref());
        assert_eq!(results[0].diagnostics().len(), 0);
    }

    #[tokio::test]
    async fn it_removes_documents() {
        let dir = TempDir::new().expect("failed to create temporary directory");
        let foo = dir.path().join("foo.wdl");
        fs::write(
            &foo,
            r#"version 1.1
workflow test {
}
"#,
        )
        .expect("failed to create test file");

        let bar = dir.path().join("bar.wdl");
        fs::write(
            &bar,
            r#"version 1.1
workflow test {
}
"#,
        )
        .expect("failed to create test file");

        let baz = dir.path().join("baz.wdl");
        fs::write(
            &baz,
            r#"version 1.1
workflow test {
}
"#,
        )
        .expect("failed to create test file");

        // Add all three documents to the analyzer
        let analyzer = Analyzer::new(|_: (), _, _, _| async {});
        analyzer
            .add_documents(vec![dir.path().to_path_buf()])
            .await
            .expect("should add documents");

        // Analyze the documents
        let results = analyzer.analyze(()).await.unwrap();
        assert_eq!(results.len(), 3);
        assert!(results[0].diagnostics().is_empty());
        assert!(results[1].diagnostics().is_empty());
        assert!(results[2].diagnostics().is_empty());

        // Analyze the documents again
        let results = analyzer.analyze(()).await.unwrap();
        assert_eq!(results.len(), 3);

        // Remove the documents by directory
        analyzer
            .remove_documents(vec![
                path_to_uri(dir.path()).expect("should convert to URI"),
            ])
            .await
            .unwrap();
        let results = analyzer.analyze(()).await.unwrap();
        assert!(results.is_empty());
    }
}
