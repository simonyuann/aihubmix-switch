import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { fileURLToPath, URL } from 'node:url'
const root = fileURLToPath(new URL('.', import.meta.url))
import { resolve } from 'node:path'

export default defineConfig({
  root: resolve(root, 'src/renderer'),
  plugins: [react()],
  clearScreen: false,
  server: { host: '127.0.0.1', port: 1420, strictPort: true },
  build: { outDir: resolve(root, 'dist'), emptyOutDir: true, target: 'es2021' },
  envPrefix: ['VITE_', 'TAURI_ENV_']
})
