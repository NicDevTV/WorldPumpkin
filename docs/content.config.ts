import { defineCollection, defineContentConfig, z } from '@nuxt/content'

export default defineContentConfig({
  collections: {
    docs: defineCollection({
      type: 'page',
      source: '**/*.md',
      schema: z.object({
        title: z.string(),
        description: z.string().optional(),
        navigation: z
          .object({
            title: z.string().optional(),
            order: z.number().optional(),
          })
          .optional(),
      }),
    }),
  },
})
