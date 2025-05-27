<script setup>
</script>

<template>
  <section class="vp-raw showcase__background">
    <div class="container">
      <div class="showcase__content">
        <!-- Header -->
        <div class="showcase__header">
          <h2 class="typo-h1 showcase__title">
            Designed to Work with Your Tools,
            <span class="showcase__title-highlight">on Your Infrastructure</span>
          </h2>
          <a href="/overview" class="typo-btn showcase__btn">
            Explore Documentation
          </a>
        </div>

        <!-- Columns -->
        <div class="showcase__columns">
          <!-- Column 1: Local Environments -->
          <div class="showcase__column">
            <h3 class="typo-body2 showcase__column-title">Local Environments</h3>
            <h4 class="typo-h2 showcase__column-subtitle">Where Sprocket Runs</h4>
            
            <div class="showcase__items">
              <div class="showcase__item">
                <div class="showcase__icon-box">
                  <span class="showcase__icon">🪟</span>
                </div>
                <span class="typo-body2 showcase__label">Windows</span>
              </div>
              <div class="showcase__item">
                <div class="showcase__icon-box">
                  <span class="showcase__icon">🖥️</span>
                </div>
                <span class="typo-body2 showcase__label">MacOS</span>
              </div>
              <div class="showcase__item">
                <div class="showcase__icon-box">
                  <span class="showcase__icon">🐧</span>
                </div>
                <span class="typo-body2 showcase__label">Linux</span>
              </div>
            </div>
          </div>

          <!-- Column 2: Cloud Providers -->
          <div class="showcase__column">
            <h3 class="typo-body2 showcase__column-title">Cloud Providers</h3>
            <h4 class="typo-h2 showcase__column-subtitle">Where the data lives</h4>
            
            <div class="showcase__items">
              <div class="showcase__item">
                <div class="showcase__icon-box">
                  <span class="showcase__icon">☁️</span>
                </div>
                <span class="typo-body2 showcase__label">Amazon AWS</span>
              </div>
              <div class="showcase__item">
                <div class="showcase__icon-box">
                  <span class="showcase__icon">🌩️</span>
                </div>
                <span class="typo-body2 showcase__label">Google Cloud Provider</span>
              </div>
              <div class="showcase__item">
                <div class="showcase__icon-box">
                  <span class="showcase__icon">🔷</span>
                </div>
                <span class="typo-body2 showcase__label">Azure</span>
              </div>
            </div>
          </div>

          <!-- Column 3: Supported Technologies -->
          <div class="showcase__column">
            <h3 class="typo-body2 showcase__column-title">Supported Technologies</h3>
            <h4 class="typo-h2 showcase__column-subtitle">Where the execution happens</h4>
            
            <div class="showcase__items">
              <div class="showcase__item">
                <div class="showcase__icon-box">
                  <span class="showcase__icon">☸️</span>
                </div>
                <span class="typo-body2 showcase__label">Kubernetes</span>
              </div>
              <div class="showcase__item">
                <div class="showcase__icon-box">
                  <span class="showcase__icon">📋</span>
                </div>
                <span class="typo-body2 showcase__label">TES</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* ========================================
  Layout & Container Styles
  ======================================== */
.showcase__background {
  padding: 4rem 0;
  background: var(--theme-blue-900);
  color: var(--theme-blue-50);
}

.showcase__content {
  display: flex;
  flex-direction: column;
  gap: 4rem;
}

/* ========================================
  Header Styles
  ======================================== */
.showcase__header {
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2rem;
}

.showcase__title {
  color: var(--theme-blue-50);
}

.showcase__title-highlight {
  background: linear-gradient(
    90deg,
    var(--theme-gradient-stop-start),
    var(--theme-gradient-stop-middle),
    var(--theme-gradient-stop-end)
  );
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
  color: transparent;
}

.showcase__btn {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.75rem 2rem;
  background: var(--theme-blue-600);
  color: var(--theme-blue-50);
  border: 1px solid var(--theme-blue-400);
  border-radius: 2rem;
  text-decoration: none;
  transition: all 0.2s;
  font-weight: 500;
}

.showcase__btn:hover {
  background: var(--theme-blue-500);
  border-color: var(--theme-violet-800);
}

/* ========================================
  Columns Layout
  ======================================== */
.showcase__columns {
  display: grid;
  grid-template-columns: 1fr;
  gap: 3rem;
}

.showcase__column {
  text-align: center;
}

.showcase__column-title {
  color: var(--theme-blue-200);
  margin-bottom: 0.5rem;
}

.showcase__column-subtitle {
  color: var(--theme-blue-50);
  margin-bottom: 2rem;
}

/* ========================================
  Items Grid
  ======================================== */
.showcase__items {
  display: flex;
  gap: 1.5rem;
  justify-content: center;
  flex-wrap: wrap;
}

.showcase__item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 0.75rem;
  flex: 0 0 auto;
}

.showcase__icon-box {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 4rem;
  height: 4rem;
  background: var(--theme-blue-700);
  border: 1px solid var(--theme-blue-500);
  border-radius: 0.75rem;
  transition: all 0.2s;
}

.showcase__item:hover .showcase__icon-box {
  background: var(--theme-blue-600);
  border-color: var(--theme-violet-800);
  transform: translateY(-2px);
}

.showcase__icon {
  font-size: 2rem;
}

.showcase__label {
  color: var(--theme-blue-100);
  transition: color 0.2s;
  text-align: center;
  width: 100px;
}

.showcase__item:hover .showcase__label {
  color: var(--theme-blue-50);
}


/* ========================================
  Responsive Layout
  ======================================== */
@media (min-width: 1024px) {
  .showcase__background {
    padding: 6rem 0;
  }


  .showcase__columns {
    grid-template-columns: repeat(3, 1fr);
    gap: 3rem;
  }

  .showcase__items {
    flex-direction: column;
    gap: 0;
  }

  .showcase__item {
    flex-direction: row;
    margin-bottom: 1.5rem;
  }

  .showcase__item:last-child {
    margin-bottom: 0;
  }

  .showcase__label {
    text-align: left;
    width: auto;
  }
}
</style>