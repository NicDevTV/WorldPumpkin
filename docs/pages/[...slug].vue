<script setup lang="ts">
const route = useRoute()
const path = computed(() => {
  const slug = route.params.slug
  if (!slug || slug.length === 0) {
    return '/'
  }
  return `/${Array.isArray(slug) ? slug.join('/') : slug}`
})

const { data: page } = await useAsyncData(`content:${path.value}`, () => {
  return queryCollection('docs').path(path.value).first()
})

const navigation = [
  { title: 'Overview', path: '/' },
  { title: 'Installation', path: '/installation' },
  { title: 'Commands', path: '/commands' },
  { title: 'Configuration', path: '/configuration' },
  { title: 'Examples', path: '/examples' },
]
const runtimeConfig = useRuntimeConfig()
const iconSrc = `${runtimeConfig.app.baseURL}icon.png`
const iconFailed = ref(false)

if (!page.value) {
  throw createError({ statusCode: 404, statusMessage: 'Page not found' })
}

useSeoMeta({
  title: () => `${page.value?.title || 'Wiki'} - WorldPumpkin`,
  description: () => page.value?.description || 'WorldPumpkin documentation',
})
</script>

<template>
  <div class="site-shell">
    <aside class="sidebar">
      <NuxtLink class="brand" to="/">
        <img
          v-if="!iconFailed"
          class="brand-icon"
          :src="iconSrc"
          alt=""
          @error="iconFailed = true"
        >
        <span v-else class="brand-mark">WP</span>
        <span>
          <strong>WorldPumpkin</strong>
          <small>Wiki</small>
        </span>
      </NuxtLink>

      <nav class="nav" aria-label="Wiki navigation">
        <NuxtLink
          v-for="item in navigation"
          :key="item.path"
          :to="item.path"
        >
          {{ item.title }}
        </NuxtLink>
      </nav>
    </aside>

    <main class="content">
      <article class="prose">
        <ContentRenderer :value="page" />
      </article>
    </main>
  </div>
</template>
