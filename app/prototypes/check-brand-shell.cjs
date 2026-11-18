const fs = require('node:fs')
const path = require('node:path')
const Module = require('node:module')
const { chromium } = require('playwright')
process.env.RERUNS_SHOT_FRAMES ||= path.resolve(__dirname, '../tests/local-assets')
process.env.RERUNS_STEAM_ART ||= path.resolve(__dirname, '../tests/local-assets/steam')
process.env.RERUNS_SHOT_CLIP ||= path.resolve(__dirname, '../tests/local-assets/clip.webm')
const fixturePath = path.resolve(__dirname, '../tests/ui.cjs')
const fixturesModule = new Module(fixturePath, module)
fixturesModule.filename = fixturePath
fixturesModule.paths = Module._nodeModulePaths(path.dirname(fixturePath))
fixturesModule._compile(fs.readFileSync(fixturePath, 'utf8').replace('module.exports = { fixtures, mockBridge }', 'module.exports = { fixtures, mockBridge, serve, shotFixtures }'), fixturePath)
const { mockBridge, serve, shotFixtures } = fixturesModule.exports
;(async () => {
  const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH || 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' })
  try {
    for (const [width,height] of [[1280,800],[900,600]]) {
      const page = await browser.newPage({viewport:{width,height},colorScheme:'dark'})
      await page.addInitScript(mockBridge,shotFixtures())
      await serve(page,{failFirst:false})
      await page.goto('http://localhost:5173/')
      await page.locator('.clip-open').first().waitFor()
      await page.waitForTimeout(500)
      await page.locator('.media-card').first().hover()
      await page.waitForTimeout(200)
      await page.screenshot({path:path.resolve(__dirname,`../tests/shots/brand-audit-${width}-hover.png`)})
      await page.locator('.clip-open').first().click()
      await page.getByRole('button',{name:'Back to the library',exact:true}).waitFor()
      await page.waitForTimeout(500)
      console.log('viewer',width,await page.locator('.picture-stage').evaluate(el=>({background:getComputedStyle(el,'::before').backgroundImage,opacity:getComputedStyle(el,'::before').opacity,display:getComputedStyle(el,'::before').display})))
      await page.screenshot({path:path.resolve(__dirname,`../tests/shots/brand-audit-${width}-viewer.png`)})
      await page.close()
    }
  } finally {await browser.close()}
})().catch(e=>{console.error(e);process.exit(1)})
