// UI smoke test: runs the real frontend in Chromium with the Tauri bridge
// mocked, drives every view, asserts a few behaviours and saves screenshots
// to tests/shots/. No Tauri, no GStreamer, no GPU needed.
//
//   pnpm test:ui            # all scenarios
//   pnpm test:ui full       # one of: full, empty, big, worst
//   pnpm test:ui shots      # pictures only, at three window sizes (see below)
//
// Needs `playwright` (a devDependency) and a Chromium it can launch
// (`npx playwright install chromium` once, unless one is preinstalled).
//
// The clips here have no picture files, so the grid shows empty frames. For
// screenshots with real pictures, on a PC that has them (never committed):
//   RERUNS_SHOT_FRAMES  a folder of thumb-N.jpg and sprite-N.jpg (ten frames
//                       in a row), used for clip N modulo how many there are
//   RERUNS_STEAM_ART    Steam's appcache/librarycache folder
//   RERUNS_SHOT_CLIP    a short video with sound that Chromium can play
//                       (the fixture has none, so no waveform is drawn)
// `shots` needs all three and a frames.json beside the pictures, so that
// what a card says is what it shows:
//   [{ "game": "…", "steam_app_id": 1 | null, "frames": [N, …], "titles": { "N": "…" } }]
// The first game's first frame is the clip RERUNS_SHOT_CLIP plays.

const { chromium } = require('playwright')
const { spawn } = require('node:child_process')
const path = require('node:path')
const fs = require('node:fs')
const assert = require('node:assert/strict')

const PORT = 5173
const out = path.join(__dirname, 'shots')
fs.mkdirSync(out, { recursive: true })

// ---------------------------------------------------------------- fixtures

function fixtures(scenario) {
  const now = Date.now()
  const h = 3600e3
  const mult = scenario === 'big' ? 40 : 1
  const games = [
    game('ELDEN RING', 1245620, 7, { last: now - 2 * h, total: 14 * h, playing: true, sessions: [{ start: now - 3 * h, end: now - 2 * h }] }),
    game('Counter-Strike 2', 730, 12, { last: now - 26 * h, total: 40 * h }),
    game('Hades II', 1145350, 3, { last: now - 5 * 24 * h, total: 6 * h }),
  ]
  // Steam art; the test server fails each file's first request.
  for (const g of games) {
    const art = `/home/u/.local/share/Steam/appcache/librarycache/${g.steam_app_id}/`
    g.art = { capsule: art + 'library_600x900.jpg', hero: art + 'library_hero.jpg', logo: art + 'logo.png' }
  }
  const clips = []
  let i = 0
  for (const g of games) {
    for (let k = 0; k < g.clip_count * mult; k++) {
      const t = now - (i * 7 + 1) * h
      clips.push({
        path: `/home/u/Videos/Reruns/${g.name}/clip-${i}.mp4`, game: g.name, name: `clip-${i}`, created_ms: t,
        // One clip is already small enough for Discord as it is.
        size_bytes: i === 2 ? 8e6 : 60e6 + i * 9e6, duration: 20 + (i % 5) * 11, audio_tracks: 2, is_edit: i % 6 === 0,
        steam_app_id: g.steam_app_id, thumbnail: `/thumbs/${i}.svg`, sprite: `/sprites/${i}.svg`, sprite_frames: 10,
        title: i === 1 ? 'Malenia no-hit (finally)' : null, favorite: i % 4 === 1,
        labels: i % 3 === 1 ? ['clutch', 'boss'] : i === 4 ? ['funny'] : [],
        markers: i === 1 ? [1.5, 3.5] : [],
        // Game and mic mixed into a first track, then each on its own.
        ...(i === 1 ? { audio_tracks: 3, mixed_audio: true } : {}),
      })
      i++
    }
  }
  if (scenario === 'worst') worstCase(games, clips, now)
  const empty = scenario === 'empty'
  // F9 screenshots, saved next to the game's clips.
  const shot = (t, ago) => ({ path: `/home/u/Videos/Reruns/ELDEN RING/${t}.png`, game: 'ELDEN RING', size: 4e6, modified_ms: now - ago })
  const screenshots = empty ? [] : [shot('2026-11-12_10-00-00', h / 2), shot('2026-11-12_09-00-00', h * 1.5)]
  return {
    // 'full' runs as on Windows: no system frame, the header carries the
    // window controls. The others keep the frame, as on Linux.
    __decorated: scenario !== 'full',
    get_status: empty
      ? { state: 'waiting', game: null, steam_app_id: null, session_start_ms: null, buffered_seconds: 0, replay_seconds: 30, encoder: null, capture_path: null, error: null, backend: 'portal', needs_screen: true, hdr: null, hdr_tonemapped: false }
      : { state: 'recording', game: 'ELDEN RING', steam_app_id: 1245620, session_start_ms: now - h, buffered_seconds: 30, replay_seconds: 30, encoder: 'nvenc', capture_path: 'zerocopy', error: null, backend: 'kwin', needs_screen: false, hdr: { sdr_white_nits: 240, peak_nits: 1000 }, hdr_tonemapped: false },
    get_settings: {
      replay_seconds: 30, mic: true, clips_dir: '/home/u/Videos/Reruns', hotkey: 'F8', screenshot_hotkey: 'F9', marker_hotkey: 'F7', capture_backend: null,
      launch_at_login: false, start_in_tray: true, close_to_tray: true, notify_on_save: true, sound_on_save: true, sound_volume: 100, clip_sound: { kind: 'builtin' }, screenshot_sound: null, overlay: true, check_for_updates: true, discord_presence: true,
      fps: 60, bitrate_kbps: 20000, codec: 'h264', encoder: null, cursor: true, zero_copy: true, hdr_compatibility: false, hdr_window_capture: false, ignored_games: [], desktop_audio: true, desktop_device: 'out-headset',
      mic_device: null, audio_bitrate_kbps: 160, storage_limit_gb: 25, protect_edits: true, protect_favorites: true, use_trash: true, ads: false,
    },
    list_clips: empty ? [] : clips,
    list_games: empty ? [] : games,
    screenshots,
    get_storage: scenario === 'worst'
      ? { clips_bytes: 1284.5e9, clip_count: clips.length, disk_free: 0.2e9, disk_total: 2000e9, limit_bytes: 1000e9 }
      : { clips_bytes: 3.7e9, clip_count: clips.length, disk_free: 120e9, disk_total: 500e9, limit_bytes: 25e9 },
    get_capabilities: { display_hz: 144, encoders: [{ encoder: 'nvenc', codecs: ['h264', 'hevc', 'av1'] }, { encoder: 'x264', codecs: ['h264'] }], mic_devices: [{ id: 'mic1', name: 'Blue Yeti' }], output_devices: [{ id: 'out-speakers', name: 'Speakers (Realtek Audio)' }, { id: 'out-headset', name: 'Headset Earphone (Arctis Nova 7)' }] },
    media_base: 'http://127.0.0.1:1/x/',
    hotkey_backend: 'kglobalaccel',
    update_support: { configured: true, version: '0.1.0' },
    check_update: { version: '0.2.0', current_version: '0.1.0', notes: null },
  }
}

// What real libraries hold that demo data never does: the longest names
// Steam has, a one-letter game, names that start with an emoji or have no
// spaces, a game with no art, a clip named to the 120-character limit, one
// with a dozen tags, one with no picture or length yet, one with no sound.
function worstCase(games, clips, now) {
  const h = 3600e3
  games[0].name = "Tom Clancy's Rainbow Six Siege"
  games[1].name = "PLAYERUNKNOWN'S BATTLEGROUNDS: Experimental Server"
  games[2].name = 'Q'
  const more = ['🦊 Fox Hunt', '原神', 'Benachrichtigungseinstellungen-Simulator', 'Deadlock', 'ASTRONEER', 'Dead by Daylight', 'Fortnite', 'Overwatch®', 'Dead Sector', 'Hades II', 'Counter-Strike 2', 'ELDEN RING', 'Baldur’s Gate 3', 'Lethal Company']
  for (const c of clips) c.game = games[['ELDEN RING', 'Counter-Strike 2', 'Hades II'].indexOf(c.game)].name
  more.forEach((name, k) => {
    games.push(game(name, null, 1, { last: now - (k + 2) * 30 * 24 * h, total: k ? k * 1284 * h : 0 }))
    clips.push({ ...clips[0], path: `/home/u/Videos/Reruns/${name}/only.mp4`, game: name, name: 'only', created_ms: now - (k + 2) * 30 * 24 * h, title: null, labels: [], markers: [], favorite: false, is_edit: false, steam_app_id: null })
  })
  Object.assign(clips[0], { title: 'x', duration: null, thumbnail: null, sprite: null, size_bytes: 12.4e9, audio_tracks: 0 })
  Object.assign(clips[1], {
    title: 'The one where Aleksandra Wiśniewska-Kowalczyk clutches a 1v5 on Chalet with 3 HP and the whole lobby loses it (FINAL v12)',
    labels: ['clutch', 'ace', '1v5', 'ranked', 'chalet', 'customer-feedback-from-enterprise-onboarding', 'funny', 'send-to-discord', 'montage', 'pb', 'q3', 'needs-review'],
    markers: [0, 0.2, 1.5, 3.5, 30.9],
    favorite: true,
    is_edit: true,
  })
  Object.assign(clips[2], { title: '<b>&amp; **bold**</b>', created_ms: now - 3 * 365 * 24 * h })
  clips[3].title = 'Benachrichtigungseinstellungen_IMG_20250914_183022_HDR_portrait_edited_edited'
}

// No drawn stand-ins for game frames: clips have no thumbnail file here, so
// the grid shows the app's own empty media frame. Screenshots and Steam art
// need a picture that loads; it is one flat grey.
const frame = () => `<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180"><rect width="320" height="180" fill="#2a2a30"/></svg>`
const SVG = { 'Content-Type': 'image/svg+xml', 'Access-Control-Allow-Origin': '*' }

// Real pictures for local screenshots (see the top of this file).
const FRAMES = process.env.RERUNS_SHOT_FRAMES
const frameCount = FRAMES ? fs.readdirSync(FRAMES).filter((f) => /^thumb-\d+\.jpg$/.test(f)).length : 0
const JPG = { 'Content-Type': 'image/jpeg', 'Access-Control-Allow-Origin': '*' }
/** thumb-N.jpg or sprite-N.jpg for clip `i`, or null without the folder. */
const realFrame = (kind, i) => (frameCount ? fs.readFileSync(path.join(FRAMES, `${kind}-${i % frameCount}.jpg`)) : null)
/** Steam's own file for a librarycache path: by name anywhere under the
 *  game's folder (Steam keeps each in a hashed folder, and calls the
 *  portrait either library_600x900 or library_capsule). */
function steamArt(file) {
  const m = /librarycache\/(\d+)\/(.+)$/.exec(file)
  if (!process.env.RERUNS_STEAM_ART || !m) return null
  const names = m[2] === 'library_600x900.jpg' ? [m[2], 'library_capsule.jpg'] : [m[2]]
  const walk = (dir) => fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(path.join(dir, e.name)) : names.includes(e.name) ? [path.join(dir, e.name)] : []))
  try {
    const found = walk(path.join(process.env.RERUNS_STEAM_ART, m[1]))[0]
    return found ? fs.readFileSync(found) : null
  } catch {
    return null
  }
}

function game(name, id, clips, o) {
  return {
    name, steam_app_id: id, art: { capsule: null, hero: null, logo: null }, fallback_image: null,
    last_played_ms: o.last, last_activity_ms: o.last, total_play_ms: o.total, clip_count: clips, clip_bytes: clips * 1.5e8,
    sessions: (o.sessions ?? []).map((s) => ({ game: name, steam_app_id: id, start_ms: s.start, end_ms: s.end })),
    playing: !!o.playing,
  }
}

// ------------------------------------------------------------- tauri mock

// Installed before the page's scripts run; `@tauri-apps/api` talks to this.
function mockBridge(data) {
  let n = 0
  // What @tauri-apps/api calls to stop listening (pages that unlisten).
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} }
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main', windowLabel: 'main' } },
    transformCallback: (cb) => { const id = ++n; window[`_${id}`] = cb; return id },
    convertFileSrc: (p) => 'http://127.0.0.1:2/' + encodeURIComponent(p),
    invoke: async (cmd, args) => {
      // In order, so a test can check what the page did first.
      (window.__calls ||= []).push(cmd === 'plugin:event|listen' ? `listen ${args.event}` : cmd)
      if (cmd === 'open_for_edit') return data.list_clips.find((c) => c.path === args.path) || { ...data.list_clips[0], path: args.path }
      if (cmd === 'update_clip') {
        const c = data.list_clips.find((c) => c.path === args.path)
        Object.assign(c, {
          title: args.patch.title === undefined ? c.title : args.patch.title.trim() || null,
          favorite: args.patch.favorite ?? c.favorite,
          labels: args.patch.labels ?? c.labels,
          cover: args.patch.cover === undefined ? c.cover : args.patch.cover < 0 ? null : args.patch.cover,
        })
        window.__patch = args.patch
        return c
      }
      if (cmd === 'ignore_current_game') {
        data.get_settings.ignored_games.push({ name: data.get_status.game, exe: 'eldenring' })
        return data.get_settings
      }
      if (cmd === 'export_plan') {
        // The real planner is tested in Rust; this only feeds the hint.
        const kbps = Math.round((args.maxMb * 1e6 * 8 * 0.92) / args.seconds / 1000)
        return { video_kbps: kbps - 128, audio_kbps: 128, max_height: 720, fps: 30 }
      }
      if (cmd === 'render_clip') {
        window.__lastRender = args
        // A test can hold a render open, or make it fail.
        if (window.__renderGate) await window.__renderGate
        if (window.__renderError) throw window.__renderError
        // As the backend does: the edit is announced as saved, then returned.
        const edit = args.path.replace(/\.mp4$/, '_edit.mp4')
        const saved = window.__listeners?.['clip-saved']
        if (saved) window[`_${saved}`]({ event: 'clip-saved', id: 1, payload: edit })
        return edit
      }
      if (cmd === 'copy_clip') { if (window.__copyError) throw window.__copyError; window.__copied = args.path; return null }
      if (cmd === 'set_settings') {
        if (window.__settingsError) throw window.__settingsError
        data.get_settings = args.settings
        return null
      }
      if (cmd === 'list_library') {
        const ignored = data.get_settings.ignored_games.map((i) => i.name.toLowerCase())
        return {
          clips: data.list_clips,
          games: data.list_games.filter((g) => !ignored.includes(g.name.toLowerCase())),
          screenshots: data.screenshots,
        }
      }
      if (cmd === 'delete_clip') {
        data.list_clips = data.list_clips.filter((c) => c.path !== args.path)
        data.screenshots = data.screenshots.filter((s) => s.path !== args.path)
        return null
      }
      if (cmd === 'plugin:event|listen') {
        // Remember the handler so a test can fire the event at the page.
        (window.__listeners ||= {})[args.event] = args.handler
        return args.handler
      }
      if (cmd === 'plugin:window|is_decorated') return data.__decorated
      if (cmd === 'plugin:window|is_maximized') return false
      if (cmd === 'plugin:dialog|open') return '/home/u/Music/airhorn.mp3'
      if (cmd === 'import_sound') return { kind: 'custom', path: `/home/u/.config/reruns/sounds/${args.slot}-1.mp3`, name: 'airhorn.mp3' }
      if (cmd === 'preview_sound') { (window.__previews ||= []).push(args); return null }
      if (cmd === 'open_log_folder') {
        window.__logOpens = (window.__logOpens || 0) + 1
        if (window.__logError) throw window.__logError
        return null
      }
      if (cmd.startsWith('plugin:')) return 1
      if (cmd in data) return data[cmd]
      return null
    },
  }
}

