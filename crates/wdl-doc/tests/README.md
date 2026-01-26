## UI Testing

The `wdl-doc` UI tests are used to test the HTML/CSS/JS as rendered in a browser.

The tests are broken into categories, with each directory under [./ui](./ui) being a category. Within a category,
a single WDL workspace is documented and all tests operate on it.

### Adding a Test

To add a test, create a new file under a category (e.g. [base](./ui/base))

After defining a test, add it to the `all_tests()` map in the category's `mod.rs` (see [base/mod.rs](./ui/base/mod.rs) for an example).

### Adding a Category

To add a test category, create a new directory under [./ui](./ui) with the following structure:

```
ui/
├─ <category>/
│  ├─ assets/
│  ├─ mod.rs
```

Copy the WDL workspace to document into `<category>/assets`.

In `mod.rs`, define a function named `all_tests()` (see [base/mod.rs](./ui/base/mod.rs) for an example).

In [ui.rs](./ui.rs), add the category into the `TEST_CATEGORIES` map:

```rust
static TEST_CATEGORIES: LazyLock<HashMap<&'static str, TestMap>> = LazyLock::new(|| {
    let mut categories = HashMap::new();
    categories.extend([
        ("base", base::all_tests()),
        // Add here
        ("<category>", category::all_tests()),
    ]);
    categories
});
```

## Updating Outputs

By default, the documentation will be reused between runs. Setting the `BLESS` environment variable will force
a regeneration.

For example:

```bash
BLESS=1 cargo test
```

