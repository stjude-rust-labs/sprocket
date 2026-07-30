//! Test that documentation code blocks expose optional copy, expand, and
//! line-number controls.

use std::path::Path;
use std::time::Duration;

use anyhow::Context;
use anyhow::bail;
use thirtyfour::WebDriver;

use crate::UiTest;

/// The task page whose command section renders an enhanced `<sprocket-code>`
/// block.
const TASK_PAGE: &str = "flag_filter/validate_string_is_12bit_oct_dec_or_hex-task.html";

/// Test that the command code block on a task page gains the copy, expand, and
/// line-number controls and that they behave correctly.
pub struct CodeBlock;

#[async_trait::async_trait]
impl UiTest for CodeBlock {
    fn name(&self) -> &'static str {
        "code_block"
    }

    async fn run(&self, driver: &mut WebDriver, _docs_path: &Path) -> anyhow::Result<()> {
        // Navigate to the task page that renders a non-empty command section.
        let base = driver.current_url().await?;
        let target = base
            .join(TASK_PAGE)
            .context("failed to build task page url")?;
        driver.goto(target.as_str()).await?;

        // Wait for the command `<sprocket-code>` element to upgrade and finish
        // highlighting inside its shadow root.
        let mut ready = false;
        for _ in 0..150 {
            let ret = driver
                .execute(
                    r#"
                    const host = document.querySelector('sprocket-code.pt-8');
                    if (!host || !host.shadowRoot) return false;
                    const block = host.shadowRoot.querySelector('.code-block .shiki');
                    const lines = host.shadowRoot.querySelectorAll('.line');
                    return !!block && lines.length > 0;
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
            bail!("the command code block never rendered its highlighted shadow content");
        }

        // Inspect the rendered controls and line structure.
        let info = driver
            .execute(
                r#"
                const root = document.querySelector('sprocket-code.pt-8').shadowRoot;
                return {
                  hasCopy: !!root.querySelector('.code-block__copy'),
                  hasExpand: !!root.querySelector('.code-block__expand'),
                  copyLabel: root.querySelector('.code-block__copy')?.getAttribute('aria-label') || null,
                  expandLabel: root.querySelector('.code-block__expand')?.getAttribute('aria-label') || null,
                  lineCount: root.querySelectorAll('.line').length,
                  lineNumberCount: root.querySelectorAll('.line-number').length,
                  lineHeight: getComputedStyle(root.querySelector('.line')).lineHeight,
                  lineNumberGap:
                    getComputedStyle(root.querySelector('.line-number')).marginRight,
                  codeText: root.querySelector('code')?.textContent || '',
                };
                "#,
                Vec::new(),
            )
            .await?;
        let info = info.json();

        if !info["hasCopy"].as_bool().unwrap_or(false) {
            bail!("expected a `.code-block__copy` control in the command block");
        }
        if !info["hasExpand"].as_bool().unwrap_or(false) {
            bail!("expected a `.code-block__expand` control in the command block");
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
            bail!("expected the command block to render at least one line");
        }
        if line_number_count != line_count {
            bail!(
                "expected a `.line-number` per rendered line: {line_number_count} line numbers \
                 for {line_count} lines"
            );
        }
        let line_height = info["lineHeight"]
            .as_str()
            .context("expected a computed code line height")?
            .trim_end_matches("px")
            .parse::<f64>()
            .context("expected the code line height in pixels")?;
        if line_height > 21.0 {
            bail!("expected compact code line spacing, found a {line_height}px line height");
        }
        let line_number_gap = info["lineNumberGap"]
            .as_str()
            .context("expected a computed line-number gap")?
            .trim_end_matches("px")
            .parse::<f64>()
            .context("expected the line-number gap in pixels")?;
        if !(8.0..=24.0).contains(&line_number_gap) {
            bail!(
                "expected a readable gap between line numbers and code (8-24px), found a \
                 {line_number_gap}px gap"
            );
        }

        let code_text = info["codeText"].as_str().unwrap_or_default().to_string();

        // Install a clipboard stub so we can capture the copied text, then click
        // the copy control.
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
                document.querySelector('sprocket-code.pt-8')
                  .shadowRoot.querySelector('.code-block__copy').click();
                "#,
                Vec::new(),
            )
            .await?;

        // Allow the async copy handler to settle.
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
                "copied text did not match the command source.\n copied: {copied:?}\n source: \
                 {code_text:?}"
            );
        }
        if !copied.contains("echo") {
            bail!("copied command text did not contain the expected script contents");
        }

        // Clicking expand must toggle the expanded class onto the host element.
        driver
            .execute(
                r#"
                document.querySelector('sprocket-code.pt-8')
                  .shadowRoot.querySelector('.code-block__expand').click();
                "#,
                Vec::new(),
            )
            .await?;
        let expanded = driver
            .execute(
                r#"return document.querySelector('sprocket-code.pt-8')
                     .classList.contains('code-block--expanded');"#,
                Vec::new(),
            )
            .await?;
        if !expanded.json().as_bool().unwrap_or(false) {
            bail!("expected the host element to receive the `code-block--expanded` class");
        }

        // Expansion must be reversible.
        driver
            .execute(
                r#"
                document.querySelector('sprocket-code.pt-8')
                  .shadowRoot.querySelector('.code-block__expand').click();
                "#,
                Vec::new(),
            )
            .await?;
        let collapsed = driver
            .execute(
                r#"return document.querySelector('sprocket-code.pt-8')
                     .classList.contains('code-block--expanded');"#,
                Vec::new(),
            )
            .await?;
        if collapsed.json().as_bool().unwrap_or(true) {
            bail!("expected the expand control to collapse the block when toggled again");
        }

        // The css-variables theme emits spans that reference `--shiki-*` custom
        // properties rather than baked hex, and those properties flip with the
        // site theme. Confirm both: a token span references a `--shiki-*`
        // variable, and its computed color changes when the theme toggles.
        let theme_probe = driver
            .execute(
                r#"
                const root = document.querySelector('sprocket-code.pt-8').shadowRoot;
                const varSpan = root.querySelector('span[style*="var(--shiki"]');
                if (!varSpan) return { usesVars: false };
                // Establish a known theme first; a prior test may have left the
                // persisted theme in light mode, which `goto` reapplies on load.
                document.documentElement.classList.remove('light');
                document.documentElement.classList.add('dark');
                const darkColor = getComputedStyle(varSpan).color;
                document.documentElement.classList.add('light');
                document.documentElement.classList.remove('dark');
                const lightColor = getComputedStyle(varSpan).color;
                // Restore dark mode so the page is left as it was found.
                document.documentElement.classList.add('dark');
                document.documentElement.classList.remove('light');
                return { usesVars: true, darkColor, lightColor };
                "#,
                Vec::new(),
            )
            .await?;
        let theme_probe = theme_probe.json();
        if !theme_probe["usesVars"].as_bool().unwrap_or(false) {
            bail!("expected highlighted spans to reference `--shiki-*` CSS variables");
        }
        let dark_color = theme_probe["darkColor"].as_str().unwrap_or_default();
        let light_color = theme_probe["lightColor"].as_str().unwrap_or_default();
        if dark_color.is_empty() || dark_color == light_color {
            bail!(
                "expected the token color to change with the theme, got dark={dark_color:?} \
                 light={light_color:?}"
            );
        }

        Ok(())
    }
}
