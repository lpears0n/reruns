const fs = require('node:fs')
const path = require('node:path')
const Module = require('node:module')
const { chromium } = require('playwright')
const fixturePath = path.resolve(__dirname, '../tests/ui.cjs')
const fixturesModule = new Module(fixturePath, module)
fixturesModule.filename = fixturePath
fixturesModule.paths = Module._nodeModulePaths(path.dirname(fixturePath))
fixturesModule._compile(fs.readFileSync(fixturePath, 'utf8').replace('module.exports = { fixtures, mockBridge }', 'module.exports = { fixtures, mockBridge, serve, shotFixtures }'), fixturePath)
const { mockBridge, serve, shotFixtures } = fixturesModule.exports

;(async () => {
  const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH })
  try {
    for (const [width, height] of [[1280, 800], [900, 600]]) {
      const page = await browser.newPage({ viewport: { width, height }, colorScheme: 'dark' })
      page.on('pageerror', e => { throw e })
      const data = shotFixtures()
      await page.addInitScript(mockBridge, data)
      await serve(page, { failFirst: false })
      await page.goto('http://localhost:5173/')
      await page.locator('.clip-open').first().waitFor()
      await page.waitForTimeout(800)
      await page.screenshot({ path: path.resolve(__dirname, `../tests/shots/milestone-${width}-library.png`) })
      await page.evaluate(path => window[`_${window.__listeners['clip-saved']}`]({ event: 'clip-saved', id: 1, payload: path }), data.list_clips[0].path)
      await page.getByRole('region', { name: 'Notification' }).waitFor()
      await page.waitForTimeout(350)
      await page.screenshot({ path: path.resolve(__dirname, `../tests/shots/milestone-${width}-saved.png`) })
      await page.getByRole('button', { name: 'Dismiss notification' }).click()
      await page.getByRole('button', { name: 'Settings', exact: true }).click()
      await page.screenshot({ path: path.resolve(__dirname, `../tests/shots/milestone-${width}-settings.png`) })
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)
      if (overflow) throw new Error(`Horizontal overflow at ${width}`)
      await page.close()
    }
  } finally { await browser.close() }
})().catch(e => { console.error(e); process.exit(1) })
