// UI smoke test: runs the real frontend in Chromium with the Tauri bridge
// mocked, drives every view, asserts a few behaviours and saves screenshots
// to tests/shots/. No Tauri, no GStreamer, no GPU needed.
//
//   pnpm test:ui            # all scenarios
//   pnpm test:ui full       # one of: full, empty, big
//
// Needs `playwright` (a devDependency) and a Chromium it can launch
// (`npx playwright install chromium` once, unless one is preinstalled).

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
  const art = '/home/u/.local/share/Steam/appcache/librarycache/1245620/'
  games[0].art = { capsule: art + 'library_600x900.jpg', hero: art + 'library_hero.jpg', logo: art + 'logo.png' }
  const clips = []
  let i = 0
  for (const g of games) {
    for (let k = 0; k < g.clip_count * mult; k++) {
      const t = now - (i * 7 + 1) * h
      clips.push({
        path: `/home/u/Videos/Reruns/${g.name}/clip-${i}.mp4`, game: g.name, name: `clip-${i}`, created_ms: t,
        size_bytes: 60e6 + i * 9e6, duration: 20 + (i % 5) * 11, audio_tracks: 2, is_edit: i % 6 === 0,
        steam_app_id: g.steam_app_id, thumbnail: null, sprite: null, sprite_frames: 10,
        title: i === 1 ? 'Malenia no-hit (finally)' : null, favorite: i % 4 === 1,
        labels: i % 3 === 1 ? ['clutch', 'boss'] : i === 4 ? ['funny'] : [],
        markers: i === 1 ? [1.5, 3.5] : [],
        // Game and mic mixed into a first track, then each on its own.
        ...(i === 1 ? { audio_tracks: 3, mixed_audio: true } : {}),
      })
      i++
    }
  }
  const empty = scenario === 'empty'
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
    get_storage: { clips_bytes: 3.7e9, clip_count: clips.length, disk_free: 120e9, disk_total: 500e9, limit_bytes: 25e9 },
    get_capabilities: { display_hz: 144, encoders: [{ encoder: 'nvenc', codecs: ['h264', 'hevc', 'av1'] }, { encoder: 'x264', codecs: ['h264'] }], mic_devices: [{ id: 'mic1', name: 'Blue Yeti' }], output_devices: [{ id: 'out-speakers', name: 'Speakers (Realtek Audio)' }, { id: 'out-headset', name: 'Headset Earphone (Arctis Nova 7)' }] },
    media_base: 'http://127.0.0.1:1/x/',
    hotkey_backend: 'kglobalaccel',
    update_support: { configured: true, version: '0.1.0' },
    check_update: { version: '0.2.0', current_version: '0.1.0', notes: null },
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
        })
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
      if (cmd === 'render_clip') { window.__lastRender = args; return args.path }
      if (cmd === 'copy_clip') { window.__copied = args.path; return null }
      if (cmd === 'set_settings') { data.get_settings = args.settings; return null }
      if (cmd === 'list_library') {
        const ignored = data.get_settings.ignored_games.map((i) => i.name.toLowerCase())
        return { clips: data.list_clips, games: data.list_games.filter((g) => !ignored.includes(g.name.toLowerCase())) }
      }
      if (cmd === 'delete_clip') { data.list_clips = data.list_clips.filter((c) => c.path !== args.path); return null }
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

