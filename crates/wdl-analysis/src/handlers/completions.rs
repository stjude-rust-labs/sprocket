//! Handlers for code completion requests.
//!
//! This module implements the LSP `textDocument/completion` functionality for
//! WDL files. It provides context-aware completions for various WDL language
//! constructs including:
//!
//! - Keywords appropriate to the current context (task, workflow and
//!   root-level)
//! - Variables and declarations visible in the current scope
//! - Standard library functions with signatures and documentation
//! - User-defined structs and their members
//! - Callable items (tasks and workflows) from local and imported namespaces
//! - Member access completions for struct fields, call outputs, and pair
//!   elements
//! - Import namespace identifiers
//! - Snippets for common WDL constructs
//!
//! See: [LSP Specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocument_completion)

use std::fmt::format;
use std::sync::Arc;

use anyhow::Result;
use anyhow::bail;
use indexmap::IndexMap;
use indexmap::IndexSet;
use line_index::LineIndex;
use lsp_types::CompletionItem;
use lsp_types::CompletionItemKind;
use lsp_types::CompletionTextEdit;
use lsp_types::InsertTextFormat;
use lsp_types::Range;
use lsp_types::TextEdit;
use rowan::TextSize;
use tracing::debug;
use url::Url;
use wdl_ast::AstNode;
use wdl_ast::AstToken;
use wdl_ast::SupportedVersion;
use wdl_ast::SyntaxKind;
use wdl_ast::SyntaxNode;
use wdl_ast::SyntaxToken;
use wdl_ast::TreeNode;
use wdl_ast::lexer::TokenSet;
use wdl_ast::lexer::VersionStatementToken;
use wdl_ast::lexer::v1::Token;
use wdl_ast::v1::BoundDecl;
use wdl_ast::v1::Expr;
use wdl_ast::v1::LiteralExpr;
use wdl_ast::v1::MetadataValue;
use wdl_ast::v1::REQUIREMENTS_KEY;
use wdl_ast::v1::RUNTIME_KEYS;
use wdl_ast::v1::TASK_FIELD_META;
use wdl_ast::v1::TASK_FIELD_PARAMETER_META;
use wdl_ast::v1::TASK_FIELDS;
use wdl_ast::v1::TASK_HINT_KEYS;
use wdl_ast::v1::TaskDefinition;
use wdl_ast::v1::WORKFLOW_HINT_KEYS;
use wdl_grammar::grammar::v1::NESTED_WORKFLOW_STATEMENT_KEYWORDS;
use wdl_grammar::grammar::v1::ROOT_SECTION_KEYWORDS;
use wdl_grammar::grammar::v1::STRUCT_SECTION_KEYWORDS;
use wdl_grammar::grammar::v1::TASK_ITEM_EXPECTED_SET;
use wdl_grammar::grammar::v1::WORKFLOW_ITEM_EXPECTED_SET;
use wdl_grammar::parser::ParserToken;

use crate::Document;
use crate::SourcePosition;
use crate::SourcePositionEncoding;
use crate::document::ScopeRef;
use crate::document::TASK_VAR_NAME;
use crate::graph::DocumentGraph;
use crate::graph::ParseState;
use crate::handlers::TypeEvalContext;
use crate::handlers::common::make_md_docs;
use crate::handlers::common::position;
use crate::handlers::common::position_to_offset;
use crate::handlers::common::provide_struct_documentation;
use crate::handlers::common::provide_task_documentation;
use crate::handlers::common::provide_workflow_documentation;
use crate::handlers::snippets;
use crate::stdlib::Function;
use crate::stdlib::STDLIB;
use crate::stdlib::TypeParameters;
use crate::types::CompoundType;
use crate::types::Type;
use crate::types::v1::ExprTypeEvaluator;
use crate::types::v1::task_hint_types;
use crate::types::v1::task_member_type;
use crate::types::v1::task_requirement_types;

