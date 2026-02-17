//! Source validation and security tests.

use anyhow::Result;
use sprocket::ServerConfig;
use sprocket::analysis::Source;
use sprocket::system::v1::exec::ConfigError;
use sprocket::system::v1::exec::validate_source;
use tempfile::TempDir;

fn make_config(
    output_directory: std::path::PathBuf,
    allowed_file_paths: Vec<std::path::PathBuf>,
    allowed_urls: Vec<String>,
) -> ServerConfig {
    ServerConfig {
        output_directory,
        allowed_file_paths,
        allowed_urls,
        ..Default::default()
    }
}

#[test]
fn validate_file_in_allowed_directory() -> Result<()> {
    let temp = TempDir::new()?;
    let temp_path = temp.path();

    let file = temp_path.join("workflow.wdl");
    std::fs::write(&file, "version 1.2")?;

    let mut config = make_config(
        temp_path.to_path_buf(),
        vec![temp_path.to_path_buf()],
        vec![],
    );
    config.validate()?;

    let source = validate_source(file.to_str().unwrap(), &config)?;
    assert!(matches!(source, Source::File(_)));

    Ok(())
}

#[test]
fn validate_file_outside_allowed_directory() -> Result<()> {
    let temp = TempDir::new()?;
    let allowed_dir = temp.path().join("allowed");
    let outside_dir = temp.path().join("outside");

    std::fs::create_dir(&allowed_dir)?;
    std::fs::create_dir(&outside_dir)?;

    let file = outside_dir.join("workflow.wdl");
    std::fs::write(&file, "version 1.2")?;

    let mut config = make_config(temp.path().to_path_buf(), vec![allowed_dir], vec![]);
    config.validate()?;

    let result = validate_source(file.to_str().unwrap(), &config);
    assert!(matches!(result, Err(ConfigError::FilePathForbidden(_))));

    Ok(())
}

#[test]
fn validate_file_with_path_traversal() -> Result<()> {
    let temp = TempDir::new()?;
    let allowed_dir = temp.path().join("allowed");
    std::fs::create_dir(&allowed_dir)?;

    // Create a file outside the allowed directory
    let outside_file = temp.path().join("secret.wdl");
    std::fs::write(&outside_file, "version 1.2")?;

    let mut config = make_config(temp.path().to_path_buf(), vec![allowed_dir.clone()], vec![]);
    config.validate()?;

    // Try to access file outside allowed directory using `..`
    let traversal_path = allowed_dir.join("..").join("secret.wdl");
    let result = validate_source(traversal_path.to_str().unwrap(), &config);

    // Should be rejected as forbidden (canonicalization resolves `..` and checks)
    assert!(matches!(result, Err(ConfigError::FilePathForbidden(_))));

    Ok(())
}

#[test]
fn validate_file_with_symlink_escape() -> Result<()> {
    let temp = TempDir::new()?;
    let allowed_dir = temp.path().join("allowed");
    let outside_dir = temp.path().join("outside");

    std::fs::create_dir(&allowed_dir)?;
    std::fs::create_dir(&outside_dir)?;

    let outside_file = outside_dir.join("secret.wdl");
    std::fs::write(&outside_file, "version 1.2")?;

    let symlink = allowed_dir.join("link.wdl");

    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside_file, &symlink)?;

    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&outside_file, &symlink)?;

    let mut config = make_config(temp.path().to_path_buf(), vec![allowed_dir], vec![]);
    config.validate()?;

    // Symlink is inside allowed dir, but points outside
    let result = validate_source(symlink.to_str().unwrap(), &config);

    // Should be rejected because canonical path is outside allowed
    assert!(matches!(result, Err(ConfigError::FilePathForbidden(_))));

    Ok(())
}

#[test]
fn validate_file_not_found_inside_allowed() -> Result<()> {
    let temp = TempDir::new()?;
    let allowed_dir = temp.path().join("allowed");
    std::fs::create_dir(&allowed_dir)?;

    let nonexistent = allowed_dir.join("nonexistent.wdl");

    let mut config = make_config(temp.path().to_path_buf(), vec![allowed_dir], vec![]);
    config.validate()?;

    let result = validate_source(nonexistent.to_str().unwrap(), &config);

    // Should report as not found (not forbidden) because parent is allowed
    assert!(matches!(result, Err(ConfigError::FileNotFound(_))));

    Ok(())
}

#[test]
fn validate_file_not_found_outside_allowed() -> Result<()> {
    let temp = TempDir::new()?;
    let allowed_dir = temp.path().join("allowed");
    let outside_dir = temp.path().join("outside");

    std::fs::create_dir(&allowed_dir)?;
    std::fs::create_dir(&outside_dir)?;

    let nonexistent = outside_dir.join("nonexistent.wdl");

    let mut config = make_config(temp.path().to_path_buf(), vec![allowed_dir], vec![]);
    config.validate()?;

    let result = validate_source(nonexistent.to_str().unwrap(), &config);

    // Should report as forbidden (not reveal file existence outside allowed)
    assert!(matches!(result, Err(ConfigError::FilePathForbidden(_))));

    Ok(())
}