// ---------------------------------------------------------------- scenarios

/** The app's media server and the files on disk, as the page asks for them.
 *  `failFirst`: Steam's portrait fails its first request (the retry test). */
async function serve(page, { failFirst = true, clip = process.env.RERUNS_SHOT_CLIP } = {}) {
  // Every clip plays the same small ultrawide test video (VP9: the test
  // Chromium has no H.264), so the viewer is shot with a real picture.
  const video = fs.readFileSync(clip || path.join(__dirname, 'fixtures', 'clip.webm'))
  await page.route('http://127.0.0.1:1/**', (r) => {
    // A screenshot's card-sized image from the media server.
    if (r.request().url().endsWith('?thumb')) return r.fulfill(frameCount ? { status: 200, body: realFrame('thumb', 7), headers: JPG } : { status: 200, body: frame(40), headers: SVG })
    // Byte ranges, like the app's media server, so the player can seek.
    const m = /bytes=(\d+)-(\d*)/.exec(r.request().headers().range ?? '')
    const from = m ? Number(m[1]) : 0
    const to = m && m[2] ? Number(m[2]) : video.length - 1
    return r.fulfill({
      status: m ? 206 : 200,
      body: video.subarray(from, to + 1),
      headers: {
        'Content-Type': 'video/webm',
        'Accept-Ranges': 'bytes',
        // As the app's media server: the viewer reads the audio for its waveform.
        'Access-Control-Allow-Origin': '*',
        ...(m ? { 'Content-Range': `bytes ${from}-${to}/${video.length}` } : {}),
      },
    })
  })
  // Files on disk: screenshots load, Steam's portrait loads on its second
  // request, the rest is missing.
  const hits = {}
  await page.route('http://127.0.0.1:2/**', async (r) => {
    const file = decodeURIComponent(new URL(r.request().url()).pathname.slice(1))
    // The art tint reads the same pictures with CORS: only the page's own
    // images count towards "fails first".
    if (!(await r.request().headerValue('origin'))) hits[file] = (hits[file] ?? 0) + 1
    const made = /^\/(thumbs|sprites)\/(\d+)\.svg$/.exec(file)
    if (made && frameCount) return r.fulfill({ status: 200, body: realFrame(made[1] === 'thumbs' ? 'thumb' : 'sprite', Number(made[2])), headers: JPG })
    if (file.startsWith('/home/u/Videos/') && file.endsWith('.png')) return r.fulfill(frameCount ? { status: 200, body: realFrame('thumb', 7), headers: JPG } : { status: 200, body: frame(40), headers: SVG })
    // Only the navigation's picture fails first: that retry is what is tested.
    if (!file.includes('/librarycache/') || (failFirst && hits[file] === 1 && file.endsWith('library_600x900.jpg'))) return r.fulfill({ status: 404 })
    const art = steamArt(file)
    if (art) return r.fulfill({ status: 200, body: art, headers: JPG })
    if (file.endsWith('logo.png')) return r.fulfill({ status: 404 })
    return r.fulfill({ status: 200, body: frame(3), headers: SVG })
  })
}

/** Open a menu by its button and choose a row (by the start of its name). */
const choose = async (page, menu, item, role = 'menuitem') => {
  await page.getByRole('button', { name: menu, exact: true }).click()
  // A plain row may end in its key ("Rename N"); the others are matched whole.
  await page.getByRole(role, { name: item, exact: role !== 'menuitem' }).first().click()
}

const viewOption = (page, name) => choose(page, 'View options', name, 'menuitemradio')

const gameChoice = (page, name) => page.getByRole('navigation', { name: 'Library' }).getByRole('button', { name, exact: typeof name === 'string' })
const selectGame = async (page, name) => {
  if (name === 'All games') {
    const clear = page.getByRole('button', { name: 'Show all games', exact: true })
    if (await clear.count()) await clear.click()
  } else await gameChoice(page, name).click()
}

const startTrimming = async (page) => {
  const trim = page.getByRole('button', { name: 'Trim', exact: true })
  if (await trim.count()) await trim.click()
  await page.getByRole('slider', { name: 'Trim start' }).waitFor()
}

const cardAction = async (page, card, name, screenshot = false) => {
  await card.hover()
  await card.getByRole('button', { name: screenshot ? 'Screenshot actions' : 'Clip actions', exact: true }).click()
  await page.getByRole('menu', { name: 'Clip actions' }).getByRole('menuitem', { name, exact: true }).click()
}

async function quickCopy(page) {
  const small = page.locator('.media-card[data-clip-path$="/clip-2.mp4"]')
  const large = page.locator('.media-card[data-clip-path$="/clip-0.mp4"]')
  const probes = await page.evaluate(() => window.__calls.filter((cmd) => cmd === 'open_for_edit').length)
  await page.evaluate(() => { window.__lastRender = null; window.__copied = null })
  await small.hover()
  const copyWidth = await small.locator('.clip-copy').evaluate((el) => el.getBoundingClientRect().width)
  await small.getByRole('button', { name: 'Copy clip', exact: true }).click()
  await small.getByRole('button', { name: 'Copy clip', exact: true }).filter({ hasText: 'Copied' }).waitFor()
  assert.equal(await small.locator('.clip-copy').evaluate((el) => el.getBoundingClientRect().width), copyWidth, 'Copied keeps the card button its width')
  assert.match(await page.evaluate(() => window.__copied), /clip-2\.mp4$/, 'a small clip copies directly from the library')
  assert.equal(await page.evaluate(() => window.__lastRender), null, 'a small clip needs no preparation')
  assert.equal(await page.evaluate(() => window.__calls.filter((cmd) => cmd === 'open_for_edit').length), probes, 'quick copying a small clip needs no media probe')
  assert.equal(await page.locator('video').count(), 0, 'copying does not open the viewer')

  // Cancellation must win even when the backend finishes just after the
  // request: a late render result must never replace the clipboard.
  const other = page.locator('.media-card[data-clip-path$="/clip-3.mp4"]')
  await cardAction(page, other, 'Delete')
  await page.getByRole('button', { name: 'Undo', exact: true }).waitFor()
  assert.equal(await other.count(), 0, 'a deleted clip is awaiting its undo deadline')
  await page.evaluate(() => {
    window.__copied = null
    window.__renderGate = new Promise((go) => (window.__renderGo = go))
  })
  await large.hover()
  await large.getByRole('button', { name: 'Copy clip', exact: true }).click()
  await page.getByText(/Preparing clip… \d+%/).waitFor()
  await page.waitForFunction(() => window.__lastRender)
  await page.getByRole('button', { name: 'Undo delete', exact: true }).click()
  await other.waitFor()
  assert.ok(await page.getByRole('button', { name: 'Cancel', exact: true }).isVisible(), 'undoing deletion keeps the independent copy job running')
  await page.getByRole('button', { name: 'Cancel', exact: true }).click()
  await page.waitForFunction(() => window.__calls.includes('cancel_render'))
  await page.evaluate(() => { window.__renderGo(); window.__renderGate = null })
  await page.getByRole('button', { name: 'Cancel', exact: true }).waitFor({ state: 'hidden' })
  assert.equal(await page.evaluate(() => window.__copied), null, 'cancelled preparation cannot copy its late result')

  await page.evaluate(() => {
    window.__lastRender = null
    window.__renderGate = new Promise((go) => (window.__renderGo = go))
  })
  await large.hover()
  await large.getByRole('button', { name: 'Copy clip', exact: true }).click()
  await page.getByText(/Preparing clip… \d+%/).waitFor()
  await page.waitForFunction(() => window.__lastRender)
  const prepared = await page.evaluate(() => window.__lastRender)
  assert.equal(prepared.maxMb, 10, 'library Copy clip honors the same Sharing limit as the viewer')
  assert.deepEqual([prepared.start, prepared.end], [0, 20], 'quick copy keeps the whole clip')
  await page.evaluate(() => { window.__renderGo(); window.__renderGate = null })
  await large.getByRole('button', { name: 'Copy clip', exact: true }).filter({ hasText: 'Copied' }).waitFor()
  assert.match(await page.evaluate(() => window.__copied), /clip-0_edit\.mp4$/, 'a large clip copies the prepared file')
  assert.equal(await page.locator('video').count(), 0, 'preparation stays in the library')
}

/** A focused card's own keys, Delete's neighbour, and Ctrl+Z. */
async function cardKeys(page) {
  const card = page.locator('.media-card[data-clip-path$="/clip-2.mp4"]')
  const focusedPath = () => page.evaluate(() => document.activeElement?.closest('[data-clip-path]')?.dataset.clipPath ?? null)
  assert.equal(await card.locator('button:not([tabindex="-1"])').count(), 1, 'one tab stop per card')
  await page.evaluate(() => { window.__copied = null; window.__patch = null })
  await card.locator('.clip-open').focus()
  await page.keyboard.press('Control+c')
  await page.waitForFunction(() => window.__copied)
  assert.match(await page.evaluate(() => window.__copied), /clip-2\.mp4$/, 'Ctrl+C copies the focused card')
  await page.waitForFunction(() => !document.querySelector('.clip-copy[aria-busy="true"]'))
  const favorite = await card.locator('.favorite-mark').count()
  await page.keyboard.press('f')
  await page.waitForFunction(() => window.__patch)
  assert.equal(await page.evaluate(() => window.__patch.favorite), !favorite, 'F toggles the focused card favorite')
  await page.keyboard.press('f')
  // Delete hands the keyboard to the next card; Ctrl+Z puts the card back with it.
  const next = await page.evaluate(() => {
    const all = [...document.querySelectorAll('main [data-clip-path]')]
    const i = all.findIndex((c) => c.dataset.clipPath.endsWith('/clip-2.mp4'))
    return all[i + 1]?.dataset.clipPath
  })
  await page.keyboard.press('Delete')
  await card.waitFor({ state: 'detached' })
  assert.equal(await focusedPath(), next, 'Delete moves the keyboard to the next card')
  assert.equal(await page.getByRole('button', { name: 'Undo', exact: true }).getAttribute('aria-keyshortcuts'), 'Control+Z', 'the notice names its key')
  await page.keyboard.press('Control+z')
  await card.waitFor()
  assert.match(await focusedPath(), /clip-2\.mp4$/, 'Ctrl+Z restores the card with the keyboard on it')
  // The notice leaves with a short fade; then it is gone.
  await page.getByRole('button', { name: 'Undo', exact: true }).waitFor({ state: 'detached', timeout: 1000 })
  // Another collection starts at its newest clips.
  const scroller = page.locator('.library-scroll')
  await scroller.evaluate((el) => el.scrollTo(0, el.scrollHeight))
  assert.ok(await scroller.evaluate((el) => el.scrollTop) > 0, 'the library scrolled')
  await page.getByRole('navigation', { name: 'Library' }).getByRole('button', { name: 'Favorites', exact: true }).click()
  await page.waitForTimeout(100)
  assert.equal(await scroller.evaluate((el) => el.scrollTop), 0, 'a new collection starts at the top')
  await page.getByRole('navigation', { name: 'Library' }).getByRole('button', { name: 'All clips', exact: true }).click()
}

const seekTo = (page, t) =>
  page.evaluate(
    (t) =>
      new Promise((done) => {
        const v = document.querySelector('video')
        v.addEventListener('seeked', done, { once: true })
        v.currentTime = t
      }),
    t,
  )

