import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { fileURLToPath } from 'node:url'

const server = process.env.SERVER_URL ?? 'http://127.0.0.1:3000'
const mock = process.env.VITE_MOCK ?? '0'

export default defineConfig({
  plugins: [react()],
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  define: { 'import.meta.env.VITE_MOCK': JSON.stringify(mock) },
  server: { port: 5199, proxy: { '/api': { target: server, changeOrigin: true } } },
  build: { target: 'es2020', chunkSizeWarningLimit: 4000 },
})
