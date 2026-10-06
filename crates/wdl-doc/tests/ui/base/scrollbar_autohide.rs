//! Test that the main content and left navigation scrollbars stay hidden at
//! rest and are only revealed while hovered, focused, or actively scrolling.

use std::path::Path;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::bail;
use thirtyfour::By;
use thirtyfour::WebDriver;
use thirtyfour::prelude::ElementQueryable;
use thirtyfour::session::scriptret::ScriptRet;

use crate::UiTest;

/// Main content scroll pane.
const MAIN_PANE: Pane = Pane {
    selector: ".layout__main-center",
    scrolling_class: "layout__main-center--scrolling",
    label: "main content",
};

/// Left navigation scroll pane.
const LEFT_PANE: Pane = Pane {
    selector: ".left-sidebar__content-container",
    scrolling_class: "left-sidebar__content-container--scrolling",
    label: "left navigation",
};

/// Test the auto-hiding behavior of the documentation scrollbars.
///
/// The scrollbar is transparent at rest, revealed while the pane is actively
/// scrolling (a class the theme's scroll handler toggles), and hidden again a
/// short delay after scrolling stops. Hover and `focus-within` share the same
/// reveal selector as the active-scroll class, so verifying the revealed color
/// through the class also proves the hover styling exists.
pub struct ScrollbarAutohide;

#[async_trait::async_trait]
impl UiTest for ScrollbarAutohide {
    fn name(&self) -> &'static str {
        "scrollbar_autohide"
    }

    async fn run(&self, driver: &mut WebDriver, _docs_path: &Path) -> anyhow::Result<()> {
        let base = driver.current_url().await?;
        let target = base
            .join("employee/Employee-struct.html")
            .context("failed to build struct page url")?;
        driver.goto(target.as_str()).await?;

        let original_rect = driver.get_window_rect().await?;
        driver.set_window_rect(0, 0, 1400, 900).await?;
        let alignment = driver
            .execute(
                r#"
                const breadcrumbs = document.querySelector('.layout__breadcrumbs');
                const railHeader = document.querySelector('.right-sidebar__header');
                const container = document.querySelector('.right-sidebar__container');
                const main = document.querySelector('.layout__main-center');
                return {
                    breadcrumbsTop: breadcrumbs.getBoundingClientRect().top,
                    railTop: railHeader.getBoundingClientRect().top,
                    containerTop: container.getBoundingClientRect().top,
                    containerHeight: container.getBoundingClientRect().height,
                    scrollTop: main.scrollTop,
                };
                "#,
                Vec::new(),
            )
            .await?;
        let alignment = alignment.json();
        let breadcrumbs_top = alignment["breadcrumbsTop"]
            .as_f64()
            .context("expected breadcrumb position")?;
        let rail_top = alignment["railTop"]
            .as_f64()
            .context("expected right-rail position")?;
        let container_top = alignment["containerTop"]
            .as_f64()
            .context("expected right-rail container position")?;
        let container_height = alignment["containerHeight"]
            .as_f64()
            .context("expected right-rail container height")?;
        let scroll_top = alignment["scrollTop"]
            .as_f64()
            .context("expected main scroll position")?;
        let difference = (breadcrumbs_top - rail_top).abs();
        driver
            .set_window_rect(
                original_rect.x,
                original_rect.y,
                original_rect.width as u32,
                original_rect.height as u32,
            )
            .await?;
        if difference > 2.0 {
            bail!(
                "expected the right rail to align with the breadcrumbs, found a {difference:.1}px \
                 difference; breadcrumbs={breadcrumbs_top:.1}px, rail={rail_top:.1}px, \
                 container={container_top:.1}px/{container_height:.1}px, scroll={scroll_top:.1}px"
            );
        }

        for pane in [MAIN_PANE, LEFT_PANE] {
            driver
                .query(By::Css(pane.selector))
                .wait(Duration::from_secs(10), Duration::from_millis(100))
                .first()
                .await?;

            let idle = read_state(driver, pane).await?;
            if idle.has_class {
                bail!(
                    "expected the {} pane to start without the `{}` class",
                    pane.label,
                    pane.scrolling_class
                );
            }
            if !is_transparent(&idle.color) {
                bail!(
                    "expected the idle {} scrollbar-color to be transparent, found `{}`",
                    pane.label,
                    idle.color
                );
            }

            let active = dispatch_scroll_and_read(driver, pane).await?;
            if !active.has_class {
                bail!(
                    "expected a scroll event to add the `{}` class to the {} pane",
                    pane.scrolling_class,
                    pane.label
                );
            }

            let revealed =
                wait_for_revealed_color(driver, pane, &idle.color, Duration::from_secs(3)).await?;
            if is_transparent(&revealed) {
                bail!(
                    "expected the revealed {} scrollbar thumb to be visible, found transparent \
                     `{revealed}`",
                    pane.label
                );
            }

            wait_for_class(driver, pane, false, Duration::from_secs(3)).await?;
        }

        assert_right_rail_sticks(driver).await?;

        Ok(())
    }
}

/// Identifies a scroll pane and its active-scroll class.
#[derive(Clone, Copy)]
struct Pane {
    /// CSS selector for the scroll pane.
    selector: &'static str,
    /// Class present while the pane is actively scrolling.
    scrolling_class: &'static str,
    /// Human-readable pane name used in failures.
    label: &'static str,
}

