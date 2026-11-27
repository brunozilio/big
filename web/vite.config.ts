import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { fileURLToPath } from 'node:url'

const server = process.env.SERVER_URL ?? 'http://127.0.0.1:3000'
const mock = process.env.VITE_MOCK ?? '0'

const vendors: [string, RegExp][] = [
  ['vendor-react', /\/node_modules\/(react|react-dom|scheduler|react-router|react-router-dom|@remix-run\/router)\//],
  ['vendor-ui', /\/node_modules\/(clsx|tailwind-merge|class-variance-authority)\//],
  ['vendor-motion', /\/node_modules\/(framer-motion|motion-dom|motion-utils)\//],
  ['vendor-radix', /\/node_modules\/(@radix-ui|@floating-ui|react-remove-scroll|react-remove-scroll-bar|react-style-singleton|use-callback-ref|use-sidecar|aria-hidden)\//],
  ['vendor-editor', /\/node_modules\/(@tiptap|prosemirror-[^/]+|tiptap-markdown|markdown-it|markdown-it-task-lists|linkifyjs|tippy\.js|@popperjs)\//],
  ['vendor-code', /\/node_modules\/(@codemirror|@lezer|@marijn|style-mod|w3c-keyname|crelt)\//],
  ['vendor-markdown', /\/node_modules\/(react-markdown|remark-[^/]+|mdast-[^/]+|micromark[^/]*|unified|unist-[^/]+|hast-[^/]+|vfile[^/]*)\//],
  ['vendor-charts', /\/node_modules\/(recharts|recharts-scale|react-smooth|victory-vendor|d3-[^/]+|decimal\.js-light|internmap)\//],
  ['vendor-avatars', /\/node_modules\/@dicebear\//],
]

function vendorChunk(id: string): string | undefined {
  return vendors.find(([, match]) => match.test(id))?.[0]
}

export default defineConfig({
  base: './',
  plugins: [react()],
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  define: { 'import.meta.env.VITE_MOCK': JSON.stringify(mock) },
  server: { port: 5199, proxy: { '/api': { target: server, changeOrigin: true } } },
  build: { target: 'es2020', chunkSizeWarningLimit: 600, rollupOptions: { output: { manualChunks: vendorChunk } } },
})
