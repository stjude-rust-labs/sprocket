// Accessibility audit for generated `wdl-doc` output.
//
// Usage:
//   node run.mjs <baseUrl> <docsDir>
//
//   <baseUrl>  the HTTP origin the docs are served from (e.g. http://localhost:8080)
//   <docsDir>  the on-disk directory of the generated docs (used to discover pages)
//
// What it does:
//   Audits one page of each documentation template — the home index, a nested
//   directory index, and the first task, workflow, struct, and enum pages — in
//   both the dark and light themes, using axe-core with the WCAG 2.1 A/AA rule
//   tags. It fails on confirmed violations and reports axe "incomplete" results
//   (needs manual review — e.g. short or non-BMP text, or partially obscured
//   elements) as warnings only.
//
// Exit codes:
//   0  no confirmed violations
//   1  at least one confirmed violation
//   2  bad usage or no pages found
//
// Design notes:
//   axe-core is driven directly through Puppeteer rather than through a wrapper
//   like pa11y for two reasons. First, pa11y reports axe "incomplete" results as
//   errors with no clean way to separate them; driving axe directly lets us fail
//   on `violations` while treating `incomplete` as warnings. Second, the theme is
//   client-side state (Alpine `$persist` on `<html>`), so it must be seeded via
//   `localStorage` *before* the page's scripts run — a post-load class change is
//   reverted by Alpine's reactive `x-bind:class`. `page.evaluateOnNewDocument`
//   gives us that pre-load hook.
//
// Chromium comes bundled with Puppeteer in CI. Set `PUPPETEER_EXECUTABLE_PATH`
// to reuse an existing browser in environments that block the download.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, sep } from "node:path";

import puppeteer from "puppeteer";

// The axe-core source, injected into each page via `addScriptTag`.
const AXE = readFileSync(new URL("./node_modules/axe-core/axe.min.js", import.meta.url), "utf8");

// WCAG 2.1 levels A and AA — the rule set the audit enforces.
const WCAG_TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"];

const [, , baseUrl, docsDir] = process.argv;
if (!baseUrl || !docsDir) {
  console.error("usage: node run.mjs <baseUrl> <docsDir>");
  process.exit(2);
}

/**
 * Recursively collects every `.html` file under `dir`, returning paths relative
 * to `docsDir` and sorted so page selection is deterministic.
 *
 * @param {string} dir directory to scan (absolute or relative to cwd)
 * @returns {string[]} `docsDir`-relative paths of every HTML file found
 */
function htmlFiles(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) out.push(...htmlFiles(full));
    else if (name.endsWith(".html")) out.push(relative(docsDir, full));
  }
  return out.sort();
}

/**
 * Selects one representative page per documentation template: the root index, a
 * nested directory index, and the first task, workflow, struct, and enum pages.
 * Missing templates are skipped, so the audit stays correct as the fixture
 * changes. Discovering pages this way avoids re-auditing structurally identical
 * pages (every task page shares a template, and so on).
 *
 * @param {string[]} all `docsDir`-relative HTML paths, sorted
 * @returns {string[]} the chosen page paths
 */
function representativePages(all) {
  const pages = [];
  const push = (p) => {
    if (p && !pages.includes(p)) pages.push(p);
  };
  push(all.find((p) => p === "index.html")); // home / module overview
  push(all.find((p) => p.endsWith(`${sep}index.html`))); // a nested directory index
  for (const suffix of ["-task.html", "-workflow.html", "-struct.html", "-enum.html"]) {
    push(all.find((p) => p.endsWith(suffix)));
  }
  return pages;
}

const base = baseUrl.endsWith("/") ? baseUrl : `${baseUrl}/`;
const pages = representativePages(htmlFiles(docsDir));
if (pages.length === 0) {
  console.error(`no pages found under ${docsDir}`);
  process.exit(2);
}

const browser = await puppeteer.launch({
  executablePath: process.env.PUPPETEER_EXECUTABLE_PATH || undefined,
  args: ["--no-sandbox", "--disable-dev-shm-usage"],
});

const violations = [];
let incompleteCount = 0;
for (const rel of pages) {
  const url = new URL(rel.split(sep).join("/"), base).href;
  for (const theme of ["dark", "light"]) {
    const page = await browser.newPage();
    // Audit at a desktop width so the full chrome is present — the right rail is
    // hidden below 1280px, and hidden elements are skipped by axe.
    await page.setViewport({ width: 1280, height: 1024 });
    // Seed the persisted theme before any page script runs; the head bootstrap
    // and Alpine both read `_x_theme`, so the theme sticks (a post-load class
    // change is reverted by Alpine's reactive `x-bind:class`).
    await page.evaluateOnNewDocument((t) => {
      localStorage.setItem("_x_theme", JSON.stringify(t));
    }, theme);
    await page.goto(url, { waitUntil: "domcontentloaded" });
    // Inject axe-core into the page, then run it against the live DOM.
    await page.addScriptTag({ content: AXE });
    const res = await page.evaluate(
      async (tags) => await axe.run(document, { runOnly: { type: "tag", values: tags } }),
      WCAG_TAGS,
    );
    const vCount = res.violations.reduce((a, v) => a + v.nodes.length, 0);
    incompleteCount += res.incomplete.reduce((a, v) => a + v.nodes.length, 0);
    console.log(`${theme.padEnd(5)}  ${rel}  — ${vCount} violation(s), ${res.incomplete.length} incomplete rule(s)`);
    for (const v of res.violations) {
      for (const n of v.nodes) {
        violations.push(`[${theme}] ${rel}\n    ${v.id} (${v.impact})\n    ${v.help} <${v.helpUrl}>\n    ${n.target.join(", ")}`);
      }
    }
    await page.close();
  }
}
await browser.close();

if (incompleteCount > 0) {
  console.log(`\n${incompleteCount} axe "incomplete" result(s) reported as warnings (manual review; not failing).`);
}
if (violations.length > 0) {
  console.error(`\nAccessibility violations (${violations.length}):\n\n${violations.join("\n\n")}`);
  process.exit(1);
}
console.log("\nNo accessibility violations found.");
