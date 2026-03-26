//! Representation of the analysis document graph.

use std::collections::HashSet;
use std::fs;
use std::panic;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use indexmap::IndexMap;
use indexmap::IndexSet;
use line_index::LineCol;
use line_index::LineIndex;
use line_index::WideEncoding;
use line_index::WideLineCol;
use petgraph::Direction;
use petgraph::algo::has_path_connecting;
use petgraph::graph::NodeIndex;
use petgraph::stable_graph::StableDiGraph;
use petgraph::visit::Bfs;
use petgraph::visit::EdgeRef;
use petgraph::visit::Visitable;
use petgraph::visit::Walker;
use reqwest::Client;
use rowan::GreenNode;
use rowan::GreenToken;
use rowan::NodeOrToken;
use rowan::TextRange;
use rowan::TextSize;
use rowan::TokenAtOffset;
use tokio::runtime::Handle;
use tracing::debug;
use tracing::trace;
use url::Url;
use wdl_ast::AstNode;
use wdl_ast::AstToken as _;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_grammar::construct_tree;
use wdl_grammar::grammar::v1;
use wdl_grammar::lexer::Lexer;
use wdl_grammar::lexer::v1::Token as V1Token;
use wdl_grammar::parser::Parser;
use wdl_grammar::parser::ParserToken;

use crate::Config;
use crate::IncrementalChange;
use crate::document::Document;
use crate::rules::USING_FALLBACK_VERSION;

/// Represents space for a DFS search of a document graph.
pub type DfsSpace =
    petgraph::algo::DfsSpace<NodeIndex, <StableDiGraph<DocumentGraphNode, ()> as Visitable>::Map>;

/// Represents the parse state of a document graph node.
#[derive(Debug, Clone)]
pub enum ParseState {
    /// The document is not parsed.
    NotParsed,
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
        /// The WDL version of the document.
        ///
        /// This usually comes from the `version` statement in the parsed
        /// document, but can be overridden by
        /// [`Config::with_fallback_version()`].
        wdl_version: Option<SupportedVersion>,
        /// The root CST node of.
        root: GreenNode,
        /// The line index of the document.
        lines: Arc<LineIndex>,
        /// The diagnostics from the parse.
        diagnostics: Vec<Diagnostic>,
    },
}

impl ParseState {
    /// Gets the version of parsed document.
    pub fn version(&self) -> Option<i32> {
        match self {
            ParseState::Parsed { version, .. } => *version,
            _ => None,
        }
    }

    /// Gets the line index of parsed document.
    pub fn lines(&self) -> Option<&Arc<LineIndex>> {
        match self {
            ParseState::Parsed { lines, .. } => Some(lines),
            _ => None,
        }
    }
}

/// Represents a node in a document graph.
#[derive(Debug)]
pub struct DocumentGraphNode {
    /// The analyzer configuration.
    config: Config,
    /// The URI of the document.
    uri: Arc<Url>,
    /// The current incremental change to the document.
    ///
    /// If `None`, there is no pending incremental change applied to the node.
    change: Option<IncrementalChange>,
    /// The parse state of the document.
    parse_state: ParseState,
    /// The analyzed document for the node.
    ///
    /// If `None`, an analysis does not exist for the current state of the node.
    /// This will also be `None` if analysis panicked
    document: Option<Document>,
    /// An error that occurred during the analysis phase for this node
    analysis_error: Option<Arc<anyhow::Error>>,
}

impl DocumentGraphNode {
    /// Constructs a new unparsed document graph node.
    pub fn new(config: Config, uri: Arc<Url>) -> Self {
        Self {
            config,
            uri,
            change: None,
            parse_state: ParseState::NotParsed,
            document: None,
            analysis_error: None,
        }
    }

    /// Gets the URI of the document node.
    pub fn uri(&self) -> &Arc<Url> {
        &self.uri
    }

    /// Notifies the document node that there's been an incremental change.
    pub fn notify_incremental_change(&mut self, change: IncrementalChange) {
        trace!("document `{uri}` has incrementally changed", uri = self.uri);

        // Clear the analyzed document as there has been a change
        self.document = None;

        // Attempt to merge the edits of the change
        if let Some(IncrementalChange {
            version: existing_version,
            start: existing_start,
            edits: existing_edits,
        }) = &mut self.change
        {
            let IncrementalChange {
                version,
                start,
                edits,
            } = change;
            *existing_version = version;
            if start.is_some() {
                *existing_start = start;
                *existing_edits = edits;
            } else {
                existing_edits.extend(edits);
            }
        } else {
            self.change = Some(change)
        }
    }

    /// Notifies the document node that the document has fully changed.
    pub fn notify_change(&mut self, discard_pending: bool) {
        trace!("document `{uri}` has changed", uri = self.uri);

        // Clear the analyzed document as there has been a change
        self.document = None;
        self.analysis_error = None;

        if !matches!(
            self.parse_state,
            ParseState::Parsed {
                version: Some(_),
                ..
            }
        ) || discard_pending
        {
            self.parse_state = ParseState::NotParsed;
            self.change = None;
        }
    }

