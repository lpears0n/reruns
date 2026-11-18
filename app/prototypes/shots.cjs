// Pictures of the design directions side by side: the current app ("base"),
// direction A and direction B, from one library whose cards, art and footage
// agree, at the window sizes the app ships at. Not a test.
//
//   node prototypes/shots.cjs [base|a|b ...] [--only=<name substring>]
//
// Needs the three prototype servers (prototypes/README.md) and the same
// local assets as `pnpm test:ui shots`:
//   RERUNS_SHOT_FRAMES  tests/local-assets (thumb-N.jpg, sprite-N.jpg, frames.json)
//   RERUNS_STEAM_ART    Steam's appcache/librarycache
//   RERUNS_SHOT_CLIP    tests/local-assets/clip.webm (clip-16x9.webm beside it)
// Writes prototypes/shots/<build>-<width>-<what>.png.
//
// Time is fixed (Saturday 14 November 2026, 8:30 PM local) so day headings
// and session times read the same on every run. Sessions are shaped like
// the recorder's sessions.jsonl; clips sit inside them.

const { chromium } = require('playwright')
const path = require('node:path')
const fs = require('node:fs')
const { mockBridge, fixtures } = require('../tests/ui.cjs')

const PORTS = { a: 5174, b: 5175, base: 5176 }
const out = path.join(__dirname, 'shots')
fs.mkdirSync(out, { recursive: true })

const FRAMES = process.env.RERUNS_SHOT_FRAMES
const ART = process.env.RERUNS_STEAM_ART
const CLIP = process.env.RERUNS_SHOT_CLIP
if (!FRAMES || !ART || !CLIP) throw new Error('set RERUNS_SHOT_FRAMES, RERUNS_STEAM_ART and RERUNS_SHOT_CLIP (see the top of this file)')
const CLIP169 = path.join(path.dirname(CLIP), 'clip-16x9.webm')

const NOW = new Date(2026, 10, 14, 20, 30).getTime()
const at = (daysAgo, h, m) => new Date(2026, 10, 14 - daysAgo, h, m).getTime()
const LIB = '/home/u/.local/share/Steam/appcache/librarycache/'

// ------------------------------------------------------------------ library

/** Games, their logged sessions (newest first) and the clips saved in them:
 *  the real frames of frames.json, at times inside those sessions. */
const PLAN = [
  {
    game: "Tom Clancy's Rainbow Six Siege", id: 359550, playing: true,
    sessions: [[0, [18, 5], null], [3, [21, 10], [22, 40]], [6, [20, 0], [23, 5]]],
    clips: [{ f: 0, at: [0, 20, 12], title: 'Match point hold', duration: 12, markers: [4, 9] }, { f: 1, at: [0, 19, 31] }],
  },
  {
    game: 'Fortnite', id: null,
    sessions: [[0, [16, 10], [17, 35]], [1, [21, 5], [23, 20]], [5, [19, 30], [20, 45]]],
    clips: [
      { f: 8, at: [0, 17, 21] }, { f: 9, at: [0, 16, 48], favorite: true }, { f: 10, at: [0, 16, 22] },
      { f: 11, at: [1, 23, 2] }, { f: 12, at: [1, 22, 15], edit: true }, { f: 13, at: [1, 21, 40] }, { f: 14, at: [1, 21, 12] },
    ],
  },
  {
    game: 'Overwatch® 2', id: 2357570,
    sessions: [[1, [19, 0], [20, 30]]],
    clips: [{ f: 3, at: [1, 20, 11], favorite: true }, { f: 7, at: [1, 19, 24] }],
  },
  {
    game: 'Deadlock', id: 1422450,
    sessions: [[2, [20, 40], [23, 55]]],
    clips: [{ f: 17, at: [2, 23, 31], markers: [3, 14, 20.5] }, { f: 20, at: [2, 21, 58] }],
  },
  {
    game: 'Dead by Daylight', id: 381210,
    sessions: [[4, [22, 0], [23, 10]]],
    clips: [{ f: 21, at: [4, 22, 47] }],
  },
]

