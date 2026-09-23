//! Language server protocol handlers.

pub mod code_lens;
pub mod diagnostic;

/// Get the WDL file path associated with a test definition YAML file.
///
/// A test YAML file *must* have an associated WDL file, otherwise we don't
/// consider it valid.
///
/// See [`is_sprocket_test_file()`](crate::test::is_sprocket_test_file)
pub(crate) fn associated_wdl_file_path(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let base_name = path.file_name()?;
    let expected_wdl = std::path::Path::new(base_name).with_extension("wdl");
    let parent = path.parent()?;

    let sibling_wdl = parent.join(&expected_wdl);
    if sibling_wdl.is_file() {
        return Some(sibling_wdl);
    }

    let in_test_dir =
        parent.is_dir() && parent.file_name().and_then(|s| s.to_str()) == Some("test");
    if !in_test_dir {
        return None;
    }

    let parent = parent.parent()?;
    let associated_wdl_path = parent.join(expected_wdl);
    if !associated_wdl_path.is_file() {
        return None;
    }

    Some(associated_wdl_path)
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use tempfile::tempdir;

    use super::associated_wdl_file_path;

    #[test]
    fn associated_wdl_search() {
        for (wdl_path, yaml_path) in [
            ("foo.wdl", "foo.yaml"),
            ("foo.wdl", "test/foo.yaml"),
            // Make sure we don't get tripped up on WDL files _inside_ test directories
            ("test/foo.wdl", "test/foo.yaml"),
        ] {
            let dir = tempdir().unwrap();
            std::fs::create_dir(dir.path().join("test")).unwrap();

            let expected_wdl_path = dir.path().join(wdl_path);
            File::create(&expected_wdl_path).unwrap();

            let yaml_path = dir.path().join(yaml_path);
            File::create(&yaml_path).unwrap();

            assert_eq!(
                associated_wdl_file_path(&yaml_path).unwrap(),
                expected_wdl_path
            );
        }
    }
}
