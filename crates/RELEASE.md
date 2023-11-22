# Release

The release process for the `wdl` family of crates is intentionally disjoint
across the repository. When releasing any new packages:

* You should first release each of the component crates (all crates that are not
`wdl`) in a sequential fashion using the [component crates](#component-crates)
section.
* Next (if desired), you should release the convenience crate (the `wdl` crate)
  by following the [convenience crate](#convenience-crate) section.

Notably, updates to the files listed below should be grouped in a single commit
**per crate**, but updates to these files across crates should not be contained
within a single commit.

## Component Crates

**Note:** in this example, we will be using the `wdl-grammar` crate. Please
substitute the name of the crate that you are working on.

For every component crate that has changes:

  * [ ] Update version in `Cargo.toml`.
  * [ ] Update `CHANGELOG.md` with version and publication date.
  * [ ] Run tests: `cargo test --all-features`.
  * [ ] Run linting: `cargo clippy --all-features`.
  * [ ] Run fmt: `cargo fmt --check`.
  * [ ] Run doc: `cargo doc`.
  * [ ] Stage changes: `git add Cargo.lock Cargo.toml CHANGELOG.md`.
  * [ ] Create git commit:
    ```
    git commit -m "release: bumps `wdl-grammar` version to v0.1.0"
    ```
  * [ ] Create git tag:
    ```
    git tag wdl-grammar-v0.1.0
    ```
  * [ ] Push release: `git push && git push --tags`.
  * [ ] Publish the component crate: `cargo publish --all-features`.
  * [ ] Go to the Releases page in Github, create a Release for this tag, and
    copy the notes from the `CHANGELOG.md` file.

## Convenience Crate

From the root directory:

  * [ ] Update the version of the top-level crate and all component crates in
    the root `Cargo.toml`.
    * **Note:** changes to the version number will be automatically reflected in
    `wdl/Cargo.toml`, as the version there is specified as `version.workspace =
    true`.
  * [ ] Run tests: `cargo test --all-features`.
  * [ ] Run linting: `cargo clippy --all-features`.
  * [ ] Run fmt: `cargo fmt --check`.
  * [ ] Run doc: `cargo doc`.
  * [ ] Stage changes: `git add Cargo.lock Cargo.toml`.
  * [ ] Create git commit:
    ```
    git commit
    ```

    The commit message should have a body conforming to this style:

    ```
    release: bumps `wdl` version to v0.1.0

    ## Component Crate Updates

    * `wdl-grammar`: bumped from v0.1.0 to v0.2.0
    ```
  * [ ] Create git tag: `git tag wdl-v0.1.0`.
  * [ ] Push release: `git push && git push --tags`.
  * [ ] Publish the new crate: `cargo publish --all-features`.
  * [ ] Go to the Releases page in Github, create a Release for this tag, and
    copy the body from the commit message that describes the package version
    updates. 