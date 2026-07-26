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
// rendered in the page content, preserving their order.
function buildPageSections() {
    const container = document.querySelector('[data-page-sections]');
    if (!container) return;

    container.replaceChildren();
    const used = new Set();
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
        link.className = heading.tagName === 'H3'
            ? 'right-sidebar__section-item'
            : 'right-sidebar__section-header';
        container.append(link);
    }
}

Alpine.start();
requestAnimationFrame(buildPageSections);

// Highlight Markdown fenced code blocks and give them the same copy, expand,
// and line-number controls as the generated `<sprocket-code>` blocks. These
// controls are opt-in, so enabling them here keeps other web-common consumers
// unaffected.
initManualHighlighting([], { copyable: true, expandable: true, lineNumbers: true });