    /// Gets the parse state of the document node.
    pub fn parse_state(&self) -> &ParseState {
        &self.parse_state
    }

    /// Marks the parse as completed.
    pub fn parse_completed(&mut self, state: ParseState) {
        assert!(!matches!(state, ParseState::NotParsed));
        self.parse_state = state;
        self.analysis_error = None;

        // Clear any document change
        self.change = None;
    }

    /// Gets the analyzed document for the node.
    ///
    /// Returns `None` if the document hasn't been analyzed.
    pub fn document(&self) -> Option<&Document> {
        self.document.as_ref()
    }

    /// Gets the analysis error, if any
    pub fn analysis_error(&self) -> Option<&Arc<anyhow::Error>> {
        self.analysis_error.as_ref()
    }

    /// Marks the analysis as completed.
    pub fn analysis_completed(&mut self, document: Document) {
        self.document = Some(document);
        self.analysis_error = None;
    }

    /// Marks the analysis as failed with an error
    pub fn analysis_failed(&mut self, error: Arc<anyhow::Error>) {
        self.document = None;
        self.analysis_error = Some(error);
    }

    /// Marks the document node for reanalysis.
    ///
    /// This may occur when a dependency has changed.
    pub fn reanalyze(&mut self) {
        self.analysis_error = None;
        self.document = None;
    }

    /// Gets the root AST node of the document.
    ///
    /// Returns `None` if the document was not parsed.
    pub fn root(&self) -> Option<wdl_ast::Document> {
        if let ParseState::Parsed { root, .. } = &self.parse_state {
            return Some(
                wdl_ast::Document::cast(SyntaxNode::new_root(root.clone()))
                    .expect("node should cast"),
            );
        }

        None
    }

    /// Gets the WDL version of the document.
    ///
    /// Returns `None` if the document was not parsed or was missing a version
    /// statement.
    pub fn wdl_version(&self) -> Option<SupportedVersion> {
        if let ParseState::Parsed {
            wdl_version: Some(v),
            ..
        } = &self.parse_state
        {
            Some(*v)
        } else {
            None
        }
    }

    /// Determines if the document needs to be parsed.
    pub fn needs_parse(&self) -> bool {
        self.change.is_some() || matches!(self.parse_state, ParseState::NotParsed)
    }

    /// Parses the document.
    ///
    /// If a parse is not necessary, the current parse state is returned.
    ///
    /// Otherwise, the new parse state is returned.
    pub fn parse(&self, tokio: &Handle, client: &Client) -> Result<ParseState> {
        if !self.needs_parse() {
            return Ok(self.parse_state.clone());
        }

        // First attempt an incremental parse
        if let Some(state) = self.incremental_parse() {
            return Ok(state);
        }

        // Otherwise, fall back to a full parse.
        self.full_parse(tokio, client)
    }

    /// Performs an incremental parse of the document.
    ///
    /// Returns an error with the given change if the document needs a full
    /// parse.
    fn incremental_parse(&self) -> Option<ParseState> {
        match &self.change {
            None | Some(IncrementalChange { start: Some(_), .. }) => None,
            Some(IncrementalChange {
                version,
                start: None,
                edits,
            }) => {
                let ParseState::Parsed {
                    wdl_version,
                    root,
                    lines,
                    diagnostics,
                    ..
                } = &self.parse_state
                else {
                    return None;
                };

                let mut source = SyntaxNode::new_root(root.clone()).text().to_string();
                let mut lines = lines.clone();
                let mut root = root.clone();
                let mut diagnostics = diagnostics.clone();

                for edit in edits {
                    let range = edit_text_range(edit, &source, &lines).ok()?;
                    let current_root = SyntaxNode::new_root(root.clone());
                    let version_statement = version_statement(&current_root)?;
                    let version_span = Span::from(version_statement.text_range());

                    if Span::from(range).intersect(version_span).is_some() {
                        return None;
                    }

                    if let Some((updated_root, replaced_span, replacement_len)) =
                        try_replace_token(&current_root, range, edit.text())
                    {
                        trace!(
                            "incrementally replaced token in `{uri}` at {span}",
                            uri = self.uri,
                            span = replaced_span
                        );
                        root = updated_root;
                        diagnostics = diagnostics
                            .into_iter()
                            .filter_map(|diagnostic| {
                                remap_diagnostic_after_edit(
                                    diagnostic,
                                    replaced_span,
                                    replacement_len,
                                )
                            })
                            .collect();
                    } else if let Some((updated_root, body_diagnostics)) = reparse_document_body(
                        &current_root,
                        &source,
                        &version_statement,
                        range,
                        edit,
                    ) {
                        trace!(
                            "incrementally reparsed document body for `{uri}`",
                            uri = self.uri
                        );
                        root = updated_root;
                        diagnostics.retain(|diagnostic| {
                            diagnostic
                                .labels()
                                .all(|label| label.span().end() <= version_span.end())
                        });
                        diagnostics.extend(body_diagnostics);
                    } else {
                        return None;
                    }

                    edit.apply(&mut source, &lines).ok()?;
                    lines = Arc::new(LineIndex::new(&source));
                }

                diagnostics.sort();
                Some(ParseState::Parsed {
                    version: Some(*version),
                    wdl_version: *wdl_version,
                    root,
                    lines,
                    diagnostics,
                })
            }
        }
    }

