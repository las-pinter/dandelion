import path from 'node:path'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { '@': path.resolve(import.meta.dirname, './src') },
  },
  server: {
    // The Rust server serves the API; the dev server proxies to it.
    // Run the server with DANDELION_PUBLIC_URL=http://localhost:5173 so the
    // WebAuthn relying party matches the page, and use http://localhost:5173.
    proxy: { '/api': { target: 'http://127.0.0.1:7443', ws: true } },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    css: false,
  },
})
