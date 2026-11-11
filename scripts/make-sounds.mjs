// Makes the built-in sounds: app/src-tauri/sounds/clip-saved.wav,
// screenshot.wav and save-failed.wav. Synthesized here from nothing but arithmetic, so they
// are ours, come out the same on every run, and can be tuned by editing
// the numbers below and running this again:
//
//   node scripts/make-sounds.mjs
//
// Both are short and quiet on purpose: heard under a game, not over it.
// 48 kHz mono 16-bit PCM WAV, the format every player takes.

import { writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const RATE = 48_000
/** Loudest sample of the chime before its echo: -14 dBFS, as the sound
 *  it replaces. */
const CHIME_PEAK_DB = -14
/** Clicks are over in a few milliseconds, so the same peak sounds far
 *  quieter; 6 dB more puts the shutter's average near the chime's. */
const SHUTTER_PEAK_DB = -8

const seconds = (s) => Math.round(s * RATE)

/** Rises from 0 to 1 over `attack` seconds (half a cosine: no click). */
const attack = (t, a) => (t >= a ? 1 : 0.5 - 0.5 * Math.cos((Math.PI * t) / a))

/** Long enough for any sound here to fade to nothing by itself. A sound
 *  is written into this much room and trimmed afterwards ([`wav`]): one
 *  stopped while still ringing clicks, and its echoes repeat the click. */
const ROOM = 1.2

/** One soft note: a sine with a little second and third harmonic, a
 *  gentle attack (12 ms unless given) and an exponential fade with time
 *  constant `decay`. */
function note(out, at, hz, decay, level, rise = 0.012) {
  for (let i = seconds(at); i < out.length; i++) {
    const t = i / RATE - at
    const tone =
      Math.sin(2 * Math.PI * hz * t) +
      0.16 * Math.sin(2 * Math.PI * 2 * hz * t) +
      0.05 * Math.sin(2 * Math.PI * 3 * hz * t)
    out[i] += level * tone * attack(t, rise) * Math.exp(-t / decay)
  }
}

/** The same noise every run. */
function noise(seed) {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return (((t ^ (t >>> 14)) >>> 0) / 2 ** 32) * 2 - 1
  }
}

/** A short burst of noise around `hz` (a band-pass filter of width
 *  `hz / q`), fading with time constant `decay`: a click with a pitch. */
function click(out, at, hz, q, decay, level, seed) {
  const w = (2 * Math.PI * hz) / RATE
  const alpha = Math.sin(w) / (2 * q)
  const [b0, b2, a0, a1, a2] = [alpha, -alpha, 1 + alpha, -2 * Math.cos(w), 1 - alpha]
  const rand = noise(seed)
  let [x1, x2, y1, y2] = [0, 0, 0, 0]
  for (let i = seconds(at); i < out.length; i++) {
    const t = i / RATE - at
    const x = rand()
    const y = (b0 * x + b2 * x2 - a1 * y1 - a2 * y2) / a0
    ;[x2, x1, y2, y1] = [x1, x, y1, y]
    out[i] += level * y * attack(t, 0.001) * Math.exp(-t / decay)
  }
}

/** A small room around a sound: `taps` are [delay in seconds, level]
 *  copies added after it, each a little duller than the sound itself (a
 *  gentle low-pass), as a wall gives it back. It fills the sound out and
 *  lets it ring a moment longer without making its peak any louder.
 *  Returns a longer array: the sound plus its last echo. */
function echo(samples, taps) {
  const longest = Math.max(...taps.map(([delay]) => delay))
  const out = new Float64Array(samples.length + seconds(longest))
  out.set(samples)
  // One-pole low-pass at about 3.5 kHz.
  const k = 1 - Math.exp((-2 * Math.PI * 3500) / RATE)
  const dull = new Float64Array(samples.length)
  let y = 0
  samples.forEach((x, i) => (dull[i] = y += k * (x - y)))
  for (const [delay, level] of taps) {
    const shift = seconds(delay)
    dull.forEach((x, i) => (out[i + shift] += level * x))
  }
  return out
}

/** The echo on the two chimes: three quick reflections, each quieter.
 *  Their delays are whole numbers of periods of every note used (6.81 ms
 *  holds 2 periods of D4, 3 of A4, 4 of D5 and 6 of A5), so each
 *  reflection arrives in step with its note and adds to it; at an
 *  arbitrary delay it can arrive out of step and cancel the note instead
 *  (47 ms did: the chime came out quieter with echo than without). */
