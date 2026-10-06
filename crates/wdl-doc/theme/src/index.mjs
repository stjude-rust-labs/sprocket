import { initManualHighlighting } from "common.js";
import Alpine from 'alpinejs';
import persist from '@alpinejs/persist';

Alpine.plugin(persist);

Alpine.store('search', {
    query: '',
    results: [],
    loading: false,
    pagefind: null,

    async init() {
        try {
            await window.pagefind.then((pagefind) => {
                this.pagefind = pagefind;
                pagefind.init()
            });
        } catch (e) {
            console.error("Failed to load Pagefind", e);
        }

        Alpine.effect(() => {
            this.performSearch(this.query);
        });
    },

    async performSearch(query) {
        if (!query || query.trim() === '') {
            this.results = [];
            this.loading = false;
            return;
        }

        this.loading = true;

        const filters = {};
        const typeFilter = query.match(/type:(\S+)/);
        if (typeFilter) {
            filters.type = typeFilter[1];
            query = query.replace(typeFilter[0], "").trim();
        }

        try {
            const search = await this.pagefind.search(query || null, {
                filters
            });

            this.results = await Promise.all(
                search.results.slice(0, 10).map(r => r.data())
            );
        } catch (e) {
            console.error("Search error:", e);
            this.results = [];
        } finally {
            this.loading = false;
        }
    },

    clear() {
        this.query = '';
        this.results = [];
    }
});

window.Alpine = Alpine;

// Build the "on this page" navigation from the headings that are actually
// rendered in the page content, preserving their order and visible nesting.
//
// `h2` headings become top-level section links; consecutive `h3` headings are
// grouped into an indented `.right-sidebar__section-items` container beneath the
// preceding `h2`, mirroring the static right-rail markup so the DOM-generated
// rail keeps the same nesting and indentation.
function buildPageSections() {
    const container = document.querySelector('[data-page-sections]');
    if (!container) return;

    container.replaceChildren();
    const used = new Set();
    let currentGroup = null;
    for (const heading of document.querySelectorAll(
        '.layout__main-center-content h2, .layout__main-center-content h3'
    )) {
        // Skip headings that are not rendered, such as the search results
        // header that stays hidden while the search query is empty.
        if (heading.getClientRects().length === 0) continue;

        let id = heading.id || heading.textContent.trim().toLowerCase()
            .replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
        const base = id || 'section';
        for (let suffix = 2; used.has(id); suffix += 1) id = `${base}-${suffix}`;
        used.add(id);
        heading.id = id;

        const link = document.createElement('a');
        link.href = `#${id}`;
        link.textContent = heading.textContent.trim();

        if (heading.tagName === 'H3') {
            link.className = 'right-sidebar__section-item';
            // Nest consecutive sub-headings under an indented group beneath the
            // most recent top-level heading.
            if (!currentGroup) {
                currentGroup = document.createElement('div');
                currentGroup.className = 'right-sidebar__section-items';
                container.append(currentGroup);
            }
            currentGroup.append(link);
        } else {
            link.className = 'right-sidebar__section-header';
            container.append(link);
            currentGroup = null;
        }
    }
}

// Show only the keyboard shortcut hint that applies to the visitor's platform:
// `⌘ K` on macOS, `Ctrl K` elsewhere. Both variants are always rendered so the
// static markup stays platform-neutral.
function applyPlatformShortcutHint() {
    const platform = navigator.userAgentData?.platform || navigator.platform || '';
    const isMac = /mac|iphone|ipad|ipod/i.test(platform);
    for (const el of document.querySelectorAll('[data-shortcut]')) {
        el.hidden = el.dataset.shortcut !== (isMac ? 'mac' : 'other');
    }
}

const SCROLL_PANES = [
    ['.layout__main-center', 'layout__main-center--scrolling'],
    [
        '.left-sidebar__content-container',
        'left-sidebar__content-container--scrolling'
    ],
];
const SCROLL_IDLE_DELAY_MS = 700;

// Reveal each documentation scrollbar while its pane is actively scrolling,
// then hide it shortly after scrolling stops. Hover and `focus-within` reveal
// scrollbars through CSS; this drives only the transient active-scroll state.
//
// Each pane receives one passive listener and reuses one timer. A data
// attribute prevents repeated initialization from accumulating handlers.
function initScrollbarAutohide() {
    for (const [selector, activeClass] of SCROLL_PANES) {
        const pane = document.querySelector(selector);
        if (!pane || pane.dataset.scrollbarAutohide === 'true') continue;
        pane.dataset.scrollbarAutohide = 'true';

        let idleTimer = null;
        pane.addEventListener(
            'scroll',
            () => {
                pane.classList.add(activeClass);
                if (idleTimer !== null) clearTimeout(idleTimer);
                idleTimer = setTimeout(() => {
                    pane.classList.remove(activeClass);
                    idleTimer = null;
                }, SCROLL_IDLE_DELAY_MS);
            },
            { passive: true }
        );
    }
}

Alpine.start();
requestAnimationFrame(buildPageSections);
applyPlatformShortcutHint();
initScrollbarAutohide();

// Highlight Markdown fenced code blocks and give them the same copy, expand,
// and line-number controls as the generated `<sprocket-code>` blocks. These
// controls are opt-in, so enabling them here keeps other web-common consumers
// unaffected.
initManualHighlighting([], { copyable: true, expandable: true, lineNumbers: true });
