<script setup lang="ts">
const { public: config } = useRuntimeConfig()
</script>

<template>
  <p class="artifact-file">
    <span>File</span>
    <code>{{ config.artifactName }}</code>
  </p>
</template>