async function cardInteractions(page) {
  const card = page.locator('.media-card:has(.moments i)').first()
  const frame = card.locator('.frame')
  const caption = card.locator('.card-caption')
  const tools = card.locator('.media-tools')
  await card.scrollIntoViewIfNeeded()
  await page.getByLabel('Search clips').focus()
  await page.getByLabel('Search clips').hover()
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.media-card:has(.moments i) .media-tools')).opacity === '0')
  const original = { frame: await frame.boundingBox(), caption: await caption.boundingBox() }
  const stable = async (state) => {
    assert.deepEqual(await frame.boundingBox(), original.frame, `the picture stays fixed during ${state}`)
    assert.deepEqual(await caption.boundingBox(), original.caption, `the caption stays fixed during ${state}`)
    assert.ok(await tools.evaluate((el) => {
      const r = el.getBoundingClientRect(), picture = el.closest('.frame').getBoundingClientRect()
      return r.left >= picture.left && r.right <= picture.right && r.top >= picture.top && r.bottom <= picture.bottom
    }), 'the actions stay within the picture')
  }
  await card.hover()
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.media-card:has(.moments i) .media-tools')).opacity === '1')
  await stable('hover')
  await page.getByLabel('Search clips').hover()
  await card.getByRole('button', { name: 'Copy clip', exact: true }).focus()
  await page.waitForFunction(() => getComputedStyle(document.querySelector('.media-card:has(.moments i) .media-tools')).opacity === '1')
  await stable('keyboard focus')
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await stable('reduced motion')
  await page.emulateMedia({ reducedMotion: 'no-preference' })
  await page.getByLabel('Search clips').focus()
}
async function runScenario(scenario, browser) {
  const data = fixtures(scenario)
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, colorScheme: 'dark' })
  page.on('pageerror', (e) => { throw new Error(`page error in ${scenario}: ${e.message}`) })
  await page.addInitScript(mockBridge, data)
  // Tags are a preference, off until asked for: this run has them on.
  if (scenario === 'full') await page.addInitScript(() => localStorage.getItem('reruns/prefs') || localStorage.setItem('reruns/prefs', JSON.stringify({ tagging: true })))
  await serve(page)
  await page.goto(`http://localhost:${PORT}/`)
  await page.waitForTimeout(800)
  const shot = (name) => page.screenshot({ path: path.join(out, `${scenario}-${name}.png`) })
  const cards = () => page.locator('.clip-grid > *').count()
  const fire = (event, payload) => page.evaluate(([event, payload]) => window[`_${window.__listeners[event]}`]({ event, id: 1, payload }), [event, payload])

  const libraryNav = page.getByRole('navigation', { name: 'Library' })
  await shot('library')
  if (scenario === 'full') {
    // Every clip, in dated sections, newest first; the toolbar counts them.
    const days = await page.locator('main h2').allTextContents()
    // The newest clip is an hour old: yesterday's, just after midnight.
    assert.ok(['Today', 'Yesterday'].includes(days[0]), `newest day first: ${days}`)
    assert.equal(new Set(days).size, days.length, 'one section per period')
    // Recent periods, then months; a card says what its heading leaves out.
    const f = await page.evaluate(async () => {
      const { period, stamp } = await import('/src/lib/format.ts')
      const now = new Date(2026, 10, 18, 12)
      const at = (d, h = 9) => new Date(2026, 10, d, h).getTime()
      return [period(at(18), now).label, period(at(17), now).label, period(at(13), now).label, period(at(11), now).label, period(new Date(2025, 9, 2).getTime(), now).label, stamp(at(13, 13), now), stamp(at(2), now)]
    })
    assert.deepEqual(f.slice(0, 4), ['Today', 'Yesterday', 'Past week', 'November'], `periods: ${f}`)
    assert.match(f[4], /October 2025/, 'another year is named')
    assert.match(f[5], /^\S+ 1:00\s?PM$|^\S+ 13:00$/, `a past-week card names its weekday: ${f[5]}`)
    assert.doesNotMatch(f[6], /:/, `an older card names its date only: ${f[6]}`)
    assert.equal(await page.locator('main h1').first().innerText(), 'All clips', 'the heading identifies the listed collection')
    assert.ok((await page.locator('.library-toolbar').innerText()).includes('22 clips'), 'and how many')
    assert.equal(await libraryNav.getByRole('button', { name: 'All clips', exact: true }).getAttribute('aria-current'), 'page', 'opens on every clip')
    assert.equal(await gameChoice(page, 'Counter-Strike 2').count(), 1, 'games are named in the collection navigation')
    assert.equal(await cards(), 24, 'every clip is in the grid, with the screenshots')
    assert.equal(await page.getByRole('button', { name: /^Open .* screenshot/ }).count(), 2, 'screenshots are listed among every clip')
    assert.ok((await page.locator('.library-toolbar').innerText()).includes('22 clips, 2 screenshots'), 'and counted apart')

    // A game's picture that fails to load leaves the game's initial, and
    // loads on the next listing (here: coming back to the window).
    const icon = gameChoice(page, 'Counter-Strike 2').locator('img[src*="library_600x900"]')
    await icon.waitFor({ state: 'hidden' })
    assert.equal(await icon.count(), 0, 'failed game art leaves no broken picture')
    await page.evaluate(() => window.dispatchEvent(new Event('focus')))
    await page.waitForFunction(() => {
      const img = document.querySelector('aside img[src*="%2F730%2F"]')
      return img?.complete && img.naturalWidth > 0
    })
    assert.match(await icon.getAttribute('src'), /\?retry=1$/)
    await page.keyboard.press('Escape')

    await cardInteractions(page)

    // Search narrows the grid; nothing found says so and offers the way back.
    await page.getByLabel('Search clips').fill('malenia')
    await page.waitForTimeout(100)
    assert.equal(await cards(), 1, 'search finds the titled clip')
    await page.getByLabel('Search clips').fill('zzzz')
    await page.getByText('No clips match').waitFor()
    await shot('search-none')
    await page.getByRole('button', { name: 'Clear search', exact: true }).last().click()
    await page.waitForTimeout(100)
    assert.equal(await cards(), 24, 'clearing the search lists every clip (and screenshot) again')
    await page.getByLabel('Search clips').fill('malenia')
    await page.keyboard.press('Escape')
    await page.waitForTimeout(100)
    assert.equal(await cards(), 24, 'Esc clears the search')

    // Display preferences share a menu; the resting toolbar stays focused on finding clips.
    await page.getByRole('button', { name: 'View options' }).click()
    await page.waitForTimeout(150)
    await shot('library-menu')
    await page.keyboard.press('Escape')
    await viewOption(page, 'Largest first')
    await page.waitForTimeout(100)
    assert.equal(await page.locator('main h2').count(), 0, 'no day headings when sorted by size')
    assert.equal(await cards(), 22, 'and still every clip')
    await page.getByRole('button', { name: 'View options' }).click()
    assert.equal(await page.getByRole('menuitemradio', { name: 'Largest first' }).getAttribute('aria-checked'), 'true', 'the menu shows the current sort')
    await page.keyboard.press('Escape')
    await viewOption(page, 'Newest first')

    // Four clips to a row at this size; bigger thumbnails are fewer to a row; it is remembered.
    const columns = () => page.locator('.clip-grid > *').evaluateAll((els) => new Set(els.map((e) => Math.round(e.getBoundingClientRect().x))).size)
    const cardWidth = () => page.locator('.clip-grid > *').first().evaluate((el) => el.getBoundingClientRect().width)
    assert.equal(await columns(), 4, 'four medium columns at 1280')
    const medium = await cardWidth()
    await viewOption(page, 'Large thumbnails')
    await page.waitForTimeout(100)
    assert.ok((await columns()) === 2 && (await cardWidth()) > medium, 'large thumbnails: fewer columns, wider cards')
    assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('reruns/prefs')).thumbSize), 'large', 'the size is kept')
    await viewOption(page, 'Medium thumbnails')
    // View preferences remain operable by keyboard through the menu.
    await page.getByRole('button', { name: 'View options' }).click()
    await page.getByRole('menuitemradio', { name: 'Medium thumbnails', exact: true }).focus()
    await page.keyboard.press('ArrowDown')
    await page.keyboard.press('Enter')
    assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('reruns/prefs')).thumbSize), 'large', 'keyboard navigation chooses the next size')
    await viewOption(page, 'Medium thumbnails')
    // A clip nobody named is titled by its game, as people write it; its length is in the picture's corner.
    const titles = await page.locator('.clip-grid .card-title').allTextContents()
    assert.ok(titles.includes('Elden Ring') && !titles.includes('ELDEN RING'), `an untitled clip is titled by its game, in natural case: ${titles.slice(0, 3)}`)
    assert.ok(!titles.some((t) => /^Clip ·/.test(t)), 'no card is titled "Clip · time"')
    // (The newest item is a screenshot now that they sit among every clip.)
    assert.match(await page.locator('.clip-grid .card .frame span').first().innerText(), /^\d+:\d\d$/, 'the length is on the picture')

    // A game filter lists its clips, and a card there doesn't say the game again.
    await selectGame(page, 'Hades II')
    assert.equal(await cards(), 3, "only that game's clips")
    assert.equal(await gameChoice(page, 'Hades II').getAttribute('aria-current'), 'page', 'the current game remains visible')
    assert.equal(await page.locator('main h1').first().innerText(), 'Hades II', 'the heading names the game')
    assert.ok(!(await page.locator('.clip-grid').first().innerText()).includes('Hades'), 'no game name on a card in its own list')
    // Game-specific actions are separate from library view options.
    await choose(page, 'Library actions', 'Hades II is not a game')
    await page.getByText('Ignoring Hades II').waitFor()
    await page.waitForTimeout(200)
    assert.equal(await gameChoice(page, /^Hades II/).count(), 0, 'game removed')
    assert.equal(await gameChoice(page, /^Counter-Strike 2/).count(), 1, 'others stay')
    assert.equal(await libraryNav.getByRole('button', { name: /^All clips/ }).getAttribute('aria-current'), 'page', 'back to every clip')

    // Screenshots show among their game's clips, open in the library's
    // place, copy as a file, and delete with Undo like clips.
    await selectGame(page, 'Elden Ring')
    const shots = page.getByRole('button', { name: /^Open Elden Ring screenshot/ })
    await shots.first().waitFor()
    assert.equal(await shots.count(), 2, "both screenshots among the game's clips")
    assert.equal(await page.locator('main').getByText(/\d(?:h|m) played/).count(), 0, 'library sections omit play-time counters')
    await page.waitForFunction(() => {
      const img = document.querySelector('main img[src$="?thumb"]')
      return img?.complete && img.naturalWidth > 0
    })
    await shots.first().click()
    const viewer = page.getByRole('dialog', { name: 'Elden Ring screenshot' })
    await viewer.waitFor()
    await page.waitForFunction(() => document.querySelector('[role="dialog"] img')?.naturalWidth > 0)
    assert.equal(await page.locator('main').evaluate((el) => el.closest('[inert]') !== null), true, 'the page behind is out of reach')
    await page.screenshot({ path: path.join(out, 'screenshot-viewer.png') })
    await viewer.getByRole('button', { name: 'Copy screenshot' }).click()
    await viewer.getByText('Screenshot copied').waitFor()
    assert.match(await page.evaluate(() => window.__copied), /\.png$/, 'the PNG went to copy_clip')
    await page.keyboard.press('Escape')
    assert.equal(await viewer.count(), 0, 'Esc closes the screenshot')
    assert.equal(await gameChoice(page, 'Elden Ring').getAttribute('aria-current'), 'page', "and stays on the game's clips")
    assert.match(await page.evaluate(() => document.activeElement.getAttribute('aria-label') ?? ''), /^Open Elden Ring screenshot/, 'focus is back on the card it was opened from')
    await page.screenshot({ path: path.join(out, 'game-screenshots.png') })
    // Copied from its card, a screenshot says so there, as a clip does.
    const shotCard = page.locator('.screenshot-card').first()
    await shotCard.hover()
    await shotCard.getByRole('button', { name: 'Copy screenshot', exact: true }).click()
    await shotCard.getByRole('button', { name: 'Copy screenshot', exact: true }).filter({ hasText: 'Copied' }).waitFor()
    await cardAction(page, page.locator('.clip-grid > *').filter({ has: page.getByRole('button', { name: /^Open Elden Ring screenshot/ }) }).first(), 'Delete', true)
    await page.getByText('Screenshot deleted').waitFor()
    assert.equal(await shots.count(), 1, 'a deleted screenshot vanishes at once')
    await page.getByRole('button', { name: 'Undo' }).click()
    await page.waitForFunction(() => document.querySelectorAll('[aria-label^="Open Elden Ring screenshot"]').length === 2)
    await libraryNav.getByRole('button', { name: /^All clips/ }).click()
    await selectGame(page, 'All games')
  }
  // The window's own buttons show only where the system frame is off.
  assert.equal(await page.getByRole('button', { name: 'Close', exact: true }).count(), scenario === 'full' ? 1 : 0)
  if (scenario === 'empty') {
    await page.getByRole('region', { name: 'Capture', exact: true }).getByRole('button', { name: 'Choose screen', exact: true }).waitFor()
    const start = page.getByRole('region', { name: 'Getting started', exact: true })
    await start.getByRole('heading', { name: 'No clips yet', exact: true }).waitFor()
    assert.ok((await start.innerText()).includes('Press F8 while playing to save a clip.'), 'the empty library explains its configured shortcut')
    assert.equal(await page.getByRole('button', { name: 'Save clip', exact: true }).count(), 0, 'no Save replay before a screen is chosen')
    await start.getByRole('button', { name: 'Choose screen', exact: true }).click()
    await page.waitForFunction(() => window.__calls.includes('choose_screen'))
    await start.getByRole('button', { name: 'Change hotkey', exact: true }).click()
    await page.getByRole('heading', { name: 'Hotkeys', exact: true }).waitFor()
    await page.getByRole('button', { name: 'Back to clips', exact: true }).click()
    await start.getByRole('button', { name: 'Clips folder', exact: true }).click()
    await page.getByRole('heading', { name: 'Storage', exact: true }).waitFor()
    await page.close()
    return
  }

  if (scenario === 'big') {
    await page.mouse.move(700, 400)
    for (let i = 0; i < 30; i++) { await page.mouse.wheel(0, 600); await page.waitForTimeout(16) }
    await page.waitForTimeout(300)
    await shot('scrolled')
    const n = await cards()
    const height = await page.evaluate(() => document.querySelector('.library-scroll').scrollHeight)
    assert.ok(n > 0 && n < 60, `virtual grid should window ${data.list_clips.length} clips, rendered ${n}`)
    assert.ok(height > 20_000 && height < 200_000, `grid height ${height}px is wrong for ${data.list_clips.length} clips`)
    // The rows drawn are the ones in view: a card and a day heading are on screen.
    assert.ok(await page.locator('.clip-grid > *').first().isVisible(), 'cards where the window is scrolled to')
    assert.ok((await page.locator('main h2').count()) > 0, 'with their day headings')
    await page.close()
    return
  }

  await shot('clips')
  const before = await cards()
  // A card keeps its actions and metadata in place while its picture responds to hover.
  const hovered = page.locator('.clip-grid > *').nth(1)
  const at = await hovered.boundingBox()
  const caption = await hovered.locator('.card-caption').boundingBox()
  await hovered.hover()
  await page.waitForTimeout(200)
  assert.equal(await hovered.locator('.media-tools').evaluate((el) => getComputedStyle(el).opacity), '1', 'hover reveals the clip actions')
  await shot('clips-hover')
  assert.deepEqual(await hovered.boundingBox(), at, 'a hovered card keeps its place and size')
  assert.deepEqual(await hovered.locator('.card-caption').boundingBox(), caption, 'hover does not replace or move the clip caption')
  assert.equal(await hovered.getByRole('button').count(), 3, 'open, copy and more actions')
  // A right-click opens the card's menu; Esc closes the menu and nothing else.
  await hovered.click({ button: 'right' })
  const context = page.getByRole('menu', { name: 'Clip actions' })
  await context.getByRole('menuitem', { name: 'Show in folder' }).waitFor()
  assert.ok(await context.locator('[role^="menuitem"]').last().evaluate((el) => {
    const r = el.getBoundingClientRect()
    return r.left >= 0 && r.top >= 0 && r.right <= innerWidth && r.bottom <= innerHeight && el.contains(document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2))
  }), 'the context menu keeps its final action in view and reachable above the workspace')
  await shot('clips-context')
  await page.keyboard.press('Escape')
  assert.equal(await context.count(), 0, 'Esc closes the menu')
  // Opening a fixed menu must tolerate a queued scroll from bringing its
  // trigger into view, but genuinely moving its source still dismisses it.
  await hovered.getByRole('button', { name: 'Clip actions', exact: true }).click()
  await context.getByRole('menuitem', { name: 'Delete', exact: true }).waitFor()
  await page.locator('.library-scroll').evaluate((el) => el.scrollBy(0, 30))
  await context.waitFor({ state: 'hidden' })
  await page.locator('.library-scroll').evaluate((el) => el.scrollTo(0, 0))
  // The arrow keys move between cards.
  await page.locator('.clip-open').first().focus()
  await page.keyboard.press('ArrowRight')
  assert.equal(await page.evaluate(() => [...document.querySelectorAll('.clip-open')].indexOf(document.activeElement)), 1, 'Right moves to the next card')

  await quickCopy(page)
  await cardKeys(page)

  // Selection, bulk delete, undo.
  await choose(page, 'Library actions', 'Select clips')
  await page.getByRole('button', { name: /^Select .* clip/ }).nth(1).click()
  await page.getByRole('button', { name: /^Select .* clip/ }).nth(3).click()
  await page.waitForTimeout(200)
  await shot('clips-select')
  // Selection actions stay beside the library, while replay capture remains available.
  const strip = page.getByRole('toolbar', { name: 'Selection actions' })
  assert.match(await strip.innerText(), /2 selected\s*\d+ MB/, 'it says how many and how much')
  const box = await strip.boundingBox()
  const toolbarBounds = await page.locator('.library-toolbar').boundingBox()
  assert.equal(box.y, toolbarBounds.y + toolbarBounds.height, 'selection actions follow the library toolbar')
  assert.ok(await page.getByRole('region', { name: 'Capture', exact: true }).getByRole('button', { name: 'Save clip', exact: true }).isEnabled(), 'replay capture stays available during selection')
  await page.getByRole('button', { name: 'Delete', exact: true }).click()
  await page.waitForTimeout(200)
  assert.equal(await cards(), before - 2, 'deleted clips vanish at once')
  await shot('clips-undo')
  await page.getByRole('button', { name: 'Undo' }).click()
  await page.waitForTimeout(200)
  assert.equal(await cards(), before, 'undo brings the clips back')
  // Ctrl-click starts picking without the menu; Shift-click takes the range; Esc ends it.
  await page.locator('.card .clip-open').first().click({ modifiers: ['Control'] })
  await page.locator('.card .clip-open').nth(2).click({ modifiers: ['Shift'] })
  assert.match(await strip.innerText(), /3 selected/, 'Ctrl-click then Shift-click picks the range')
  await page.keyboard.press('Escape')
  assert.equal(await strip.count(), 0, 'Esc stops selecting')

  // Favorites, from the navigation.
  await libraryNav.getByRole('button', { name: /^Favorites/ }).click()
  await page.waitForTimeout(200)
  assert.ok((await cards()) > 0 && (await cards()) < before, 'Favorites narrows the grid')
  await shot('clips-favorites')
  await selectGame(page, 'Elden Ring')
  assert.equal(await libraryNav.getByRole('button', { name: 'Favorites', exact: true }).getAttribute('aria-current'), 'page', 'choosing a game keeps the Favorites collection')
  assert.ok((await cards()) > 0, 'the game has favorite clips')
  assert.ok((await page.locator('.clip-open').evaluateAll((buttons) => buttons.map((button) => button.getAttribute('aria-label')))).every((name) => name.includes('Elden Ring')), 'Favorites and game filtering compose')
  await selectGame(page, 'All games')
  assert.equal(await libraryNav.getByRole('button', { name: 'Favorites', exact: true }).getAttribute('aria-current'), 'page', 'clearing the game filter preserves Favorites')
  await libraryNav.getByRole('button', { name: /^All clips/ }).click()
  assert.equal(await page.getByRole('button', { name: 'Show all games', exact: true }).count(), 0, 'All clips clears game filtering')

  // Viewer: playback, stepping, the menu, rename, tags, trim, copy, save.
  await page.getByRole('button', { name: /^Open .*Elden Ring clip/ }).nth(1).click()
  await page.waitForFunction(() => document.querySelector('video')?.readyState >= 2)
  await seekTo(page, 2.5)
  await page.waitForTimeout(300)
  await shot('viewer')
  // Fullscreen covers the whole window, not the picture's place in the column.
  await page.getByRole('button', { name: 'Fullscreen', exact: true }).click()
  assert.deepEqual(await page.locator('.player').boundingBox(), { x: 0, y: 0, width: 1280, height: 800 }, 'fullscreen fills the whole window from its top-left corner')
  assert.equal(await page.locator('[data-deck]').count(), 0, 'fullscreen removes the editing deck')
  await page.keyboard.press('f')
  assert.equal(await page.getByRole('button', { name: 'Fullscreen', exact: true }).count(), 1, 'F leaves fullscreen')
  assert.equal(await page.getByRole('slider', { name: 'Trim start' }).count(), 0, 'a clip opens for review without trim handles')
  assert.equal(await page.getByRole('button', { name: /^Save (?:trimmed|edited) copy$/ }).count(), 0, 'an unchanged saved clip does not offer another save')
  // While playing, the playhead moves every frame, not at timeupdate's ~4 Hz.
  const positions = await page.evaluate(
    () =>
      new Promise((done) => {
        const v = document.querySelector('video')
        const seek = document.querySelector('[aria-label="Seek"]')
        const seen = new Set()
        v.muted = true
        v.play().then(() => {
          const until = performance.now() + 500
          requestAnimationFrame(function frame(now) {
            seen.add(seek.getAttribute('aria-valuenow'))
            if (now < until) requestAnimationFrame(frame)
            else {
              v.pause()
              done(seen.size)
            }
          })
        })
      }),
  )
  assert.ok(positions >= 12, `playhead drew ${positions} positions in 0.5 s (want a smooth ~30)`)
  // The viewer takes the library's place, opaque: everything under the
  // window's own strip, with no capture strip of its own.
  const dialog = page.getByRole('dialog')
  assert.deepEqual(await dialog.boundingBox(), { x: 0, y: 32, width: 1280, height: 768 }, 'the viewer fills the window under its strip')
  assert.equal(await dialog.locator('footer').count(), 0, 'no capture strip in the viewer')
  assert.equal(await dialog.getByRole('button', { name: 'Copy clip' }).count(), 1, 'Copy clip')
  assert.equal(await dialog.getByRole('button', { name: 'Trim', exact: true }).count(), 1, 'trimming is available when needed')
  assert.match(await dialog.getByLabel('Time', { exact: true }).innerText(), /^0:0\d \/ 0:\d\d$/, 'one time readout')
  assert.equal(await dialog.getByText(/selected/).count(), 0, 'nothing says "selected" while the whole clip is')
  // Left and Right step to the clips either side without leaving it.
  const playing = () => page.locator('video').getAttribute('src')
  const here = await playing()
  await page.keyboard.press('ArrowRight')
  await page.waitForFunction((was) => document.querySelector('video').getAttribute('src') !== was, here)
  await page.keyboard.press('ArrowLeft')
  await page.waitForFunction((was) => document.querySelector('video').getAttribute('src') === was, here)
  await page.waitForFunction(() => document.querySelector('video')?.readyState >= 2)
  // Everything else is in the one menu. The saved clip's levels: faders of
  // our own, not the browser's range input, in a labelled panel.
  await page.getByRole('button', { name: 'Clip options' }).click()
  await page.waitForTimeout(150)
  await shot('viewer-menu')
  await page.getByRole('menuitem', { name: 'Audio mix' }).click()
  assert.equal(await page.locator('input[type="range"]').count(), 0, 'no native range inputs')
  const level = page.getByRole('slider', { name: 'Game level' })
  await level.focus()
  await page.keyboard.press('ArrowLeft')
  assert.equal(await level.getAttribute('aria-valuenow'), '95', 'arrow keys step a fader')
  assert.equal(await playing(), here, 'without stepping to another clip')
  await level.dblclick()
  assert.equal(await level.getAttribute('aria-valuenow'), '100', 'a double-click returns it to 100%')
  // Esc closes the innermost thing first: the panel, not the viewer.
  await page.keyboard.press('Escape')
  assert.equal(await page.getByRole('dialog', { name: 'Audio mix' }).count(), 0, 'Esc closes the panel')
  assert.equal(await page.locator('video').count(), 1, 'and leaves the viewer open')
  await seekTo(page, 2.5)
  // Ctrl+C is Copy clip: this one is over the sharing limit, so a smaller copy goes.
  await page.keyboard.press('Control+c')
  await page.locator('.edit-status').getByText('Clip copied, under 10 MB').waitFor()
  assert.match(await page.evaluate(() => window.__copied), /_edit\.mp4$/, 'the fitted copy went to copy_clip')
  assert.equal(await page.evaluate(() => window.__lastRender.maxMb), 10, 'fitted under the limit from Settings → Sharing')
  assert.equal(await page.locator('.edit-status').getByText('Clip copied, under 10 MB').evaluate((el) => !!el.closest('[data-deck]')), true, 'said beside the actions')
  // Marked moments: ticks on the line, [ and ] jump between them.
  await startTrimming(page)
  assert.equal(await page.locator('[aria-label="Timeline"] .tick:not(.saved)').count(), 2, 'two marked moments on the strip')
  assert.equal(await page.locator('[aria-label="Timeline"] .tick.saved').count(), 1, 'and where it was saved, apart from them')
  const nearTime = (t) =>
    page.waitForFunction((t) => Math.abs(document.querySelector('video').currentTime - t) < 0.05, t, { timeout: 3000 })
  await page.keyboard.press(']')
  await nearTime(3.5)
  await page.keyboard.press('[')
  await nearTime(1.5)
  await page.keyboard.press('[')
  await page.waitForTimeout(150)
  await nearTime(1.5)
  // T keeps the frame on screen as the clip's picture; Undo puts back the one before.
  await page.evaluate(() => document.querySelector('video').pause())
  await page.keyboard.press('t')
  await dialog.locator('.cover-feedback').getByText('Cover set').waitFor()
  assert.ok(Math.abs((await page.evaluate(() => window.__patch.cover)) - 1.5) < 0.05, 'the frame on screen is the cover')
  await dialog.getByRole('button', { name: 'Undo', exact: true }).click()
  assert.equal(await page.evaluate(() => window.__patch.cover), -1, 'Undo clears it again')
  await page.keyboard.press('n')
  await page.keyboard.type('Malenia no-hit, take 2')
  await page.keyboard.press('Enter')
  assert.equal(await dialog.locator('h1').innerText(), 'Malenia no-hit, take 2', 'N renames')
  await choose(page, 'Clip options', 'Add a tag')
  await page.keyboard.type('pb')
  await page.keyboard.press('Enter')
  await page.keyboard.press('Escape')
  await page.waitForTimeout(200)
  await page.keyboard.press('s')
  await page.waitForTimeout(200)
  await shot('viewer-meta')
  assert.equal(await page.locator('.tag', { hasText: 'pb' }).count(), 1, 'tag added')
  await page.keyboard.press('?')
  await page.waitForTimeout(150)
  await shot('viewer-help')
  await page.keyboard.press('Escape')
  await seekTo(page, 1)
  await page.keyboard.press('i')
  await seekTo(page, 4.5)
  await page.keyboard.press('o')
  // Favoriting mid-trim (a new clip object from the page) keeps the trim.
  await page.keyboard.press('s')
  await page.waitForTimeout(200)
  await seekTo(page, 2.5)
  await page.waitForTimeout(200)
  await shot('viewer-trim')
  // The strip under the picture is the one timeline; the handles on it trim.
  const trimStart = page.getByRole('slider', { name: 'Trim start' })
  const trimEnd = page.getByRole('slider', { name: 'Trim end' })
  assert.equal(await page.locator('[aria-label="Timeline"]').count(), 1, 'one timeline')
  assert.equal(await page.locator('.player [aria-label="Timeline"]').count(), 0, 'under the video, not over it')
  assert.equal(await trimStart.getAttribute('aria-valuenow'), '1', 'in at 1 s')
  assert.equal(await trimEnd.getAttribute('aria-valuenow'), '4.5', 'out at 4.5 s')
  assert.match(await trimStart.evaluate((el) => getComputedStyle(el, '::after').backgroundColor), /^(oklab|color|rgb)\(/, 'the handles are drawn, a step under the accent')
  assert.ok((await page.locator('[aria-label="Timeline"] .film > div').count()) > 4, "the strip shows the clip's frames")
  await dialog.getByText('3.5 seconds selected').waitFor()
  // A handle is moved from the keyboard, says where it is while it has
  // focus, and never crosses the other.
  await trimEnd.focus()
  await page.waitForTimeout(250)
  assert.equal(await trimEnd.locator('.stamp').evaluate((el) => getComputedStyle(el).opacity), '1', 'a focused handle shows its time')
  await page.keyboard.press('Home')
  assert.equal(await trimEnd.getAttribute('aria-valuenow'), '1.1', 'the end stops short of the start')
  // Exact points, from the menu.
  await choose(page, 'Clip options', 'Set trim points')
  await page.getByRole('textbox', { name: 'Trim end' }).fill('0:00.5')
  await page.getByRole('button', { name: 'Apply' }).click()
  await page.getByRole('alert').filter({ hasText: 'The end has to come after the start.' }).waitFor()
  await page.getByRole('textbox', { name: 'Trim end' }).fill('0:04.5')
  await page.getByRole('button', { name: 'Apply' }).click()
  assert.equal(await trimEnd.getAttribute('aria-valuenow'), '4.5', 'typed points move the handles')
  await dialog.getByRole('button', { name: 'Done trimming' }).click()
  assert.equal(await trimStart.count(), 0, 'Done returns to preview without edit handles')
  assert.ok(await dialog.getByRole('button', { name: 'Save trimmed copy', exact: true }).isVisible(), 'the pending trim remains available to save')
  await startTrimming(page)
  assert.deepEqual([await trimStart.getAttribute('aria-valuenow'), await trimEnd.getAttribute('aria-valuenow')], ['1', '4.5'], 'returning to Trim preserves the chosen range')
  // The library keeps its place behind the viewer, out of sight and reach.
  assert.equal(await cards(), before, 'the grid is still there')
  assert.equal(await page.locator('main').evaluate((el) => el.closest('[inert]') !== null), true, 'and inert')
  assert.equal(await page.locator('main').evaluate((el) => getComputedStyle(el.parentElement).visibility), 'hidden', 'and not drawn')
  // Copy clip: the selection, fitted under the limit, then onto the clipboard.
  await page.evaluate(() => (window.__lastRender = window.__copied = null))
  await dialog.getByRole('button', { name: 'Copy clip' }).click()
  await page.waitForFunction(() => window.__copied)
  const fitted = await page.evaluate(() => window.__lastRender)
  assert.equal(fitted.maxMb, 10, 'fitted under 10 MB')
  assert.deepEqual([fitted.start, fitted.end], [1, 4.5], 'the selected range')
  assert.match(await page.evaluate(() => window.__copied), /_edit\.mp4$/, 'the fitted clip went to the clipboard')
  assert.equal(await page.getByText('Clip saved').count(), 0, 'and is not announced as saved')
  // Save clip: progress with a way out, no second submission, then "Clip saved".
  await page.evaluate(() => {
    window.__lastRender = null
    window.__renderGate = new Promise((go) => (window.__renderGo = go))
  })
  const announcement = await dialog.locator('[role="status"][aria-atomic="true"]').elementHandle()
  await dialog.getByRole('button', { name: /^Save (?:trimmed|edited) copy$/ }).click()
  await dialog.locator('.edit-status').getByText(/Saving clip… \d+%/).waitFor()
  assert.ok(await announcement.evaluate((el) => el.isConnected && el.textContent.includes('Saving clip')), 'the existing live region announces progress')
  assert.equal(await dialog.getByRole('button', { name: /^Save (?:trimmed|edited) copy$/ }).isDisabled(), true, 'Save clip is off while it saves')
  assert.equal(await dialog.getByRole('button', { name: 'Copy clip' }).isDisabled(), true, 'and so is Copy clip')
  assert.equal(await dialog.getByRole('button', { name: 'Cancel' }).count(), 1, 'with a Cancel')
  await shot('viewer-saving')
  await page.evaluate(() => {
    window.__renderGo()
    window.__renderGate = null
  })
  await dialog.locator('.edit-status').getByText('Clip saved').waitFor()
  const rendered = await page.evaluate(() => window.__lastRender)
  assert.equal(rendered.maxMb, null, 'a saved clip is not shrunk unless Settings → Sharing says so')
  // The clip starts with game and mic mixed: levels for the two sources
  // only (the app leaves the mix out of the edit).
  assert.deepEqual(rendered.volumes, [1, 1], 'a level per source track, none for the mix')
  assert.deepEqual([rendered.start, rendered.end], [1, 4.5], 'the selected range')
  assert.equal(rendered.vertical, null, 'original format unless asked')
  assert.equal(rendered.slow, null, 'no slow motion unless asked')
  // A failure says what and why, keeps the selection, and offers the same again.
  await page.evaluate(() => (window.__renderError = 'the encoder stopped'))
  await dialog.getByRole('button', { name: /^Save (?:trimmed|edited) copy$/ }).click()
  await dialog.locator('.edit-status').getByText("Couldn't save the clip: the encoder stopped").waitFor()
  await shot('viewer-failed')
  assert.equal(await trimStart.getAttribute('aria-valuenow'), '1', 'the selection is kept')
  // A retry belongs to the failed source even if another clip is now open.
  await page.keyboard.press('Escape')
  await page.locator('.media-card[data-clip-path$="/clip-2.mp4"] .clip-open').click()
  await page.waitForFunction(() => document.querySelector('video')?.readyState >= 2)
  const reviewingDuringRetry = await playing()
  await page.evaluate(() => { window.__renderError = null; window.__lastRender = null })
  await dialog.getByRole('button', { name: 'Try again' }).click()
  await dialog.locator('.edit-status').getByText('Clip saved').waitFor()
  const retried = await page.evaluate(() => window.__lastRender)
  assert.equal(retried.path, rendered.path, 'Save retry stays pinned to the original clip after navigation')
  assert.deepEqual([retried.start, retried.end], [1, 4.5], 'Save retry keeps the original trim')
  assert.equal(await playing(), reviewingDuringRetry, 'the background retry leaves the current preview in place')
  await page.keyboard.press('Escape')
  await page.locator('.media-card[data-clip-path$="/clip-1.mp4"] .clip-open').click()
  await page.waitForFunction(() => document.querySelector('video')?.readyState >= 2)
  // Vertical 9:16 for Shorts and TikTok, with a slow-motion range: both from the menu.
  await choose(page, 'Clip options', 'Vertical 9:16', 'menuitemcheckbox')
  await page.getByRole('slider', { name: 'Vertical frame position' }).waitFor()
  await choose(page, 'Clip options', 'Slow motion ½×', 'menuitemradio')
  await page.getByRole('slider', { name: 'Slow motion start' }).waitFor()
  await dialog.getByText('3.5 seconds selected\u20039:16\u2003½× slow motion').waitFor()
  await page.waitForTimeout(150)
  await shot('viewer-vertical-slow')
  await page.evaluate(() => (window.__lastRender = null))
  await dialog.getByRole('button', { name: /^Save (?:trimmed|edited) copy$/ }).click()
  await page.waitForFunction(() => window.__lastRender)
  const shaped = await page.evaluate(() => window.__lastRender)
  assert.equal(shaped.vertical, 0.5, 'vertical frame centred by default')
  assert.equal(shaped.slow.factor, 2, 'half speed')
  assert.ok(shaped.slow.from > 1 && shaped.slow.to < 4.5 && shaped.slow.from < shaped.slow.to, 'slow range inside the trim')
  await dialog.locator('.edit-status').getByText('Clip saved').waitFor()
  // Back, and the library is as it was left: the same card has focus.
  await page.keyboard.press('Escape')
  assert.equal(await page.locator('video').count(), 0, 'Esc closes the viewer')
  assert.match(await page.evaluate(() => document.activeElement.getAttribute('aria-label') ?? ''), /^Open Malenia no-hit, take 2/, 'focus is back on the clip that was open')
  // Opening it again finds the selection as it was left; Reset returns to the whole clip.
  await page.keyboard.press('Enter')
  await page.waitForFunction(() => document.querySelector('video')?.readyState >= 2)
  assert.equal(await trimStart.getAttribute('aria-valuenow'), '1', 'the trim is kept for the session')
  await dialog.getByRole('button', { name: 'Reset' }).click()
  assert.equal(await trimStart.count(), 0, 'Reset returns to the simple whole-clip preview')
  assert.equal(await dialog.getByRole('button', { name: /^Save (?:trimmed|edited) copy$/ }).count(), 0, 'Reset removes the redundant save action')
  assert.equal(await dialog.getByRole('button', { name: 'Reset' }).count(), 0, 'and Reset goes with it')
  await page.keyboard.press('Escape')
  // A clip already under the limit goes to the clipboard as it is, at once.
  await page.evaluate(() => (window.__lastRender = null))
  await page.getByRole('button', { name: /^Open .*Elden Ring clip/ }).nth(2).click()
  // A clipboard that refuses: said with Try again, and the button never says Copied.
  const copyButton = dialog.locator('[data-deck] .btn-primary')
  const copyWidth = (await copyButton.boundingBox()).width
  await page.evaluate(() => (window.__copyError = 'the clipboard is busy'))
  await copyButton.click()
  await dialog.locator('.edit-status').getByText("Couldn't copy the clip: the clipboard is busy").waitFor()
  assert.equal(await dialog.getByRole('button', { name: 'Copied' }).count(), 0, 'a failed copy never says Copied')
  await page.waitForTimeout(250)
  await shot('viewer-copy-failed')
  await page.evaluate(() => (window.__copyError = null))
  await dialog.getByRole('button', { name: 'Try again' }).click()
  // Done: the button says so for a moment, at the same width, then goes back.
  await dialog.getByRole('button', { name: 'Copied' }).waitFor()
  assert.equal((await copyButton.boundingBox()).width, copyWidth, 'Copied keeps the button its width')
  await page.waitForTimeout(250)
  await shot('viewer-copied')
  assert.match(await page.evaluate(() => window.__copied), /clip-2\.mp4$/, 'the clip itself, not an edit')
  assert.equal(await page.evaluate(() => window.__lastRender), null, 'and nothing was rendered')
  await dialog.getByRole('button', { name: 'Copy clip' }).waitFor({ timeout: 3000 })
  await page.keyboard.press('Escape')

  // A single capture command remains available without live counters in the library.
  const capture = page.getByRole('region', { name: 'Capture', exact: true })
  await capture.getByText('Elden Ring', { exact: true }).waitFor()
  assert.match(await capture.getByRole('status').textContent(), /^Clipping Elden Ring/, 'the capture status is announced')
  assert.equal(await capture.locator('.split').evaluate((el) => Math.round(el.getBoundingClientRect().height)), 44, 'one compact capture control')
  assert.equal(await capture.getByText(/\d+s ready/).count(), 0, 'there is no live buffer counter')
  assert.equal(await capture.getByRole('button', { name: 'Save clip', exact: true }).isEnabled(), true, 'Save clip works while a game records')
  assert.equal(await capture.getByText(/not a game/i).count(), 0, '"not a game" is not shown beside the game')
  assert.equal(await capture.getByRole('progressbar').count(), 0, 'there is no buffer meter')
  assert.equal(await page.locator('.state-dot').count(), 0, 'the library has no blinking status indicator')
  await fire('status-changed', { ...data.get_status, session_start_ms: null, buffered_seconds: 0 })
  assert.equal(await capture.getByRole('button', { name: 'Save clip', exact: true }).isEnabled(), false, 'an empty replay buffer cannot be saved')
  await fire('status-changed', { ...data.get_status, state: 'waiting', game: null, session_start_ms: null, buffered_seconds: 0 })
  await capture.locator('.line', { hasText: 'Waiting for a game' }).waitFor()
  assert.equal(await capture.getByRole('button', { name: 'Save clip', exact: true }).count(), 0, 'no dead Save clip button while waiting for a game')
  assert.equal(await capture.getByRole('button', { name: 'Capture options', exact: true }).count(), 1, 'its options stay reachable')
  await fire('status-changed', data.get_status)
  assert.equal(await capture.getByRole('button', { name: 'Save clip', exact: true }).isEnabled(), true, 'an available replay enables Save clip again')
  await choose(page, 'Capture options', 'Elden Ring is not a game')
  await page.getByText('Ignoring ELDEN RING').waitFor()

  // Settings use the sidebar while capture remains available below them.
  await page.getByRole('button', { name: 'Settings', exact: true }).click()
  const settingsPage = (name) => page.getByRole('navigation', { name: 'Settings pages' }).getByRole('button', { name, exact: true })
  assert.deepEqual(await page.getByRole('navigation', { name: 'Settings pages' }).getByRole('button').allInnerTexts(), ['Back to clips', 'General', 'Recording', 'Audio', 'Hotkeys', 'Storage', 'Sharing', 'Appearance'], 'Back to clips, then the settings pages')
  await shot('settings-general')
  await settingsPage('Appearance').click()
  await page.waitForTimeout(150)
  assert.equal(await page.getByRole('switch', { name: 'Tags' }).getAttribute('aria-checked'), 'true', 'tags are a preference')
  await shot('settings-appearance')
  // Theme: a colour swaps the accent and tints the mark; Rainbow moves the
  // identity only while focused and with motion allowed.
  const accent = () => page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue('--color-accent').trim())
  const fills = () => page.locator('.library-sidebar svg g').evaluateAll((gs) => gs.map((g) => getComputedStyle(g).fill))
  const moving = () => page.locator('.library-sidebar .brand-lockup').evaluate((el) => el.getAnimations({ subtree: true }).length)
  const theme = (name) => page.getByRole('radiogroup', { name: 'Theme' }).getByRole('radio', { name, exact: true })
  assert.equal(await theme('Off-white').getAttribute('aria-checked'), 'true', 'off-white by default')
  assert.equal(await accent(), '#e4e5df')
  await theme('Blue').click()
  assert.equal(await accent(), '#8db7ff', 'Blue swaps the accent')
  assert.equal(await page.locator('.library-sidebar .mark').evaluate((m) => getComputedStyle(m).fill), 'rgb(141, 183, 255)', 'and tints the mark')
  assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('reruns/prefs')).theme), 'blue', 'the theme is kept')
  await theme('Rainbow').click()
  assert.equal(await accent(), '#e4e5df', 'Rainbow keeps offwhite controls')
  const f = await fills()
  assert.equal(f.length, 8, 'eight glyphs: rr and reruns')
  assert.equal(new Set(f).size, 8, `each glyph its own colour: ${f}`)
  assert.equal(await moving(), 2 + 8, 'both pictures turn their hue and each glyph wiggles')
  await shot('settings-theme-rainbow')
  await page.evaluate(() => window.dispatchEvent(new Event('blur')))
  assert.equal(await moving(), 0, 'nothing moves without focus')
  await page.evaluate(() => window.dispatchEvent(new Event('focus')))
  await page.getByRole('switch', { name: 'Reduce motion' }).click()
  assert.equal(await moving(), 0, 'reduced motion stops it')
  assert.deepEqual(await fills(), f, 'and the rainbow holds still')
  await page.getByRole('switch', { name: 'Reduce motion' }).click()
  await theme('Off-white').click()
  assert.equal(await page.evaluate(() => document.documentElement.hasAttribute('data-theme')), false, 'the default sets no attribute')
  // Sharing: the size Copy clip fits under.
  await settingsPage('Sharing').click()
  assert.equal(await page.getByRole('switch', { name: 'Limit copied clips' }).getAttribute('aria-checked'), 'true', 'copies are limited, as Copy for Discord was')
  await page.getByLabel('Maximum size', { exact: true }).selectOption('25')
  assert.equal(await page.evaluate(() => JSON.parse(localStorage.getItem('reruns/prefs')).shareMaxMb), 25, 'the limit is kept')
  await shot('settings-sharing')
  await page.getByLabel('Maximum size', { exact: true }).selectOption('10')
  await settingsPage('Recording').click()
  await page.waitForTimeout(150)
  await shot('settings-recording')
  // Settings scroll below their header while their navigation and capture stay in view.
  const scroller = page.locator('.settings > div')
  await scroller.evaluate((el) => el.scrollTo(0, el.scrollHeight))
  await shot('settings-bottom')
  const pane = await scroller.boundingBox()
  const head = await page.locator('.settings > header').boundingBox()
  assert.ok(pane.y >= head.y + head.height, 'the page scrolls below its header')
  assert.ok(await capture.isVisible(), 'capture stays visible in Settings')
  const settingsBounds = await page.locator('main').boundingBox()
  assert.equal(pane.y + pane.height, settingsBounds.y + settingsBounds.height, 'the page runs to the workspace bottom edge')
  assert.equal(await page.getByRole('navigation', { name: 'Settings pages' }).count(), 1, 'one local settings navigation')
  assert.equal(await page.getByRole('navigation', { name: 'Settings pages' }).getByRole('button', { name: 'Back to clips' }).count(), 1, 'with Back to clips')
  const last = await page.locator('.settings section').last().boundingBox()
  assert.ok(last.y + last.height <= pane.y + pane.height, 'the last row clears the strip')
  const categoryBounds = await settingsPage('General').boundingBox()
  assert.ok(categoryBounds.x + categoryBounds.width <= pane.x, 'settings navigation stays beside the scrolling page')
  await scroller.evaluate((el) => el.scrollTo(0, 0))
  assert.equal(await page.getByRole('button', { name: 'Detect ELDEN RING again' }).count(), 1, 'ignored app listed')
  await page.getByText('HDR display').waitFor()
  assert.ok(await page.getByText('SDR white 240 nits').isVisible(), 'HDR row shows the SDR white level')
  // Windows: HDR compatibility, and the experimental window capture under it.
  await fire('status-changed', { state: 'recording', game: 'ELDEN RING', steam_app_id: 1245620, session_start_ms: Date.now(), buffered_seconds: 30, replay_seconds: 30, encoder: 'nvenc', capture_path: 'zerocopy', error: null, backend: 'windows', needs_screen: false, hdr: { sdr_white_nits: 240, peak_nits: 1000 }, hdr_tonemapped: true })
  await page.getByRole('switch', { name: 'HDR compatibility' }).click()
  await page.getByRole('switch', { name: 'Capture the window in HDR' }).waitFor()
  await page.getByRole('switch', { name: 'Capture the window in HDR' }).click()
  await page.waitForTimeout(250) // let the switch finish moving before the shot
  assert.equal(await page.getByRole('switch', { name: 'Capture the window in HDR' }).getAttribute('aria-checked'), 'true', 'HDR window capture on')
  await shot('settings-recording-windows')
  // Storage: favorites are kept by the limit unless turned off.
  await settingsPage('Storage').click()
  const favorites = page.getByRole('switch', { name: 'Protect favorites' })
  assert.equal(await favorites.getAttribute('aria-checked'), 'true', 'favorites protected by default')
  await favorites.click()
  await page.waitForFunction(
    () => document.querySelector('[role="switch"][aria-label="Protect favorites"]').getAttribute('aria-checked') === 'false',
  )
  await favorites.click()
  await shot('settings-storage')
  // Back to the library for the recorder's states.
  await page.getByRole('button', { name: 'Back to clips' }).click()
  // Capture failing mid-game: the strip says so instead of "waiting".
  await fire('status-changed', { state: 'retrying', game: 'Overwatch', steam_app_id: null, session_start_ms: Date.now(), buffered_seconds: 0, replay_seconds: 30, encoder: 'nvd3d11h264enc', capture_path: null, error: 'Could not encode stream: Failed to open session', backend: 'windows', needs_screen: false, hdr: null, hdr_tonemapped: false })
  await capture.locator('.line', { hasText: 'Capture interrupted' }).waitFor()
  await capture.getByText('Retrying…', { exact: true }).waitFor()
  assert.equal(await page.getByText('Waiting for a game').count(), 0, 'no "waiting" while a game is failing')
  assert.equal(await capture.getByRole('button', { name: 'Save clip', exact: true }).count(), 0, 'no Save replay while nothing records')
  await shot('status-retrying')
  // NVENC refused at the game's start: AMD records it, and the strip says so.
  await fire('status-changed', { state: 'recording', game: 'Fortnite', steam_app_id: null, session_start_ms: Date.now(), buffered_seconds: 30, replay_seconds: 30, encoder: 'amfh264enc', encoder_fallback: { preferred: 'nvenc', using: 'amf', reason: 'Failed to open session', set_aside: false }, capture_path: 'zerocopy', error: null, backend: 'windows', needs_screen: false, hdr: null, hdr_tonemapped: false })
  await capture.getByText('Fortnite', { exact: true }).waitFor()
  await capture.getByText('· AMF', { exact: true }).waitFor()
  await page.getByRole('button', { name: 'Capture options', exact: true }).click()
  const why = await page.getByRole('menu', { name: 'Capture options' }).innerText()
  assert.ok(why.includes('NVIDIA NVENC') && why.includes('Failed to open session'), `fallback says why: ${why}`)
  assert.ok(why.includes('next game'), 'and that the next game tries again')
  await page.keyboard.press('Escape')
  await shot('status-encoder-fallback')
  // Set aside after failing while recording: no promise of the next game.
  await fire('status-changed', { state: 'recording', game: 'Fortnite', steam_app_id: null, session_start_ms: Date.now(), buffered_seconds: 30, replay_seconds: 30, encoder: 'amfh264enc', encoder_fallback: { preferred: 'nvenc', using: 'amf', reason: 'Failed to open session', set_aside: true }, capture_path: 'zerocopy', error: null, backend: 'windows', needs_screen: false, hdr: null, hdr_tonemapped: false })
  await page.getByRole('button', { name: 'Capture options', exact: true }).click()
  const fallbackNote = await page.getByRole('menu', { name: 'Capture options' }).innerText()
  assert.ok(fallbackNote.includes('after Reruns restarts'), `set aside says when: ${fallbackNote}`)
  assert.ok(!fallbackNote.includes('next game'), 'no promise of the next game')
  await page.keyboard.press('Escape')
  // The usual encoder: no note.
  await fire('status-changed', { state: 'recording', game: 'Fortnite', steam_app_id: null, session_start_ms: Date.now(), buffered_seconds: 30, replay_seconds: 30, encoder: 'nvd3d11h264enc', encoder_fallback: null, capture_path: 'zerocopy', error: null, backend: 'windows', needs_screen: false, hdr: null, hdr_tonemapped: false })
  await page.getByRole('button', { name: 'Capture options', exact: true }).click()
  assert.ok(!(await page.getByRole('menu', { name: 'Capture options' }).innerText()).includes('AMD AMF'), 'the usual encoder has no fallback note')
  assert.equal(await capture.getByText(/AMD AMF/).count(), 0, 'and the control names no encoder')
  await page.keyboard.press('Escape')
  await page.getByRole('button', { name: 'Settings', exact: true }).click()
  await settingsPage('Audio').click()
  await page.waitForTimeout(150)
  assert.equal(await page.getByLabel('Output device').inputValue(), 'out-headset', 'chosen output is selected')
  assert.equal(await page.getByLabel('Output device').locator('option').count(), 3, 'default + two outputs')
  // Live meters: the page started them; push a reading for each track.
  await page.waitForFunction(() => window.__listeners && window.__listeners['audio-level'])
  await fire('audio-level', { track: 'desktop', peak_db: -14, error: null })
  await fire('audio-level', { track: 'mic', peak_db: null, error: 'device not found' })
  await page.getByText('Hearing sound').waitFor()
  await page.getByText("Can't open: device not found").waitFor()
  await page.waitForTimeout(150)
  await shot('settings-audio')
  // Save sounds: a chime for clips and a shutter for screenshots; pick a
  // file for clips and screenshots follow it.
  await settingsPage('General').click()
  assert.ok(await page.getByText('Built-in chime').isVisible(), 'clips start on the chime')
  assert.ok(await page.getByText('Built-in shutter').isVisible(), 'screenshots start on the shutter')
  await page.getByRole('button', { name: 'Play the clip sound' }).click()
  await page.getByRole('button', { name: 'Play the screenshot sound' }).click()
  await page.getByRole('button', { name: 'Choose file…' }).first().click()
  await page.getByText('airhorn.mp3').first().waitFor()
  assert.equal(await page.getByRole('button', { name: 'Use default' }).count(), 1, 'a custom sound can be reset')
  assert.ok(await page.getByText('Same as clips').isVisible(), 'screenshots use the clip sound')
  const previews = await page.evaluate(() => window.__previews)
  assert.deepEqual(previews[0], { sound: null, slot: 'clip', volume: 100 }, 'preview plays what clips are set to')
  assert.deepEqual(previews[1], { sound: null, slot: 'screenshot', volume: 100 }, 'and what screenshots are set to')
  assert.equal(previews.at(-1).sound.name, 'airhorn.mp3', 'a picked sound is played once')
  await shot('settings-sounds')
  // Updates: checked on request; an update shows in Settings and the navigation.
  await page.getByText('Reruns 0.1.0').waitFor()
  await page.getByRole('button', { name: 'Check now' }).click()
  await page.getByText('Version 0.2.0 available.').waitFor()
  assert.equal(await page.locator('aside').getByRole('button', { name: 'Update to 0.2.0' }).count(), 1, 'the sidebar shows the update')
  await page.getByRole('heading', { name: 'Updates' }).scrollIntoViewIfNeeded()
  await shot('settings-updates')
  // Settings installs through the same prompt as the navigation.
  await page.getByRole('button', { name: 'Update…' }).click()
  await page.getByRole('dialog', { name: 'Reruns 0.2.0' }).getByRole('button', { name: 'Update and restart' }).waitFor()
  await page.getByRole('button', { name: 'Later' }).click()
  // Logs: the button asks the backend to open the folder; a failure shows
  // the message, which names the folder.
  await page.getByRole('button', { name: 'Open the log folder' }).click()
  await page.waitForFunction(() => window.__logOpens === 1)
  await page.evaluate(() => (window.__logError = "Can't open /home/u/.local/share/reruns/logs: no file manager"))
  await page.getByRole('button', { name: 'Open the log folder' }).click()
  await page.getByText("Can't open /home/u/.local/share/reruns/logs: no file manager").waitFor()
  assert.equal(await page.evaluate(() => window.__logOpens), 2, 'each click opens the folder')
  await page.evaluate(() => (window.__logError = null))
  await page.locator('.settings > div').evaluate((el) => el.scrollTo(0, el.scrollHeight))
  await shot('settings-logs')
  // An update found at start-up: the prompt opens once; "Later" leaves a
  // still icon in the navigation that opens it again.
  const updatesBeforeEvent = await page.evaluate(() => window.__calls.filter((c) => c === 'found_update').length)
  await fire('update-available', { version: '1.2.0', current_version: '1.1.0', notes: null, prompt: true })
  await page.getByRole('dialog', { name: 'Reruns 1.2.0' }).waitFor()
  assert.equal(
    await page.evaluate(() => window.__calls.filter((c) => c === 'found_update').length),
    updatesBeforeEvent + 1,
    'a prompt shown from the event is marked as shown',
  )
  await shot('update-prompt')
  await page.getByRole('button', { name: 'Later' }).click()
  assert.equal(await page.getByRole('dialog', { name: 'Reruns 1.2.0' }).count(), 0, 'Later closes the prompt')
  await page.getByRole('button', { name: 'Update to 1.2.0' }).click()
  await page.getByRole('dialog', { name: 'Reruns 1.2.0' }).waitFor()
  await page.keyboard.press('Escape')
  await settingsPage('Hotkeys').click()
  await page.waitForTimeout(150)
  assert.equal(await page.getByRole('button', { name: 'F9', exact: true }).count(), 1, 'screenshot hotkey row')
  assert.equal(await page.locator('main kbd').count(), 0, 'keys are named in plain type')
  // The keyboard's focus ring shows on what it lands on.
  await page.keyboard.press('Tab')
  assert.equal(await page.evaluate(() => getComputedStyle(document.activeElement).outlineStyle), 'solid', 'a visible focus ring from the keyboard')
  await shot('settings-hotkeys')

  await page.close()
}