    /// Performs a full parse of the node.
    fn full_parse(&self, tokio: &Handle, client: &Client) -> Result<ParseState> {
        let (version, source, lines) = match &self.change {
            None => {
                // Fetch the source
                let result = match self.uri.to_file_path() {
                    Ok(path) => fs::read_to_string(path).map_err(Into::into),
                    Err(_) => match self.uri.scheme() {
                        "https" | "http" => Self::download_source(tokio, client, &self.uri),
                        scheme => Err(anyhow!("unsupported URI scheme `{scheme}`")),
                    },
                };

                match result {
                    Ok(source) => {
                        let lines = Arc::new(LineIndex::new(&source));
                        (None, source, lines)
                    }
                    Err(e) => return Ok(ParseState::Error(e.into())),
                }
            }
            Some(IncrementalChange {
                version,
                start,
                edits,
            }) => {
                // The document has been edited; if there is start source, apply the edits to it
                let (mut source, mut lines) = if let Some(start) = start {
                    let source = start.clone();
                    let lines = Arc::new(LineIndex::new(&source));
                    (source, lines)
                } else {
                    // Otherwise, apply the edits to the last parse
                    match &self.parse_state {
                        ParseState::Parsed { root, lines, .. } => (
                            SyntaxNode::new_root(root.clone()).text().to_string(),
                            lines.clone(),
                        ),
                        _ => panic!(
                            "cannot apply edits to a document that was not previously parsed"
                        ),
                    }
                };

                // We keep track of the last line we've processed so we only rebuild the line
                // index when there is a change that crosses a line
                let mut last_line = !0u32;
                for edit in edits {
                    let range = edit.range();
                    if last_line <= range.end.line {
                        // Only rebuild the line index if the edit has changed lines
                        lines = Arc::new(LineIndex::new(&source));
                    }

                    last_line = range.start.line;
                    edit.apply(&mut source, &lines)?;
                }

                if !edits.is_empty() {
                    // Rebuild the line index after all edits have been applied
                    lines = Arc::new(LineIndex::new(&source));
                }

                (Some(*version), source, lines)
            }
        };

        // Reparse from the source
        let start = Instant::now();
        let (document, mut diagnostics) = wdl_ast::Document::parse(&source);
        debug!(
            "parsing of `{uri}` completed in {elapsed:?}",
            uri = self.uri,
            elapsed = start.elapsed()
        );

        // Apply version fallback logic at this point, so that appropriate diagnostics
        // will prevent subsequent analysis from occurring on an unexpected
        // version
        let mut wdl_version = None;
        if let Some(version_token) = document.version_statement().map(|stmt| stmt.version()) {
            match (
                version_token.text().parse::<SupportedVersion>(),
                self.config.fallback_version(),
            ) {
                // The version in the document is supported, so there's no diagnostic to add
                (Ok(version), _) => {
                    wdl_version = Some(version);
                }
                // The version in the document is not supported, but fallback behavior is configured
                (Err(unrecognized), Some(fallback)) => {
                    if let Some(severity) = self.config.diagnostics_config().using_fallback_version
                    {
                        diagnostics.push(
                            Diagnostic::warning(format!(
                                "unsupported WDL version `{unrecognized}`; interpreting document \
                                 as version `{fallback}`"
                            ))
                            .with_rule(USING_FALLBACK_VERSION)
                            .with_severity(severity)
                            .with_label(
                                "this version of WDL is not supported",
                                version_token.span(),
                            ),
                        );
                    }
                    wdl_version = Some(fallback);
                }
                // Add an error diagnostic if the version is unsupported and don't overwrite
                // `wdl_version`
                (Err(unrecognized), None) => {
                    diagnostics.push(
                        Diagnostic::error(format!("unsupported WDL version `{unrecognized}`"))
                            .with_label(
                                "this version of WDL is not supported",
                                version_token.span(),
                            ),
                    );
                }
            };
        }

        Ok(ParseState::Parsed {
            version,
            wdl_version,
            root: document.inner().green().into(),
            lines,
            diagnostics,
        })
    }

    /// Downloads the source of a `http` or `https` scheme URI.
    ///
    /// This makes a request on the provided tokio runtime to download the
    /// source.
    fn download_source(tokio: &Handle, client: &Client, uri: &Url) -> Result<String> {
        /// The timeout for downloading the source, in seconds.
        const TIMEOUT_IN_SECS: u64 = 30;

        debug!("downloading source from `{uri}`");

        tokio.block_on(async {
            let resp = client
                .get(uri.as_str())
                .timeout(Duration::from_secs(TIMEOUT_IN_SECS))
                .send()
                .await?;

            let code = resp.status();
            if !code.is_success() {
                bail!(
                    "server response for `{uri}` was {code} ({message})",
                    code = code.as_u16(),
                    message = code.canonical_reason().unwrap_or("unknown")
                );
            }

            resp.text()
                .await
                .with_context(|| format!("failed to read response body for `{uri}`"))
        })
    }
}

