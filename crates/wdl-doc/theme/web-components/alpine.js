import Alpine from 'alpinejs';
import persist from '@alpinejs/persist';

Alpine.plugin(persist);

Alpine.data('search', () => ({
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

        this.$watch('query', (value) => {
            this.performSearch(value);
        });
    },

    async performSearch(term) {
        if (!term || term.trim() === '') {
            this.results = [];
            this.loading = false;
            return;
        }

        this.loading = true;

        try {
            const search = await this.pagefind.search(term);

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
}));

window.Alpine = Alpine;
Alpine.start();
