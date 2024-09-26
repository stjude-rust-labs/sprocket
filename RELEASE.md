# Release

  * [ ] Update version in `Cargo.toml`.
  * [ ] Update `CHANGELOG.md` with version and publication date.
  * [ ] Update the [docs site](https://stjude-rust-labs.github.io/sprocket/) to the new version ([here](https://github.com/stjude-rust-labs/sprocket/blob/main/docs/.vitepress/config.mts#L17))
    * To get the changes to the crate since the last release, you can use a
      command like the following:
      ```bash
      git log sprocket-v0.1.0..HEAD --oneline
      ```
  * [ ] Run tests: `cargo test --all-features`.
  * [ ] Run linting: `cargo clippy --all-features`.
  * [ ] Run fmt: `cargo fmt --check`.
  * [ ] Run doc: `cargo doc`.
  * [ ] Stage changes: `git add Cargo.toml CHANGELOG.md`.
  * [ ] Create git commit:
    ```
    git commit -m "release: bumps `sprocket` version to v0.1.0"
    ```
  * [ ] Create git tag:
    ```
    git tag sprocket-v0.1.0
    ```
  * [ ] Push release: `git push && git push --tags`.
  * [ ] Go to the Releases page in Github, create a Release for this tag, and
    copy the notes from the `CHANGELOG.md` file.