/// Finds the version statement node in a parsed document root.
fn version_statement(root: &SyntaxNode) -> Option<SyntaxNode> {
    root.children()
        .find(|node| node.kind() == SyntaxKind::VersionStatementNode)
}

/// Converts an edit to a byte range in the given source.
fn edit_text_range(
    edit: &crate::analyzer::SourceEdit,
    source: &str,
    lines: &LineIndex,
) -> Result<TextRange> {
    let (start, end) = match edit.encoding() {
        crate::analyzer::SourcePositionEncoding::UTF8 => (
            LineCol {
                line: edit.range().start.line,
                col: edit.range().start.character,
            },
            LineCol {
                line: edit.range().end.line,
                col: edit.range().end.character,
            },
        ),
        crate::analyzer::SourcePositionEncoding::UTF16 => (
            lines
                .to_utf8(
                    WideEncoding::Utf16,
                    WideLineCol {
                        line: edit.range().start.line,
                        col: edit.range().start.character,
                    },
                )
                .context("invalid edit start position")?,
            lines
                .to_utf8(
                    WideEncoding::Utf16,
                    WideLineCol {
                        line: edit.range().end.line,
                        col: edit.range().end.character,
                    },
                )
                .context("invalid edit end position")?,
        ),
    };

    let range: std::ops::Range<usize> = lines
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

    Ok(TextRange::new(
        TextSize::try_from(range.start).expect("source offset should fit in TextSize"),
        TextSize::try_from(range.end).expect("source offset should fit in TextSize"),
    ))
}

/// Attempts to replace a token directly in the syntax tree.
fn try_replace_token(
    root: &SyntaxNode,
    edit_range: TextRange,
    replacement: &str,
) -> Option<(GreenNode, Span, usize)> {
    let token = covering_token(root, edit_range)?;
    if token
        .parent_ancestors()
        .any(|node| node.kind() == SyntaxKind::VersionStatementNode)
    {
        return None;
    }

    let token_range = token.text_range();
    let relative_start = usize::from(edit_range.start()) - usize::from(token_range.start());
    let relative_end = usize::from(edit_range.end()) - usize::from(token_range.start());

    let mut text = token.text().to_string();
    if !text.is_char_boundary(relative_start) || !text.is_char_boundary(relative_end) {
        return None;
    }

    text.replace_range(relative_start..relative_end, replacement);
    if !lexes_as_same_kind(token.kind(), &text) {
        return None;
    }

    let new_root = token.replace_with(GreenToken::new(token.kind().into(), &text));
    let span = Span::from(token_range);
    Some((new_root, span, text.len()))
}

/// Gets the covering token for an edit range.
fn covering_token(root: &SyntaxNode, range: TextRange) -> Option<wdl_ast::SyntaxToken> {
    if range.is_empty() {
        match root.token_at_offset(range.start()) {
            TokenAtOffset::Single(token) => Some(token),
            TokenAtOffset::Between(_, _) | TokenAtOffset::None => None,
        }
    } else {
        match root.covering_element(range) {
            NodeOrToken::Token(token) => Some(token),
            NodeOrToken::Node(_) => None,
        }
    }
}

