//! Symlink creation tests.

use std::fs;

use anyhow::Result;
use sprocket::provenance::index::create_or_resymlink;
use tempfile::TempDir;

#[test]
fn create_file_symlink() -> Result<()> {
    let temp = TempDir::new()?;
    let base = temp.path();

    let target = base.join("target.txt");
    fs::write(&target, "test content")?;

    let link_dir = base.join("links");
    fs::create_dir(&link_dir)?;
    let link = link_dir.join("link.txt");

    create_or_resymlink(&link, &target)?;

    assert!(link.exists());
    assert!(link.is_symlink());

    let content = fs::read_to_string(&link)?;
    assert_eq!(content, "test content");

    Ok(())
}

#[test]
fn create_directory_symlink() -> Result<()> {
    let temp = TempDir::new()?;
    let base = temp.path();

    let target_dir = base.join("target_dir");
    fs::create_dir(&target_dir)?;
    fs::write(target_dir.join("file.txt"), "content")?;

    let link_dir = base.join("links");
    fs::create_dir(&link_dir)?;
    let link = link_dir.join("dir_link");

    create_or_resymlink(&link, &target_dir)?;

    assert!(link.exists());
    assert!(link.is_symlink());

    let file_through_link = link.join("file.txt");
    assert!(file_through_link.exists());
    let content = fs::read_to_string(&file_through_link)?;
    assert_eq!(content, "content");

    Ok(())
}
