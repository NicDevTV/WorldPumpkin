import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

const cargoToml = readFileSync(resolve('..', 'Cargo.toml'), 'utf8')
const pluginVersion =
  cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1] || '0.0.0'
const artifactName = 'world_pumpkin.wasm'

export default defineNuxtConfig({
  modules: ['@nuxt/content'],
  css: ['~/assets/css/main.css'],
  runtimeConfig: {
    public: {
      pluginVersion,
      artifactName,
      releaseUrl: `https://github.com/NicDevTV/WorldPumpkin/releases/latest/download/${artifactName}`,
    },
  },
  app: {
    baseURL: process.env.NUXT_APP_BASE_URL || '/',
    head: {
      title: 'WorldPumpkin Wiki',
      meta: [
        {
          name: 'description',
          content: 'Documentation for the WorldPumpkin Pumpkin server plugin.',
        },
      ],
    },
  },
  compatibilityDate: '2026-07-03',
})