// ------------------------------------------------------------------ runner

// The in-game popup page: fire the events the backend sends and check what
// it draws. The window is transparent, so it's shot over a stand-in game.
async function overlayScenario(browser) {
  // The window's size on a 1080-line screen (WIDTH × HEIGHT in overlay.rs).
  const page = await browser.newPage({ viewport: { width: 480, height: 130 }, colorScheme: 'dark' })
  page.on('pageerror', (e) => { throw new Error(`page error in overlay: ${e.message}`) })
  await page.addInitScript(mockBridge, fixtures('full'))
  await page.goto(`http://localhost:${PORT}/overlay.html`)
  await page.waitForFunction(() => window.__listeners && window.__listeners['overlay-show'])
  await page.addStyleTag({ content: 'html { background: linear-gradient(135deg, #3a4a5a, #6b5a3a) !important }' })
  const fire = (payload) =>
    page.evaluate((p) => window[`_${window.__listeners['overlay-show']}`]({ event: 'overlay-show', id: 1, payload: p }), payload)

  await fire({ kind: 'ready', title: "Recording Tom Clancy's Rainbow Six Siege", detail: '', key: 'F8', ms: 3500 })
  await page.waitForTimeout(300)
  assert.equal(await page.locator('#title').textContent(), "Recording Tom Clancy's Rainbow Six Siege")
  assert.equal(await page.locator('#detail').textContent(), 'Press F8 to clip')
  assert.equal(await page.locator('#popup').getAttribute('data-kind'), 'ready', 'the popup retains the event kind')
  await page.waitForFunction(() => document.querySelector('.mark img')?.naturalWidth > 0)
  assert.equal(await page.locator('.mark img').getAttribute('src'), '/brand-mark.svg', 'the popup uses the app mark')
  assert.equal(await page.locator('.rec, .state-dot').count(), 0, 'the popup has no recording LED')
  await page.screenshot({ path: path.join(out, 'overlay-ready.png') })

  // Event-specific text carries the meaning while the app mark stays consistent.
  for (const [kind, title] of [['screenshot', 'Screenshot saved'], ['marked', 'Moment marked']]) {
    await fire({ kind, title, detail: 'Fortnite', key: null, ms: 1500 })
    await page.waitForTimeout(100)
    assert.equal(await page.locator('#popup').getAttribute('data-kind'), kind, `correct event kind on ${kind}`)
    assert.equal(await page.locator('#title').textContent(), title)
    assert.equal(await page.locator('#detail').textContent(), 'Fortnite')
    assert.ok(await page.locator('.mark img').isVisible(), `the app mark remains on ${kind}`)
  }

  await fire({ kind: 'saved', title: 'Clip saved', detail: "Last 30\u2009s · Tom Clancy's Rainbow Six Siege", key: null, ms: 900 })
  await page.waitForTimeout(500)
  assert.equal(await page.locator('#title').textContent(), 'Clip saved')
  assert.equal(await page.locator('#popup').getAttribute('data-kind'), 'saved', 'the popup reports a saved clip')
  assert.ok(await page.locator('.mark img').isVisible(), 'the app mark remains on saved clips')
  assert.ok(!(await page.locator('#detail').textContent()).includes('Press'), 'no key hint on saved')
  await page.screenshot({ path: path.join(out, 'overlay-saved.png') })
  // Fades out on its own before the backend hides the window.
  await page.waitForTimeout(700)
  assert.equal(await page.locator('#popup').evaluate((e) => e.classList.contains('in')), false, 'fades out')

  // A bigger screen gets a bigger window; the card keeps its share of it.
  const small = await page.locator('#popup').boundingBox()
  await page.setViewportSize({ width: 960, height: 260 })
  await fire({ kind: 'saved', title: 'Clip saved', detail: "Tom Clancy's Rainbow Six Siege", key: null, ms: 3000 })
  await page.waitForTimeout(500)
  const big = await page.locator('#popup').boundingBox()
  assert.ok(Math.abs(big.height / small.height - 2) < 0.05, `card scales with the window (${small.height} → ${big.height})`)
  await page.screenshot({ path: path.join(out, 'overlay-saved-4k.png') })
  await page.close()
}

