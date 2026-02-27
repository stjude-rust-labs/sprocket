import { initializeHighlighter } from "./sprocket-code.utils.js";

const CODE_BLOCK_STYLES = `
  :host { display: block; font-size: 14px; }
  .code-block {
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
    background: var(--shiki-bg, #2e3440) !important;
  }
`;

// Manual highlighting for pages generated without <sprocket-code> elements
export async function initManualHighlighting(languagesToLoad = []) {
  try {
    const highlighter = await initializeHighlighter(languagesToLoad);
    for (const codeElem of document.querySelectorAll('pre > code[class*="language-"]')) {
      const langClass = [...codeElem.classList].find(c => c.startsWith('language-'));
      if (!langClass) continue;

      const lang = langClass.replace('language-', '');
      const code = codeElem.textContent

      const highlighted = await highlighter.codeToHtml(code, {
        lang: lang,
        theme: 'material-theme-ocean'
      });

      const host = document.createElement('div');
      const shadow = host.attachShadow({ mode: 'open' });

      shadow.innerHTML = `
      <style>${CODE_BLOCK_STYLES}</style>
      <div class="code-block">
        ${highlighted}
      </div>
    `;

      codeElem.parentElement.replaceWith(host);
    }
  } catch (err) {
    console.error("Failed to initialize syntax highlighting: ", err)
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
      const code = this.textContent.trim();
      this.textContent = '';

      const html = highlighter.codeToHtml(code, {
        lang: this.getAttribute('language') || 'wdl',
        theme: 'material-theme-ocean'
      });

      this.shadowRoot.innerHTML = `
        <style>${CODE_BLOCK_STYLES}</style>
        <div class="code-block">${html}</div>
      `;
    } catch {}
  }
}

customElements.define('sprocket-code', SprocketCode);