/// Provides code completion suggestions for the given position in a document.
///
/// Analyzes the context at the specified position and returns appropriate
/// completion items based on the surrounding syntax and scope. The completions
/// are filtered by any partial word already typed at the cursor position.
///
/// Provides context-aware suggestions by:
/// 1. Determining if the cursor is in a member access context (i.e. after a `.`
///    dot)
/// 2. Walking up the CST to find the appropriate completion context
/// 3. Adding relevant completions based on the context (keywords, scope items,
///    etc.)
/// 4. Filtering results by any partially typed identifier
pub fn completion(
    graph: &DocumentGraph,
    document_uri: &Url,
    position: SourcePosition,
    encoding: SourcePositionEncoding,
) -> Result<Vec<CompletionItem>> {
    let Some(index) = graph.get_index(document_uri) else {
        bail!("document `{document_uri}` not found in graph")
    };
    let node = graph.get(index);
    let (root, lines) = match node.parse_state() {
        ParseState::Parsed { lines, root, .. } => {
            (SyntaxNode::new_root(root.clone()), lines.clone())
        }
        _ => bail!("document `{uri}` has not been parsed", uri = document_uri),
    };

    let Some(document) = node.document() else {
        bail!("document analysis data not available for {}", document_uri);
    };

    let offset = position_to_offset(&lines, position, encoding)?;
    let token = root.token_at_offset(offset).left_biased();

    let mut items = Vec::new();

    if let Some(token) = token.as_ref() {
        if token.parent().map(|p| p.kind()) == Some(SyntaxKind::VersionStatementNode) {
            let _ = add_version_completions(token, &lines, &mut items);
            return Ok(items);
        }

        // NOTE: Custom handling for version completion. If the token to the immediate
        // left of the cursor (ignoring whitespace) is the `version` keyword, we are
        // very likely completing the version number.
        let mut non_trivia = token.clone();
        if non_trivia.kind().is_trivia()
            && let Some(prev) = non_trivia.prev_token()
        {
            non_trivia = prev;
        }
        if non_trivia.kind() == SyntaxKind::VersionKeyword {
            let _ = add_version_completions(token, &lines, &mut items);
            return Ok(items);
        }
    }

    let partial_word = token
        .as_ref()
        .filter(|t| t.kind() == SyntaxKind::Ident && t.text_range().contains_inclusive(offset))
        .map(|t| {
            let start = t.text_range().start();
            let len = offset - start;
            t.text()[..len.into()].to_string()
        });

    let parent = token
        .as_ref()
        .and_then(|t| t.parent())
        .unwrap_or_else(|| root.clone());

    // Trigger member access completions if the cursor is on a dot, or on an
    // identifier immediately following a dot.
    let is_member_access = if let Some(t) = &token {
        match t.kind() {
            SyntaxKind::Dot | SyntaxKind::OpenBracket => true,
            SyntaxKind::Ident => t
                .prev_token()
                .filter(|prev| !prev.kind().is_trivia())
                .is_some_and(|prev| prev.kind() == SyntaxKind::Dot),
            _ => false,
        }
    } else {
        false
    };

    if is_member_access {
        add_member_access_completions(document, &parent, &mut items)?;
    } else {
        let mut visited_kinds = IndexSet::new();
        let mut current = Some(parent);
        while let Some(node) = current {
            if visited_kinds.insert(node.kind()) {
                add_snippet_completions(document, &node, &mut items);
            }

            match node.kind() {
                SyntaxKind::WorkflowDefinitionNode => {
                    add_keyword_completions(&WORKFLOW_ITEM_EXPECTED_SET, &mut items);
                    if let Some(scope) = document.find_scope_by_position(offset.into()) {
                        add_scope_completions(scope, &mut items);
                    }
                    add_stdlib_completions(&mut items);
                    add_struct_completions(document, &mut items);
                    add_namespace_completions(document, &mut items);
                    add_callable_completions(document, &mut items);
                    break;
                }
                SyntaxKind::ScatterStatementNode | SyntaxKind::ConditionalStatementNode => {
                    add_keyword_completions(&NESTED_WORKFLOW_STATEMENT_KEYWORDS, &mut items);
                    if let Some(scope) = document.find_scope_by_position(offset.into()) {
                        add_scope_completions(scope, &mut items);
                    }
                    add_stdlib_completions(&mut items);
                    add_struct_completions(document, &mut items);
                    add_namespace_completions(document, &mut items);
                    add_callable_completions(document, &mut items);
                    break;
                }

                SyntaxKind::TaskDefinitionNode => {
                    add_keyword_completions(&TASK_ITEM_EXPECTED_SET, &mut items);
                    if let Some(scope) = document.find_scope_by_position(offset.into()) {
                        add_scope_completions(scope, &mut items);
                    }
                    add_stdlib_completions(&mut items);
                    add_struct_completions(document, &mut items);
                    break;
                }

                SyntaxKind::StructDefinitionNode => {
                    add_struct_completions(document, &mut items);
                    add_keyword_completions(&STRUCT_SECTION_KEYWORDS, &mut items);
                    break;
                }

                SyntaxKind::RuntimeSectionNode => {
                    add_runtime_key_completions(document.version(), &mut items);
                    break;
                }

                SyntaxKind::RequirementsSectionNode => {
                    add_requirements_key_completions(document.version(), &mut items);
                    break;
                }
                SyntaxKind::TaskHintsSectionNode => {
                    add_task_hints_key_completions(document.version(), &mut items);
                    break;
                }

                SyntaxKind::WorkflowHintsSectionNode => {
                    add_workflow_hints_key_completions(&mut items);
                    break;
                }

                SyntaxKind::RootNode => {
                    add_keyword_completions(&ROOT_SECTION_KEYWORDS, &mut items);
                    add_struct_completions(document, &mut items);
                    add_namespace_completions(document, &mut items);
                    break;
                }
                _ => current = node.parent(),
            }
        }
    }

    match partial_word {
        Some(partial) => {
            let items = items
                .into_iter()
                .filter(|item| item.label.starts_with(&partial))
                .collect();
            Ok(items)
        }
        None => Ok(items),
    }
}

