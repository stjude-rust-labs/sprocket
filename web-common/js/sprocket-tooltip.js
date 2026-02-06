import { computePosition, flip, shift, offset } from '@floating-ui/dom';

class SprocketTooltip extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this._cleanup = null;
  }

  _getStyles() {
    return `
      :host { display: inline-block; }
      .tooltip {
        position: absolute;
        background: #333;
        color: white;
        padding: 6px 12px;
        border-radius: 4px;
        font-size: 14px;
        white-space: nowrap;
        pointer-events: none;
        opacity: 0;
        transition: opacity 0.2s;
        transition-delay: 0ms;
        z-index: 100;
        top: 0;
        left: 0;
      }
      .tooltip.visible {
        opacity: 1;
        transition-delay: 1000ms;
      }
    `;
  }

  _updatePosition() {
    const tooltipElement = this.shadowRoot.querySelector('.tooltip');
    const position = this.getAttribute('position') || 'top';

    computePosition(this, tooltipElement, {
      placement: position,
      middleware: [
        offset(8),
        flip(),
        shift({ padding: 5 })
      ]
    }).then(({ x, y }) => {
      Object.assign(tooltipElement.style, {
        left: `${x}px`,
        top: `${y}px`
      });
    });
  }

  connectedCallback() {
    const position = this.getAttribute('position') || 'top';
    const tooltip = this.getAttribute('content') || '';

    this.shadowRoot.innerHTML = `
      <style>${this._getStyles()}</style>
      <div class="tooltip">${tooltip}</div>
      <slot></slot>
    `;

    const tooltipElement = this.shadowRoot.querySelector('.tooltip');

    const showTooltip = () => {
      tooltipElement.classList.add('visible');
      this._updatePosition();
    };

    const hideTooltip = () => {
      tooltipElement.classList.remove('visible');
    };

    this.addEventListener('mouseenter', showTooltip);
    this.addEventListener('mouseleave', hideTooltip);

    // Update position on scroll and resize
    const update = () => {
      if (tooltipElement.classList.contains('visible')) {
        this._updatePosition();
      }
    };

    window.addEventListener('scroll', update, true);
    window.addEventListener('resize', update);

    // Store cleanup function
    this._cleanup = () => {
      window.removeEventListener('scroll', update, true);
      window.removeEventListener('resize', update);
      this.removeEventListener('mouseenter', showTooltip);
      this.removeEventListener('mouseleave', hideTooltip);
    };
  }

  disconnectedCallback() {
    if (this._cleanup) {
      this._cleanup();
      this._cleanup = null;
    }
  }
}

customElements.define('sprocket-tooltip', SprocketTooltip);