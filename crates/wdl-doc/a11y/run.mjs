// Accessibility audit for generated `wdl-doc` output.
//
// Usage: node run.mjs <baseUrl> <docsDir>
//
// Audits one page of each template (home index, a nested index, and the first
// task, workflow, struct, and enum pages) in both the dark and light themes
// with axe-core (WCAG 2.1 A/AA). Fails on confirmed violations; axe "incomplete"
// results (needs manual review — e.g. short/non-BMP text, partially obscured
// elements) are reported as warnings and do not fail the build.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, sep } from "node:path";

import puppeteer from "puppeteer";

const AXE = readFileSync(new URL("./node_modules/axe-core/axe.min.js", import.meta.url), "utf8");
const WCAG_TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"];

const [, , baseUrl, docsDir] = process.argv;
if (!baseUrl || !docsDir) {
  console.error("usage: node run.mjs <baseUrl> <docsDir>");
  process.exit(2);
}

function htmlFiles(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) out.push(...htmlFiles(full));
    else if (name.endsWith(".html")) out.push(relative(docsDir, full));
  }
  return out.sort();
}

function representativePages(all) {
  const pages = [];
  const push = (p) => {
    if (p && !pages.includes(p)) pages.push(p);
  };
  push(all.find((p) => p === "index.html"));
  push(all.find((p) => p.endsWith(`${sep}index.html`)));
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
    await page.setViewport({ width: 1280, height: 1024 });
    // Seed the persisted theme before any page script runs; the head bootstrap
    // and Alpine both read `_x_theme`, so the theme sticks (a post-load class
    // change is reverted by Alpine's reactive `x-bind:class`).
    await page.evaluateOnNewDocument((t) => {
      localStorage.setItem("_x_theme", JSON.stringify(t));
    }, theme);
    await page.goto(url, { waitUntil: "domcontentloaded" });
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
