class SprocketCode extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
  }

  _getStyles() {
    return `
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
      .code-block :global(pre.shiki) {
        background: var(--shiki-bg, #2e3440) !important;
      }
    `;
  }

  async connectedCallback() {
    try {
      const { initializeHighlighter } = await import('./sprocket-code.utils.js');
      const highlighter = await initializeHighlighter();
      const code = this.textContent.trim();
      this.textContent = '';
      
      const html = highlighter.codeToHtml(code, {
        lang: this.getAttribute('language') || 'wdl',
        theme: 'material-theme-ocean'
      });

      this.shadowRoot.innerHTML = `
        <style>${this._getStyles()}</style>
        <div class="code-block">${html}</div>
      `;
    } catch {}
  }
}

customElements.define('sprocket-code', SprocketCode);