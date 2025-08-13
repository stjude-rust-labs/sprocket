# Release

* [ ] Update version in `Cargo.toml`.
* [ ] Run tests: `cargo test --all-features`.
* [ ] Run linting: `cargo clippy --all-features -- -D warnings`.
* [ ] Run fmt: `cargo +nightly fmt --check`.
* [ ] Run doc: `cargo doc`.
* [ ] Run `cargo update`.
* [ ] Update any references in the `README.md` to the new version.
* [ ] Update the [docs site](https://stjude-rust-labs.github.io/sprocket/) to the new version
  * [in the VitePress configuration file](https://github.com/stjude-rust-labs/sprocket.bio/blob/main/.vitepress/config.mts#L19), and
  * [on the installation page of the documentation](https://github.com/stjude-rust-labs/sprocket.bio/blob/main/installation.md).
* [ ] Update `CHANGELOG.md` with version and publication date.
* [ ] Create git commit:
  ```
  git commit -m "release: bumps `sprocket` version to v0.1.0"
  ```
* [ ] Create git tag for the HEAD commit where `{VERSION}` is the new version being released.
  ```
  git tag v{VERSION}
  ```
* [ ] Push the tag: `git push --tags`.

And the CI should handle the rest!