// The check at start-up can finish before the page listens for its event:
// the page asks for what it found, and offers it as if the event came.
async function startupUpdateScenario(browser) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, colorScheme: 'dark' })
  page.on('pageerror', (e) => { throw new Error(`page error in startup update: ${e.message}`) })
  const data = fixtures('full')
  data.found_update = { version: '1.2.0', current_version: '1.1.0', notes: 'Smoother clips.\n\n## Everyone\n\n- **No more freezes:** clips keep to 60 frames a second.\n- Updates wait for your game to close.', prompt: true }
  await page.addInitScript(mockBridge, data)
  await page.goto(`http://localhost:${PORT}/`)
  await page.getByRole('dialog', { name: 'Reruns 1.2.0' }).waitFor()
  const calls = await page.evaluate(() => window.__calls)
  assert.ok(
    calls.indexOf('listen update-available') < calls.indexOf('found_update'),
    'asks for the found update only once listening for it',
  )
  // The release's notes, with their bold and bullets, not as raw Markdown.
  const dialog = page.getByRole('dialog', { name: 'Reruns 1.2.0' })
  await dialog.getByText('No more freezes:').waitFor()
  assert.ok(!(await dialog.innerText()).includes('**'), 'notes are shown formatted')
  await page.screenshot({ path: path.join(out, 'update-prompt.png') })
  await page.getByRole('button', { name: 'Later' }).click()
  const icon = page.locator('aside').getByRole('button', { name: 'Update to 1.2.0' })
  assert.equal(await icon.count(), 1, 'top bar shows the update')
  // Skipping a version stops the prompt at the next start, not the icon.
  await icon.click()
  await page.getByRole('button', { name: 'Skip this version' }).click()
  await page.reload()
  await icon.waitFor()
  await page.waitForTimeout(300)
  assert.equal(await page.getByRole('dialog').count(), 0, 'a skipped version is not prompted again')
  await page.close()
}