/// Generates completion items for WDL keywords based on the provided token set.
///
/// Converts raw token values to completion items with appropriate labels,
/// kinds, and descriptions.
fn add_keyword_completions(token_set: &TokenSet, items: &mut Vec<CompletionItem>) {
    items.extend(token_set.iter().map(|raw| {
        let token = Token::from_raw(raw);
        let label = token
            .describe()
            .trim_start_matches("`")
            .split("`")
            .next()
            .unwrap();

        CompletionItem {
            label: label.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            ..Default::default()
        }
    }))
}

/// Adds completions for member access expressions.
///
/// Takes a syntax node containing the member access expression (parent of the
/// `.` token) and handles different types of member access completions:
///
/// - Namespace access
/// - Struct member access
/// - Call output access
/// - Pair element access (when accessing `.left` and `.right` of pair types)
///
/// For namespace access, it directly looks up the identifier before the dot.
/// For other types, it evaluates the expression type to determine available
/// members.
///
/// The node is the parent of the `.` token. For incomplete document, it might
/// not be fully-formed `AccessExprNode`. We find the expression to the left
/// of the dot.
fn add_member_access_completions(
    document: &Document,
    node: &SyntaxNode,
    items: &mut Vec<CompletionItem>,
) -> Result<()> {
    let Some(accessor_token) = node
        .children_with_tokens()
        .find(|t| matches!(t.kind(), SyntaxKind::Dot | SyntaxKind::OpenBracket))
    else {
        debug!("could not find accessor token ( or [");
        return Ok(());
    };

    let Some(target_element) = accessor_token.prev_sibling_or_token() else {
        return Ok(());
    };

    // Namespace completions
    if let Some(token) = target_element.as_token()
        && token.kind() == SyntaxKind::Ident
        && let Some(ns) = document.namespace(token.text())
    {
        let ns_root = ns.document().root();
        let ns_doc_version = document.version();
        for task in ns.document().tasks() {
            items.push(CompletionItem {
                label: task.name().to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(format!("task {}", task.name())),
                documentation: provide_task_documentation(task, &ns_root).and_then(make_md_docs),
                ..Default::default()
            });

            let snippet = build_call_snippet(task.name(), task.inputs(), ns_doc_version);
            items.push(CompletionItem {
                label: format!("{} {{...}}", task.name()),
                kind: Some(CompletionItemKind::SNIPPET),
                detail: Some(format!("call task {} with required inputs", task.name())),
                documentation: provide_task_documentation(task, &ns_root).and_then(make_md_docs),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                insert_text: Some(snippet),
                filter_text: Some(task.name().to_string()),
                ..Default::default()
            })
        }

        if let Some(workflow) = ns.document().workflow() {
            let name = workflow.name();
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(format!("workflow {}", name)),
                documentation: provide_workflow_documentation(workflow, &ns_root)
                    .and_then(make_md_docs),
                ..Default::default()
            });
            let snippet = build_call_snippet(name, workflow.inputs(), ns_doc_version);
            items.push(CompletionItem {
                label: format!("{} {{...}}", name),
                kind: Some(CompletionItemKind::SNIPPET),
                detail: Some(format!("call workflow {} with required inputs", name)),
                documentation: provide_workflow_documentation(workflow, &ns_root)
                    .and_then(make_md_docs),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                insert_text: Some(snippet),
                ..Default::default()
            });
        }

        return Ok(());
    }

    let Some(target_node) = target_element.as_node() else {
        return Ok(());
    };

    let Some(target_expr) = Expr::cast(target_node.clone()) else {
        return Ok(());
    };

    // `task.` variable completions
    if let Some(name_ref) = target_expr.as_name_ref() {
        if name_ref.name().text() == TASK_VAR_NAME
            && document.version() >= Some(SupportedVersion::V1(wdl_ast::version::V1::Two))
            && node.ancestors().any(|n| {
                matches!(
                    n.kind(),
                    SyntaxKind::CommandSectionNode | SyntaxKind::OutputSectionNode
                )
            })
        {
            add_task_variable_completions(items);
            return Ok(());
        }
    } else if let Some(access_expr) = target_expr.as_access() {
        // Inferred `task.meta.*` and `task.parameter_meta.*` completions.
        // TODO: recurse on `Objects`
        let (expr, member) = access_expr.operands();
        if let Some(name_ref) = expr.as_name_ref()
            && name_ref.name().text() == TASK_VAR_NAME
        {
            let member_name = member.text();
            // `task.meta.*` completions.
            if member_name == TASK_FIELD_META {
                if let Some(task_def) = node.ancestors().find_map(TaskDefinition::cast)
                    && let Some(meta_section) = task_def.metadata()
                {
                    for item in meta_section.items() {
                        items.push(CompletionItem {
                            label: item.name().text().to_string(),
                            kind: Some(CompletionItemKind::PROPERTY),
                            detail: Some(format_ty(item.value()).to_string()),
                            documentation: make_md_docs(item.value().text().to_string()),
                            ..Default::default()
                        });
                    }
                }
                return Ok(());
            } else if member_name == TASK_FIELD_PARAMETER_META {
                // `task.parameter_meta.*` completions.
                if let Some(task_def) = node.ancestors().find_map(TaskDefinition::cast)
                    && let Some(param_meta_section) = task_def.parameter_metadata()
                {
                    for item in param_meta_section.items() {
                        items.push(CompletionItem {
                            label: item.name().text().to_string(),
                            kind: Some(CompletionItemKind::PROPERTY),
                            detail: Some(format_ty(item.value()).to_string()),
                            documentation: make_md_docs(item.value().text().to_string()),
                            ..Default::default()
                        });
                    }
                }
                return Ok(());
            }
        }
    }

    // NOTE: we do type evaluation only for non namespaces or complex types

    let Some(scope) = document.find_scope_by_position(node.span().start()) else {
        bail!("could not find scope for access expression")
    };

    let mut ctx = TypeEvalContext { scope, document };
    let mut evaluator = ExprTypeEvaluator::new(&mut ctx);
    let target_type = evaluator.evaluate_expr(&target_expr).unwrap_or(Type::Union);

    match (accessor_token.kind(), target_type) {
        (SyntaxKind::Dot, Type::Compound(CompoundType::Struct(s), _)) => {
            for (name, ty) in s.members() {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: Some(CompletionItemKind::FIELD),
                    detail: Some(ty.to_string()),
                    ..Default::default()
                });
            }
        }
        (SyntaxKind::Dot, Type::Call(call)) => {
            for (name, output) in call.outputs() {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: Some(CompletionItemKind::FIELD),
                    detail: Some(output.ty().to_string()),
                    ..Default::default()
                });
            }
        }
        (SyntaxKind::Dot, Type::Compound(CompoundType::Pair(p), _)) => {
            items.push(CompletionItem {
                label: "left".to_string(),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some(p.left_type().to_string()),
                ..Default::default()
            });

            items.push(CompletionItem {
                label: "right".to_string(),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some(p.right_type().to_string()),
                ..Default::default()
            });
        }
        (SyntaxKind::OpenBracket, Type::Compound(CompoundType::Map(_), _)) => {
            if let Expr::NameRef(name_ref) = target_expr {
                let var_name = name_ref.name();

                if let Some(decl_span) = scope.lookup(var_name.text()).map(|n| n.span()) {
                    let token_at_decl = document
                        .root()
                        .inner()
                        .token_at_offset(TextSize::try_from(decl_span.start())?)
                        .left_biased();

                    if let Some(decl_node) =
                        token_at_decl.and_then(|t| t.parent_ancestors().find_map(BoundDecl::cast))
                        && let Expr::Literal(LiteralExpr::Map(map_literal)) = decl_node.expr()
                    {
                        for item in map_literal.items() {
                            let (key, _) = item.key_value();
                            if let Expr::Literal(literal_key) = key {
                                match literal_key {
                                    LiteralExpr::String(s) => {
                                        if let Some(text) = s.text() {
                                            items.push(CompletionItem {
                                                label: format!("\"{}\"", text.text()),
                                                kind: Some(CompletionItemKind::VALUE),
                                                ..Default::default()
                                            });
                                        }
                                    }

                                    LiteralExpr::Integer(i) => {
                                        items.push(CompletionItem {
                                            label: format!("{}", i.text()),
                                            kind: Some(CompletionItemKind::VALUE),
                                            ..Default::default()
                                        });
                                    }

                                    LiteralExpr::Float(f) => {
                                        items.push(CompletionItem {
                                            label: format!("{}", f.text()),
                                            kind: Some(CompletionItemKind::VALUE),
                                            ..Default::default()
                                        });
                                    }

                                    LiteralExpr::Boolean(b) => {
                                        items.push(CompletionItem {
                                            label: format!("{}", b.text()),
                                            kind: Some(CompletionItemKind::VALUE),
                                            ..Default::default()
                                        });
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            } else if let Expr::Literal(LiteralExpr::Map(map_literal)) = target_expr {
                for item in map_literal.items() {
                    let (key, _) = item.key_value();
                    if let Expr::Literal(literal_key) = key {
                        match literal_key {
                            LiteralExpr::String(s) => {
                                if let Some(text) = s.text() {
                                    items.push(CompletionItem {
                                        label: format!("\"{}\"", text.text()),
                                        kind: Some(CompletionItemKind::VALUE),
                                        ..Default::default()
                                    });
                                }
                            }

                            LiteralExpr::Integer(i) => {
                                items.push(CompletionItem {
                                    label: format!("{}", i.text()),
                                    kind: Some(CompletionItemKind::VALUE),
                                    ..Default::default()
                                });
                            }

                            LiteralExpr::Float(f) => {
                                items.push(CompletionItem {
                                    label: format!("{}", f.text()),
                                    kind: Some(CompletionItemKind::VALUE),
                                    ..Default::default()
                                });
                            }

                            LiteralExpr::Boolean(b) => {
                                items.push(CompletionItem {
                                    label: format!("{}", b.text()),
                                    kind: Some(CompletionItemKind::VALUE),
                                    ..Default::default()
                                });
                            }
                            _ => {}
                        }
                    }
                }
            };
        }
        _ => {
            debug!(
                "No specific access completion logic for this type {:?}",
                accessor_token.kind()
            );
        }
    }

    Ok(())
}

/// Adds completions for callable items available in the current document.
///
/// Includes both local and imported tasks and workflows.
fn add_callable_completions(document: &Document, items: &mut Vec<CompletionItem>) {
    let root_node = document.root();
    let version = document.version();

    for task in document.tasks() {
        let name = task.name();
        items.push(CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(format!("task {}", name)),
            documentation: provide_task_documentation(task, &root_node).and_then(make_md_docs),
            ..Default::default()
        });

        let snippet = build_call_snippet(name, task.inputs(), version);
        items.push(CompletionItem {
            label: format!("{} {{...}}", name),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some(format!("call task {} with required inputs", name)),
            documentation: provide_task_documentation(task, &root_node).and_then(make_md_docs),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            insert_text: Some(snippet),
            ..Default::default()
        });
    }
    if let Some(workflow) = document.workflow() {
        let name = workflow.name();
        items.push(CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(format!("workflow {}", name)),
            documentation: provide_workflow_documentation(workflow, &root_node)
                .and_then(make_md_docs),
            ..Default::default()
        });

        let snippet = build_call_snippet(name, workflow.inputs(), version);
        items.push(CompletionItem {
            label: format!("{} {{...}}", name),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some(format!("call task {} with required_inputs", name)),
            documentation: provide_workflow_documentation(workflow, &root_node)
                .and_then(make_md_docs),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            insert_text: Some(snippet),
            ..Default::default()
        });
    }

    for (ns_name, ns) in document.namespaces() {
        let ns_root = ns.document().root();

        for task in ns.document().tasks() {
            let name = task.name();
            let label = format!("{ns_name}.{name}");
            items.push(CompletionItem {
                label: label.clone(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some("task".to_string()),
                documentation: provide_task_documentation(task, &ns_root).and_then(make_md_docs),
                ..Default::default()
            });

            let snippet = build_call_snippet(&label, task.inputs(), version);
            items.push(CompletionItem {
                label: format!("{} {{...}}", label),
                kind: Some(CompletionItemKind::SNIPPET),
                detail: Some(format!("call task {} with required inputs", label)),
                documentation: provide_task_documentation(task, &ns_root).and_then(make_md_docs),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                insert_text: Some(snippet),
                ..Default::default()
            });
        }
        if let Some(workflow) = ns.document().workflow() {
            let name = workflow.name();
            let label = format!("{ns_name}.{name}");

            items.push(CompletionItem {
                label: label.clone(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some("workflow".to_string()),
                documentation: provide_workflow_documentation(workflow, &ns_root)
                    .and_then(make_md_docs),
                ..Default::default()
            });

            let snippet = build_call_snippet(&label, workflow.inputs(), version);
            items.push(CompletionItem {
                label: format!("{} {{...}}", label),
                kind: Some(CompletionItemKind::SNIPPET),
                detail: Some(format!("call workflow {} with required inputs", label)),
                documentation: provide_workflow_documentation(workflow, &ns_root)
                    .and_then(make_md_docs),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                insert_text: Some(snippet),
                ..Default::default()
            });
        }
    }
}

/// Adds completions for variables and declarations visible in the current
/// scope.
fn add_scope_completions(scope: ScopeRef<'_>, items: &mut Vec<CompletionItem>) {
    let mut current_scope = Some(scope);
    while let Some(s) = current_scope {
        for (name, name_info) in s.names() {
            if !items.iter().any(|i| i.label == name) {
                let (kind, detail) = match name_info.ty() {
                    Type::Call(_) => (
                        Some(CompletionItemKind::FIELD),
                        Some(format!("call output: {}", name_info.ty())),
                    ),
                    _ => (
                        Some(CompletionItemKind::VARIABLE),
                        Some(name_info.ty().to_string()),
                    ),
                };

                items.push(CompletionItem {
                    label: name.to_string(),
                    kind,
                    detail,
                    ..Default::default()
                });
            }
        }
        current_scope = s.parent();
    }
}

/// Adds completions for all WDL standard library functions.
fn add_stdlib_completions(items: &mut Vec<CompletionItem>) {
    for (name, func) in STDLIB.functions() {
        match func {
            Function::Monomorphic(m) => {
                let sig = m.signature();
                let params = TypeParameters::new(sig.type_parameters());
                let detail = Some(format!("{name}{}", sig.display(&params)));
                let docs = sig.definition().and_then(|d| make_md_docs(d.to_string()));
                let snippet = build_function_snippet(name, sig);
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: Some(CompletionItemKind::FUNCTION),
                    detail,
                    documentation: docs,
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    insert_text: Some(snippet),
                    ..Default::default()
                })
            }
            Function::Polymorphic(p) => {
                for sig in p.signatures() {
                    let params = TypeParameters::new(sig.type_parameters());
                    let detail = Some(format!("{name}{}", sig.display(&params)));
                    let docs = sig.definition().and_then(|d| make_md_docs(d.to_string()));
                    let snippet = build_function_snippet(name, sig);
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail,
                        documentation: docs,
                        insert_text_format: Some(InsertTextFormat::SNIPPET),
                        insert_text: Some(snippet),
                        ..Default::default()
                    });
                }
            }
        };
    }
}

