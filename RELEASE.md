# Release

Open a new PR with the title `release: bump versions` with the following changes:

* [ ] Update version in `Cargo.toml`.
* [ ] Run `cargo update`.
* [ ] Update `CHANGELOG.md` with version and publication date.
* [ ] Update any references in the `README.md` to the new version.
* [ ] Update the [docs site](https://stjude-rust-labs.github.io/sprocket/) to the new version ([here](https://github.com/stjude-rust-labs/sprocket/blob/main/docs/.vitepress/config.mts#L17))

Once the above PR merges:

* [ ] Create git tag for the HEAD commit:
    ```
    git tag v{VERSION}
    ```
    * where `{VERSION}` is the new version being released
* [ ] Push the tag: `git push --tags`.

And the CI should handle the rest!