#[test]
fn validate_file_with_unicode_and_spaces() -> Result<()> {
    let temp = TempDir::new()?;
    let temp_path = temp.path();

    // Test emoji filename
    let emoji_file = temp_path.join("🚀workflow.wdl");
    std::fs::write(&emoji_file, "version 1.2")?;

    // Test spaces in filename
    let spaces_file = temp_path.join("my workflow file.wdl");
    std::fs::write(&spaces_file, "version 1.2")?;

    let mut config = make_config(
        temp_path.to_path_buf(),
        vec![temp_path.to_path_buf()],
        vec![],
    );
    config.validate()?;

    // Both should be valid
    let source1 = validate_source(emoji_file.to_str().unwrap(), &config)?;
    assert!(matches!(source1, Source::File(_)));

    let source2 = validate_source(spaces_file.to_str().unwrap(), &config)?;
    assert!(matches!(source2, Source::File(_)));

    Ok(())
}

#[test]
fn validate_url_with_prefix_matching() {
    let mut config = make_config(
        "./out".into(),
        vec![],
        vec!["https://example.com/".to_string()],
    );
    config.validate().unwrap();

    // Exact prefix match
    let source1 = validate_source("https://example.com/workflow.wdl", &config).unwrap();
    assert!(matches!(source1, Source::Url(_)));

    // Extended path
    let source2 = validate_source("https://example.com/path/to/workflow.wdl", &config).unwrap();
    assert!(matches!(source2, Source::Url(_)));

    // Query parameters
    let source3 = validate_source("https://example.com/workflow.wdl?version=1", &config).unwrap();
    assert!(matches!(source3, Source::Url(_)));
}

#[test]
fn validate_url_without_allowed_prefix() {
    let mut config = make_config(
        "./out".into(),
        vec![],
        vec!["https://example.com/".to_string()],
    );
    config.validate().unwrap();

    // Different domain
    let result = validate_source("https://different.com/workflow.wdl", &config);
    assert!(matches!(result, Err(ConfigError::UrlForbidden(_))));
}

#[test]
fn validate_url_scheme_must_match() {
    let mut config = make_config(
        "./out".into(),
        vec![],
        vec!["https://example.com/".to_string()],
    );
    config.validate().unwrap();

    // HTTP vs HTTPS
    let result = validate_source("http://example.com/workflow.wdl", &config);
    assert!(matches!(result, Err(ConfigError::UrlForbidden(_))));
}

#[test]
fn validate_url_subdomain_must_match() {
    let mut config = make_config(
        "./out".into(),
        vec![],
        vec!["https://example.com/".to_string()],
    );
    config.validate().unwrap();

    // Subdomain should not match
    let result = validate_source("https://sub.example.com/workflow.wdl", &config);
    assert!(matches!(result, Err(ConfigError::UrlForbidden(_))));
}

#[test]
fn validate_url_with_port_and_special_formats() {
    let mut config = make_config(
        "./out".into(),
        vec![],
        vec![
            "http://localhost:8080/".to_string(),
            "http://192.168.1.1/".to_string(),
            "http://[::1]/".to_string(),
        ],
    );
    config.validate().unwrap();

    let source1 = validate_source("http://localhost:8080/workflow.wdl", &config).unwrap();
    assert!(matches!(source1, Source::Url(_)));

    let source2 = validate_source("http://192.168.1.1/workflow.wdl", &config).unwrap();
    assert!(matches!(source2, Source::Url(_)));

    let source3 = validate_source("http://[::1]/workflow.wdl", &config).unwrap();
    assert!(matches!(source3, Source::Url(_)));
}

#[test]
fn validate_url_with_unicode_and_encoding() {
    let mut config = make_config(
        "./out".into(),
        vec![],
        vec!["https://example.com/".to_string()],
    );
    config.validate().unwrap();

    // URL-encoded characters
    let source1 = validate_source("https://example.com/my%20workflow.wdl", &config).unwrap();
    assert!(matches!(source1, Source::Url(_)));

    // Emoji in path (gets automatically encoded by Url::parse)
    let source2 = validate_source("https://example.com/🚀workflow.wdl", &config).unwrap();
    assert!(matches!(source2, Source::Url(_)));
}

#[test]
fn source_to_url_and_display() -> Result<()> {
    let temp = TempDir::new()?;
    let temp_path = temp.path();

    let file = temp_path.join("workflow.wdl");
    std::fs::write(&file, "version 1.2")?;

    let mut config = make_config(
        temp_path.to_path_buf(),
        vec![temp_path.to_path_buf()],
        vec!["https://example.com/".to_string()],
    );
    config.validate()?;

    // Test file source
    let file_source = validate_source(file.to_str().unwrap(), &config)?;
    let file_url = file_source.to_url();
    assert_eq!(file_url.scheme(), "file");

    // Test URL source
    let url_source = validate_source("https://example.com/workflow.wdl", &config)?;
    let url = url_source.to_url();
    assert_eq!(url.as_str(), "https://example.com/workflow.wdl");

    // Test Display trait
    assert!(format!("{}", file_source).contains("workflow.wdl"));
    assert_eq!(
        format!("{}", url_source),
        "https://example.com/workflow.wdl"
    );

    Ok(())
}

#[test]
fn validate_source_with_empty_configuration() -> Result<()> {
    let temp = TempDir::new()?;
    let temp_path = temp.path();

    let file = temp_path.join("workflow.wdl");
    std::fs::write(&file, "version 1.2")?;

    let mut config = make_config(temp_path.to_path_buf(), vec![], vec![]);
    config.validate()?;

    // File should be forbidden
    let result = validate_source(file.to_str().unwrap(), &config);
    assert!(matches!(result, Err(ConfigError::FilePathForbidden(_))));

    // URL should be forbidden
    let result = validate_source("https://example.com/workflow.wdl", &config);
    assert!(matches!(result, Err(ConfigError::UrlForbidden(_))));

    Ok(())
}
