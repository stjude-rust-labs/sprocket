//! The wdl-doc tests.
//!
//! This test documents the contents of the `tests/codebase` directory.
//!
//! The built docs are expected to be in the `tests/output_docs` directory.
//!
//! The docs may be automatically generated or updated by
//! setting the `BLESS` environment variable when running this test.

use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::process::exit;

use wdl_doc::document_workspace;

/// Copied from https://stackoverflow.com/a/65192210
fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    #[cfg(not(windows))]
    let test_dir = Path::new("tests/codebase");
    #[cfg(not(windows))]
    let docs_dir = Path::new("tests/output_docs");

    #[cfg(windows)]
    let test_dir = Path::new("tests\\codebase");
    #[cfg(windows)]
    let docs_dir = Path::new("tests\\output_docs");

    // If `tests/codebase/docs` exists, delete it
    if test_dir.join("docs").exists() {
        fs::remove_dir_all(test_dir.join("docs")).unwrap();
    }

    match document_workspace(test_dir.to_path_buf(), None::<&str>, true).await {
        Ok(_) => {
            println!("Successfully generated docs");
        }
        Err(e) => {
            eprintln!("Failed to generate docs: {e}");
            exit(1);
        }
    }

    // If the `BLESS` environment variable is set, update the expected output
    // by deleting the contents of the `tests/output_docs` directory and
    // repopulating it with the generated docs (at `tests/codebase/docs/`).
    if env::var("BLESS").is_ok() {
        if docs_dir.exists() {
            fs::remove_dir_all(docs_dir).unwrap();
        }
        fs::create_dir_all(docs_dir).unwrap();
        copy_dir_all(test_dir.join("docs"), docs_dir).unwrap();

        println!("Blessed docs");
        exit(0);
    }

    // Compare the generated docs with the expected output
    // For now, check that the paths exist as expected.
    // TODO: check HTML content.
    let mut success = true;
    for entry in fs::read_dir(docs_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        // Normalize the path to be relative to the `docs` directory
        // regardless of OS path separator.
        let expected_path = test_dir
            .join("docs")
            .join(path.strip_prefix(docs_dir).unwrap());
        if !expected_path.exists() {
            eprintln!("Expected path does not exist: {}", expected_path.display());
            success = false;
        }
    }

    if success {
        println!("Docs are as expected");
        exit(0);
    } else {
        exit(1);
    }
}