// Things that used to go wrong quietly: keys reaching the page behind the
// update prompt, a toast taking the place of a pending Undo, and a setting
// that failed to save without a word.
async function uxScenario(browser) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, colorScheme: 'dark' })
  page.on('pageerror', (e) => { throw new Error(`page error in ux: ${e.message}`) })
  await page.addInitScript(mockBridge, fixtures('full'))
  // Nothing is on disk here.
  await page.route('http://127.0.0.1:2/**', (r) => r.fulfill({ status: 404 }))
  await page.goto(`http://localhost:${PORT}/`)
  const libraryNav = page.getByRole('navigation', { name: 'Library' })
  const cards = () => page.locator('.clip-grid > *').count()
  await page.locator('.clip-grid > *').first().waitFor()
  const before = await cards()

  // Two deletes, then a plain toast: Undo stays and brings both back.
  for (let i = 0; i < 2; i++) {
    const card = page.locator('.clip-grid > .card').first()
    await card.hover()
    await cardAction(page, card, 'Delete')
    await page.waitForTimeout(150)
  }
  assert.equal(await cards(), before - 2, 'two clips deleted')
  const card = page.locator('.clip-grid > .card').first()
  await card.hover()
  await card.getByRole('button', { name: 'Copy clip', exact: true }).click()
  await page.waitForTimeout(150)
  assert.equal(await page.getByRole('button', { name: 'Undo' }).count(), 1, 'a new toast leaves Undo up')
  await page.getByRole('button', { name: 'Undo' }).click()
  await page.waitForTimeout(200)
  assert.equal(await cards(), before, 'Undo brings back every pending delete')
  await page.getByText('Clip copied').waitFor()

  // With the update prompt open, keys stay in it.
  await page.getByRole('button', { name: /Open .* clip/ }).first().click()
  await page.locator('video').waitFor()
  await page.evaluate(
    (u) => window[`_${window.__listeners['update-available']}`]({ event: 'update-available', id: 1, payload: u }),
    { version: '1.2.0', current_version: '1.1.0', notes: null, prompt: true },
  )
  const dialog = page.getByRole('dialog', { name: 'Reruns 1.2.0' })
  await dialog.waitFor()
  assert.ok(await dialog.evaluate((d) => d.contains(document.activeElement)), 'focus moves into the prompt')
  await page.keyboard.press('Escape')
  await page.waitForTimeout(150)
  assert.equal(await dialog.count(), 0, 'Escape closes the prompt')
  assert.equal(await page.locator('video').count(), 1, 'and leaves the viewer open behind it')
  await page.keyboard.press('Escape')
  await page.waitForTimeout(150)
  assert.equal(await page.locator('video').count(), 0, 'a second Escape closes the viewer')

  // Settings opens over the library, which keeps its search and scroll.
  await page.getByLabel('Search clips').fill('clip')
  await page.locator('.library-scroll').evaluate((el) => el.scrollTo(0, 400))
  const scrolled = await page.locator('.library-scroll').evaluate((el) => el.scrollTop)
  assert.ok(scrolled > 0, 'the library scrolled')
  // A setting that fails to save says so while the settings page remains open.
  await page.getByRole('button', { name: 'Settings', exact: true }).click()
  await page.getByRole('button', { name: 'General' }).click()
  await page.evaluate(() => (window.__settingsError = "Can't write settings.json"))
  await page.getByRole('switch', { name: 'Launch at login' }).click()
  await page.getByRole('region', { name: 'Notification', exact: true }).getByText("Can't write settings.json").waitFor({ timeout: 2000 })
  await page.evaluate(() => (window.__settingsError = null))
  await page.getByRole('button', { name: 'Back to clips' }).click()
  assert.equal(await page.getByLabel('Search clips').inputValue(), 'clip', 'Back to clips keeps the search')
  assert.equal(await page.locator('.library-scroll').evaluate((el) => el.scrollTop), scrolled, 'and the scroll')
  await page.getByLabel('Search clips').fill('')

  // "Not a game" can be undone from its notice.
  await selectGame(page, 'Hades II')
  await choose(page, 'Library actions', 'Hades II is not a game')
  await page.getByRole('button', { name: 'Undo' }).click()
  await gameChoice(page, 'Hades II').waitFor({ timeout: 2000 })

  // Everything fits the narrowest window, and / goes to search.
  await page.setViewportSize({ width: 900, height: 600 })
  await page.locator('.clip-grid > *').first().waitFor()
  await page.waitForTimeout(300)
  assert.equal(await page.locator('.library-toolbar').evaluate((h) => h.scrollWidth - h.clientWidth), 0, 'the toolbar fits its controls')
  const navigationBounds = await page.locator('aside').boundingBox()
  const libraryBounds = await page.locator('main').boundingBox()
  assert.ok(navigationBounds.x + navigationBounds.width <= libraryBounds.x, 'the narrow collection navigation stays clear of the clips')
  const overflow = await page.evaluate(() => {
    const m = document.querySelector('.library-scroll')
    return m.scrollWidth - m.clientWidth
  })
  assert.equal(overflow, 0, 'no sideways scroll at the minimum window width')
  await page.screenshot({ path: path.join(out, 'clips-narrow.png') })
  await page.setViewportSize({ width: 1280, height: 800 })
  await page.waitForTimeout(300)
  assert.equal((await page.locator('main').boundingBox()).x + (await page.locator('main').boundingBox()).width, 1280, 'the wider library reaches the window edge')
  await page.keyboard.press('/')
  const search = page.getByLabel('Search clips')
  assert.ok(await search.evaluate((el) => el === document.activeElement), '/ goes to search')
  assert.equal(await search.inputValue(), '', 'and types nothing')
  await page.keyboard.press('Escape')

  // Clip cards are real buttons, so Space opens one as Enter does, and a
  // card still drags out as a file.
  assert.equal(await page.locator('[role="button"] button').count(), 0, 'no buttons inside a button')
  await page.getByRole('button', { name: /Open .* clip/ }).first().scrollIntoViewIfNeeded()
  const box = await page.getByRole('button', { name: /Open .* clip/ }).first().boundingBox()
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
  await page.mouse.down()
  await page.mouse.move(box.x + box.width / 2 + 40, box.y + box.height / 2 + 40, { steps: 5 })
  await page.mouse.up()
  await page.waitForFunction(() => window.__calls.some((c) => c.startsWith('plugin:drag|')), null, { timeout: 2000 })
  // Without the plugin's OS drag, Chromium ends the drag in a click.
  await page.waitForTimeout(200)
  if (await page.locator('video').count()) await page.keyboard.press('Escape')
  await page.getByRole('button', { name: /Open .* clip/ }).first().focus()
  await page.keyboard.press('Space')
  await page.locator('video').waitFor({ timeout: 1000 })

  // Tags are off until Settings → Appearance turns them on.
  await page.getByRole('button', { name: 'Clip options' }).click()
  await page.getByRole('menuitem', { name: 'Rename' }).waitFor()
  assert.equal(await page.getByRole('menuitem', { name: 'Add a tag' }).count(), 0, 'no tag rows by default')
  await page.keyboard.press('Escape')
  assert.equal(await page.locator('video').count(), 1, 'Esc closed the menu, not the viewer')

  // A clip saved from the viewer isn't a replay saved from the game: no
  // flash on Save replay, and its notice opens the new clip.
  await page.evaluate(() => {
    window.__flashed = false
    new MutationObserver(() => document.querySelector('.shutter') && (window.__flashed = true)).observe(document.body, {
      childList: true,
      subtree: true,
    })
  })
  await startTrimming(page)
  await page.getByRole('slider', { name: 'Trim end' }).focus()
  await page.keyboard.press('ArrowLeft')
  await page.getByRole('button', { name: /^Save (?:trimmed|edited) copy$/ }).click()
  await page.locator('.edit-status').getByText('Clip saved').waitFor({ timeout: 1000 })
  assert.equal(await page.evaluate(() => window.__flashed), false, 'no shutter flash for a clip saved from the viewer')
  assert.equal(await page.getByText('Replay saved').count(), 0, 'and it is not called a replay')
  const opens = () => page.evaluate(() => window.__calls.filter((c) => c === 'open_for_edit').length)
  const opened = await opens()
  await page.getByRole('button', { name: 'Open', exact: true }).click()
  await page.waitForFunction((n) => window.__calls.filter((c) => c === 'open_for_edit').length === n + 1, opened)
  await page.keyboard.press('Escape')

  // Opening a newly saved replay from its notice removes that notice.
  // Back therefore needs to restore focus to the corresponding clip.
  const savedPath = '/home/u/Videos/Reruns/ELDEN RING/clip-2.mp4'
  await page.evaluate((path) => window[`_${window.__listeners['clip-saved']}`]({ event: 'clip-saved', id: 1, payload: path }), savedPath)
  const notice = page.getByRole('region', { name: 'Notification', exact: true })
  await notice.getByRole('button', { name: 'Open', exact: true }).click()
  await page.locator('video').waitFor()
  await page.keyboard.press('Escape')
  await page.waitForFunction((path) => document.activeElement?.closest('[data-clip-path]')?.getAttribute('data-clip-path') === path, savedPath)
  await page.close()
}

