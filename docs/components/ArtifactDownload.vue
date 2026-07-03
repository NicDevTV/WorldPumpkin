<script setup lang="ts">
const { public: config } = useRuntimeConfig()
</script>

<template>
  <a class="download-button" :href="config.releaseUrl">
    Download latest release
  </a>
</template>