const IN_STEP = 2 / 293.665
const CHIME_ECHO = [[7 * IN_STEP, 0.36], [14 * IN_STEP, 0.17], [21 * IN_STEP, 0.07]]
/** And on the shutter: shorter, and between its two clicks (80 ms
 *  apart), so they stay two. */
const SHUTTER_ECHO = [[0.041, 0.45], [0.105, 0.22]]

/** Scale `dry` so its loudest sample is at `peakDb`, add its echo on
 *  top (so the echo makes the sound fuller, not the sound itself
 *  quieter to make room for it), and wrap as a WAV. The file ends once
 *  the sound has faded to a five-hundredth of its peak (-54 dB), with a
 *  40 ms fade to exact silence over what is left. */
function wav(dry, taps, peakDb) {
  const peak = 10 ** (peakDb / 20)
  const loudest = dry.reduce((m, s) => Math.max(m, Math.abs(s)), 0)
  const whole = echo(dry, taps)
  let last = whole.length - 1
  while (last > 0 && Math.abs(whole[last]) < loudest / 500) last--
  const fade = seconds(0.04)
  const samples = whole.slice(0, Math.min(whole.length, last + fade))
  for (let i = 0; i < fade; i++) {
    samples[samples.length - 1 - i] *= 0.5 - 0.5 * Math.cos((Math.PI * i) / fade)
  }
  const data = Buffer.alloc(44 + samples.length * 2)
  data.write('RIFF', 0)
  data.writeUInt32LE(36 + samples.length * 2, 4)
  data.write('WAVEfmt ', 8)
  data.writeUInt32LE(16, 16) // fmt chunk size
  data.writeUInt16LE(1, 20) // PCM
  data.writeUInt16LE(1, 22) // mono
  data.writeUInt32LE(RATE, 24)
  data.writeUInt32LE(RATE * 2, 28) // bytes a second
  data.writeUInt16LE(2, 32) // bytes a sample
  data.writeUInt16LE(16, 34) // bits a sample
  data.write('data', 36)
  data.writeUInt32LE(samples.length * 2, 40)
  samples.forEach((s, i) => data.writeInt16LE(Math.round((s / loudest) * peak * 32767), 44 + i * 2))
  return data
}

/** Clip saved: two soft notes, the second a fifth above the first
 *  (D5 then A5), like a small "got it", with a short echo. Unhurried:
 *  the second note comes 110 ms after the first and rings a while. */
function clipSaved() {
  const out = new Float64Array(seconds(ROOM))
  note(out, 0, 587.33, 0.065, 0.9)
  note(out, 0.11, 880.0, 0.09, 1.0)
  return wav(out, CHIME_ECHO, CHIME_PEAK_DB)
}

/** Screenshot: a camera shutter in miniature. Two bright clicks 80 ms
 *  apart (open, close), the first with a faint low thump under it. */
function screenshot() {
  const out = new Float64Array(seconds(ROOM))
  click(out, 0, 3200, 1.2, 0.009, 1.0, 1)
  note(out, 0, 150, 0.022, 0.35, 0.004)
  click(out, 0.08, 2300, 1.4, 0.014, 0.8, 2)
  return wav(out, SHUTTER_ECHO, SHUTTER_PEAK_DB)
}

/** A save that didn't work: the chime's two notes the other way round
 *  and an octave down (A4 then D4), so it reads as "no" without a buzz. */
function saveFailed() {
  const out = new Float64Array(seconds(ROOM))
  note(out, 0, 440.0, 0.075, 1.0)
  note(out, 0.13, 293.66, 0.1, 0.9)
  return wav(out, CHIME_ECHO, CHIME_PEAK_DB)
}

const dir = join(dirname(fileURLToPath(import.meta.url)), '..', 'app', 'src-tauri', 'sounds')
for (const [name, make] of [['clip-saved.wav', clipSaved], ['screenshot.wav', screenshot], ['save-failed.wav', saveFailed]]) {
  const data = make()
  writeFileSync(join(dir, name), data)
  console.log(`${name}: ${((data.length - 44) / 2 / RATE * 1000).toFixed(0)} ms, ${data.length} bytes`)
}