function library({ stress = false } = {}) {
  const base = fixtures('full')
  const games = []
  const clips = []
  for (const p of PLAN) {
    const sessions = p.sessions.map(([d, [h, m], end]) => ({
      game: p.game, steam_app_id: p.id, start_ms: at(d, h, m), end_ms: end ? at(d, end[0], end[1]) : NOW,
    }))
    const art = p.id ? { capsule: `${LIB}${p.id}/library_600x900.jpg`, hero: `${LIB}${p.id}/library_hero.jpg`, logo: `${LIB}${p.id}/logo.png` } : { capsule: null, hero: null, logo: null }
    games.push({
      name: p.game, steam_app_id: p.id, art, fallback_image: null,
      last_played_ms: sessions[0].end_ms, last_activity_ms: at(...p.clips[0].at),
      total_play_ms: sessions.reduce((n, s) => n + s.end_ms - s.start_ms, 0),
      clip_count: p.clips.length, clip_bytes: p.clips.length * 1.2e8, sessions, playing: !!p.playing,
    })
    for (const c of p.clips) {
      clips.push({
        path: `/home/u/Videos/Reruns/${p.game}/clip-${c.f}.mp4`, game: p.game, name: `clip-${c.f}`, created_ms: at(...c.at),
        size_bytes: 40e6 + c.f * 7e6, duration: c.duration ?? 15 + (c.f % 4) * 9, audio_tracks: 2, is_edit: !!c.edit,
        steam_app_id: p.id, thumbnail: `/thumbs/${c.f}.svg`, sprite: `/sprites/${c.f}.svg`, sprite_frames: 10,
        title: c.title ?? null, favorite: !!c.favorite, labels: [], markers: c.markers ?? [],
      })
    }
  }
  clips.sort((a, b) => b.created_ms - a.created_ms)
  games.sort((a, b) => b.last_activity_ms - a.last_activity_ms)
  if (stress) {
    // The longest things a real library holds, on the clips shown first.
    clips[1].title = 'The one where the whole lobby loses it on Chalet with 3 HP left and a defuser in hand (FINAL v12)'
    clips[3].title = 'Benachrichtigungseinstellungen_IMG_20250914_183022_HDR_portrait_edited'
    games[1].name = 'PLAYERUNKNOWN’S BATTLEGROUNDS: Experimental Server'
    for (const c of clips) if (c.game === 'Fortnite') c.game = games[1].name
    for (const s of games[1].sessions) s.game = games[1].name
  }
  return {
    ...base,
    __decorated: false,
    list_clips: clips,
    list_games: games,
    screenshots: [],
    get_status: { ...base.get_status, game: PLAN[0].game, steam_app_id: PLAN[0].id, session_start_ms: at(0, 18, 5), backend: 'windows' },
    get_storage: { clips_bytes: clips.reduce((n, c) => n + c.size_bytes, 0), clip_count: clips.length, disk_free: 412e9, disk_total: 1000e9, limit_bytes: 25e9 },
  }
}

// ------------------------------------------------------------------ files

const read = (kind, i) => fs.readFileSync(path.join(FRAMES, `${kind}-${i}.jpg`))
function steamArt(file) {
  const m = /librarycache\/(\d+)\/(.+)$/.exec(file)
  if (!m) return null
  const names = m[2] === 'library_600x900.jpg' ? [m[2], 'library_capsule.jpg'] : [m[2]]
  const walk = (dir) => fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(path.join(dir, e.name)) : names.includes(e.name) ? [path.join(dir, e.name)] : []))
  try {
    const found = walk(path.join(ART, m[1]))[0]
    return found ? fs.readFileSync(found) : null
  } catch {
    return null
  }
}

/** The app's media server and the files on disk. `artDelay`: hold Steam's
 *  pictures back this long (the loading state). */
async function serve(page, { clip = CLIP, artDelay = 0 } = {}) {
  const video = fs.readFileSync(clip)
  await page.route('http://127.0.0.1:1/**', (r) => {
    const m = /bytes=(\d+)-(\d*)/.exec(r.request().headers().range ?? '')
    const from = m ? Number(m[1]) : 0
    const to = m && m[2] ? Number(m[2]) : video.length - 1
    return r.fulfill({
      status: m ? 206 : 200,
      body: video.subarray(from, to + 1),
      headers: { 'Content-Type': 'video/webm', 'Accept-Ranges': 'bytes', 'Access-Control-Allow-Origin': '*', ...(m ? { 'Content-Range': `bytes ${from}-${to}/${video.length}` } : {}) },
    })
  })
  const cors = { 'Access-Control-Allow-Origin': '*' }
  await page.route('http://127.0.0.1:2/**', async (r) => {
    const file = decodeURIComponent(new URL(r.request().url()).pathname.slice(1))
    const made = /^\/(thumbs|sprites)\/(\d+)\.svg$/.exec(file)
    if (made) return r.fulfill({ status: 200, body: read(made[1] === 'thumbs' ? 'thumb' : 'sprite', Number(made[2])), headers: { 'Content-Type': 'image/jpeg', ...cors } })
    if (!file.includes('/librarycache/')) return r.fulfill({ status: 404 })
    if (artDelay) await new Promise((go) => setTimeout(go, artDelay))
    const art = steamArt(file)
    if (!art) return r.fulfill({ status: 404 })
    return r.fulfill({ status: 200, body: art, headers: { 'Content-Type': file.endsWith('.png') ? 'image/png' : 'image/jpeg', ...cors } })
  })
}