// The worst realistic data in the smallest window the app allows (900×600,
// no system frame, so the window's own strip is the app's to draw):
// nothing may run off an edge, sit under something else, or become
// unreachable. Shots are saved as worst-*.png to be looked at.
async function worstScenario(browser) {
  const data = fixtures('worst')
  data.__decorated = false
  data.get_status = { ...data.get_status, game: "Tom Clancy's Rainbow Six Siege", encoder_fallback: { preferred: 'nvenc', using: 'amf', reason: 'Failed to open session', set_aside: false } }
  data.get_settings.hotkey = 'Ctrl+Alt+Shift+PageDown'
  data.get_settings.clips_dir = '/home/aleksandra.wisniewska-kowalczyk/Videos/Game Captures/Reruns (new drive)/clips'
  for (const c of data.list_clips) c.path = c.path.replace('/home/u/Videos/Reruns', data.get_settings.clips_dir)
  data.get_settings.ignored_games = [{ name: 'Benachrichtigungseinstellungen-Simulator Launcher', exe: 'launcher' }, { name: 'Steam Client WebHelper' }, { name: 'obs64' }]
  const page = await browser.newPage({ viewport: { width: 900, height: 600 }, colorScheme: 'dark' })
  page.on('pageerror', (e) => { throw new Error(`page error in worst: ${e.message}`) })
  await page.addInitScript(mockBridge, data)
  // Seeking needs the same byte-range responses as the real media server.
  await serve(page, { failFirst: false })
  await page.route('http://127.0.0.1:2/**', (r) => {
    const made = /^\/(thumbs|sprites)\/(\d+)\.svg$/.exec(decodeURIComponent(new URL(r.request().url()).pathname.slice(1)))
    return made && frameCount ? r.fulfill({ status: 200, body: realFrame(made[1] === 'thumbs' ? 'thumb' : 'sprite', Number(made[2])), headers: JPG }) : r.fulfill({ status: 404 })
  })
  await page.goto(`http://localhost:${PORT}/`)
  await page.locator('.clip-grid > *').first().waitFor()
  const shot = (name) => page.screenshot({ path: path.join(out, `worst-${name}.png`) })
  // A long games list scrolls under All clips and Favorites, which stay put.
  const allClips = page.getByRole('navigation', { name: 'Library' }).getByRole('button', { name: 'All clips', exact: true })
  const pinned = await allClips.boundingBox()
  const games = page.locator('.games-scroll')
  assert.ok(await games.evaluate((el) => el.scrollHeight > el.clientHeight), 'the games list overflows at 900x600')
  await games.evaluate((el) => el.scrollTo(0, el.scrollHeight))
  assert.deepEqual(await allClips.boundingBox(), pinned, 'All clips stays in place while the games scroll')
  await games.evaluate((el) => el.scrollTo(0, 0))
  // Everything that can be pressed or read in the bars is inside the
  // window, and no two of a bar's controls overlap.
  const fits = async (where) => {
    const bad = await page.evaluate(() => {
      const out = []
      if (document.documentElement.scrollWidth > innerWidth) out.push('the page scrolls sideways')
      for (const bar of document.querySelectorAll('header, footer, .library-toolbar, [data-deck]')) {
        if (bar.closest('[inert]') || !bar.offsetParent) continue
        // Not what is scrolled out of sight inside a strip of its own (a clip's tags).
        const clipped = (e) => {
          const strip = e.closest('.tags')
          return !!strip && (e.getBoundingClientRect().right > strip.getBoundingClientRect().right || e.getBoundingClientRect().left < strip.getBoundingClientRect().left)
        }
        const els = [...bar.querySelectorAll('button, select, input, [role="slider"], kbd')].filter((e) => e.getClientRects().length && !e.parentElement.closest('button, label') && !clipped(e))
        const b = bar.getBoundingClientRect()
        // Nothing spills out of its own part of the bar (what is cut with an ellipsis or scrolls is meant to).
        for (const part of bar.children) {
          const s = getComputedStyle(part)
          if (s.textOverflow !== 'ellipsis' && s.overflowX !== 'auto' && [...part.children].some((c) => c.getBoundingClientRect().right > part.getBoundingClientRect().right + 1 && getComputedStyle(part).overflowX === 'visible')) out.push(`${bar.tagName} part "${part.innerText.slice(0, 30)}" spills over its neighbours`)
        }
        const rects = els.map((e) => [e, e.getBoundingClientRect()])
        for (const [e, r] of rects) {
          if (r.right > innerWidth + 0.5 || r.left < -0.5 || r.bottom > b.bottom + 0.5 || r.top < b.top - 0.5) out.push(`${bar.tagName} "${(e.getAttribute('aria-label') || e.innerText || e.tagName).slice(0, 30)}" is outside its bar`)
        }
        for (let i = 0; i < rects.length; i++) for (let j = i + 1; j < rects.length; j++) {
          const [a, ra] = rects[i], [c, rc] = rects[j]
          if (a.contains(c) || c.contains(a)) continue
          // Trim and slow-motion handles deliberately sit on the seek track.
          // Markers have a separate lane and are still checked for overlap.
          const timelineHandle = (track, handle) => track.getAttribute('aria-label') === 'Seek' && handle.matches('.handle, .slow-marker') && track.closest('.strip') === handle.closest('.strip')
          if (timelineHandle(a, c) || timelineHandle(c, a)) continue
          if (ra.left < rc.right - 1 && rc.left < ra.right - 1 && ra.top < rc.bottom - 1 && rc.top < ra.bottom - 1) out.push(`${bar.tagName} "${(a.getAttribute('aria-label') || a.innerText).slice(0, 20)}" overlaps "${(c.getAttribute('aria-label') || c.innerText).slice(0, 20)}"`)
        }
      }
      return out
    })
    assert.deepEqual(bad, [], `${where}: ${bad.join('; ')}`)
  }
  const libraryNav = page.getByRole('navigation', { name: 'Library' })

  await shot('library')
  await fits('library at 900')
  assert.ok(!/[\uD800-\uDBFF](?![\uDC00-\uDFFF])/.test(await libraryNav.evaluate((el) => el.textContent)), 'an emoji-first name keeps its whole first character')
  assert.ok((await page.locator('main').innerText()).includes('<b>&amp; **bold**</b>'), 'markup in a title is shown as typed')
  // The longest name, shown as people write it; its page's header still fits.
  await selectGame(page, /^Rainbow Six Siege/)
  await page.waitForTimeout(200)
  await shot('game')
  await fits('long game name at 900')

  // Pick everything, delete it: the strip holds the count, then the notice with its Undo.
  await choose(page, 'Library actions', 'Select clips')
  await page.keyboard.press('Control+a')
  await shot('select')
  await fits('selecting at 900')
  await page.getByRole('button', { name: 'Delete', exact: true }).click()
  await page.getByRole('button', { name: 'Undo' }).waitFor()
  await shot('undo')
  await fits('undo notice at 900')
  await page.getByRole('button', { name: 'Undo' }).click()
  await libraryNav.getByRole('button', { name: /^All clips/ }).click()
  await selectGame(page, 'All games')

  // A long title deleted alone: the notice names it, and Undo stays in reach.
  const long = page.locator('.clip-grid > *', { hasText: 'Aleksandra' })
  await viewOption(page, 'Largest first')
  await viewOption(page, 'Small thumbnails')
  assert.ok(await long.locator('.card-caption').evaluate((el) => el.scrollWidth <= el.clientWidth && el.offsetHeight === 20), 'small cards fit title, game and trim on one line')
  assert.ok(await long.evaluate((el) => el.querySelector('.card-caption').getBoundingClientRect().top >= el.querySelector('.media-tools').getBoundingClientRect().bottom), 'long metadata stays separate from actions over the image')
  assert.ok(await long.locator('.card-caption').evaluate((el) => {
    const title = el.querySelector('.card-title')
    const star = el.querySelector('.favorite-mark').getBoundingClientRect()
    const row = title.parentElement.getBoundingClientRect()
    return title.getBoundingClientRect().right <= star.left && star.right <= row.right + 0.5
  }), 'a long title leaves its favorite visible on the primary line')
  await viewOption(page, 'Newest first')
  await viewOption(page, 'Medium thumbnails')
  await long.hover()
  await cardAction(page, long, 'Delete')
  await page.getByRole('button', { name: 'Undo' }).waitFor()
  await page.waitForTimeout(300)
  await shot('undo-long')
  await fits('long undo notice at 900')
  await page.getByRole('button', { name: 'Undo' }).click()

  // The studio on the clip with the 120-character title and twelve tags.
  await page.locator('.clip-grid > *', { hasText: 'Aleksandra' }).getByRole('button', { name: /^Open/ }).click()
  await page.waitForFunction(() => document.querySelector('video')?.readyState >= 2)
  await page.waitForTimeout(400)
  await shot('studio')
  await fits('studio at 900')
  const pic = await page.locator('.player').boundingBox()
  assert.ok(pic.height >= 150, `the picture keeps room at 900×600 (${Math.round(pic.height)} px)`)
  // Its one menu, with every row, opens inside the window.
  await page.getByRole('button', { name: 'Clip options' }).click()
  await page.waitForTimeout(200)
  await shot('studio-menu')
  const menu = await page.getByRole('menu', { name: 'Clip options' }).boundingBox()
  assert.ok(menu.x >= 0 && menu.y >= 0 && menu.x + menu.width <= 900 && menu.y + menu.height <= 600, 'the menu is inside the window')
  await page.keyboard.press('Escape')
  // Trimmed, with every shaping on: the row still holds the summary and both actions.
  await startTrimming(page)
  await page.getByRole('slider', { name: 'Trim end' }).focus()
  await page.keyboard.press('ArrowLeft')
  await choose(page, 'Clip options', 'Vertical 9:16', 'menuitemcheckbox')
  await choose(page, 'Clip options', 'Slow motion ¼×', 'menuitemradio')
  await page.waitForTimeout(200)
  await shot('studio-shaped')
  await fits('studio with a shaped selection at 900')
  // Closely spaced moments remain independently clickable on the narrow
  // timeline, including when the editing summary takes another row.
  await page.locator('video').evaluate((video) => video.pause())
  for (const [label, time] of [['0:00.00', 0], ['0:00.20', 0.2]]) {
    await page.getByRole('button', { name: `Marked moment at ${label}`, exact: true }).click()
    await page.waitForFunction((time) => Math.abs(document.querySelector('video').currentTime - time) < 0.03, time, { timeout: 3000 }).catch(async () => {
      const state = await page.evaluate(() => ({ actual: document.querySelector('video').currentTime, seeking: document.querySelector('video').seeking, paused: document.querySelector('video').paused, indicated: document.querySelector('[aria-label="Seek"]').getAttribute('aria-valuenow') }))
      assert.fail(`the nearby mark at ${label} did not seek: ${JSON.stringify(state)}`)
    })
  }
  // The clip with no sound, no picture and no length: no audio lane, no levels, no NaN.
  await page.keyboard.press('Escape')
  await page.getByRole('button', { name: /^Open .* clip from / }).first().click()
  await page.waitForFunction(() => document.querySelector('video')?.readyState >= 2)
  await page.waitForTimeout(300)
  await shot('studio-silent')
  await fits('silent clip in the studio')
  assert.equal(await page.locator('[aria-label="Timeline"] .wave').count(), 0, 'no audio lane for a clip without sound')
  assert.ok(!/NaN|undefined|null/.test(await page.getByRole('dialog').innerText()), 'nothing raw in the studio')
  await page.keyboard.press('Escape')

  // Settings: every page, scrolled to its end.
  await page.keyboard.press('2')
  for (const c of ['General', 'Recording', 'Audio', 'Hotkeys', 'Storage', 'Sharing', 'Appearance']) {
    await page.getByRole('navigation', { name: 'Settings pages' }).getByRole('button', { name: c, exact: true }).click()
    await page.waitForTimeout(150)
    await fits(`settings ${c} at 900`)
    const wide = await page.locator('.settings > div').evaluate((el) => el.scrollWidth - el.clientWidth)
    assert.equal(wide, 0, `settings ${c} does not scroll sideways`)
    if (c === 'Recording' || c === 'Storage' || c === 'Hotkeys') await shot(`settings-${c.toLowerCase()}`)
  }
  assert.ok(!/NaN|undefined/.test(await page.locator('body').innerText()), 'nothing raw anywhere')

  // The default window, for the pictures.
  await page.setViewportSize({ width: 1280, height: 800 })
  await page.keyboard.press('1')
  await page.locator('.clip-grid > *').first().waitFor()
  await page.waitForTimeout(300)
  await shot('library-1280')
  await fits('library at 1280')
  // Long game names remain complete in the collection's accessible label and tooltip.
  const longName = gameChoice(page, /^Benachrichtigungseinstellungen-Simulator/)
  await longName.scrollIntoViewIfNeeded()
  assert.ok((await longName.getAttribute('title')).includes('Benachrichtigungseinstellungen-Simulator'), 'a truncated game choice has its full title')
  await longName.click()
  assert.ok((await page.locator('main h1').getAttribute('title')).includes('Benachrichtigungseinstellungen-Simulator'), 'the selected game heading also retains its full title')
  await page.close()
}

