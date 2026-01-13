# Release Process

## Between Releases

Various CI features have been implemented to ease the release process, but some parts of it are still intended to be manual (e.g., each CHANGELOG for each individual crate should be written by hand _prior to release_). We subscribe to the philosophy outlined on the [keepachangelog site](https://keepachangelog.com/en/1.1.0/). The short version is that (almost) every PR should include a manually written entry to one or more CHANGELOGs in the repository under the `## Unreleased` header.

## Time to Release!

The first step in making a release is to run the [`bump` GitHub action](https://github.com/stjude-rust-labs/sprocket/actions/workflows/bump.yml). This action will do two things:

1. Go through each publishable crate (i.e., each `wdl-*` crate, `wdl`, and `sprocket`) and increment the version in `Cargo.toml` (as well as match any internal dependency versions that need to be bumped).
2. Update each CHANGELOG.md file with a new release header.
    * This piece of the `ci` code relies on the line `## Unreleased` being present in the CHANGELOG.md file (see the [`ci` crate code](https://github.com/stjude-rust-labs/sprocket/blob/main/crates/ci/src/main.rs) for details).

Then the `bump` action will open a PR with the above changes for manual review. Please ensure everything looks good and the CI is passing before merging the PR!

Once the bump PR merges, tag the HEAD commit with `v{VERSION}` (where `VERSION` matches the latest version in the root `Cargo.toml`) and push the tag:

```bash
git tag v{VERSION}
git push --tags
```

The CI should handle publishing each crate to crates.io.

Next up is making a GitHub release, which should be done manually. Please review the most recent releases, as we sometimes change the GitHub "Release Notes" formatting. Each crate's most recent CHANGELOG entries should be copy and pasted into the release notes. These should be ordered topologically (starting with `wdl-grammar`, ending with `sprocket` itself if that had non-dependency changes). Format each section so that it looks like:

```
### `<crate name>`

<copy and pasted CHANGELOG entries>
```

Make sure to mark this release as the latest.

## Post-Release

After the release is complete, the following tasks should be performed:

- [ ] Merge the `next` branch in [`stjude-rust-labs/sprocket.bio`](https://github.com/stjude-rust-labs/sprocket.bio) if there are any pending documentation changes.
- [ ] Update the Sprocket version in [`stjude-rust-labs/sprocket-action`](https://github.com/stjude-rust-labs/sprocket-action).
- [ ] Merge any changes needed in [`stjude-rust-labs/sprocket-vscode`](https://github.com/stjude-rust-labs/sprocket-vscode).
- [ ] Release the latest version on the St. Jude HPC module system.
- [ ] Update the official WDL documentation for the Sprocket entries if anything changed.
- [ ] Post a message to Slack channels with the updated version.
