import "common.js";
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
Alpine.start();