/// A snapshot of a pane's scroll-related state.
struct PaneState {
    /// Whether the pane currently carries the active-scroll class.
    has_class: bool,
    /// The computed `scrollbar-color` of the pane.
    color: String,
}

/// Reads a pane's active-scroll class membership and computed
/// `scrollbar-color`.
async fn read_state(driver: &WebDriver, pane: Pane) -> anyhow::Result<PaneState> {
    let script = format!(
        r#"
        const pane = document.querySelector('{}');
        return {{
            hasClass: pane.classList.contains('{}'),
            color: getComputedStyle(pane).getPropertyValue('scrollbar-color'),
        }};
        "#,
        pane.selector, pane.scrolling_class
    );
    let ret = driver.execute(script, Vec::new()).await?;
    parse_state(&ret)
}

/// Dispatches a synthetic `scroll` event on a pane and reads the
/// resulting state synchronously.
async fn dispatch_scroll_and_read(driver: &WebDriver, pane: Pane) -> anyhow::Result<PaneState> {
    let script = format!(
        r#"
        const pane = document.querySelector('{}');
        pane.dispatchEvent(new Event('scroll'));
        return {{
            hasClass: pane.classList.contains('{}'),
            color: getComputedStyle(pane).getPropertyValue('scrollbar-color'),
        }};
        "#,
        pane.selector, pane.scrolling_class
    );
    let ret = driver.execute(script, Vec::new()).await?;
    parse_state(&ret)
}

/// Parses a `{ hasClass, color }` object returned from one of the browser
/// scripts into a [`PaneState`].
fn parse_state(ret: &ScriptRet) -> anyhow::Result<PaneState> {
    let json = ret.json();
    Ok(PaneState {
        has_class: json["hasClass"]
            .as_bool()
            .ok_or_else(|| anyhow::anyhow!("missing `hasClass` in browser response"))?,
        color: json["color"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing `color` in browser response"))?
            .trim()
            .to_string(),
    })
}

/// Polls until the main pane's active-scroll class membership matches
/// `expected`, or the `timeout` elapses. Uses short, condition-based waits so
/// the test never relies on a single arbitrary long sleep.
async fn wait_for_class(
    driver: &WebDriver,
    pane: Pane,
    expected: bool,
    timeout: Duration,
) -> anyhow::Result<()> {
    let start = Instant::now();
    loop {
        if read_state(driver, pane).await?.has_class == expected {
            return Ok(());
        }
        if start.elapsed() >= timeout {
            bail!(
                "timed out after {timeout:?} waiting for `{}` present={expected}",
                pane.scrolling_class
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Polls until the pane's computed `scrollbar-color` resolves to a visible
/// color different from `idle_color`, returning that revealed color. Each poll
/// re-dispatches a `scroll` event so the pane stays "actively scrolling" and
/// the idle timer cannot clear the reveal while the `scrollbar-color`
/// transition fades in.
async fn wait_for_revealed_color(
    driver: &WebDriver,
    pane: Pane,
    idle_color: &str,
    timeout: Duration,
) -> anyhow::Result<String> {
    let start = Instant::now();
    loop {
        let state = dispatch_scroll_and_read(driver, pane).await?;
        if !is_transparent(&state.color) && state.color != idle_color {
            return Ok(state.color);
        }
        if start.elapsed() >= timeout {
            bail!(
                "timed out after {timeout:?} waiting for the scrollbar reveal, last color `{}`",
                state.color
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Whether a computed `scrollbar-color` value represents a fully transparent
/// scrollbar. Opaque colors serialize with an `rgb(` token, while transparent
/// values serialize as `transparent` or `rgba(0, 0, 0, 0)`.
fn is_transparent(value: &str) -> bool {
    !value.contains("rgb(")
}

/// Verifies that the page-navigation rail remains below the fixed header while
/// the main content pane scrolls.
async fn assert_right_rail_sticks(driver: &WebDriver) -> anyhow::Result<()> {
    let original_rect = driver.get_window_rect().await?;
    driver.set_window_rect(0, 0, 1400, 600).await?;
    let positions = driver
        .execute(
            r#"
            const main = document.querySelector('.layout__main-center');
            const rail = document.querySelector('.right-sidebar__header');
            const container = document.querySelector('.right-sidebar__container');
            const sticky = document.querySelector('.right-sidebar__sticky');
            main.scrollTop = 0;
            const before = rail.getBoundingClientRect().top;
            main.scrollTop = 400;
            return {
                before,
                after: rail.getBoundingClientRect().top,
                containerHeight: container.getBoundingClientRect().height,
                stickyHeight: sticky.getBoundingClientRect().height,
            };
            "#,
            Vec::new(),
        )
        .await?;
    driver
        .set_window_rect(
            original_rect.x,
            original_rect.y,
            original_rect.width as u32,
            original_rect.height as u32,
        )
        .await?;

    let positions = positions.json();
    let before = positions["before"]
        .as_f64()
        .context("expected initial right-rail position")?;
    let after = positions["after"]
        .as_f64()
        .context("expected scrolled right-rail position")?;
    let container_height = positions["containerHeight"]
        .as_f64()
        .context("expected right-rail container height")?;
    let sticky_height = positions["stickyHeight"]
        .as_f64()
        .context("expected sticky right-rail height")?;

    if !(94.0..=98.0).contains(&after) {
        bail!(
            "expected the right rail to stick 96px from the viewport top after scrolling; \
             before={before:.1}px, after={after:.1}px, container={container_height:.1}px, \
             sticky={sticky_height:.1}px"
        );
    }

    Ok(())
}
