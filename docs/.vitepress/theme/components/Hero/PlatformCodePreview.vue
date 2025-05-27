<script setup>
import { ref, computed } from "vue";
import CodePreview from "./CodePreview.vue";

const props = defineProps({
  windowsCommand: {
    type: String,
    required: true,
  },
  macCommand: {
    type: String,
    required: true,
  },
  unixCommand: {
    type: String,
    required: true,
  },
});

const activeTab = ref("Windows");
const currentCommand = computed(() => {
  switch (activeTab.value) {
    case "Windows":
      return props.windowsCommand;
    case "Mac":
      return props.macCommand;
    case "Unix":
      return props.unixCommand;
    default:
      return props.windowsCommand;
  }
});
</script>

<template>
  <div class="card">
    <div class="card-header">
      <span class="typo-caption3" style="color: var(--theme-blue-200)"
        >RUN WITH</span
      >
      <div class="tabs">
        <button
          v-for="tab in ['Windows', 'Mac', 'Unix']"
          :key="tab"
          class="typo-caption2"
          :class="{ active: activeTab === tab }"
          @click="activeTab = tab"
        >
          {{ tab }}
        </button>
      </div>
    </div>
    <CodePreview :value="currentCommand" />
  </div>
</template>

<style scoped>
/* ========================================
  Card Header Layout
  ======================================== */
.card-header {
  display: flex;
  align-items: center;
  gap: 1rem;
  margin-bottom: 0.5rem;
}

/* ========================================
  Tab Navigation
   ======================================== */
.tabs {
  display: flex;
  gap: 0.3rem;
  padding: 2px;
  border: 1px solid var(--theme-blue-400);
  border-radius: 50px;
}

.tabs button {
  padding: 2px 6px;
  background: none;
  border: none;
  border-radius: 40px;
  color: var(--theme-lilac-100);
  cursor: pointer;
  transition: background 0.15s;
}

.tabs button.active,
.tabs button:hover {
  background: var(--theme-lilac-100);
  color: var(--theme-blue-900);
}
</style>
