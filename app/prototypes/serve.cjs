// Start one prototype server: `node prototypes/serve.cjs <a|b|base>`.
// a → 5174, b → 5175, base (the app as it is) → 5176.
const PORTS = { a: 5174, b: 5175, base: 5176 }
const which = process.argv[2]
if (!(which in PORTS)) throw new Error('usage: node prototypes/serve.cjs <a|b|base>')
process.env.PROTO = which

import('vite').then(async ({ createServer }) => {
  const server = await createServer({ configFile: require('node:path').join(__dirname, 'vite.config.ts'), server: { port: PORTS[which] } })
  await server.listen()
  server.printUrls()
})
