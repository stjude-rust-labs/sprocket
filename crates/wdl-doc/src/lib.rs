//! Library for generating HTML documentation from WDL files.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(rustdoc::broken_intra_doc_links)]
#![recursion_limit = "512"]

pub mod parameter;
pub mod r#struct;
pub mod task;
pub mod workflow;

use std::collections::HashMap;
use std::fmt::Display;
use std::path::PathBuf;

use anyhow::Result;
use anyhow::anyhow;
use html::content;
use html::text_content;
use pulldown_cmark::Options;
use pulldown_cmark::Parser;
use tokio::io::AsyncWriteExt;
use wdl_analysis::Analyzer;
use wdl_analysis::rules;
use wdl_ast::AstToken;
use wdl_ast::Document as AstDocument;
use wdl_ast::SyntaxTokenExt;
use wdl_ast::Version;
use wdl_ast::v1::DocumentItem;
use wdl_ast::v1::MetadataValue;

/// The directory where the generated documentation will be stored.
///
/// This directory will be created in the workspace directory.
const DOCS_DIR: &str = "docs";

/// A WDL document.
#[derive(Debug)]
pub struct Document {
    /// The name of the document.
    ///
    /// This is the filename of the document without the extension.
    name: String,
    /// The version of the document.
    version: Version,
    /// The Markdown preamble comments.
    preamble: String,
}

impl Document {
    /// Create a new document.
    pub fn new(name: String, version: Version, preamble: String) -> Self {
        Self {
            name,
            version,
            preamble,
        }
    }

    /// Get the name of the document.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the version of the document.
    pub fn version(&self) -> String {
        self.version.as_str().to_owned()
    }

    /// Get the preamble comments of the document.
    pub fn preamble(&self) -> &str {
        &self.preamble
    }
}

impl Display for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let document_name = content::Heading1::builder()
            .text(self.name().to_owned())
            .build();
        let version = text_content::Paragraph::builder()
            .text(format!("WDL Version: {}", self.version()))
            .build();

        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        let parser = Parser::new_ext(&self.preamble, options);
        let mut preamble = String::new();
        pulldown_cmark::html::push_html(&mut preamble, parser);

        write!(f, "{}", document_name)?;
        write!(f, "{}", version)?;
        write!(f, "{}", preamble)
    }
}

/// Fetch the preamble comments from a document.
pub fn fetch_preamble_comments(document: AstDocument) -> String {
    let comments = match document.version_statement() {
        Some(version) => {
            let comments = version
                .keyword()
                .syntax()
                .preceding_trivia()
                .map(|t| match t.kind() {
                    wdl_ast::SyntaxKind::Comment => match t.to_string().strip_prefix("## ") {
                        Some(comment) => comment.to_string(),
                        None => "".to_string(),
                    },
                    wdl_ast::SyntaxKind::Whitespace => "".to_string(),
                    _ => {
                        panic!("Unexpected token kind: {:?}", t.kind())
                    }
                })
                .collect::<Vec<_>>();
            comments
        }
        None => {
            vec![]
        }
    }
    .join("\n");
    comments
}