/// Determines if the given text still lexes as the same token kind.
fn lexes_as_same_kind(kind: SyntaxKind, text: &str) -> bool {
    match kind {
        SyntaxKind::Version
        | SyntaxKind::LiteralStringText
        | SyntaxKind::LiteralCommandText
        | SyntaxKind::PlaceholderOpen
        | SyntaxKind::Unknown
        | SyntaxKind::Unparsed
        | SyntaxKind::Abandoned
        | SyntaxKind::RootNode
        | SyntaxKind::VersionStatementNode
        | SyntaxKind::ImportStatementNode
        | SyntaxKind::ImportAliasNode
        | SyntaxKind::StructDefinitionNode
        | SyntaxKind::EnumDefinitionNode
        | SyntaxKind::EnumTypeParameterNode
        | SyntaxKind::EnumVariantNode
        | SyntaxKind::TaskDefinitionNode
        | SyntaxKind::WorkflowDefinitionNode
        | SyntaxKind::UnboundDeclNode
        | SyntaxKind::BoundDeclNode
        | SyntaxKind::InputSectionNode
        | SyntaxKind::OutputSectionNode
        | SyntaxKind::CommandSectionNode
        | SyntaxKind::RequirementsSectionNode
        | SyntaxKind::RequirementsItemNode
        | SyntaxKind::TaskHintsSectionNode
        | SyntaxKind::WorkflowHintsSectionNode
        | SyntaxKind::TaskHintsItemNode
        | SyntaxKind::WorkflowHintsItemNode
        | SyntaxKind::WorkflowHintsObjectNode
        | SyntaxKind::WorkflowHintsObjectItemNode
        | SyntaxKind::WorkflowHintsArrayNode
        | SyntaxKind::RuntimeSectionNode
        | SyntaxKind::RuntimeItemNode
        | SyntaxKind::PrimitiveTypeNode
        | SyntaxKind::MapTypeNode
        | SyntaxKind::ArrayTypeNode
        | SyntaxKind::PairTypeNode
        | SyntaxKind::ObjectTypeNode
        | SyntaxKind::TypeRefNode
        | SyntaxKind::MetadataSectionNode
        | SyntaxKind::ParameterMetadataSectionNode
        | SyntaxKind::MetadataObjectItemNode
        | SyntaxKind::MetadataObjectNode
        | SyntaxKind::MetadataArrayNode
        | SyntaxKind::LiteralIntegerNode
        | SyntaxKind::LiteralFloatNode
        | SyntaxKind::LiteralBooleanNode
        | SyntaxKind::LiteralNoneNode
        | SyntaxKind::LiteralNullNode
        | SyntaxKind::LiteralStringNode
        | SyntaxKind::LiteralPairNode
        | SyntaxKind::LiteralArrayNode
        | SyntaxKind::LiteralMapNode
        | SyntaxKind::LiteralMapItemNode
        | SyntaxKind::LiteralObjectNode
        | SyntaxKind::LiteralObjectItemNode
        | SyntaxKind::LiteralStructNode
        | SyntaxKind::LiteralStructItemNode
        | SyntaxKind::LiteralHintsNode
        | SyntaxKind::LiteralHintsItemNode
        | SyntaxKind::LiteralInputNode
        | SyntaxKind::LiteralInputItemNode
        | SyntaxKind::LiteralOutputNode
        | SyntaxKind::LiteralOutputItemNode
        | SyntaxKind::ParenthesizedExprNode
        | SyntaxKind::NameRefExprNode
        | SyntaxKind::IfExprNode
        | SyntaxKind::LogicalNotExprNode
        | SyntaxKind::NegationExprNode
        | SyntaxKind::LogicalOrExprNode
        | SyntaxKind::LogicalAndExprNode
        | SyntaxKind::EqualityExprNode
        | SyntaxKind::InequalityExprNode
        | SyntaxKind::LessExprNode
        | SyntaxKind::LessEqualExprNode
        | SyntaxKind::GreaterExprNode
        | SyntaxKind::GreaterEqualExprNode
        | SyntaxKind::AdditionExprNode
        | SyntaxKind::SubtractionExprNode
        | SyntaxKind::MultiplicationExprNode
        | SyntaxKind::DivisionExprNode
        | SyntaxKind::ModuloExprNode
        | SyntaxKind::ExponentiationExprNode
        | SyntaxKind::CallExprNode
        | SyntaxKind::IndexExprNode
        | SyntaxKind::AccessExprNode
        | SyntaxKind::PlaceholderNode
        | SyntaxKind::PlaceholderSepOptionNode
        | SyntaxKind::PlaceholderDefaultOptionNode
        | SyntaxKind::PlaceholderTrueFalseOptionNode
        | SyntaxKind::ConditionalStatementNode
        | SyntaxKind::ConditionalStatementClauseNode
        | SyntaxKind::ScatterStatementNode
        | SyntaxKind::CallStatementNode
        | SyntaxKind::CallTargetNode
        | SyntaxKind::CallAliasNode
        | SyntaxKind::CallAfterNode
        | SyntaxKind::CallInputItemNode
        | SyntaxKind::MAX => false,
        _ => lex_single_v1_token(text) == Some(kind),
    }
}

/// Attempts to lex the source as a single v1 token.
fn lex_single_v1_token(text: &str) -> Option<SyntaxKind> {
    let mut lexer = Lexer::<V1Token>::new(text);
    let (token, span) = lexer.next()?;
    let token = token.ok()?;
    if span != Span::new(0, text.len()) || lexer.next().is_some() {
        return None;
    }

    Some(token.into_syntax())
}

/// Attempts to reparse the document body after the version statement.
fn reparse_document_body(
    root: &SyntaxNode,
    source: &str,
    version_statement: &SyntaxNode,
    edit_range: TextRange,
    edit: &crate::analyzer::SourceEdit,
) -> Option<(GreenNode, Vec<Diagnostic>)> {
    let body_start = usize::from(version_statement.text_range().end());
    let absolute_range: std::ops::Range<usize> =
        usize::from(edit_range.start())..usize::from(edit_range.end());
    if absolute_range.start < body_start || absolute_range.end < body_start {
        return None;
    }

    let mut body_source = source.get(body_start..)?.to_string();
    let relative_range = absolute_range.start - body_start..absolute_range.end - body_start;
    if !body_source.is_char_boundary(relative_range.start)
        || !body_source.is_char_boundary(relative_range.end)
    {
        return None;
    }

    body_source.replace_range(relative_range, edit.text());

    let mut parser = Parser::new(Lexer::<V1Token>::new(&body_source));
    let marker = parser.start();
    v1::items(&mut parser);
    marker.complete(&mut parser, SyntaxKind::RootNode);
    let output = parser.finish();
    let reparsed = construct_tree(&body_source, output.events).clone_for_update();

    let updated_root = root.clone_for_update();
    let insertion_index = version_statement.index() + 1;
    let child_count = updated_root.children_with_tokens().count();
    let new_children = reparsed.children_with_tokens().collect::<Vec<_>>();
    updated_root.splice_children(insertion_index..child_count, new_children);

    Some((
        updated_root.green().into(),
        output
            .diagnostics
            .into_iter()
            .map(|diagnostic| shift_diagnostic(diagnostic, body_start))
            .collect(),
    ))
}

