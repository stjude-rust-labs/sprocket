<script setup>
import { ref, onMounted, onBeforeUnmount } from 'vue'

const videoRef = ref(null)
const observer = ref(null)

onMounted(() => {
  if ('IntersectionObserver' in window) {
    observer.value = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting && videoRef.value) {
            videoRef.value.play()
            // Once played, disconnect the observer since we don't need to watch anymore
            observer.value.disconnect()
          }
        })
      },
      { threshold: 0.2 }
    )

    if (videoRef.value) {
      observer.value.observe(videoRef.value)
    }
  }
})

onBeforeUnmount(() => {
  if (observer.value) {
    observer.value.disconnect()
  }
})
</script>

<template>
  <section class="homepage__background homepage__background--video">
    <div class="container">
      <div class="homepage__content">
        <!-- Header -->
        <div class="homepage__header">
          <h2 class="typo-h1 homepage__title">
            How
            <span class="homepage__title-highlight">Sprocket Powers</span>
            Your Workflows
          </h2>
          <p class="typo-body1 homepage__subtitle">Sprocket is built for speed and efficiency, orchestrating complex WDL-based workflows with the power of high-performance computing. </p>
          <a href="/overview" class="typo-btn homepage__btn">
            Explore Documentation
          </a>
          <div class="homepage__video">
            <!-- Sprocket_Video.mp4 -->
            <video ref="videoRef" src="/Sprocket_Video.mp4" loop muted playsinline></video>
          </div>
        </div>
      </div>
    </div>
  </section>
  <section class="homepage__background homepage__background--glow">
    <div class="container">
      <div class="homepage__content" style="padding-top: 6rem; padding-bottom: 6rem;">
        <!-- Header -->
        <div class="homepage__header">
          <h2 class="typo-h1 homepage__title">
            Join
            <span class="homepage__title-highlight">Our Community</span>
          </h2>
          <p class="typo-body1 homepage__subtitle">Connect with fellow researchers and developers, stay updated on the latest features,and share ideas to push the boundaries of bioinformatics.</p>
          <a href="/overview" class="typo-btn homepage__btn homepage__btn--slack">
            <span class="homepage__btn-slack-icon"></span> Join Slack
          </a>
        </div>
      </div>
    </div>
  </section>
  <footer class="container homepage__footer">
    <img src="/sprocket-logo-dark.png" alt="Sprocket Logo" style="height: 35px;">
    <div class="homepage__footer-links">
      <a class="typo-body2" href="/terms">Terms & Conditions</a>
      <a class="typo-body2" href="/privacy">Privacy Policy</a>
    </div>
  </footer>
</template>

<style scoped>
/* ========================================
  Layout & Container Styles
  ======================================== */
.homepage__background {
  padding: 4rem 0;
  color: var(--theme-blue-50);
}

.homepage__background--video video {
  width: 100%;
}

.homepage__background--glow {
  background:
    radial-gradient(ellipse 50% 50% at 50% 100%, #713FAA 0%, transparent 100%);
}

.homepage__content {
  display: flex;
  flex-direction: column;
  gap: 6rem;
}

/* ========================================
  Header Styles
  ======================================== */
.homepage__header {
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2rem;
}

.homepage__title {
  color: var(--theme-blue-50);
  max-width: 38rem;
}

.homepage__title-highlight {
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

.homepage__subtitle {
  color: var(--theme-blue-100);
  max-width: 41.25rem;
}

.homepage__btn {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  min-height: 2.5882352941rem;
  padding: 0.5rem 1.5rem;
  background: var(--theme-blue-600);
  color: var(--theme-blue-50);
  border: 1px solid transparent;
  border-radius: 2rem;
  text-decoration: none;
  transition: all 0.2s;
  cursor: pointer;
}

.homepage__btn:hover {
  background: var(--theme-blue-500);
  border-color: var(--theme-violet-800);
}

.homepage__btn--slack {
  background: rgba(255, 255, 255, 0.1);
  color: #fff;
}

.homepage__btn--slack:hover {
  border-color: var(--theme-violet-800);
  background: var(--theme-blue-600);
}

.homepage__btn-slack-icon {
  display: inline-block;
  width: 1.2em;
  height: 1.2em;
  background: url("https://a.slack-edge.com/80588/marketing/img/icons/icon_slack_hash_colored.png")
    no-repeat center/contain;
}

.homepage__video {
  margin-top: 5rem;
  width: 100%;
  aspect-ratio: 16 / 9;
  background: linear-gradient(135deg, var(--theme-blue-400), var(--theme-blue-900));
}

/* ========================================
  Responsive Layout
  ======================================== */
@media (min-width: 1024px) {
  .homepage__background {
    padding: 6rem 0;
  }
}

.homepage__footer {
  padding-top: 4rem;
  padding-bottom: 4rem;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  align-items: center;
  gap: 1rem;
}

@media (min-width: 768px) {
  .homepage__footer {
    flex-direction: row;
  }
}

.homepage__footer-links {
  display: flex;
  gap: 1rem;
  color: var(--theme-blue-100);
  transition: all 0.2s;
}

.homepage__footer-links a:hover {
  color: var(--theme-gradient-stop-middle);
}
</style>