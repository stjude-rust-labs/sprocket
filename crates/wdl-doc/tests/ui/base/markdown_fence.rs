//! Test that the theme's manual Markdown-fence highlighter decorates a
//! supported fenced code block with the configured copy, expand, and
//! line-number controls, and that an unsupported fence appearing before it
//! does not prevent the supported fence from being decorated.

use std::path::Path;
use std::time::Duration;

use anyhow::Context;
use anyhow::bail;
use thirtyfour::WebDriver;

use crate::UiTest;

/// The document overview page whose authored Markdown preamble contains an
/// unsupported `json` fence followed by a supported `wdl` fence.
const OVERVIEW_PAGE: &str = "flag_filter/index.html";

/// A token that only appears inside the authored `wdl` Markdown fence, used to
/// identify the decorated host among any other code blocks on the page.
const FENCE_MARKER: &str = "sprocket_fence_demo";

/// Test that authored Markdown fenced code receives the shared code-block
/// controls through the theme's manual highlighter.
pub struct MarkdownFence;

#[async_trait::async_trait]
impl UiTest for MarkdownFence {
    fn name(&self) -> &'static str {
        "markdown_fence"
    }

    async fn run(&self, driver: &mut WebDriver, _docs_path: &Path) -> anyhow::Result<()> {
        // Navigate to the document overview page that renders the authored
        // Markdown fences from the document preamble.
        let base = driver.current_url().await?;
        let target = base
            .join(OVERVIEW_PAGE)
            .context("failed to build overview page url")?;
        driver.goto(target.as_str()).await?;

        // Wait for the manual highlighter to replace the supported `wdl` fence
        // with a shadow-root host. The host is a plain `<div>` created by
        // `initManualHighlighting`; identify it by the unique fence marker and
        // tag it with a stable id for follow-up queries.
        let mut ready = false;
        for _ in 0..150 {
            let ret = driver
                .execute(
                    r#"
                    const hosts = [...document.querySelectorAll('div')].filter(
                      d => d.shadowRoot && d.shadowRoot.querySelector('.code-block .shiki')
                    );
                    const marker = 'sprocket_fence_demo';
                    const host = hosts.find(h =>
                      (h.shadowRoot.querySelector('.shiki code')?.textContent || '').includes(marker)
                    );
                    if (!host) return false;
                    host.id = 'sprocket-md-fence-host';
                    return host.shadowRoot.querySelectorAll('.line').length > 0;
                    "#,
                    Vec::new(),
                )
                .await?;
            if ret.json().as_bool().unwrap_or(false) {
                ready = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if !ready {
            bail!(
                "the supported `wdl` Markdown fence was never decorated; an earlier unsupported \
                 fence likely aborted the highlighter"
            );
        }

        // Inspect the decorated host and the surrounding document to prove both
        // decoration of the supported fence and isolation of the unsupported
        // one.
        let info = driver
            .execute(
                r#"
                const host = document.getElementById('sprocket-md-fence-host');
                const root = host.shadowRoot;
                return {
                  hostTag: host.tagName,
                  hasCopy: !!root.querySelector('.code-block__copy'),
                  hasExpand: !!root.querySelector('.code-block__expand'),
                  copyLabel: root.querySelector('.code-block__copy')?.getAttribute('aria-label') || null,
                  expandLabel: root.querySelector('.code-block__expand')?.getAttribute('aria-label') || null,
                  lineCount: root.querySelectorAll('.line').length,
                  lineNumberCount: root.querySelectorAll('.line-number').length,
                  codeText: root.querySelector('.shiki code')?.textContent || '',
                  // Isolation: the unsupported `json` fence must remain a plain,
                  // un-upgraded `<pre><code>` so it stays readable.
                  jsonStillPlain: !!document.querySelector('pre > code.language-json'),
                  // The supported `wdl` fence's original markup must be gone
                  // because it was replaced by the decorated host.
                  wdlReplaced: !document.querySelector('pre > code.language-wdl'),
                };
                "#,
                Vec::new(),
            )
            .await?;
        let info = info.json();

        if info["hostTag"].as_str() != Some("DIV") {
            bail!(
                "expected the manual highlighter to mount a `<div>` host, got {:?}",
                info["hostTag"]
            );
        }
        if !info["hasCopy"].as_bool().unwrap_or(false) {
            bail!("expected a `.code-block__copy` control on the decorated Markdown fence");
        }
        if !info["hasExpand"].as_bool().unwrap_or(false) {
            bail!("expected a `.code-block__expand` control on the decorated Markdown fence");
        }
        if info["copyLabel"].as_str() != Some("Copy code") {
            bail!(
                "expected the copy control to be labeled `Copy code`, got {:?}",
                info["copyLabel"]
            );
        }
        if info["expandLabel"].as_str() != Some("Expand code") {
            bail!(
                "expected the expand control to be labeled `Expand code`, got {:?}",
                info["expandLabel"]
            );
        }

        let line_count = info["lineCount"].as_u64().unwrap_or(0);
        let line_number_count = info["lineNumberCount"].as_u64().unwrap_or(0);
        if line_count == 0 {
            bail!("expected the decorated Markdown fence to render at least one line");
        }
        if line_number_count != line_count {
            bail!(
                "expected a `.line-number` per rendered line: {line_number_count} line numbers \
                 for {line_count} lines"
            );
        }

        let code_text = info["codeText"].as_str().unwrap_or_default().to_string();
        if !code_text.contains(FENCE_MARKER) {
            bail!("the decorated fence did not contain the authored `wdl` source");
        }

        if !info["jsonStillPlain"].as_bool().unwrap_or(false) {
            bail!(
                "expected the unsupported `json` fence to remain a plain `<pre><code>`; its \
                 failure must not remove it from the page"
            );
        }
        if !info["wdlReplaced"].as_bool().unwrap_or(false) {
            bail!("expected the supported `wdl` fence's original markup to be replaced");
        }

        // Install a clipboard stub and confirm the copy control writes the
        // authored fence source.
        driver
            .execute(
                r#"
                window.__sprocketCopied = null;
                Object.defineProperty(navigator, 'clipboard', {
                  configurable: true,
                  value: {
                    writeText: (text) => {
                      window.__sprocketCopied = text;
                      return Promise.resolve();
                    },
                  },
                });
                document.getElementById('sprocket-md-fence-host')
                  .shadowRoot.querySelector('.code-block__copy').click();
                "#,
                Vec::new(),
            )
            .await?;
        tokio::time::sleep(Duration::from_millis(100)).await;

        let copied = driver
            .execute(r#"return window.__sprocketCopied;"#, Vec::new())
            .await?;
        let copied = copied
            .json()
            .as_str()
            .map(str::to_string)
            .context("the copy control did not write any text to the clipboard")?;
        if copied.trim() != code_text.trim() {
            bail!(
                "copied text did not match the fence source.\n copied: {copied:?}\n source: \
                 {code_text:?}"
            );
        }

        // The expand control must toggle the expanded class on the host and be
        // reversible.
        driver
            .execute(
                r#"
                document.getElementById('sprocket-md-fence-host')
                  .shadowRoot.querySelector('.code-block__expand').click();
                "#,
                Vec::new(),
            )
            .await?;
        let expanded = driver
            .execute(
                r#"return document.getElementById('sprocket-md-fence-host')
                     .classList.contains('code-block--expanded');"#,
                Vec::new(),
            )
            .await?;
        if !expanded.json().as_bool().unwrap_or(false) {
            bail!("expected the host element to receive the `code-block--expanded` class");
        }

        driver
            .execute(
                r#"
                document.getElementById('sprocket-md-fence-host')
                  .shadowRoot.querySelector('.code-block__expand').click();
                "#,
                Vec::new(),
            )
            .await?;
        let collapsed = driver
            .execute(
                r#"return document.getElementById('sprocket-md-fence-host')
                     .classList.contains('code-block--expanded');"#,
                Vec::new(),
            )
            .await?;
        if collapsed.json().as_bool().unwrap_or(true) {
            bail!("expected the expand control to collapse the block when toggled again");
        }

        Ok(())
    }
}
