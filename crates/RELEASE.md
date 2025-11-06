# Release Process

## Between Releases

Various CI features have been implemented to ease the release process, but some parts of it are still intended to be manual. e.g. each CHANGELOG for each individual crate should be written by hand _prior to release_. We subscribe to the philosphy outlined on the [keepachangelog site](https://keepachangelog.com/en/1.1.0/). The short version is that (almost) every PR should include a manually written entry to one or more CHANGELOGs in the repository under the `## Unreleased` header.

## Time to Release!

The first step in making a release is to run the [`bump` GitHub action](https://github.com/stjude-rust-labs/wdl/actions/workflows/bump.yml). This action will do two things:

1. Go through each publishable `wdl-*` crate and increment the version in `Cargo.toml` (as well as match any internal dependency versions that need to be bumped).
2. Update each CHANGELOG.md file with a new release header.
    * This piece of the `ci` code relies on the line `## Unreleased` being present in the CHANGELOG.md file (see the [`ci` crate code](https://github.com/stjude-rust-labs/wdl/blob/main/ci/src/main.rs) for details).

Then the `bump` action will open a PR with the above changes for manual review. Please ensure everything looks good and the CI is passing before merging the PR!

Once the bump PR merges, tag the HEAD commit with `wdl-v{VERSION}` (where `VERSION` matches the latest version in `wdl/Cargo.toml`) and the CI should handle publishing each crate to crates.io.

Next up is making a GitHub release, which should be done manually. Please review the most recent releases, as we sometimes change the GitHub "Release Notes" formatting (we're still doing some experimentation here). What follows is what I (a-frantz) have done most recently (`wdl-v0.15.0`), but I would not be shocked if this changes in the future and someone (me) forgets to update this document as the processs is tweaked.

Each crate's most recent CHANGELOG entries should be copy and pasted into the release notes. These should be ordered topologically (starting with `wdl-grammar`, ending with `wdl` itself if that had non-dependency changes). Format each section so that it looks like 
```
### `wdl-<crate name>`

<copy and pasted CHANGELOG entries>
```
Make sure to mark this release as the latest, and we're good to go! :tada:
