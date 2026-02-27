//! Implementation of the analyzer.

use std::ffi::OsStr;
use std::fmt;
use std::future::Future;
use std::mem::ManuallyDrop;
use std::ops::Range;
use std::path::Path;
use std::path::absolute;
use std::sync::Arc;
use std::thread::JoinHandle;

use anyhow::Context;
use anyhow::Error;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use ignore::WalkBuilder;
use indexmap::IndexSet;
use line_index::LineCol;
use line_index::LineIndex;
use line_index::WideEncoding;
use line_index::WideLineCol;
use lsp_types::CompletionResponse;
use lsp_types::DocumentSymbolResponse;
use lsp_types::GotoDefinitionResponse;
use lsp_types::Hover;
use lsp_types::InlayHint;
use lsp_types::Location;
use lsp_types::SemanticTokensResult;
use lsp_types::SignatureHelp;
use lsp_types::SymbolInformation;
use lsp_types::WorkspaceEdit;
use path_clean::PathClean;
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use url::Url;

use crate::config::Config;
use crate::document::Document;
use crate::graph::DocumentGraphNode;
use crate::graph::ParseState;
use crate::queue::AddRequest;
use crate::queue::AnalysisQueue;
use crate::queue::AnalyzeRequest;
use crate::queue::CompletionRequest;
use crate::queue::DocumentSymbolRequest;
use crate::queue::FindAllReferencesRequest;
use crate::queue::FormatRequest;
use crate::queue::GotoDefinitionRequest;
use crate::queue::HoverRequest;
use crate::queue::InlayHintsRequest;
use crate::queue::NotifyChangeRequest;
use crate::queue::NotifyIncrementalChangeRequest;
use crate::queue::RemoveRequest;
use crate::queue::RenameRequest;
use crate::queue::Request;
use crate::queue::SemanticTokenRequest;
use crate::queue::SignatureHelpRequest;
use crate::queue::WorkspaceSymbolRequest;
use crate::rayon::RayonHandle;

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

/// Converts a local file path to a file schemed URI.
pub fn path_to_uri(path: impl AsRef<Path>) -> Option<Url> {
    Url::from_file_path(absolute(path).ok()?.clean()).ok()
}

/// Represents the result of an analysis.
///
/// Analysis results are cheap to clone.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    /// The error that occurred when attempting to parse the file (e.g. the file
    /// could not be opened).
    error: Option<Arc<Error>>,
    /// The monotonic version of the document that was parsed.
    ///
    /// This value comes from incremental changes to the file.
    ///
    /// If `None`, the parsed version had no incremental changes.
    version: Option<i32>,
    /// The lines indexed for the parsed file.
    lines: Option<Arc<LineIndex>>,
    /// The analyzed document.
    document: Document,
}

impl AnalysisResult {
    /// Constructs a new analysis result for the given graph node.
    pub(crate) fn new(node: &DocumentGraphNode) -> Self {
        if let Some(error) = node.analysis_error() {
            return Self {
                error: Some(error.clone()),
                version: node.parse_state().version(),
                lines: node.parse_state().lines().cloned(),
                document: Document::default_from_uri(node.uri().clone()),
            };
        }

        let (error, version, lines) = match node.parse_state() {
            ParseState::NotParsed => unreachable!("document should have been parsed"),
            ParseState::Error(e) => (Some(e), None, None),
            ParseState::Parsed { version, lines, .. } => (None, *version, Some(lines)),
        };

        Self {
            error: error.cloned(),
            version,
            lines: lines.cloned(),
            document: node
                .document()
                .expect("analysis should have completed")
                .clone(),
        }
    }

    /// Gets the error that occurred when attempting to parse the document.
    ///
    /// An example error would be if the file could not be opened.
    ///
    /// Returns `None` if the document was parsed successfully.
    pub fn error(&self) -> Option<&Arc<Error>> {
        self.error.as_ref()
    }

    /// Gets the incremental version of the parsed document.
    ///
    /// Returns `None` if there was an error parsing the document or if the
    /// parsed document had no incremental changes.
    pub fn version(&self) -> Option<i32> {
        self.version
    }

