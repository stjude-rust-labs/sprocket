<script setup>
import { ref, nextTick } from 'vue';

const props = defineProps({
  header: {
    type: String,
    default: ''
  },
  preformatted: {
    type: Boolean,
    default: false
  },
  value: {
    type: String,
    default: ''
  }
});

const previewRef = ref(null);
const copyText = ref('Copy');

const copyToClipboard = async () => {
  await nextTick();
  let text = '';
  // Prefer the value prop; otherwise extract textContent from slot
  if (props.value) {
    text = props.value;
  } else if (previewRef.value) {
    text = previewRef.value.innerText || '';
  }
  if (text) {
    await navigator.clipboard.writeText(text);
    copyText.value = 'Copied!';
    setTimeout(() => {
      copyText.value = 'Copy';
    }, 3000);
  }
};
</script>

<template>
  <div class="code-preview" ref="previewRef">
    <div class="code-preview__header" v-if="header">
      <span class="code-preview__header-text typo-caption3" style="color: var(--theme-blue-200);">{{ header }}</span>
    </div>
    <button @click="copyToClipboard" class="code-preview__copy-button">
      <img v-if="copyText === 'Copy'" src="/svg/heroicons-outline-document-duplicate.svg" alt="Copy" class="code-preview__copy-icon">
      <img v-else src="/svg/heroicons-outline-check.svg" alt="Copied" class="code-preview__copy-icon">
    </button>
    <div v-if="$slots.default && preformatted" class="code-preview__block">
      <slot></slot>
    </div>
    <pre class="code-preview__block" v-else><code><slot></slot>{{ value }}</code></pre>
  </div>
</template>

<style scoped>
.code-preview {
  position: relative;
}

.code-preview__copy-button {
  position: absolute;
  top: 0;
  right: 0;
  padding: 0.1rem 0.25rem;
  color: var(--theme-blue-100);
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.6rem;
  transition: all 0.2s;
}

.code-preview__copy-button:hover {
  background-color: rgba(0, 0, 0, 0.2);
}

.code-preview__copy-button:hover {
  background-color: rgba(0, 0, 0, 0.4);
}

.code-preview__header {
  display: flex;
  align-items: center;
  gap: 1rem;
  margin-bottom: 0.5rem;
}

.code-preview__block {
  margin-top: 0;
  margin-bottom: 0;
  background: none;
  color: var(--theme-blue-100);
  font-family: "Fira Mono", "Menlo", "Monaco", "Consolas", monospace;
  font-size: 1rem;
  overflow-x: auto;
}
</style>