async function runScenario(scenario, browser) {
  const data = fixtures(scenario)
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 }, colorScheme: 'dark' })
  page.on('pageerror', (e) => { throw new Error(`page error in ${scenario}: ${e.message}`) })
  await page.addInitScript(mockBridge, data)
  // Every clip plays the same small ultrawide test video (VP9: the test
  // Chromium has no H.264), so the viewer is shot with a real picture.
  const video = fs.readFileSync(path.join(__dirname, 'fixtures', 'clip.webm'))
  await page.route('http://127.0.0.1:1/**', (r) => {
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
        ...(m ? { 'Content-Range': `bytes ${from}-${to}/${video.length}` } : {}),
      },
    })
  })
  // Files on disk: Steam art loads on its second request, the rest is missing.
  const PIXEL = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=', 'base64')
  const hits = {}
  await page.route('http://127.0.0.1:2/**', (r) => {
    const file = decodeURIComponent(new URL(r.request().url()).pathname.slice(1))
    hits[file] = (hits[file] ?? 0) + 1
    if (!file.includes('/librarycache/') || hits[file] === 1) return r.fulfill({ status: 404 })
    return r.fulfill({ status: 200, body: PIXEL, headers: { 'Content-Type': 'image/png' } })
  })
  await page.goto(`http://localhost:${PORT}/`)
  await page.waitForTimeout(800)
  const shot = (name) => page.screenshot({ path: path.join(out, `${scenario}-${name}.png`) })
  const cards = () => page.locator('.clip-grid > *').count()

  await shot('games')
  if (scenario === 'full') {
    // Art that fails to load shows the title, and loads on the next listing
    // (here: coming back to the window).
    const logo = page.locator('img[alt="ELDEN RING"]')
    await page.locator('h1', { hasText: 'ELDEN RING' }).waitFor()
    assert.equal(await logo.count(), 0, 'no broken logo')
    await page.evaluate(() => window.dispatchEvent(new Event('focus')))
    await page.waitForFunction(() => {
      const img = document.querySelector('img[alt="ELDEN RING"]')
      return img?.complete && img.naturalWidth > 0
    })
    assert.equal(await page.locator('h1', { hasText: 'ELDEN RING' }).count(), 0, 'logo replaces the title')
    assert.match(await logo.getAttribute('src'), /\?retry=1$/)

    // "Not a game" from a Games page card: gone at once, and stays gone.
    const card = page.getByRole('button', { name: 'Hades II is not a game' })
    await page.getByText('Hades II').first().hover()
    await card.click()
    await page.getByText('Ignoring Hades II').waitFor()
    await page.waitForTimeout(200)
    assert.equal(await page.getByRole('button', { name: 'Hades II is not a game' }).count(), 0, 'card removed')
    assert.equal(await page.getByRole('button', { name: 'Counter-Strike 2 is not a game' }).count(), 1, 'others stay')
  }
  // The window controls show only where the system frame is off.
  assert.equal(await page.getByRole('button', { name: 'Close', exact: true }).count(), scenario === 'full' ? 1 : 0)
  if (scenario === 'empty') {
    await page.getByRole('button', { name: 'Choose screen' }).waitFor()
    await page.close()
    return
  }

  await page.getByRole('button', { name: 'All clips' }).click()
  await page.waitForTimeout(300)

  if (scenario === 'big') {
    await page.mouse.move(700, 400)
    for (let i = 0; i < 30; i++) { await page.mouse.wheel(0, 600); await page.waitForTimeout(16) }
    await page.waitForTimeout(300)
    await shot('scrolled')
    const n = await cards()
    const height = await page.evaluate(() => document.querySelector('.clip-grid').scrollHeight)
    assert.ok(n < 60, `virtual grid should window ${data.list_clips.length} clips, rendered ${n}`)
    assert.ok(height < 200_000, `grid height ${height}px is runaway`)
    await page.close()
    return
  }

  await shot('clips')
  const before = await cards()

  // Selection, bulk delete, undo.
  await page.getByRole('button', { name: 'Select', exact: true }).click()
  await page.getByRole('button', { name: /^Select .* clip/ }).nth(1).click()
  await page.getByRole('button', { name: /^Select .* clip/ }).nth(3).click()
  await shot('clips-select')
  await page.getByRole('button', { name: 'Delete', exact: true }).click()
  await page.waitForTimeout(200)
  assert.equal(await cards(), before - 2, 'deleted clips vanish at once')
  await shot('clips-undo')
  await page.getByRole('button', { name: 'Undo' }).click()
  await page.waitForTimeout(200)
  assert.equal(await cards(), before, 'undo brings the clips back')

  // Favourites filter.
  await page.getByRole('button', { name: 'Only favourites' }).click()
  await page.waitForTimeout(200)
  assert.ok((await cards()) < before, 'favourites filter narrows the grid')
  await shot('clips-favourites')
  await page.getByRole('button', { name: 'Only favourites' }).click()

  // Viewer: rename, star, label, shortcuts, trim.
  await page.getByRole('button', { name: /Open ELDEN RING clip/ }).nth(1).click()
  await page.waitForFunction(() => document.querySelector('video')?.readyState >= 2)
  await page.evaluate(
    () =>
      new Promise((done) => {
        const v = document.querySelector('video')
        v.addEventListener('seeked', done, { once: true })
        v.currentTime = 2.5
      }),
  )
  await page.waitForTimeout(300)
  await shot('viewer')
  // While playing, the playhead moves every frame, not at timeupdate's ~4 Hz.
  const positions = await page.evaluate(
    () =>
      new Promise((done) => {
        const v = document.querySelector('video')
        const head = document.querySelector('.player .head')
        const seen = new Set()
        v.muted = true
        v.play().then(() => {
          const until = performance.now() + 500
          requestAnimationFrame(function frame(now) {
            seen.add(head.style.transform)
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
  // Ctrl+C copies the clip itself, ready to paste into Discord.
  await page.keyboard.press('Control+c')
  await page.getByText('Clip copied').waitFor()
  assert.match(await page.evaluate(() => window.__copied), /\.mp4$/, 'the clip file went to copy_clip')
  // Marked moments: ticks on the line, [ and ] jump between them.
  assert.equal(await page.locator('.player .tick').count(), 2, 'two marked moments on the line')
  const nearTime = (t) =>
    page.waitForFunction((t) => Math.abs(document.querySelector('video').currentTime - t) < 0.05, t, { timeout: 3000 })
  await page.keyboard.press(']')
  await nearTime(3.5)
  await page.keyboard.press('[')
  await nearTime(1.5)
  await page.keyboard.press('[')
  await page.waitForTimeout(150)
  await nearTime(1.5)
  await page.keyboard.press('n')
  await page.keyboard.type('Malenia no-hit, take 2')
  await page.keyboard.press('Enter')
  await page.getByRole('button', { name: 'Label', exact: true }).click()
  await page.keyboard.type('pb')
  await page.keyboard.press('Enter')
  await page.keyboard.press('Escape')
  await page.waitForTimeout(200)
  await page.keyboard.press('s')
  await page.waitForTimeout(200)
  await shot('viewer-meta')
  assert.equal(await page.locator('.chip', { hasText: 'pb' }).count(), 1, 'label added')
  await page.keyboard.press('?')
  await page.waitForTimeout(150)
  await shot('viewer-help')
  await page.keyboard.press('Escape')
  await page.keyboard.press('t')
  const seekTo = (t) =>
    page.evaluate(
      (t) =>
        new Promise((done) => {
          const v = document.querySelector('video')
          v.addEventListener('seeked', done, { once: true })
          v.currentTime = t
        }),
      t,
    )
  await seekTo(1)
  await page.keyboard.press('i')
  await seekTo(4.5)
  await page.keyboard.press('o')
  // Starring mid-trim (a new clip object from the page) keeps the trim.
  await page.keyboard.press('s')
  await page.waitForTimeout(200)
  await seekTo(2.5)
  await page.waitForTimeout(200)
  await shot('viewer-trim')
  // Trimming happens on the line over the video; no timeline under it.
  assert.equal(await page.locator('.player [aria-label="Trim start"]').count(), 1, 'trim markers on the video')
  assert.equal(await page.locator('[aria-label="Timeline"]').count(), 1, 'one timeline')
  assert.equal(await page.locator('.player [aria-label="Timeline"]').count(), 1, 'and it is over the video')
  assert.equal(await page.locator('[aria-label="Trim start"]').getAttribute('aria-valuenow'), '1', 'in at 1 s')
  assert.equal(await page.locator('[aria-label="Trim end"]').getAttribute('aria-valuenow'), '4.5', 'out at 4.5 s')
  // Export size: fitted under Discord's limit, with a hint of how.
  await page.getByLabel('Export size').selectOption('10')
  await page.getByRole('status').filter({ hasText: '720p30' }).waitFor()
  await shot('viewer-export-size')
  await page.getByRole('button', { name: 'Render clip' }).click()
  await page.waitForFunction(() => window.__lastRender)
  const rendered = await page.evaluate(() => window.__lastRender)
  assert.equal(rendered.maxMb, 10, 'render asks for the 10 MB fit')
  // The clip starts with game and mic mixed: levels for the two sources
  // only (the app leaves the mix out of the edit).
  assert.deepEqual(rendered.volumes, [1, 1], 'a level per source track, none for the mix')
  assert.deepEqual([rendered.start, rendered.end], [1, 4.5], 'the trimmed range')
  assert.equal(rendered.vertical, null, 'original format unless asked')
  assert.equal(rendered.slow, null, 'no slow motion unless asked')
  // Vertical 9:16 for Shorts/TikTok, with a slow-motion range.
  await page.getByLabel('Export format').selectOption('vertical')
  await page.getByRole('slider', { name: 'Vertical frame position' }).waitFor()
  await page.getByLabel('Slow motion').selectOption('2')
  await page.getByRole('slider', { name: 'Slow motion start' }).waitFor()
  // 3.5 s trimmed, the middle third (~1.17 s) at half speed adds ~1.17 s.
  await page.getByText('Length 0:04.67').waitFor()
  await page.waitForTimeout(150)
  await shot('viewer-vertical-slow')
  await page.evaluate(() => (window.__lastRender = null))
  await page.getByRole('button', { name: 'Render clip' }).click()
  await page.waitForFunction(() => window.__lastRender)
  const shaped = await page.evaluate(() => window.__lastRender)
  assert.equal(shaped.vertical, 0.5, 'vertical frame centred by default')
  assert.equal(shaped.slow.factor, 2, 'half speed')
  assert.ok(shaped.slow.from > 1 && shaped.slow.to < 4.5 && shaped.slow.from < shaped.slow.to, 'slow range inside the trim')
  await page.keyboard.press('Escape')
  await page.keyboard.press('Escape')

  // "Not a game" from the top bar, listed under Settings → Recording.
  await page.getByRole('button', { name: 'Not a game' }).click()
  await page.getByText('Ignoring ELDEN RING').waitFor()

  // Settings and appearance.
  await page.getByRole('button', { name: 'Settings 3' }).click()
  await page.getByRole('button', { name: 'Appearance' }).click()
  await page.waitForTimeout(150)
  await shot('settings-appearance')
  await page.getByRole('radio', { name: 'Blue' }).click()
  await page.getByRole('button', { name: 'Recording' }).click()
  await page.waitForTimeout(150)
  await shot('settings-recording-blue')
  assert.equal(await page.getByRole('button', { name: 'Detect ELDEN RING again' }).count(), 1, 'ignored app listed')
  await page.getByText('HDR display').waitFor()
  assert.ok(await page.getByText('SDR white 240 nits').isVisible(), 'HDR row shows the SDR white level')
  // Windows: HDR compatibility, and the experimental window capture under it.
  await page.evaluate(
    (s) => window[`_${window.__listeners['status-changed']}`]({ event: 'status-changed', id: 1, payload: s }),
    { state: 'recording', game: 'ELDEN RING', steam_app_id: 1245620, session_start_ms: Date.now(), buffered_seconds: 30, replay_seconds: 30, encoder: 'nvenc', capture_path: 'zerocopy', error: null, backend: 'windows', needs_screen: false, hdr: { sdr_white_nits: 240, peak_nits: 1000 }, hdr_tonemapped: true },
  )
  await page.getByRole('switch', { name: 'HDR compatibility' }).click()
  await page.getByRole('switch', { name: 'Capture the window in HDR' }).waitFor()
  await page.getByRole('switch', { name: 'Capture the window in HDR' }).click()
  await page.waitForTimeout(250) // let the switch finish moving before the shot
  assert.equal(await page.getByRole('switch', { name: 'Capture the window in HDR' }).getAttribute('aria-checked'), 'true', 'HDR window capture on')
  await shot('settings-recording-windows')
  // Storage: starred clips are kept by the limit unless turned off.
  await page.getByRole('button', { name: 'Storage', exact: true }).click()
  const starred = page.getByRole('switch', { name: 'Protect starred clips' })
  assert.equal(await starred.getAttribute('aria-checked'), 'true', 'starred clips protected by default')
  await starred.click()
  await page.waitForFunction(
    () => document.querySelector('[role="switch"][aria-label="Protect starred clips"]').getAttribute('aria-checked') === 'false',
  )
  await starred.click()
  // Capture failing mid-game: the header says so instead of "waiting".
  await page.evaluate(
    (s) => window[`_${window.__listeners['status-changed']}`]({ event: 'status-changed', id: 1, payload: s }),
    { state: 'retrying', game: 'Overwatch', steam_app_id: null, session_start_ms: Date.now(), buffered_seconds: 0, replay_seconds: 30, encoder: 'nvd3d11h264enc', capture_path: null, error: 'Could not encode stream: Failed to open session', backend: 'windows', needs_screen: false, hdr: null, hdr_tonemapped: false },
  )
  await page.getByText('· retrying').waitFor()
  assert.equal(await page.getByText('Waiting for a game').count(), 0, 'no "waiting" while a game is failing')
  await shot('status-retrying')
  // NVENC refused at the game's start: AMD records it, and the header says so.
  await page.evaluate(
    (s) => window[`_${window.__listeners['status-changed']}`]({ event: 'status-changed', id: 1, payload: s }),
    { state: 'recording', game: 'Fortnite', steam_app_id: null, session_start_ms: Date.now(), buffered_seconds: 30, replay_seconds: 30, encoder: 'amfh264enc', encoder_fallback: { preferred: 'nvenc', using: 'amf', reason: 'Failed to open session', set_aside: false }, capture_path: 'zerocopy', error: null, backend: 'windows', needs_screen: false, hdr: null, hdr_tonemapped: false },
  )
  const fallback = page.getByText('on AMD AMF')
  await fallback.waitFor()
  const why = await fallback.getAttribute('title')
  assert.ok(why.includes('NVIDIA NVENC refused to start (Failed to open session)'), `fallback says why: ${why}`)
  assert.ok(why.includes('tries NVIDIA NVENC again at the next game'), 'and that the next game tries again')
  await shot('status-encoder-fallback')
  // Set aside after failing while recording: no promise of the next game.
  await page.evaluate(
    (s) => window[`_${window.__listeners['status-changed']}`]({ event: 'status-changed', id: 1, payload: s }),
    { state: 'recording', game: 'Fortnite', steam_app_id: null, session_start_ms: Date.now(), buffered_seconds: 30, replay_seconds: 30, encoder: 'amfh264enc', encoder_fallback: { preferred: 'nvenc', using: 'amf', reason: 'Failed to open session', set_aside: true }, capture_path: 'zerocopy', error: null, backend: 'windows', needs_screen: false, hdr: null, hdr_tonemapped: false },
  )
  await page.waitForFunction(() => document.querySelector('[title*="kept failing while recording"]'))
  const aside = await page.getByText('on AMD AMF').getAttribute('title')
  assert.ok(aside.includes('records with AMD AMF until it restarts'), `set aside says when: ${aside}`)
  assert.ok(!aside.includes('next game'), 'no promise of the next game')
  // The usual encoder: no note.
  await page.evaluate(
    (s) => window[`_${window.__listeners['status-changed']}`]({ event: 'status-changed', id: 1, payload: s }),
    { state: 'recording', game: 'Fortnite', steam_app_id: null, session_start_ms: Date.now(), buffered_seconds: 30, replay_seconds: 30, encoder: 'nvd3d11h264enc', encoder_fallback: null, capture_path: 'zerocopy', error: null, backend: 'windows', needs_screen: false, hdr: null, hdr_tonemapped: false },
  )
  await page.waitForFunction(() => !document.body.innerText.includes('on AMD AMF'))
  await page.getByRole('button', { name: 'Audio' }).click()
  await page.waitForTimeout(150)
  assert.equal(await page.getByLabel('Output device').inputValue(), 'out-headset', 'chosen output is selected')
  assert.equal(await page.getByLabel('Output device').locator('option').count(), 3, 'default + two outputs')
  // Live meters: the page started them; push a reading for each track.
  await page.waitForFunction(() => window.__listeners && window.__listeners['audio-level'])
  const level = (l) => page.evaluate((p) => window[`_${window.__listeners['audio-level']}`]({ event: 'audio-level', id: 1, payload: p }), l)
  await level({ track: 'desktop', peak_db: -14, error: null })
  await level({ track: 'mic', peak_db: null, error: 'device not found' })
  await page.getByText('Hearing sound').waitFor()
  await page.getByText("Can't open: device not found").waitFor()
  await page.waitForTimeout(150)
  await shot('settings-audio')
  // Save sounds: a chime for clips and a shutter for screenshots; pick a
  // file for clips and screenshots follow it.
  await page.getByRole('button', { name: 'General' }).click()
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
  // Updates: checked on request; an update shows in Settings and the top bar.
  await page.getByText('Reruns 0.1.0').waitFor()
  await page.getByRole('button', { name: 'Check now' }).click()
  await page.getByText('Version 0.2.0 available.').waitFor()
  assert.equal(await page.getByRole('button', { name: 'Update and restart' }).count(), 1, 'update button')
  assert.equal(await page.locator('header').getByRole('button', { name: 'Update to 0.2.0' }).count(), 1, 'top bar shows the update')
  await page.getByRole('heading', { name: 'Updates' }).scrollIntoViewIfNeeded()
  await shot('settings-updates')
  // Logs: the button asks the backend to open the folder; a failure shows
  // the message, which names the folder.
  await page.getByRole('button', { name: 'Open the log folder' }).click()
  await page.waitForFunction(() => window.__logOpens === 1)
  await page.evaluate(() => (window.__logError = "Can't open /home/u/.local/share/reruns/logs: no file manager"))
  await page.getByRole('button', { name: 'Open the log folder' }).click()
  await page.getByText("Can't open /home/u/.local/share/reruns/logs: no file manager").waitFor()
  assert.equal(await page.evaluate(() => window.__logOpens), 2, 'each click opens the folder')
  await page.evaluate(() => (window.__logError = null))
  await page.locator('main > div').evaluate((el) => el.scrollTo(0, el.scrollHeight))
  await shot('settings-logs')
  // An update found at start-up: the prompt opens once; "Later" leaves a
  // pulsing icon in the header that opens it again.
  await page.evaluate(
    (u) => window[`_${window.__listeners['update-available']}`]({ event: 'update-available', id: 1, payload: u }),
    { version: '1.2.0', current_version: '1.1.0', notes: null, prompt: true },
  )
  await page.getByRole('dialog', { name: 'Reruns 1.2.0' }).waitFor()
  assert.equal(
    await page.evaluate(() => window.__calls.filter((c) => c === 'found_update').length),
    2,
    'a prompt shown from the event is marked as shown',
  )
  await shot('update-prompt')
  await page.getByRole('button', { name: 'Later' }).click()
  assert.equal(await page.getByRole('dialog', { name: 'Reruns 1.2.0' }).count(), 0, 'Later closes the prompt')
  await page.getByRole('button', { name: 'Update to 1.2.0' }).click()
  await page.getByRole('dialog', { name: 'Reruns 1.2.0' }).waitFor()
  await page.keyboard.press('Escape')
  await page.getByRole('button', { name: 'Hotkeys' }).click()
  await page.waitForTimeout(150)
  assert.equal(await page.locator('kbd', { hasText: /^F9$/ }).count(), 1, 'screenshot hotkey row')
  await shot('settings-hotkeys')
  await page.getByRole('button', { name: 'Appearance' }).click()
  await page.getByRole('radio', { name: 'Orange' }).click()

  // Sidebar collapse.
  await page.keyboard.press('1')
  await page.keyboard.press('Control+b')
  await page.waitForTimeout(300)
  await shot('sidebar-collapsed')
  await page.keyboard.press('Control+b')
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
  assert.ok(await page.locator('.rec').isVisible(), 'recording dot on ready')
  await page.screenshot({ path: path.join(out, 'overlay-ready.png') })

  // Each kind has its own glyph.
  for (const [kind, title, glyph] of [['screenshot', 'Screenshot saved', '.camera'], ['marked', 'Moment marked', '.flag']]) {
    await fire({ kind, title, detail: 'Fortnite', key: null, ms: 1500 })
    await page.waitForTimeout(100)
    assert.ok(await page.locator(glyph).isVisible(), `${glyph} on ${kind}`)
    assert.equal(await page.locator('.glyph:visible').count(), 1, `one glyph on ${kind}`)
  }

  await fire({ kind: 'saved', title: 'Clip saved', detail: "Last 30\u2009s · Tom Clancy's Rainbow Six Siege", key: null, ms: 900 })
  await page.waitForTimeout(500)
  assert.equal(await page.locator('#title').textContent(), 'Clip saved')
  assert.ok(await page.locator('.check').isVisible(), 'check mark on saved')
  assert.equal(await page.locator('.rec').isVisible(), false, 'no recording dot on saved')
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
  const icon = page.locator('header').getByRole('button', { name: 'Update to 1.2.0' })
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

;(async () => {
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
    for (const s of wanted) {
      await runScenario(s, browser)
      console.log(`ok  ${s}`)
    }
    if (!process.argv[2]) {
      await overlayScenario(browser)
      console.log('ok  overlay')
      await startupUpdateScenario(browser)
      console.log('ok  startup update')
    }
  } finally {
    await browser.close()
    if (ownServer && server) server.kill()
  }
})().catch((e) => {
  console.error(e)
  process.exit(1)
})