// ------------------------------------------------------------------ driving

const seekTo = (page, t) =>
  page.evaluate((t) => new Promise((done) => {
    const v = document.querySelector('video')
    v.addEventListener('seeked', done, { once: true })
    v.currentTime = t
  }), t)

async function open(browser, build, [width, height], opts = {}) {
  const page = await browser.newPage({ viewport: { width, height }, colorScheme: 'dark' })
  page.on('pageerror', (e) => console.error(`  page error (${build} ${width}): ${e.message}`))
  await page.clock.setFixedTime(NOW)
  await page.addInitScript(mockBridge, library(opts))
  await serve(page, opts)
  await page.goto(`http://localhost:${PORTS[build]}/`, { waitUntil: opts.artDelay ? 'domcontentloaded' : 'load' })
  await page.locator('.clip-grid > *').first().waitFor()
  if (!opts.artDelay) {
    await page.waitForFunction(() => [...document.querySelectorAll('img')].filter((i) => i.getBoundingClientRect().top < innerHeight).every((i) => i.complete))
  }
  await page.waitForTimeout(400)
  return page
}

const nav = (page) => page.getByRole('navigation', { name: 'Library' })
const game = (page, name) => nav(page).getByRole('button', { name, exact: true }).click()

async function viewer(page, clip169 = false) {
  await page.locator('.clip-open').first().click()
  await page.waitForFunction(() => document.querySelector('video')?.readyState >= 2)
  await seekTo(page, clip169 ? 6 : 3)
  await page.waitForTimeout(600)
}

const SIZES = [[1440, 900], [1280, 800], [900, 600], [1920, 1040]]

