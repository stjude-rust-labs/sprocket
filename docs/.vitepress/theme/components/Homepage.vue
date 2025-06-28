<script setup>
import Hero from './Hero/Hero.vue'
import Showcase from './Showcase.vue'
import HomepageContent from './HomepageContent.vue'
</script>

<template>
  <Hero>
    <slot />
  </Hero>
  <Showcase />
  <HomepageContent />
</template>