    /// Gets the line index of the parsed document.
    ///
    /// Returns `None` if there was an error parsing the document.
    pub fn lines(&self) -> Option<&Arc<LineIndex>> {
        self.lines.as_ref()
    }

    /// Gets the analyzed document.
    pub fn document(&self) -> &Document {
        &self.document
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
                    .to_utf8(
                        WideEncoding::Utf16,
                        WideLineCol {
                            line: self.range.start.line,
                            col: self.range.start.character,
                        },
                    )
                    .context("invalid edit start position")?,
                lines
                    .to_utf8(
                        WideEncoding::Utf16,
                        WideLineCol {
                            line: self.range.end.line,
                            col: self.range.end.character,
                        },
                    )
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
    /// The config to use during analysis.
    config: Config,
}

impl<Context> Analyzer<Context>
where
    Context: Send + Clone + 'static,
{
    /// Constructs a new analyzer with the given config.
    ///
    /// The provided progress callback will be invoked during analysis.
    ///
    /// The analyzer will use a default validator for validation.
    ///
    /// The analyzer must be constructed from the context of a Tokio runtime.
    pub fn new<Progress, Return>(config: Config, progress: Progress) -> Self
    where
        Progress: Fn(Context, ProgressKind, usize, usize) -> Return + Send + 'static,
        Return: Future<Output = ()>,
    {
        Self::new_with_validator(config, progress, crate::Validator::default)
    }

    /// Constructs a new analyzer with the given config and validator function.
    ///
    /// The provided progress callback will be invoked during analysis.
    ///
    /// This validator function will be called once per worker thread to
    /// initialize a thread-local validator.
    ///
    /// The analyzer must be constructed from the context of a Tokio runtime.
    pub fn new_with_validator<Progress, Return, Validator>(
        config: Config,
        progress: Progress,
        validator: Validator,
    ) -> Self
    where
        Progress: Fn(Context, ProgressKind, usize, usize) -> Return + Send + 'static,
        Return: Future<Output = ()>,
        Validator: Fn() -> crate::Validator + Send + Sync + 'static,
    {
        let (tx, rx) = mpsc::unbounded_channel();
        let tokio = Handle::current();
        let inner_config = config.clone();
        let handle = std::thread::spawn(move || {
            let queue = AnalysisQueue::new(inner_config, tokio, progress, validator);
            queue.run(rx);
        });

        Self {
            sender: ManuallyDrop::new(tx),
            handle: Some(handle),
            config,
        }
    }

    /// Adds a document to the analyzer. Document can be a local file or a URL.
    ///
    /// Returns an error if the document could not be added.
    pub async fn add_document(&self, uri: Url) -> Result<()> {
        let mut documents = IndexSet::new();
        documents.insert(uri);

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

    /// Adds a directory to the analyzer. It will recursively search for WDL
    /// documents in the supplied directory.
    ///
    /// Returns an error if there was a problem discovering documents for the
    /// specified path.
    pub async fn add_directory(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref().to_path_buf();
        let config = self.config.clone();
        // Start by searching for documents
        let documents = RayonHandle::spawn(move || -> Result<IndexSet<Url>> {
            let mut documents = IndexSet::new();

            let metadata = path.metadata().with_context(|| {
                format!(
                    "failed to read metadata for `{path}`",
                    path = path.display()
                )
            })?;

            if metadata.is_file() {
                bail!("`{path}` is a file, not a directory", path = path.display());
            }

            let mut walker = WalkBuilder::new(&path);
            if let Some(ignore_filename) = config.ignore_filename() {
                walker.add_custom_ignore_filename(ignore_filename);
            }
            let walker = walker
                .standard_filters(false)
                .parents(true)
                .follow_links(true)
                .git_ignore(config.respect_gitignore())
                .build();

            for result in walker {
                let entry = result.with_context(|| {
                    format!("failed to read directory `{path}`", path = path.display())
                })?;

                // Skip entries without a file type
                let Some(file_type) = entry.file_type() else {
                    continue;
                };
                // Skip non-files
                if !file_type.is_file() {
                    continue;
                }
                // Skip files without a `.wdl` extension
                if entry.path().extension() != Some(OsStr::new("wdl")) {
                    continue;
                }

                documents.insert(path_to_uri(entry.path()).with_context(|| {
                    format!(
                        "failed to convert path `{path}` to a URI",
                        path = entry.path().display()
                    )
                })?);
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
    pub async fn remove_documents(&self, documents: Vec<Url>) -> Result<()> {
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
                document: None, // analyze all documents
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

    /// Formats a document.
    pub async fn format_document(&self, document: Url) -> Result<Option<(u32, u32, String)>> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::Format(FormatRequest {
                document,
                completed: tx,
            }))
            .map_err(|_| {
                anyhow!("failed to send format request to the queue because the channel has closed")
            })?;

        rx.await.map_err(|_| {
            anyhow!("failed to send format request to the queue because the channel has closed")
        })
    }

    /// Performs a "goto definition" for a symbol at the current position.
    pub async fn goto_definition(
        &self,
        document: Url,
        position: SourcePosition,
        encoding: SourcePositionEncoding,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::GotoDefinition(GotoDefinitionRequest {
                document,
                position,
                encoding,
                completed: tx,
            }))
            .map_err(|_| {
                anyhow!(
                    "failed to send goto definition request to analysis queue because the channel \
                     has closed"
                )
            })?;

        rx.await.map_err(|_| {
            anyhow!(
                "failed to receive goto definition response from analysis queue because the \
                 channel has closed"
            )
        })
    }

    /// Performs a `find references` for a symbol across all the documents.
    pub async fn find_all_references(
        &self,
        document: Url,
        position: SourcePosition,
        encoding: SourcePositionEncoding,
        include_declaration: bool,
    ) -> Result<Vec<Location>> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::FindAllReferences(FindAllReferencesRequest {
                document,
                position,
                encoding,
                include_declaration,
                completed: tx,
            }))
            .map_err(|_| {
                anyhow!(
                    "failed to send find all references request to analysis queue because the \
                     channel has closed"
                )
            })?;

        rx.await.map_err(|_| {
            anyhow!(
                "failed to receive find all references response from analysis queue because the \
                 client channel has closed"
            )
        })
    }

    /// Performs a `auto-completion` for a symbol.
    pub async fn completion(
        &self,
        context: Context,
        document: Url,
        position: SourcePosition,
        encoding: SourcePositionEncoding,
    ) -> Result<Option<CompletionResponse>> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::Completion(CompletionRequest {
                document,
                position,
                encoding,
                context,
                completed: tx,
            }))
            .map_err(|_| {
                anyhow!(
                    "failed to send completion request to analysis queue because the channel has \
                     closed"
                )
            })?;

        rx.await.map_err(|_| {
            anyhow!(
                "failed to send completion request to analysis queue because the channel has \
                 closed"
            )
        })
    }

