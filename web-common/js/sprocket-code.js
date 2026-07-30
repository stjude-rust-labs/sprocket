import { initializeHighlighter } from "./sprocket-code.utils.js";
import COPY_ICON from "./icons/copy.svg";
import EXPAND_ICON from "./icons/arrows-fullscreen.svg";

const CODE_BLOCK_STYLES = `
  :host { display: block; font-size: 14px; }
  .code-block {
    position: relative;
    margin: 0;
    border-radius: 6px;
    overflow: hidden;
  }
  .code-block pre {
    margin: 0;
    padding: 1em;
    overflow-x: auto;
  }
  .code-block pre.shiki {
    background: var(--shiki-background) !important;
  }
  .code-block__toolbar {
    position: absolute;
    top: 0.5em;
    right: 0.5em;
    display: flex;
    gap: 0.25em;
    z-index: 1;
  }
  .code-block__toolbar button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.9em;
    height: 1.9em;
    padding: 0;
    border: none;
    border-radius: 4px;
    background: color-mix(in srgb, var(--shiki-foreground) 12%, transparent);
    color: var(--shiki-foreground);
    cursor: pointer;
    font: inherit;
    line-height: 0;
  }
  .code-block__toolbar button:hover {
    background: color-mix(in srgb, var(--shiki-foreground) 22%, transparent);
  }
  .code-block__toolbar button:focus-visible {
    outline: 2px solid #80cbc4;
    outline-offset: 1px;
  }
  .code-block.line-numbered .shiki code {
    counter-reset: sprocket-line;
  }
  .code-block.line-numbered .line-number {
    counter-increment: sprocket-line;
    display: inline-block;
    width: 2ch;
    margin-right: 1.5ch;
    text-align: right;
    color: color-mix(in srgb, var(--shiki-foreground) 45%, transparent);
    user-select: none;
  }
  .code-block.line-numbered .line-number::before {
    content: counter(sprocket-line);
  }
  .code-block--expanded {
    position: fixed;
    inset: 0;
    z-index: 1000;
    margin: 0;
    border-radius: 0;
    overflow: auto;
    background: var(--shiki-background);
  }
  .code-block--expanded pre {
    min-height: 100%;
  }
`;

// Builds the toolbar markup for the enabled controls, or an empty string when
// no controls are requested.
function buildToolbar(copyable, expandable) {
  if (!copyable && !expandable) return '';
  let buttons = '';
  if (copyable) {
    buttons += `<button type="button" class="code-block__copy" aria-label="Copy code" title="Copy code">${COPY_ICON}</button>`;
  }
  if (expandable) {
    buttons += `<button type="button" class="code-block__expand" aria-label="Expand code" title="Expand code" aria-pressed="false">${EXPAND_ICON}</button>`;
  }
  return `<div class="code-block__toolbar">${buttons}</div>`;
}

// Populates a shadow root with a highlighted code block, optionally wiring up
// copy, expand, and line-number controls. When no controls are requested the
// output matches the plain code block used by every other web-common consumer.
function decorateCodeBlock({ host, shadow, highlighted, source, copyable, expandable, lineNumbers }) {
  const blockClasses = ['code-block'];
  if (lineNumbers) blockClasses.push('line-numbered');

  shadow.innerHTML = `
    <style>${CODE_BLOCK_STYLES}</style>
    <div class="${blockClasses.join(' ')}">
      ${buildToolbar(copyable, expandable)}
      ${highlighted}
    </div>
  `;

  const container = shadow.querySelector('.code-block');

  // Add one line-number element per Shiki line so numbers correspond exactly to
  // the rendered lines. The CSS counter provides the displayed digits.
  if (lineNumbers) {
    for (const line of shadow.querySelectorAll('.line')) {
      const number = document.createElement('span');
      number.className = 'line-number';
      number.setAttribute('aria-hidden', 'true');
      line.prepend(number);
    }
  }

  if (copyable) {
    const copyButton = shadow.querySelector('.code-block__copy');
    copyButton.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(source);
      } catch (err) {
        console.error('Failed to copy code: ', err);
      }
    });
  }

  if (expandable) {
    const expandButton = shadow.querySelector('.code-block__expand');
    expandButton.addEventListener('click', () => {
      const expanded = container.classList.toggle('code-block--expanded');
      host.classList.toggle('code-block--expanded', expanded);
      expandButton.setAttribute('aria-pressed', String(expanded));
      expandButton.setAttribute('aria-label', expanded ? 'Collapse code' : 'Expand code');
      expandButton.setAttribute('title', expanded ? 'Collapse code' : 'Expand code');
    });
  }
}

// Manual highlighting for pages generated without <sprocket-code> elements.
//
// `options` may enable the optional `copyable`, `expandable`, and `lineNumbers`
// controls. They default to `false` so existing consumers keep their plain
// code blocks.
export async function initManualHighlighting(languagesToLoad = [], options = {}) {
  const { copyable = false, expandable = false, lineNumbers = false } = options;

  // Load the highlighter once. A load failure is fatal for manual
  // highlighting, so report it explicitly and leave every fence as readable
  // plain text rather than silently swallowing the error.
  let highlighter;
  try {
    highlighter = await initializeHighlighter(languagesToLoad);
  } catch (err) {
    console.error("Failed to initialize syntax highlighting: ", err);
    return;
  }
  if (!highlighter) {
    console.error("Failed to initialize syntax highlighting: highlighter unavailable");
    return;
  }

  // Decorate each fence independently. Highlighting an unsupported language
  // throws, so isolating failures per element keeps that one fence as plain,
  // readable text while every other supported fence is still decorated.
  for (const codeElem of document.querySelectorAll('pre > code[class*="language-"]')) {
    try {
      const langClass = [...codeElem.classList].find(c => c.startsWith('language-'));
      if (!langClass) continue;

      const lang = langClass.replace('language-', '');
      const code = codeElem.textContent;

      const highlighted = await highlighter.codeToHtml(code, {
        lang: lang,
        theme: 'sprocket'
      });

      const host = document.createElement('div');
      const shadow = host.attachShadow({ mode: 'open' });

      decorateCodeBlock({
        host,
        shadow,
        highlighted,
        source: code,
        copyable,
        expandable,
        lineNumbers,
      });

      codeElem.parentElement.replaceWith(host);
    } catch (err) {
      console.error("Failed to highlight code block: ", err);
    }
  }
}

class SprocketCode extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
  }

  async connectedCallback() {
    try {
      const highlighter = await initializeHighlighter();
      // Preserve the original, untrimmed source for the copy control while
      // trimming only what gets highlighted for display.
      const source = this.textContent;
      const code = source.trim();
      this.textContent = '';

      const html = highlighter.codeToHtml(code, {
        lang: this.getAttribute('language') || 'wdl',
        theme: 'sprocket'
      });

      decorateCodeBlock({
        host: this,
        shadow: this.shadowRoot,
        highlighted: html,
        source,
        copyable: this.hasAttribute('copyable'),
        expandable: this.hasAttribute('expandable'),
        lineNumbers: this.hasAttribute('line-numbers'),
      });
    } catch {}
  }
}

customElements.define('sprocket-code', SprocketCode);
