//! Local module project fixtures.

use std::fs;

use super::command::sprocket;

pub(crate) struct ModuleFixture {
    pub(crate) dir: tempfile::TempDir,
}

impl ModuleFixture {
    pub(crate) fn with_local_dep() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let fixture = Self { dir };

        fs::create_dir_all(fixture.consumer()).unwrap();
        fs::create_dir_all(fixture.dep()).unwrap();

        fs::write(
            fixture.consumer().join("module.json"),
            r#"{
  "name": "consumer",
  "license": "MIT",
  "entrypoint": "index.wdl"
}
"#,
        )
        .unwrap();
        fs::write(fixture.consumer().join("index.wdl"), "version 1.3\n").unwrap();

        fs::write(
            fixture.dep().join("module.json"),
            r#"{
  "name": "dep",
  "license": "MIT",
  "entrypoint": "index.wdl"
}
"#,
        )
        .unwrap();
        fs::write(fixture.dep().join("index.wdl"), "version 1.3\n").unwrap();

        fixture
    }

    pub(crate) fn with_local_dep_added() -> Self {
        let fixture = Self::with_local_dep();
        let output = sprocket(&["dev", "module", "add", "utils", "../dep"])
            .current_dir(fixture.consumer())
            .output()
            .expect("failed to run sprocket dev module add");
        assert!(
            output.status.success(),
            "command failed {status}: {stderr}",
            status = output.status,
            stderr = String::from_utf8_lossy(&output.stderr)
        );
        fixture
    }

    pub(crate) fn consumer(&self) -> std::path::PathBuf {
        self.dir.path().join("consumer")
    }

    pub(crate) fn dep(&self) -> std::path::PathBuf {
        self.dir.path().join("dep")
    }
}