async function run(browser, build, only) {
  const shot = (page, width, name) => {
    if (only && !`${width}-${name}`.includes(only)) return
    return page.screenshot({ path: path.join(out, `${build}-${width}-${name}.png`) })
  }
  const want = (width, name) => !only || `${width}-${name}`.includes(only) || name.startsWith(only)
  for (const size of SIZES) {
    const [width] = size
    let page = await open(browser, build, size)
    await shot(page, width, 'library')
    await game(page, 'Rainbow Six Siege')
    await page.waitForTimeout(500)
    await shot(page, width, 'game-art')
    await game(page, 'Fortnite')
    await page.waitForTimeout(500)
    await shot(page, width, 'game-noart')
    await game(page, 'All clips')
    await page.waitForTimeout(300)
    await viewer(page)
    await shot(page, width, 'viewer')

    if (width === 1440) {
      // Trim, the handle under the pointer, then exporting and its outcome.
      await page.keyboard.press('Home')
      await seekTo(page, 2)
      await page.keyboard.press('i')
      await seekTo(page, 9.5)
      await page.keyboard.press('o')
      await seekTo(page, 4)
      const handle = await page.getByRole('slider', { name: 'Trim start' }).boundingBox()
      await page.mouse.move(handle.x + handle.width / 2, handle.y + handle.height / 2)
      await page.waitForTimeout(300)
      await shot(page, width, 'viewer-trim')
      await page.mouse.move(700, 120)
      await page.evaluate(() => { window.__renderGate = new Promise((go) => (window.__renderGo = go)) })
      await page.getByRole('dialog').getByRole('button', { name: /^Copy clip/ }).click()
      await page.waitForTimeout(100)
      await page.evaluate(() => window[`_${window.__listeners['render-progress']}`]({ event: 'render-progress', id: 1, payload: { path: window.__lastRender.path, fraction: 0.58 } }))
      await page.waitForTimeout(250)
      await shot(page, width, 'viewer-copying')
      await page.evaluate(() => { window.__renderGo(); window.__renderGate = null })
      await page.waitForTimeout(260)
      await shot(page, width, 'viewer-copied')
      await page.waitForTimeout(1800)
      // Cover frame (the directions only).
      await seekTo(page, 7.4)
      const cover = page.getByRole('button', { name: /cover/i }).first()
      let covered = false
      if ((await cover.count()) && (await cover.isVisible())) {
        await cover.click()
        covered = true
      } else {
        await page.getByRole('button', { name: 'Clip options' }).click()
        await page.waitForTimeout(150)
        const item = page.getByRole('menuitem', { name: /cover/i })
        if (await item.count()) {
          await item.click()
          covered = true
        } else await page.keyboard.press('Escape')
      }
      if (covered) {
        await page.waitForTimeout(300)
        await shot(page, width, 'viewer-cover')
      }
      await page.getByRole('button', { name: 'Clip options' }).click()
      await page.waitForTimeout(200)
      await shot(page, width, 'viewer-menu')
      await page.keyboard.press('Escape')
      await page.waitForTimeout(100)
      // Step to the next clip, then back: where does the library pick up?
      await page.keyboard.press('ArrowRight')
      await page.waitForTimeout(500)
      await page.keyboard.press('Escape')
      await page.waitForTimeout(110)
      await shot(page, width, 'return-flight')
      await page.waitForTimeout(500)
      await shot(page, width, 'return-landed')
      // Library states: a card under the pointer (scrubbing), the keyboard, picking.
      await page.mouse.move(10, 10)
      const card = page.locator('.clip-grid .frame').nth(2)
      const box = await card.boundingBox()
      await page.mouse.move(box.x + box.width * 0.3, box.y + box.height / 2)
      await page.mouse.move(box.x + box.width * 0.62, box.y + box.height / 2, { steps: 4 })
      await page.waitForTimeout(250)
      await shot(page, width, 'library-hover')
      await page.mouse.move(10, 10)
      await page.locator('.clip-open').nth(0).focus()
      await page.keyboard.press('ArrowRight')
      await page.waitForTimeout(200)
      await shot(page, width, 'library-focus')
      await page.locator('.clip-open').nth(1).click({ modifiers: ['Control'] })
      await page.locator('.clip-open').nth(3).click({ modifiers: ['Control'] })
      await page.mouse.move(10, 10)
      await page.waitForTimeout(200)
      await shot(page, width, 'library-select')
      await page.keyboard.press('Escape')
      // Navigation under the pointer.
      await nav(page).getByRole('button', { name: 'Overwatch 2', exact: true }).hover()
      await page.waitForTimeout(200)
      await shot(page, width, 'nav-hover')
      // Opening, mid-flight.
      await page.mouse.move(10, 10)
      await page.locator('.clip-open').first().click()
      await page.waitForTimeout(90)
      await shot(page, width, 'open-flight')
    }
    await page.close()

    // The same viewer on 16:9 footage.
    if (want(width, 'viewer-169')) {
      page = await open(browser, build, size, { clip: CLIP169 })
      await viewer(page, true)
      await shot(page, width, 'viewer-169')
      await page.close()
    }

    // The longest names a library holds; and Steam's art still on its way.
    if ((width === 900 || width === 1440) && want(width, 'stress')) {
      page = await open(browser, build, size, { stress: true })
      await shot(page, width, 'stress-library')
      await page.locator('.clip-open').nth(1).click()
      await page.waitForTimeout(500)
      await shot(page, width, 'stress-viewer')
      await page.close()
    }
    if (width === 1440 && want(width, 'art-loading')) {
      page = await open(browser, build, size, { artDelay: 60000 })
      await game(page, 'Rainbow Six Siege')
      await page.waitForTimeout(300)
      // Straight from the browser: Playwright's own capture waits for the held pictures.
      const cdp = await page.context().newCDPSession(page)
      const { data } = await cdp.send('Page.captureScreenshot', { format: 'png' })
      fs.writeFileSync(path.join(out, `${build}-${width}-art-loading.png`), Buffer.from(data, 'base64'))
      await page.close()
    }
  }
}

module.exports = { library, serve, NOW, PORTS }

if (require.main === module) (async () => {
  const args = process.argv.slice(2)
  const only = args.find((a) => a.startsWith('--only='))?.slice(7)
  const builds = args.filter((a) => !a.startsWith('--'))
  const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH })
  try {
    for (const b of builds.length ? builds : ['base', 'a', 'b']) {
      await run(browser, b, only)
      console.log(`ok  ${b}`)
    }
  } finally {
    await browser.close()
  }
})().catch((e) => {
  console.error(e)
  process.exit(1)
})
