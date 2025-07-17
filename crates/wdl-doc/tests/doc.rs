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
use std::path::Path;
use std::path::PathBuf;

use fs_extra::dir::CopyOptions;
use fs_extra::dir::copy;
use pretty_assertions::StrComparison;
use wdl_doc::document_workspace;

/// Recursively read every file in a directory
fn read_dir_recursively(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(read_dir_recursively(&path)?);
        } else {
            files.push(path);
        }
    }
    Ok(files)
}

#[tokio::test]
async fn document_full_codebase() {
    let test_dir = Path::new("tests").join("codebase");
    let docs_dir = Path::new("tests").join("output_docs");

    // If `tests/codebase/docs` exists, delete it
    if test_dir.join("docs").exists() {
        fs::remove_dir_all(test_dir.join("docs")).unwrap();
    }

    document_workspace(
        test_dir.to_path_buf(),
        test_dir.join("docs"),
        None::<&str>,
        None::<&str>,
    )
    .await
    .expect("failed to generate docs");

    // If the `BLESS` environment variable is set, update the expected output
    // by deleting the contents of the `tests/output_docs` directory and
    // repopulating it with the generated docs (at `tests/codebase/docs/`).
    if env::var("BLESS").is_ok() {
        if docs_dir.exists() {
            fs::remove_dir_all(&docs_dir).unwrap();
        }
        fs::create_dir_all(&docs_dir).unwrap();

        let options = CopyOptions::new().content_only(true);
        copy(test_dir.join("docs"), &docs_dir, &options).unwrap();

        return;
    }

    // Compare the generated docs with the expected output.
    // Recursively read the contents of the `tests/codebase/docs` directory
    // and compare them with the contents of the `tests/output_docs` directory.
    // If the contents are different, print the differences and exit with a
    // non-zero exit code.
    let mut failed = false;
    for file_name in read_dir_recursively(&test_dir.join("docs")).unwrap() {
        let expected_file = docs_dir.join(file_name.strip_prefix(test_dir.join("docs")).unwrap());
        if !expected_file.exists() {
            println!("missing file: {}", expected_file.display());
            failed = true;
            continue;
        }

        if expected_file.extension().and_then(|e| e.to_str()) == Some("svg") {
            // Ignore image files
            continue;
        }

        // TODO: snapshotting the HTML/CSS files is not a good test,
        // In the future, we should check out a better test framework for this.
        // Potential lead: https://github.com/Vrtgs/thirtyfour

        let expected_contents = fs::read_to_string(&expected_file)
            .unwrap()
            .replace("\\", "/")
            // serde-json pre-escapes some of the HTML paths resulting in double
            // slashes for some content during normalization.
            .replace("//", "/");
        let generated_contents = fs::read_to_string(&file_name)
            .unwrap()
            .replace("\r\n", "\n")
            .replace("\\", "/")
            .replace("//", "/");

        if expected_contents != generated_contents {
            println!("File contents differ: {}", expected_file.display());
            println!(
                "Diff:\n{}",
                StrComparison::new(&expected_contents, &generated_contents)
            );
            failed = true;
        }
    }

    if failed {
        panic!("test failed");
    }
}