    /// Performs a `hover` for a symbol at a given position in a document.
    pub async fn hover(
        &self,
        document: Url,
        position: SourcePosition,
        encoding: SourcePositionEncoding,
    ) -> Result<Option<Hover>> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::Hover(HoverRequest {
                document,
                position,
                encoding,
                completed: tx,
            }))
            .map_err(|_| {
                anyhow!(
                    "failed to send hover request to analysis queue because the channel has closed"
                )
            })?;

        rx.await.map_err(|_| {
            anyhow!("failed to send hover request to analysis queue because the channel has closed")
        })
    }

    /// Renames a symbol at a given position across the workspace.
    pub async fn rename(
        &self,
        document: Url,
        position: SourcePosition,
        encoding: SourcePositionEncoding,
        new_name: String,
    ) -> Result<Option<WorkspaceEdit>> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::Rename(RenameRequest {
                document,
                position,
                encoding,
                new_name,
                completed: tx,
            }))
            .map_err(|_| {
                anyhow!(
                    "failed to send rename request to analysis queue because the channel has \
                     closed"
                )
            })?;

        rx.await.map_err(|_| {
            anyhow!(
                "failed to receive rename response from analysis queue because the channel has \
                 closed"
            )
        })
    }

    /// Gets semantic tokens for a document
    pub async fn semantic_tokens(&self, document: Url) -> Result<Option<SemanticTokensResult>> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::SemanticTokens(SemanticTokenRequest {
                document,
                completed: tx,
            }))
            .map_err(|_| {
                anyhow!(
                    "failed to send semantic tokens request to analysis queue because the channel \
                     has closed"
                )
            })?;

        rx.await.map_err(|_| {
            anyhow!(
                "failed to receive semantic tokens response from analysis queue because the \
                 channel has closed"
            )
        })
    }

    /// Gets document symbols for a document.
    pub async fn document_symbol(&self, document: Url) -> Result<Option<DocumentSymbolResponse>> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::DocumentSymbol(DocumentSymbolRequest {
                document,
                completed: tx,
            }))
            .map_err(|_| {
                anyhow!(
                    "failed to send document symbol request to analysis queue because the channel \
                     has closed"
                )
            })?;

        rx.await.map_err(|_| {
            anyhow!(
                "failed to receive document symbol request to analysis queue because the channel \
                 has closed"
            )
        })
    }

    /// Gets document symbols for the workspace.
    pub async fn workspace_symbol(&self, query: String) -> Result<Option<Vec<SymbolInformation>>> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::WorkspaceSymbol(WorkspaceSymbolRequest {
                query,
                completed: tx,
            }))
            .map_err(|_| {
                anyhow!(
                    "failed to send workspace symbol request to analysis queue because the \
                     channel has closed"
                )
            })?;

        rx.await.map_err(|_| {
            anyhow!(
                "failed to receive workspace symbol response from analysis queue because the \
                 channel has closed"
            )
        })
    }

    /// Gets signature help for a function call at a given position.
    pub async fn signature_help(
        &self,
        document: Url,
        position: SourcePosition,
        encoding: SourcePositionEncoding,
    ) -> Result<Option<SignatureHelp>> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::SignatureHelp(SignatureHelpRequest {
                document,
                position,
                encoding,
                completed: tx,
            }))
            .map_err(|_| {
                anyhow!(
                    "failed to send signature help request to analysis queue because the channel \
                     has closed"
                )
            })?;

        rx.await.map_err(|_| {
            anyhow!(
                "failed to receive signature help response from analysis queue because the \
                 channel has closed"
            )
        })
    }

    /// Requests inlay hints for a document.
    pub async fn inlay_hints(
        &self,
        document: Url,
        range: lsp_types::Range,
    ) -> Result<Option<Vec<InlayHint>>> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Request::InlayHints(InlayHintsRequest {
                document,
                range,
                completed: tx,
            }))
            .map_err(|_| {
                anyhow!(
                    "failed to send inlay hints request to analysis queue because the channel has \
                     closed"
                )
            })?;

        rx.await.map_err(|_| {
            anyhow!(
                "failed to receive inlay hints response from analysis queue because the channel \
                 has closed"
            )
        })
    }
}