/// Shifts a diagnostic by the given byte offset.
fn shift_diagnostic(mut diagnostic: Diagnostic, offset: usize) -> Diagnostic {
    for label in diagnostic.labels_mut() {
        let span = label.span();
        label.set_span(Span::new(span.start() + offset, span.len()));
    }

    diagnostic
}

/// Updates an unaffected diagnostic after a token edit.
fn remap_diagnostic_after_edit(
    mut diagnostic: Diagnostic,
    replaced_span: Span,
    replacement_len: usize,
) -> Option<Diagnostic> {
    let delta = replacement_len as isize - replaced_span.len() as isize;
    for label in diagnostic.labels_mut() {
        let span = label.span();
        if span.intersect(replaced_span).is_some() {
            return None;
        }

        if span.start() >= replaced_span.end() {
            label.set_span(shift_span(span, delta)?);
        }
    }

    Some(diagnostic)
}

/// Shifts a span by the given signed delta.
fn shift_span(span: Span, delta: isize) -> Option<Span> {
    let start = span.start().checked_add_signed(delta)?;
    let end = span.end().checked_add_signed(delta)?;
    Some(Span::new(start, end - start))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use line_index::LineIndex;
    use url::Url;
    use wdl_ast::AstNode;
    use wdl_ast::AstToken;
    use wdl_ast::Document;
    use wdl_ast::SyntaxKind;
    use wdl_ast::SyntaxNode;

    use super::DocumentGraphNode;
    use super::ParseState;
    use super::version_statement;
    use crate::Config;
    use crate::IncrementalChange;
    use crate::SourceEdit;
    use crate::SourcePosition;
    use crate::SourcePositionEncoding;

    fn parse_state(source: &str) -> ParseState {
        let (document, diagnostics) = Document::parse(source);
        let wdl_version = document
            .version_statement()
            .and_then(|statement| statement.version().text().parse().ok());

        ParseState::Parsed {
            version: None,
            wdl_version,
            root: document.inner().green().into(),
            lines: Arc::new(LineIndex::new(source)),
            diagnostics,
        }
    }

    fn node(source: &str, edit: SourceEdit) -> DocumentGraphNode {
        DocumentGraphNode {
            config: Config::default(),
            uri: Arc::new(Url::parse("file:///test.wdl").expect("valid test URL")),
            change: Some(IncrementalChange {
                version: 2,
                start: None,
                edits: vec![edit],
            }),
            parse_state: parse_state(source),
            document: None,
            analysis_error: None,
        }
    }

    fn same_green(left: &SyntaxNode, right: &SyntaxNode) -> bool {
        std::ptr::eq(left.green().as_ref(), right.green().as_ref())
    }

    #[test]
    fn it_incrementally_replaces_tokens() {
        let source = r#"version 1.1

task helper {
    command <<<>>>
}

workflow test {
}
"#;
        let node = node(
            source,
            SourceEdit::new(
                SourcePosition::new(6, 9)..SourcePosition::new(6, 13),
                SourcePositionEncoding::UTF8,
                "renamed",
            ),
        );

        let ParseState::Parsed { root: before, .. } = &node.parse_state else {
            panic!("expected parsed state");
        };
        let before = SyntaxNode::new_root(before.clone());
        let before_version = version_statement(&before).expect("version statement");
        let before_task = before
            .children()
            .find(|node| node.kind() == SyntaxKind::TaskDefinitionNode)
            .expect("task definition");

        let Some(ParseState::Parsed { root: after, .. }) = node.incremental_parse() else {
            panic!("expected incremental parse");
        };
        let after = SyntaxNode::new_root(after);
        let after_version = version_statement(&after).expect("version statement");
        let after_task = after
            .children()
            .find(|node| node.kind() == SyntaxKind::TaskDefinitionNode)
            .expect("task definition");

        assert!(same_green(&before_version, &after_version));
        assert!(same_green(&before_task, &after_task));
    }

    #[test]
    fn it_incrementally_reparses_the_document_body() {
        let source = r#"version 1.1

task helper {
    command <<<>>>
}

workflow test {
}
"#;
        let node = node(
            source,
            SourceEdit::new(
                SourcePosition::new(6, 9)..SourcePosition::new(6, 13),
                SourcePositionEncoding::UTF8,
                "if",
            ),
        );

        let ParseState::Parsed { root: before, .. } = &node.parse_state else {
            panic!("expected parsed state");
        };
        let before = SyntaxNode::new_root(before.clone());
        let before_version = version_statement(&before).expect("version statement");
        let before_task = before
            .children()
            .find(|node| node.kind() == SyntaxKind::TaskDefinitionNode)
            .expect("task definition");

        let Some(ParseState::Parsed {
            root: after,
            diagnostics,
            ..
        }) = node.incremental_parse()
        else {
            panic!("expected incremental parse");
        };
        let after = SyntaxNode::new_root(after);
        let after_version = version_statement(&after).expect("version statement");
        let after_task = after
            .children()
            .find(|node| node.kind() == SyntaxKind::TaskDefinitionNode)
            .expect("task definition");

        assert!(same_green(&before_version, &after_version));
        assert!(!same_green(&before_task, &after_task));
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn it_falls_back_to_a_full_parse_when_the_version_statement_changes() {
        let source = r#"version 1.1

workflow test {
}
"#;
        let node = node(
            source,
            SourceEdit::new(
                SourcePosition::new(0, 10)..SourcePosition::new(0, 13),
                SourcePositionEncoding::UTF8,
                "1.2",
            ),
        );

        assert!(node.incremental_parse().is_none());
    }
}

/// Represents a graph of WDL analyzed documents.
#[derive(Debug)]
pub struct DocumentGraph {
    /// The analyzer configuration.
    config: Config,
    /// The inner directional graph.
    ///
    /// Edges in the graph denote inverse dependency relationships (i.e. "is
    /// depended upon by").
    inner: StableDiGraph<DocumentGraphNode, ()>,
    /// Map from document URI to graph node index.
    indexes: IndexMap<Arc<Url>, NodeIndex>,
    /// The current set of rooted nodes in the graph.
    ///
    /// Rooted nodes are those that were explicitly added to the analyzer.
    ///
    /// A rooted node is one that will not be collected even if the node has no
    /// outgoing edges (i.e. is not depended upon by any other file).
    roots: IndexSet<NodeIndex>,
    /// Represents dependency edges that, if they were added to the document
    /// graph, would form a cycle.
    ///
    /// The first in the pair is the dependant node and the second is the
    /// depended node.
    ///
    /// This is used to break import cycles; when analyzing the document, if the
    /// import relationship exists in this set, a diagnostic will be added and
    /// the import otherwise ignored.
    cycles: HashSet<(NodeIndex, NodeIndex)>,
}

impl DocumentGraph {
    /// Make a new [`DocumentGraph`] with the given configuration.
    pub fn new(config: Config) -> Self {
        DocumentGraph {
            config,
            inner: StableDiGraph::new(),
            indexes: IndexMap::new(),
            roots: IndexSet::new(),
            cycles: HashSet::new(),
        }
    }

    /// Add a node to the document graph.
    pub fn add_node(&mut self, uri: Url, rooted: bool) -> NodeIndex {
        let index = match self.indexes.get(&uri) {
            Some(index) => *index,
            _ => {
                debug!("inserting `{uri}` into the document graph");
                let uri = Arc::new(uri);
                let index = self
                    .inner
                    .add_node(DocumentGraphNode::new(self.config.clone(), uri.clone()));
                self.indexes.insert(uri, index);
                index
            }
        };

        if rooted {
            self.roots.insert(index);
        }

        index
    }

    /// Removes a root from the document graph.
    ///
    /// Note that this does not remove any nodes, only removes the document from
    /// the set of rooted nodes.
    ///
    /// If the node has no outgoing edges, it will be removed on the next
    /// garbage collection.
    pub fn remove_root(&mut self, uri: &Url) {
        let base = match uri.to_file_path() {
            Ok(base) => base,
            Err(_) => return,
        };

        // As the URI might be a directory containing WDL files, look for prefixed files
        let mut removed = Vec::new();
        for (uri, index) in &self.indexes {
            let path = match uri.to_file_path() {
                Ok(path) => path,
                Err(_) => continue,
            };

            if path.starts_with(&base) {
                removed.push(*index);
            }
        }

        for index in removed {
            let node = &mut self.inner[index];

            // We don't actually remove nodes from the graph, just remove it as a root.
            // If the node has no outgoing edges, it will be collected in the next GC.
            if !self.roots.swap_remove(&index) {
                debug!(
                    "document `{uri}` is no longer rooted in the graph",
                    uri = node.uri
                );
            }

            node.parse_state = ParseState::NotParsed;
            node.document = None;
            node.change = None;

            // Do a BFS traversal to trigger re-analysis in dependent documents
            self.bfs_mut(index, |graph, dependent: NodeIndex| {
                let node = graph.get_mut(dependent);
                trace!("document `{uri}` needs to be reanalyzed", uri = node.uri);
                node.document = None;
            });
        }
    }

    /// Determines if the given node is rooted.
    pub fn is_rooted(&self, index: NodeIndex) -> bool {
        self.roots.contains(&index)
    }

    /// Gets the rooted nodes in the graph.
    pub fn roots(&self) -> &IndexSet<NodeIndex> {
        &self.roots
    }

    /// Determines if the given document node should be included in analysis
    /// results.
    pub fn include_result(&self, index: NodeIndex) -> bool {
        // Only consider rooted or parsed nodes that have been analyzed
        let node = self.get(index);
        node.document().is_some()
            && (self.roots.contains(&index)
                || matches!(node.parse_state(), ParseState::Parsed { .. }))
    }

    /// Gets a node from the graph.
    pub fn get(&self, index: NodeIndex) -> &DocumentGraphNode {
        &self.inner[index]
    }

    /// Gets a mutable node from the graph.
    pub fn get_mut(&mut self, index: NodeIndex) -> &mut DocumentGraphNode {
        &mut self.inner[index]
    }

    /// Gets the node index for the given document URI.
    ///
    /// Returns `None` if the document is not in the graph.
    pub fn get_index(&self, uri: &Url) -> Option<NodeIndex> {
        self.indexes.get(uri).copied()
    }

    /// Performs a breadth-first traversal of the graph starting at the given
    /// node.
    ///
    /// Mutations to the document nodes are permitted.
    pub fn bfs_mut(&mut self, index: NodeIndex, mut cb: impl FnMut(&mut Self, NodeIndex)) {
        let mut bfs = Bfs::new(&self.inner, index);
        while let Some(node) = bfs.next(&self.inner) {
            cb(self, node);
        }
    }

    /// Gets the direct dependencies of a node.
    pub fn dependencies(&self, index: NodeIndex) -> impl Iterator<Item = NodeIndex> + '_ {
        self.inner
            .edges_directed(index, Direction::Incoming)
            .map(|e| e.source())
    }

    /// Removes all dependency edges from the given node.
    pub fn remove_dependency_edges(&mut self, index: NodeIndex) {
        // Retain all edges where the target isn't the given node (i.e. an incoming
        // edge)
        self.inner.retain_edges(|g, e| {
            let (_, target) = g.edge_endpoints(e).expect("edge should be valid");
            target != index
        });
    }

    /// Adds a dependency edge from one document to another.
    ///
    /// If a dependency edge already exists, this is a no-op.
    pub fn add_dependency_edge(&mut self, from: NodeIndex, to: NodeIndex, space: &mut DfsSpace) {
        // Check to see if there is already a path between the nodes; if so, there's a
        // cycle
        if has_path_connecting(&self.inner, from, to, Some(space)) {
            // Adding the edge would cause a cycle, so record the cycle instead
            debug!(
                "an import cycle was detected between `{from}` and `{to}`",
                from = self.inner[from].uri,
                to = self.inner[to].uri
            );
            self.cycles.insert((from, to));
        } else if !self.inner.contains_edge(to, from) {
            debug!(
                "adding dependency edge from `{from}` to `{to}`",
                from = self.inner[from].uri,
                to = self.inner[to].uri
            );

            // Note that we store inverse dependency edges in the graph, so the relationship
            // is reversed
            self.inner.add_edge(to, from, ());
        }
    }

    /// Determines if there is a cycle between the given nodes.
    pub fn contains_cycle(&self, from: NodeIndex, to: NodeIndex) -> bool {
        self.cycles.contains(&(from, to))
    }

    /// Creates a subgraph of this graph for the given nodes to include.
    pub fn subgraph(&self, nodes: &IndexSet<NodeIndex>) -> StableDiGraph<NodeIndex, ()> {
        self.inner
            .filter_map(|i, _| nodes.contains(&i).then_some(i), |_, _| Some(()))
    }

    /// Performs a garbage collection on the graph.
    ///
    /// This removes any non-rooted nodes that have no outgoing edges (i.e. are
    /// not depended upon by another document).
    pub fn gc(&mut self) {
        let mut collected = HashSet::new();
        for node in self.inner.node_indices() {
            if self.roots.contains(&node) {
                continue;
            }

            if self
                .inner
                .edges_directed(node, Direction::Outgoing)
                .next()
                .is_none()
            {
                debug!(
                    "removing document `{uri}` from the graph",
                    uri = self.inner[node].uri
                );
                collected.insert(node);
            }
        }

        if collected.is_empty() {
            return;
        }

        for node in &collected {
            self.inner.remove_node(*node);
        }

        self.indexes.retain(|_, index| !collected.contains(index));

        self.cycles
            .retain(|(from, to)| !collected.contains(from) && !collected.contains(to));
    }

    /// Gets all nodes that have a dependency on the given node.
    pub fn transitive_dependents(
        &self,
        index: petgraph::graph::NodeIndex,
    ) -> impl Iterator<Item = NodeIndex> {
        Bfs::new(&self.inner, index).iter(&self.inner)
    }

    /// Gets the inner stable dependency graph.
    pub(crate) fn inner(&self) -> &StableDiGraph<DocumentGraphNode, ()> {
        &self.inner
    }
}
