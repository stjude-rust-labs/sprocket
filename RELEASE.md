# Release Process

## Between Releases

Various CI features have been implemented to ease the release process, but some parts of it are still intended to be manual (e.g., each CHANGELOG for each individual crate should be written by hand _prior to release_). We subscribe to the philosophy outlined on the [keepachangelog site](https://keepachangelog.com/en/1.1.0/). The short version is that (almost) every PR should include a manually written entry to one or more CHANGELOGs in the repository under the `## Unreleased` header.

## Time to Release!

The following steps are handled automatically by the [release-plz](./.github/workflows/release-plz.yml) workflow.
In the event it fails, they can be performed manually.

<details>
<summary>Manual release steps</summary>

1. Go through each publishable crate (i.e., each `wdl-*` crate, `wdl`, and `sprocket`) and increment the version in `Cargo.toml` (as well as match any internal dependency versions that need to be bumped).
2. Update each CHANGELOG.md file with a new release header.
3. Create a new tag for each new crate version _excluding_ `sprocket`, with the format `{CRATE_NAME}-v{VERSION}` (where `VERSION` matches the latest version in the root `Cargo.toml`)
    * For new `sprocket` releases, the tag name format is `v{VERSION}`

    ```bash
    git tag {CRATE_NAME}-v{VERSION}
    git push --tags
    ```
4. Publish each crate to [crates.io](https://crates.io)

    ```bash
    cargo publish --workspace
    ```
5. If updating `sprocket`, create a new GitHub release with the title `v{VERSION}` and mark it as the latest release
</details>

The body of the `sprocket` GitHub releases must be updated manually, regardless of the success of the `release-plz` workflow.
By default, the release will only include the changelog of the `sprocket` crate. Each crate's most recent CHANGELOG entries should be copy and pasted into the release notes.
These should be ordered topologically (starting with `wdl-grammar`, ending with `sprocket` itself if that had non-dependency changes).

Format each section so that it looks like:

```
### `<crate name>`

<copy and pasted CHANGELOG entries>
```

## Post-Release

After the release is complete, the following tasks should be performed:

- [ ] Merge the `next` branch in [`stjude-rust-labs/sprocket.bio`](https://github.com/stjude-rust-labs/sprocket.bio) if there are any pending documentation changes.
- [ ] Update the Sprocket version in [`stjude-rust-labs/sprocket-action`](https://github.com/stjude-rust-labs/sprocket-action).
- [ ] Merge any changes needed in [`stjude-rust-labs/sprocket-vscode`](https://github.com/stjude-rust-labs/sprocket-vscode).
- [ ] Release the latest version on the St. Jude HPC module system.
- [ ] Update the official WDL documentation for the Sprocket entries if anything changed.
- [ ] Post a message to Slack channels with the updated version.