/// Adds completions for user-defined structs in the document.
fn add_struct_completions(document: &Document, items: &mut Vec<CompletionItem>) {
    let root = document.root();
    for (name, s) in document.structs() {
        items.push(CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::STRUCT),
            detail: Some(format!("struct {name}")),
            documentation: provide_struct_documentation(s, &root).and_then(make_md_docs),
            ..Default::default()
        });

        if let Some(ty) = s.ty()
            && let Some(struct_ty) = ty.as_struct()
        {
            let members = struct_ty.members();
            if !members.is_empty() {
                let (label, snippet) = build_struct_snippet(name, members);

                items.push(CompletionItem {
                    label,
                    kind: Some(CompletionItemKind::SNIPPET),
                    detail: Some(format!("struct {} with members", name)),
                    documentation: provide_struct_documentation(s, &root).and_then(make_md_docs),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    insert_text: Some(snippet),
                    ..Default::default()
                });
            }
        }
    }
}

/// Adds completions for imported namespaces (aliases).
fn add_namespace_completions(document: &Document, items: &mut Vec<CompletionItem>) {
    for (name, _) in document.namespaces() {
        items.push(CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::MODULE),
            detail: Some(format!("import alias {name}")),
            ..Default::default()
        });
    }
}

/// Adds completions for the members of the implicit `task` variable.
fn add_task_variable_completions(items: &mut Vec<CompletionItem>) {
    for (key, desc) in TASK_FIELDS {
        if let Some(ty) = task_member_type(key) {
            items.push(CompletionItem {
                label: key.to_string(),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some(ty.to_string()),
                documentation: make_md_docs(desc.to_string()),
                ..Default::default()
            });
        }
    }
}

