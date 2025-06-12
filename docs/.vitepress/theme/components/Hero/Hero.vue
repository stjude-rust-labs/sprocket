<script setup>
import PlatformCodePreview from "./PlatformCodePreview.vue";
import CodePreview from "./CodePreview.vue";
</script>

<template>
  <main class="hero__background">
    <div class="container">
      <div class="hero__content">
        <!-- Left: Headline and Actions -->
        <section class="hero__left-column">
          <h1 class="typo-h1 hero__title">
            <span class="hero__title-gradient">Sprocket:</span>
            The Bioinformatics Workflow Engine
          </h1>
          <p class="typo-body1 hero__subtitle">
            Sprocket is a high-performance, modern, and open-source workflow
            engine for bioinformatics. Create, test, and run your analyses
            locally, then seamlessly move to HPC or the cloud to handle
            thousands of parallel tasks.
          </p>
          <div class="hero__actions">
            <a href="#" class="typo-btn hero__btn hero__btn--primary">
              <span>↗</span> Explore Documentation
            </a>
            <a href="#" class="typo-btn hero__btn hero__btn--slack">
              <span class="hero__btn-slack-icon"></span> Join Slack
            </a>
          </div>
        </section>
        <!-- Right: Code Cards -->
        <section class="hero__right-column">
          <!-- Card 1: Install -->
          <PlatformCodePreview
            windows-command="cargo install sprocket"
            mac-command="cargo install sprocket"
            unix-command="cargo install sprocket"
          />

          <!-- Card 2: WDL Example -->
          <div class="card">
            <CodePreview header="WDL" preformatted>
              <slot />
            </CodePreview>
          </div>

          <!-- Card 3: Run Example -->
          <PlatformCodePreview
            windows-command="sprocket run example.wdl -vvv"
            mac-command="sprocket run example.wdl -vvv"
            unix-command="sprocket run example.wdl -vvv"
          />
        </section>
      </div>
    </div>
  </main>
</template>

<style scoped>
/* ========================================
  Layout & Container Styles
  ======================================== */
.hero__background {
  padding: 3rem 0 3rem 0;
  display: flex;
  align-items: center;
  color: #fff;
  background:
    radial-gradient(circle at top right, #7035b4 0%, transparent 55%),
    radial-gradient(circle at bottom right, #6e89be 0%, transparent 45%),
    var(--theme-blue-900);
}

.hero__content {
  display: flex;
  gap: 2rem;
  flex-direction: column;
}

.hero__title {
  max-width: 700px;
}

.hero__title-gradient {
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

.hero__subtitle {
  margin-top: 2.25rem;
  color: var(--theme-blue-100);
  max-width: 700px;
}

.hero__actions {
  margin-top: 3.75rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
  width: 100%;
}

/* Tablet and up */
@media (min-width: 768px) {
  .hero__actions {
    flex-direction: row;
    width: auto;
  }
}

/* ========================================
  Hero Content Section
  ======================================== */
.hero__left-column {
  width: 100%;
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.hero__right-column {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 1.3333333333rem;
}

/* ========================================
  Buttons
  ======================================== */
.hero__btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.5rem;
  min-height: 2.5882352941rem;
  padding: 0.5rem 1.5rem;
  border: none;
  border-radius: 2rem;
  text-decoration: none;
  cursor: pointer;
  transition: background 0.2s;
}

.hero__btn--primary {
  background: var(--theme-violet-800);
  color: #fff;
}

.hero__btn--primary:hover {
  background: var(--theme-violet-900);
}

.hero__btn--slack {
  background: #23272d;
  color: #fff;
  border: 1px solid transparent;
}

.hero__btn--slack:hover {
  border-color: var(--theme-violet-800);
  background: var(--theme-blue-600);
}

.hero__btn-slack-icon {
  display: inline-block;
  width: 1.2em;
  height: 1.2em;
  background: url("https://a.slack-edge.com/80588/marketing/img/icons/icon_slack_hash_colored.png")
    no-repeat center/contain;
}

/* ========================================
  Responsive Layout
  ======================================== */
@media (min-width: 1025px) {
  .hero__background {
    padding: 6.25rem 0 6.25rem 0;
  }
  .hero__content {
    flex-direction: row;
  }

  .hero__left-column {
    flex: 1;
  }

  .hero__right-column {
    flex: 0 0 500px;
  }
}
</style>
