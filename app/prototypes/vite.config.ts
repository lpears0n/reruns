// Design prototypes: the real app, with some of its components swapped for a
// direction's own copies. Production files are never edited; a direction is
// the folder `prototypes/<a|b>/`, and any file there whose name matches one in
// `src/lib/` (or `src/App.svelte`) replaces it. Imports inside an override
// that it doesn't have itself resolve as they would from the original file.
//
//   PROTO=a    pnpm exec vite --config prototypes/vite.config.ts --port 5174
//   PROTO=b    pnpm exec vite --config prototypes/vite.config.ts --port 5175
//   PROTO=base pnpm exec vite --config prototypes/vite.config.ts --port 5176
//
// See prototypes/README.md.
import { existsSync } from 'node:fs'
import { basename, dirname, join, resolve } from 'node:path'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig, normalizePath, type Plugin } from 'vite'

const app = normalizePath(resolve(import.meta.dirname, '..'))
const lib = `${app}/src/lib`
const src = `${app}/src`
const direction = process.env.PROTO && process.env.PROTO !== 'base' ? process.env.PROTO : null

function overrides(dir: string | null): Plugin {
  const own = dir ? `${app}/prototypes/${dir}` : ''
  /** Where the production file an override replaces lives. */
  const home = (file: string) => (basename(file) === 'App.svelte' ? src : lib)
  const swap = (id: string) => {
    const clean = normalizePath(id.split('?')[0])
    if (!own || !(clean.startsWith(`${lib}/`) || clean === `${src}/App.svelte`)) return null
    const candidate = `${own}/${basename(clean)}`
    return existsSync(candidate) ? candidate : null
  }
  return {
    name: 'reruns-prototype-overrides',
    enforce: 'pre',
    async resolveId(source, importer, options) {
      if (!own || !importer || !source.startsWith('.')) return null
      const from = normalizePath(importer.split('?')[0])
      let found = await this.resolve(source, importer, { ...options, skipSelf: true })
      if (!found && from.startsWith(`${own}/`)) {
        // An override importing a sibling it doesn't have: as from the original.
        found = await this.resolve(source, join(home(from), basename(from)), { ...options, skipSelf: true })
      }
      if (!found) return null
      return swap(found.id) ?? found
    },
    transformIndexHtml(html) {
      if (!own || !existsSync(`${own}/theme.css`)) return html
      return html.replace('</head>', `  <link rel="stylesheet" href="/prototypes/${dir}/theme.css" />\n  </head>`)
    },
  }
}

export default defineConfig({
  root: app,
  plugins: [overrides(direction), tailwindcss(), svelte({ emitCss: false, configFile: resolve(app, 'svelte.config.js') })],
  clearScreen: false,
  // Each direction's own cache, so two servers can run side by side.
  cacheDir: resolve(app, `node_modules/.vite-proto-${direction ?? 'base'}`),
  server: {
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**', '**/prototypes/shots/**'] },
  },
})