/// Adds completions for `runtime` section keys.
fn add_runtime_key_completions(version: Option<SupportedVersion>, items: &mut Vec<CompletionItem>) {
    for (key, desc) in RUNTIME_KEYS {
        let ty = version
            .and_then(|v| task_requirement_types(v, key))
            .map(|types| {
                types
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" | ")
            });

        items.push(CompletionItem {
            label: key.to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: ty,
            documentation: make_md_docs(desc.to_string()),
            ..Default::default()
        });
    }
}

/// Adds completions for `requirements` section keys.
fn add_requirements_key_completions(
    version: Option<SupportedVersion>,
    items: &mut Vec<CompletionItem>,
) {
    for (key, desc) in REQUIREMENTS_KEY {
        let ty = version
            .and_then(|v| task_requirement_types(v, key))
            .map(|types| {
                types
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" | ")
            });

        items.push(CompletionItem {
            label: key.to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: ty,
            documentation: make_md_docs(desc.to_string()),
            ..Default::default()
        });
    }
}

/// Adds completions for `task hints` section keys.
fn add_task_hints_key_completions(
    version: Option<SupportedVersion>,
    items: &mut Vec<CompletionItem>,
) {
    for (key, desc) in TASK_HINT_KEYS {
        let ty = version
            .and_then(|v| task_hint_types(v, key, false))
            .map(|types| {
                types
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" | ")
            });

        items.push(CompletionItem {
            label: key.to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: ty,
            documentation: make_md_docs(desc.to_string()),
            ..Default::default()
        });
    }
}