// Pictures to look at, not a test: a library whose cards, art and footage
// agree (frames.json), in a frameless window at three sizes. Saved as
// shots-<width>-<what>.png.
function shotFixtures() {
  const map = JSON.parse(fs.readFileSync(path.join(FRAMES, 'frames.json'), 'utf8'))
  const data = fixtures('full')
  const now = Date.now()
  const h = 3600e3
  const games = []
  const clips = []
  let k = 0
  for (const g of map) {
    const entry = game(g.game, g.steam_app_id, g.frames.length, { last: now - (k * 5 + 1) * h, total: (3 + k) * 7 * h })
    if (g.steam_app_id) {
      const art = `/home/u/.local/share/Steam/appcache/librarycache/${g.steam_app_id}/`
      entry.art = { capsule: art + 'library_600x900.jpg', hero: art + 'library_hero.jpg', logo: art + 'logo.png' }
    }
    games.push(entry)
    for (const f of g.frames) {
      clips.push({
        path: `/home/u/Videos/Reruns/${g.game}/clip-${f}.mp4`, game: g.game, name: `clip-${f}`, created_ms: now - (k * 5 + 1) * h,
        size_bytes: 40e6 + f * 7e6, duration: k === 0 ? 12 : 15 + (f % 4) * 9, audio_tracks: 2, is_edit: false,
        steam_app_id: g.steam_app_id, thumbnail: `/thumbs/${f}.svg`, sprite: `/sprites/${f}.svg`, sprite_frames: 10,
        title: g.titles?.[f] ?? null, favorite: k % 5 === 2, labels: [], markers: k === 0 ? [4, 9] : [],
      })
      k++
    }
    // Logged fixture sessions exercise the game's session headings and the
    // daily game summaries, rather than only their no-session fallback.
    const days = new Map()
    for (const clip of clips.filter((c) => c.game === g.game)) {
      const day = new Date(clip.created_ms).toDateString()
      const saved = days.get(day) || []
      saved.push(clip.created_ms)
      days.set(day, saved)
    }
    entry.sessions = [...days.values()].map((times) => ({
      game: g.game, steam_app_id: g.steam_app_id,
      start_ms: Math.min(...times) - 20 * 60e3,
      end_ms: Math.max(...times) + 10 * 60e3,
    }))
  }
  games[0].playing = true
  return {
    ...data,
    __decorated: false,
    list_clips: clips,
    list_games: games,
    screenshots: [],
    get_status: { ...data.get_status, game: map[0].game, steam_app_id: map[0].steam_app_id, backend: 'windows' },
    get_storage: { clips_bytes: clips.reduce((n, c) => n + c.size_bytes, 0), clip_count: clips.length, disk_free: 412e9, disk_total: 1000e9, limit_bytes: 25e9 },
  }
}

async function shotsScenario(browser) {
  if (!frameCount || !fs.existsSync(path.join(FRAMES, 'frames.json'))) throw new Error('shots needs RERUNS_SHOT_FRAMES with a frames.json (see the top of this file)')
  // A 16:9 cut of the clip, beside it, if there is one (the clip itself is ultrawide).
  const wide169 = process.env.RERUNS_SHOT_CLIP && path.join(path.dirname(process.env.RERUNS_SHOT_CLIP), 'clip-16x9.webm')
  // The two usual windows, and the smallest the app allows.
  for (const [width, height] of [[1440, 900], [1280, 800], [900, 600]]) {
    if (wide169 && fs.existsSync(wide169)) {
      const page = await browser.newPage({ viewport: { width, height }, colorScheme: 'dark' })
      await page.addInitScript(mockBridge, shotFixtures())
      await serve(page, { failFirst: false, clip: wide169 })
      await page.goto(`http://localhost:${PORT}/`)
      await page.locator('.clip-open').first().click()
      await page.waitForFunction(() => document.querySelector('video')?.readyState >= 2)
      await seekTo(page, 3)
      await page.waitForTimeout(500)
      await page.screenshot({ path: path.join(out, `shots-${width}-viewer-16x9.png`) })
      await page.close()
    }
    const page = await browser.newPage({ viewport: { width, height }, colorScheme: 'dark' })
    page.on('pageerror', (e) => { throw new Error(`page error in shots: ${e.message}`) })
    await page.addInitScript(mockBridge, shotFixtures())
    await serve(page, { failFirst: false })
    await page.goto(`http://localhost:${PORT}/`)
    await page.locator('.clip-grid > *').first().waitFor()
    // Only what is on screen: lazy pictures below the fold never load.
    await page.waitForFunction(() => [...document.querySelectorAll('.clip-grid img, aside img')].filter((i) => i.getBoundingClientRect().top < innerHeight).every((i) => i.complete))
    await page.waitForTimeout(400)
    const shot = (name) => page.screenshot({ path: path.join(out, `shots-${width}-${name}.png`) })
    const dialog = page.getByRole('dialog')
    await shot('library')
    await page.locator('.media-card').first().hover()
    await page.waitForTimeout(300)
    await shot('library-hover')
    await page.getByLabel('Search clips').hover()
    await page.waitForTimeout(200)

    if (width === 1440) {
      // Navigation under the pointer, then under the keyboard.
      const nav = page.getByRole('navigation', { name: 'Library' })
      await nav.getByRole('button', { name: 'Favorites' }).hover()
      await page.waitForTimeout(200)
      await shot('library-nav-hover')
      await page.mouse.move(800, 600)
      await nav.getByRole('button', { name: 'All clips' }).focus()
      await page.keyboard.press('Tab')
      await page.waitForTimeout(200)
      await shot('library-nav-focus')
      // The library's other states: its menu, a card under the keyboard, nothing found.
      await page.getByRole('button', { name: 'View options' }).click()
      await page.waitForTimeout(200)
      await shot('library-menu')
      await page.keyboard.press('Escape')
      await page.locator('.clip-open').nth(1).focus()
      await page.keyboard.press('ArrowRight')
      await page.waitForTimeout(200)
      await shot('library-focus')
      await page.getByLabel('Search clips').fill('valorant')
      await page.getByText('No clips match').waitFor()
      await shot('library-no-results')
      await page.getByLabel('Search clips').fill('')
      await gameChoice(page, 'Deadlock').hover()
      await page.waitForTimeout(200)
      await shot('library-collection-hover')
    }

    // Game identity and search share the compact header. Exercise artwork
    // and a sparse collection without obscuring either the title or controls.
    const library = page.getByRole('navigation', { name: 'Library' })
    for (const name of ['Rainbow Six Siege', 'Deadlock', 'Dead by Daylight']) {
      await selectGame(page, name)
      await page.mouse.move(width - 20, height - 60)
      await page.waitForTimeout(350)
      assert.equal(await page.locator('main h1').innerText(), name, 'the scoped game remains clearly named')
      assert.equal(await page.locator('.library-toolbar').evaluate((el) => el.scrollWidth - el.clientWidth), 0, 'the compact game header fits its controls')
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'game page fits the viewport')
      await shot(`game-${name.toLowerCase().replaceAll(' ', '-')}`)
    }
    await library.getByRole('button', { name: 'All clips', exact: true }).click()
    await selectGame(page, 'All games')

    // The viewer, on the clip the video file really is.
    await page.locator('.clip-open').first().click()
    await page.waitForFunction(() => document.querySelector('video')?.readyState >= 2)
    await seekTo(page, 3)
    await page.waitForTimeout(500)
    await shot('viewer')
    if (width === 1440) {
      // A trim handle under the pointer, then held and dragged.
      await startTrimming(page)
      const handle = await page.getByRole('slider', { name: 'Trim start' }).boundingBox()
      await page.mouse.move(handle.x + handle.width / 2, handle.y + handle.height / 2)
      await page.waitForTimeout(250)
      await shot('viewer-handle-hover')
      await page.mouse.down()
      await page.mouse.move(handle.x + 260, handle.y + handle.height / 2, { steps: 6 })
      await page.waitForTimeout(250)
      await shot('viewer-handle-drag')
      await page.mouse.up()
      await page.mouse.move(700, 120)
      await page.getByRole('button', { name: 'Reset' }).click()
      await page.getByRole('button', { name: 'Clip options' }).click()
      await page.waitForTimeout(200)
      await shot('viewer-menu')
      await page.keyboard.press('Escape')
      await seekTo(page, 2)
      await page.keyboard.press('i')
      await seekTo(page, 9)
      await page.keyboard.press('o')
      await seekTo(page, 5)
      await page.getByRole('slider', { name: 'Trim end' }).focus()
      await page.waitForTimeout(300)
      await shot('viewer-trim')
      // A block body: returning the gate would make evaluate wait on it.
      await page.evaluate(() => {
        window.__renderGate = new Promise((go) => (window.__renderGo = go))
      })
      await dialog.getByRole('button', { name: /^Save (?:trimmed|edited) copy$/ }).click()
      await dialog.locator('.edit-status').getByText(/Saving clip/).waitFor()
      await page.evaluate(() => window[`_${window.__listeners['render-progress']}`]({ event: 'render-progress', id: 1, payload: { path: document.querySelector('video') ? window.__lastRender.path : '', fraction: 0.42 } }))
      await page.waitForTimeout(300)
      await shot('viewer-saving')
      await page.evaluate(() => {
        window.__renderError = 'the encoder stopped'
        window.__renderGo()
        window.__renderGate = null
      })
      await dialog.locator('.edit-status').getByText(/Couldn't save the clip/).waitFor()
      await page.waitForTimeout(300)
      await shot('viewer-failed')
      await page.evaluate(() => (window.__renderError = null))
      await dialog.getByRole('button', { name: 'Try again' }).click()
      await dialog.locator('.edit-status').getByText('Clip saved').waitFor()
      await page.waitForTimeout(300)
      await shot('viewer-saved')
    }
    await page.keyboard.press('Escape')

    await page.getByRole('button', { name: 'Settings', exact: true }).click()
    await page.waitForTimeout(300)
    await shot('settings')
    if (width === 1440) {
      for (const c of ['Recording', 'Audio', 'Storage', 'Sharing']) {
        await page.getByRole('navigation', { name: 'Settings pages' }).getByRole('button', { name: c, exact: true }).click()
        await page.waitForTimeout(250)
        await shot(`settings-${c.toLowerCase()}`)
      }
    }
    await page.close()
  }
}

async function waitForServer() {
  for (let i = 0; i < 100; i++) {
    try {
      const r = await fetch(`http://localhost:${PORT}/`)
      if (r.ok) return
    } catch {}
    await new Promise((r) => setTimeout(r, 200))
  }
  throw new Error('dev server did not start')
}

module.exports = { fixtures, mockBridge }

if (require.main === module) (async () => {
  const wanted = process.argv[2] ? [process.argv[2]] : ['full', 'empty', 'big']
  let server = null
  let ownServer = false
  try {
    await fetch(`http://localhost:${PORT}/`)
  } catch {
    server = spawn('pnpm', ['dev'], { cwd: path.join(__dirname, '..'), stdio: 'ignore' })
    ownServer = true
  }
  // A preinstalled Chromium can be pointed at with CHROMIUM_PATH when the
  // pinned Playwright version hasn't downloaded its own.
  const executablePath = process.env.CHROMIUM_PATH || (fs.existsSync('/opt/pw-browsers/chromium') ? '/opt/pw-browsers/chromium' : undefined)
  const browser = await chromium.launch({ executablePath })
  try {
    await waitForServer()
    if (process.argv[2] === 'shots') {
      await shotsScenario(browser)
      console.log('ok  shots')
      return
    }
    for (const s of wanted.filter((s) => s !== 'worst')) {
      await runScenario(s, browser)
      console.log(`ok  ${s}`)
    }
    if (!process.argv[2] || process.argv[2] === 'worst') {
      await worstScenario(browser)
      console.log('ok  worst')
    }
    if (!process.argv[2]) {
      await overlayScenario(browser)
      console.log('ok  overlay')
      await startupUpdateScenario(browser)
      console.log('ok  startup update')
      await uxScenario(browser)
      console.log('ok  ux')
    }
  } finally {
    await browser.close()
    if (ownServer && server) server.kill()
  }
})().catch((e) => {
  console.error(e)
  process.exit(1)
})
