// Motion, recorded: opens the first clip from the library, steps to the
// next one, goes back, so the open/return continuity is judged from frames
// rather than stills. Frames come from the browser's own screencast, with
// their timestamps, so gaps (dropped frames) show too.
//
//   node prototypes/motion.cjs <base|a|b> [...]
//
// Same environment as shots.cjs. Writes prototypes/shots/motion-<build>-open.jpg
// and -back.jpg (the frames of the first 400 ms of each), and prints the frame
// intervals.
const { chromium } = require('playwright')
const path = require('node:path')
const fs = require('node:fs')
const { execFileSync } = require('node:child_process')
const { library, serve, NOW, PORTS } = require('./shots.cjs')
const { mockBridge } = require('../tests/ui.cjs')

const out = path.join(__dirname, 'shots')

;(async () => {
  const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH })
  for (const build of process.argv.slice(2)) {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, colorScheme: 'dark' })
    await page.clock.setFixedTime(NOW)
    await page.addInitScript(mockBridge, library())
    await serve(page)
    await page.goto(`http://localhost:${PORTS[build]}/`)
    await page.locator('.clip-grid > *').first().waitFor()
    await page.waitForTimeout(1200)

    const cdp = await page.context().newCDPSession(page)
    const frames = []
    cdp.on('Page.screencastFrame', async (f) => {
      frames.push({ t: f.metadata.timestamp * 1000, data: f.data })
      await cdp.send('Page.screencastFrameAck', { sessionId: f.sessionId }).catch(() => {})
    })
    await cdp.send('Page.startScreencast', { format: 'jpeg', quality: 70, maxWidth: 720, maxHeight: 450, everyNthFrame: 1 })
    await page.waitForTimeout(300)
    // Wall time on this machine: the page's own clock is fixed for the shots.
    const now = () => Date.now()
    const openAt = now()
    await page.locator('.clip-open').nth(0).click()
    await page.waitForTimeout(1200)
    await page.keyboard.press('ArrowRight')
    await page.waitForTimeout(1000)
    const backAt = now()
    await page.keyboard.press('Escape')
    await page.waitForTimeout(1000)
    await cdp.send('Page.stopScreencast')
    await page.close()

    for (const [name, at] of [['open', openAt], ['back', backAt]]) {
      const run = frames.filter((f) => f.t >= at - 20 && f.t <= at + 420)
      if (!run.length) { console.log(build, name, 'no frames; first frame at', Math.round((frames[0]?.t ?? 0) - at), 'ms'); continue }
      const dir = path.join(out, `tmp-${build}-${name}`)
      fs.rmSync(dir, { recursive: true, force: true })
      fs.mkdirSync(dir)
      // At most ten, spread over the run, labelled by their time.
      const pick = run.length <= 10 ? run : Array.from({ length: 10 }, (_, i) => run[Math.round((i * (run.length - 1)) / 9)])
      pick.forEach((f, i) => fs.writeFileSync(path.join(dir, `${String(i).padStart(2, '0')}.jpg`), Buffer.from(f.data, 'base64')))
      execFileSync('ffmpeg', ['-loglevel', 'error', '-y', '-framerate', '1', '-i', path.join(dir, '%02d.jpg'), '-vf', 'scale=480:-1,tile=5x2:padding=6:color=0x101010', '-frames:v', '1', path.join(out, `motion-${build}-${name}.jpg`)])
      fs.rmSync(dir, { recursive: true })
      const gaps = run.slice(1).map((f, i) => Math.round(f.t - run[i].t))
      console.log(`${build} ${name}: ${run.length} frames in 420 ms; at ${pick.map((f) => Math.round(f.t - at)).join(' ')} ms; longest gap ${Math.max(0, ...gaps)} ms`)
    }
  }
  await browser.close()
})().catch((e) => {
  console.error(e)
  process.exit(1)
})
