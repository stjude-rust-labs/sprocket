use std::io::ErrorKind;
use std::path::Path;

use globset::Glob;
use walkdir::WalkDir;

/// Generate an initial corpus directory for the target.
///
/// This fills `fuzz/corpus/<target>` with all of the WDL test assets in the
/// repo.
pub fn init_corpus_dir(target: &str) -> std::io::Result<()> {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("corpus")
        .join(target);
    if let Err(e) = std::fs::create_dir_all(&corpus_dir) {
        if e.kind() == ErrorKind::AlreadyExists {
            return Ok(());
        }

        return Err(e);
    }

    let project_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let glob = Glob::new("*.wdl").unwrap().compile_matcher();
    for entry in WalkDir::new(project_root).sort_by_file_name() {
        let entry = entry?;

        if entry.file_type().is_file() && glob.is_match(entry.path()) {
            std::fs::copy(
                entry.path(),
                corpus_dir.join(format!("{}.wdl", uuid::Uuid::new_v4())),
            )?;
        }
    }

    Ok(())
}
