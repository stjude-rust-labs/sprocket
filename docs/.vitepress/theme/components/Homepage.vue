<script setup>
import Hero from './Hero/Hero.vue'
import Showcase from './Showcase.vue'
</script>

<template>
  <Hero>
    <slot />
  </Hero>
  <Showcase />
</template>
