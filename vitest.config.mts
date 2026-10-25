import { defineConfig } from 'vitest/config'
import { fileURLToPath, URL } from 'node:url'
const root = fileURLToPath(new URL('.', import.meta.url))
import { resolve } from 'node:path'

export default defineConfig({
  test: {
    include: ['tests/**/*.test.{ts,tsx}'],
    environment: 'node',
    testTimeout: 15000
  },
  resolve: {
    alias: {
      '@shared': resolve(root, 'src/shared')
    }
  }
})