/// Adds completions for `workflow hints` section keys.
fn add_workflow_hints_key_completions(items: &mut Vec<CompletionItem>) {
    for (key, desc) in WORKFLOW_HINT_KEYS {
        items.push(CompletionItem {
            label: key.to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            documentation: make_md_docs(desc.to_string()),
            ..Default::default()
        });
    }
}

/// Adds completions for WDL versions.
fn add_version_completions(
    token_at_cursor: &SyntaxToken,
    lines: &Arc<LineIndex>,
    items: &mut Vec<CompletionItem>,
) -> Result<()> {
    let replacement_range =
        if token_at_cursor.kind() == VersionStatementToken::Version.into_syntax() {
            let text_range = token_at_cursor.text_range();
            Some(Range {
                start: position(lines, text_range.start())?,
                end: position(lines, text_range.end())?,
            })
        } else {
            None
        };

    for version in SupportedVersion::all() {
        items.push(CompletionItem {
            label: version.to_string(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            detail: Some("WDL version".to_string()),
            text_edit: replacement_range.map(|range| {
                CompletionTextEdit::Edit(TextEdit {
                    range,
                    new_text: version.to_string(),
                })
            }),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            ..Default::default()
        });
    }
    Ok(())
}

/// Generates completion items for snippets based on the current node.
fn add_snippet_completions(
    document: &Document,
    node: &SyntaxNode,
    items: &mut Vec<CompletionItem>,
) {
    for s in &*snippets::SNIPPETS {
        if s.contexts.contains(&node.kind()) {
            let insert_text = if s.label == "#@ except:" {
                let all_rules = document.config().all_rules().join(",");
                format!("#@ except: ${{1|{}|}}", all_rules)
            } else {
                s.insert_text.to_owned()
            };
            items.push(CompletionItem {
                label: s.label.to_string(),
                kind: Some(CompletionItemKind::SNIPPET),
                detail: Some(s.detail.to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                insert_text: Some(insert_text),
                ..Default::default()
            });
        }
    }
}

/// Builds a snippet for a `call` statement with required inputs.
///
/// NOTE: skips all optional and default inputs.
fn build_call_snippet(
    name: &str,
    inputs: &IndexMap<String, crate::document::Input>,
    version: Option<SupportedVersion>,
) -> String {
    let required_inputs: Vec<_> = inputs
        .iter()
        .filter(|(_, input)| input.required())
        .map(|(name, _)| name)
        .collect();

    if required_inputs.is_empty() {
        return format!("{} {{\n\t$0\n}}", name);
    }

    let use_input_block = version < Some(SupportedVersion::V1(wdl_ast::version::V1::Two));
    let indent = if use_input_block { "\t\t" } else { "\t" };

    let input_snippets: Vec<_> = required_inputs
        .iter()
        .enumerate()
        .map(|(i, input_name)| format!("{}{} = ${{{}}}", indent, input_name, i + 1))
        .collect();

    if use_input_block {
        format!("{} {{\n\tinput:\n{}\n}}", name, input_snippets.join("\n"))
    } else {
        format!("{} {{\n{}\n}}", name, input_snippets.join("\n"))
    }
}

/// Builds a snippet for a `struct` with its members.
fn build_struct_snippet(name: &str, members: &IndexMap<String, Type>) -> (String, String) {
    let member_names = members
        .keys()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let label = format!("{} {{ {} }}", name, member_names);

    let member_snippets: Vec<String> = members
        .keys()
        .enumerate()
        .map(|(i, member_name)| format!("\t{}: ${{{}}}", member_name, i + 1))
        .collect();
    let snippet = format!("{} {{\n{}\n}}", name, member_snippets.join(",\n"));
    (label, snippet)
}

fn build_function_snippet(name: &str, sig: &crate::stdlib::FunctionSignature) -> String {
    if sig.parameters().is_empty() {
        return format!("{name}()");
    }

    let params: String = sig
        .parameters()
        .iter()
        .enumerate()
        .map(|(i, p)| format!("${{{}:{}}}", i + 1, p.name()))
        .collect::<Vec<_>>()
        .join(", ");

    format!("{}{}", name, params)
}

/// Formats metadata value to type.
fn format_ty(value: MetadataValue) -> &'static str {
    match value {
        MetadataValue::Boolean(_) => "Boolean",
        MetadataValue::Integer(_) => "Int",
        MetadataValue::Float(_) => "Float",
        MetadataValue::String(_) => "String",
        MetadataValue::Null(_) => "Null",
        MetadataValue::Object(_) => "Object",
        MetadataValue::Array(_) => "Array",
    }
}