impl Default for Analyzer<()> {
    fn default() -> Self {
        Self::new(Default::default(), |_, _, _, _| async {})
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
        let analyzer = Analyzer::default();
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
        let analyzer = Analyzer::default();
        analyzer
            .add_document(path_to_uri(&path).expect("should convert to URI"))
            .await
            .expect("should add document");

        let results = analyzer.analyze(()).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document.diagnostics().count(), 1);
        assert_eq!(
            results[0].document.diagnostics().next().unwrap().rule(),
            None
        );
        assert_eq!(
            results[0].document.diagnostics().next().unwrap().severity(),
            Severity::Error
        );
        assert_eq!(
            results[0].document.diagnostics().next().unwrap().message(),
            "conflicting workflow name `test`"
        );

        // Analyze again and ensure the analysis result id is unchanged
        let id = results[0].document.id().clone();
        let results = analyzer.analyze(()).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document.id().as_ref(), id.as_ref());
        assert_eq!(results[0].document.diagnostics().count(), 1);
        assert_eq!(
            results[0].document.diagnostics().next().unwrap().rule(),
            None
        );
        assert_eq!(
            results[0].document.diagnostics().next().unwrap().severity(),
            Severity::Error
        );
        assert_eq!(
            results[0].document.diagnostics().next().unwrap().message(),
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
        let analyzer = Analyzer::default();
        analyzer
            .add_document(path_to_uri(&path).expect("should convert to URI"))
            .await
            .expect("should add document");

        let results = analyzer.analyze(()).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document.diagnostics().count(), 1);
        assert_eq!(
            results[0].document.diagnostics().next().unwrap().rule(),
            None
        );
        assert_eq!(
            results[0].document.diagnostics().next().unwrap().severity(),
            Severity::Error
        );
        assert_eq!(
            results[0].document.diagnostics().next().unwrap().message(),
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
        let id = results[0].document.id().clone();
        let results = analyzer.analyze(()).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].document.id().as_ref() != id.as_ref());
        assert_eq!(results[0].document.diagnostics().count(), 0);

        // Analyze again and ensure the analysis result id is unchanged
        let id = results[0].document.id().clone();
        let results = analyzer.analyze_document((), uri).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].document.id().as_ref() == id.as_ref());
        assert_eq!(results[0].document.diagnostics().count(), 0);
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
        let analyzer = Analyzer::default();
        analyzer
            .add_document(path_to_uri(&path).expect("should convert to URI"))
            .await
            .expect("should add document");

        let results = analyzer.analyze(()).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document.diagnostics().count(), 1);
        assert_eq!(
            results[0].document.diagnostics().next().unwrap().rule(),
            None
        );
        assert_eq!(
            results[0].document.diagnostics().next().unwrap().severity(),
            Severity::Error
        );
        assert_eq!(
            results[0].document.diagnostics().next().unwrap().message(),
            "conflicting workflow name `test`"
        );

        // Edit the file to correct the issue
        let uri = path_to_uri(&path).expect("should convert to URI");
        analyzer
            .notify_incremental_change(
                uri.clone(),
                IncrementalChange {
                    version: 2,
                    start: None,
                    edits: vec![SourceEdit {
                        range: SourcePosition::new(6, 9)..SourcePosition::new(6, 13),
                        encoding: SourcePositionEncoding::UTF8,
                        text: "something_else".to_string(),
                    }],
                },
            )
            .unwrap();

        // Analyze again and ensure the analysis result id is changed and the issue was
        // fixed
        let id = results[0].document.id().clone();
        let results = analyzer.analyze_document((), uri).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].document.id().as_ref() != id.as_ref());
        assert_eq!(results[0].document.diagnostics().count(), 0);
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
        let analyzer = Analyzer::default();
        analyzer
            .add_directory(dir.path())
            .await
            .expect("should add documents");

        // Analyze the documents
        let results = analyzer.analyze(()).await.unwrap();
        assert_eq!(results.len(), 3);
        assert!(results[0].document.diagnostics().next().is_none());
        assert!(results[1].document.diagnostics().next().is_none());
        assert!(results[2].document.diagnostics().next().is_none());

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

    #[tokio::test]
    async fn it_respects_gitignore() {
        let dir = TempDir::new().expect("Failed to create temporary directory");
        std::process::Command::new("git")
            .arg("init")
            .arg(dir.path())
            .output()
            .expect("Failed to initialize git repository");
        fs::write(dir.path().join(".gitignore"), "ignored.wdl\n")
            .expect("failed to create .gitignore");
        fs::write(dir.path().join("ignored.wdl"), "version 1.1\n")
            .expect("Failed to create ignored.wdl");
        fs::write(dir.path().join("included.wdl"), "version 1.1\n")
            .expect("failed to create included.wdl");
        // Analyze respect_gitignore = true
        let config = Config::default().with_respect_gitignore(true);
        let analyzer = Analyzer::new(config, |_, _, _, _| async {});
        analyzer
            .add_directory(dir.path())
            .await
            .expect("should add directory");
        let results = analyzer.analyze(()).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].document.uri().as_str().contains("included.wdl"));
    }
}
