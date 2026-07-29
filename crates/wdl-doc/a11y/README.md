# Accessibility audit

`run.mjs` audits the generated `wdl-doc` documentation for accessibility
violations using [axe-core](https://github.com/dequelabs/axe-core) (WCAG 2.1
A/AA) driven through a headless browser with
[Puppeteer](https://github.com/puppeteer/puppeteer). It checks one page of each
template — the home index, a nested index, and the first task, workflow, struct,
and enum pages — in both the dark and light themes.

It **fails on confirmed axe violations**. axe "incomplete" results (elements axe
cannot automatically verify — for example short or non-BMP text, or partially
obscured elements) are reported as warnings and do not fail the build. It runs
in the `a11y` CI job.

## Running locally

```bash
# 1. Build sprocket and generate docs for the UI-test fixture.
cargo build --release --bin sprocket
./target/release/sprocket dev doc crates/wdl-doc/tests/ui/base/assets --output /tmp/a11y-docs

# 2. Serve the generated docs.
python3 -m http.server 8080 --directory /tmp/a11y-docs &

# 3. Install dependencies (downloads a Chromium via Puppeteer) and run the audit.
cd crates/wdl-doc/a11y
npm install
node run.mjs http://localhost:8080 /tmp/a11y-docs
```

If your environment blocks Puppeteer's Chromium download, point it at an existing
browser: `PUPPETEER_EXECUTABLE_PATH=/path/to/chrome node run.mjs ...`.

Any workspace can be audited by pointing `sprocket dev doc` at it and passing the
resulting output directory (and its served base URL) to `run.mjs`.
