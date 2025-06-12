<script setup>
</script>

<template>
  <section class="showcase__background">
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
                  <img src="/svg/windows.svg" alt="Windows" class="showcase__icon">
                </div>
                <span class="typo-body2 showcase__label">Windows</span>
              </div>
              <div class="showcase__item">
                <div class="showcase__icon-box">
                  <img src="/svg/macos.svg" alt="MacOS" class="showcase__icon">
                </div>
                <span class="typo-body2 showcase__label">MacOS</span>
              </div>
              <div class="showcase__item">
                <div class="showcase__icon-box">
                  <img src="/svg/linux.svg" alt="Linux" class="showcase__icon">
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
                  <img src="/svg/aws.svg" alt="Amazon AWS" class="showcase__icon">
                </div>
                <span class="typo-body2 showcase__label">Amazon AWS</span>
              </div>
              <div class="showcase__item">
                <div class="showcase__icon-box">
                  <img src="/svg/google-cloud.svg" alt="Google Cloud Provider" class="showcase__icon">
                </div>
                <span class="typo-body2 showcase__label">Google Cloud Provider</span>
              </div>
              <div class="showcase__item">
                <div class="showcase__icon-box">
                  <img src="/svg/azure.svg" alt="Azure" class="showcase__icon">
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
                  <img src="/svg/kubernetes.svg" alt="Kubernetes" class="showcase__icon">
                </div>
                <span class="typo-body2 showcase__label">Kubernetes</span>
              </div>
              <div class="showcase__item">
                <div class="showcase__icon-box">
                  <img src="/svg/tfs.svg" alt="TES" class="showcase__icon">
                </div>
                <span class="typo-body2 showcase__label showcase__label-tes-mobile">TES</span>
                <span class="typo-body2 showcase__label showcase__label-tes-desktop">Task Execution Service</span>
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
  gap: 6rem;
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
  max-width: 850px;
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
  padding: 0.625rem 1.5rem;
  background: var(--theme-blue-600);
  color: var(--theme-blue-50);
  border: 1px solid transparent;
  border-radius: 2rem;
  text-decoration: none;
  transition: all 0.2s;
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
  background: linear-gradient(to top, var(--theme-blue-800), var(--theme-blue-600));
  border: 1px solid var(--theme-blue-500);
  border-radius: 0.75rem;
  transition: all 0.2s;
}

.showcase__icon {
  width: 2rem;
  height: 2rem;
  object-fit: contain;
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

/* TES Label */
.showcase__label-tes-desktop {
  display: none;
}

@media (min-width: 1024px) {
  .showcase__label-tes-desktop {
    display: block;
  }
  
  .showcase__label-tes-mobile {
    display: none;
  }
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

  .showcase__column {
    position: relative;
  }

  .showcase__column:not(:last-child)::after {
    content: '';
    position: absolute;
    top: 0;
    right: -1.5rem;
    width: 2px;
    height: 100%;
    background-image: url("data:image/svg+xml,%3csvg width='100%25' height='100%25' xmlns='http://www.w3.org/2000/svg'%3e%3crect width='100%25' height='100%25' fill='none' stroke='rgba(255, 255, 255, 0.1)' stroke-width='2' stroke-dasharray='6%2c 14' stroke-dashoffset='0' stroke-linecap='square'/%3e%3c/svg%3e");
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

@media (min-width: 1280px) {
  .showcase__columns {
    gap: 6rem;
  }

  .showcase__column:not(:last-child)::after {
    right: -3rem;
  }
}
</style>