/// Generate HTML documentation for a workspace.
pub async fn document_workspace(path: PathBuf) -> Result<()> {
    if !path.is_dir() {
        return Err(anyhow!("The path is not a directory"));
    }

    let abs_path = std::path::absolute(&path)?;

    let docs_dir = abs_path.clone().join(DOCS_DIR);
    if !docs_dir.exists() {
        std::fs::create_dir(&docs_dir)?;
    }

    let analyzer = Analyzer::new(rules(), |_: (), _, _, _| async {});
    analyzer.add_directory(abs_path.clone()).await?;
    let results = analyzer.analyze(()).await?;

    for result in results {
        let cur_path = result
            .document()
            .uri()
            .to_file_path()
            .expect("URI should have a file path");
        let relative_path = match cur_path.strip_prefix(&abs_path) {
            Ok(path) => path,
            Err(_) => &PathBuf::from("external").join(cur_path.strip_prefix("/").unwrap()),
        };
        let cur_dir = docs_dir.join(relative_path.with_extension(""));
        if !cur_dir.exists() {
            std::fs::create_dir_all(&cur_dir)?;
        }
        let name = cur_dir
            .file_name()
            .expect("current directory should have a file name")
            .to_string_lossy();
        let ast_doc = result.document().node();
        let version = ast_doc
            .version_statement()
            .expect("document should have a version statement")
            .version();
        let preamble = fetch_preamble_comments(ast_doc.clone());
        let ast = ast_doc.ast().unwrap_v1();

        let document = Document::new(name.to_string(), version, preamble);

        let index = cur_dir.join("index.html");
        let mut index = tokio::fs::File::create(index).await?;

        index.write_all(document.to_string().as_bytes()).await?;

        for item in ast.items() {
            match item {
                DocumentItem::Struct(s) => {
                    let struct_name = s.name().as_str().to_owned();
                    let struct_file = cur_dir.join(format!("{}-struct.html", struct_name));
                    let mut struct_file = tokio::fs::File::create(struct_file).await?;

                    let r#struct = r#struct::Struct::new(s.clone());
                    struct_file
                        .write_all(r#struct.to_string().as_bytes())
                        .await?;
                }
                DocumentItem::Task(t) => {
                    let task_name = t.name().as_str().to_owned();
                    let task_file = cur_dir.join(format!("{}-task.html", task_name));
                    let mut task_file = tokio::fs::File::create(task_file).await?;

                    let parameter_meta: HashMap<String, MetadataValue> = t
                        .parameter_metadata()
                        .into_iter()
                        .flat_map(|p| p.items())
                        .map(|p| {
                            let name = p.name().as_str().to_owned();
                            let item = p.value();
                            (name, item)
                        })
                        .collect();

                    let meta: HashMap<String, MetadataValue> = t
                        .metadata()
                        .into_iter()
                        .flat_map(|m| m.items())
                        .map(|m| {
                            let name = m.name().as_str().to_owned();
                            let item = m.value();
                            (name, item)
                        })
                        .collect();

                    let output_meta: HashMap<String, MetadataValue> = meta
                        .get("outputs")
                        .cloned()
                        .into_iter()
                        .flat_map(|v| v.unwrap_object().items())
                        .map(|m| {
                            let name = m.name().as_str().to_owned();
                            let item = m.value();
                            (name, item)
                        })
                        .collect();

                    let inputs = t
                        .input()
                        .into_iter()
                        .flat_map(|i| i.declarations())
                        .map(|decl| {
                            let name = decl.name().as_str().to_owned();
                            let meta = parameter_meta.get(&name);
                            parameter::Parameter::new(decl.clone(), meta.cloned())
                        })
                        .collect();

                    let outputs = t
                        .output()
                        .into_iter()
                        .flat_map(|o| o.declarations())
                        .map(|decl| {
                            let name = decl.name().as_str().to_owned();
                            let meta = output_meta.get(&name);
                            parameter::Parameter::new(
                                wdl_ast::v1::Decl::Bound(decl.clone()),
                                meta.cloned(),
                            )
                        })
                        .collect();

                    let task = task::Task::new(task_name, t.metadata(), inputs, outputs);

                    task_file.write_all(task.to_string().as_bytes()).await?;
                }
                DocumentItem::Workflow(w) => {
                    let workflow_name = w.name().as_str().to_owned();
                    let workflow_file = cur_dir.join(format!("{}-workflow.html", workflow_name));
                    let mut workflow_file = tokio::fs::File::create(workflow_file).await?;

                    let parameter_meta: HashMap<String, MetadataValue> = w
                        .parameter_metadata()
                        .into_iter()
                        .flat_map(|p| p.items())
                        .map(|p| {
                            let name = p.name().as_str().to_owned();
                            let item = p.value();
                            (name, item)
                        })
                        .collect();

                    let meta: HashMap<String, MetadataValue> = w
                        .metadata()
                        .into_iter()
                        .flat_map(|m| m.items())
                        .map(|m| {
                            let name = m.name().as_str().to_owned();
                            let item = m.value();
                            (name, item)
                        })
                        .collect();

                    let output_meta: HashMap<String, MetadataValue> = meta
                        .get("outputs")
                        .cloned()
                        .into_iter()
                        .flat_map(|v| v.unwrap_object().items())
                        .map(|m| {
                            let name = m.name().as_str().to_owned();
                            let item = m.value();
                            (name, item)
                        })
                        .collect();

                    let inputs = w
                        .input()
                        .into_iter()
                        .flat_map(|i| i.declarations())
                        .map(|decl| {
                            let name = decl.name().as_str().to_owned();
                            let meta = parameter_meta.get(&name);
                            parameter::Parameter::new(decl.clone(), meta.cloned())
                        })
                        .collect();

                    let outputs = w
                        .output()
                        .into_iter()
                        .flat_map(|o| o.declarations())
                        .map(|decl| {
                            let name = decl.name().as_str().to_owned();
                            let meta = output_meta.get(&name);
                            parameter::Parameter::new(
                                wdl_ast::v1::Decl::Bound(decl.clone()),
                                meta.cloned(),
                            )
                        })
                        .collect();

                    let workflow =
                        workflow::Workflow::new(workflow_name, w.metadata(), inputs, outputs);

                    workflow_file
                        .write_all(workflow.to_string().as_bytes())
                        .await?;
                }
                DocumentItem::Import(_) => {}
            }
        }
    }
    anyhow::Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetch_preamble_comments() {
        let source = r#"
        ## This is a comment
        ## This is also a comment
        version 1.0
        workflow test {
            input {
                String name
            }
            output {
                String greeting = "Hello, ${name}!"
            }
            call say_hello as say_hello {
                input:
                    name = name
            }
        }
        "#;
        let (document, _) = AstDocument::parse(source);
        let preamble = fetch_preamble_comments(document);
        assert_eq!(preamble, "This is a comment\nThis is also a comment");
    }
}
