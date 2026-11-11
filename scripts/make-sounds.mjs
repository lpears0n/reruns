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
/** Loudest sample of the chime: -14 dBFS, as the sound it replaces
 *  (which averaged -25.7 dB; this one averages -24.9). */
const CHIME_PEAK_DB = -14
/** Clicks are over in a few milliseconds, so the same peak sounds far
 *  quieter; 6 dB more puts the shutter's average near the chime's. */
const SHUTTER_PEAK_DB = -8

const seconds = (s) => Math.round(s * RATE)

/** Rises from 0 to 1 over `attack` seconds (half a cosine: no click). */
const attack = (t, a) => (t >= a ? 1 : 0.5 - 0.5 * Math.cos((Math.PI * t) / a))

/** One soft note: a sine with a little second and third harmonic, a
 *  quick attack and an exponential fade with time constant `decay`. */
function note(out, at, hz, decay, level) {
  for (let i = seconds(at); i < out.length; i++) {
    const t = i / RATE - at
    const tone =
      Math.sin(2 * Math.PI * hz * t) +
      0.16 * Math.sin(2 * Math.PI * 2 * hz * t) +
      0.05 * Math.sin(2 * Math.PI * 3 * hz * t)
    out[i] += level * tone * attack(t, 0.006) * Math.exp(-t / decay)
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

/** Scale so the loudest sample is at `peakDb`, fade the last 15 ms to
 *  silence, and wrap as a WAV. */
function wav(samples, peakDb) {
  const peak = 10 ** (peakDb / 20)
  const fade = seconds(0.015)
  for (let i = 0; i < fade; i++) samples[samples.length - 1 - i] *= i / fade
  const loudest = samples.reduce((m, s) => Math.max(m, Math.abs(s)), 0)
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
 *  (D5 then A5), like a small "got it". About a quarter of a second. */
function clipSaved() {
  const out = new Float64Array(seconds(0.27))
  note(out, 0, 587.33, 0.05, 0.9)
  note(out, 0.085, 880.0, 0.07, 1.0)
  return wav(out, CHIME_PEAK_DB)
}

/** Screenshot: a camera shutter in miniature. Two bright clicks 60 ms
 *  apart (open, close), the first with a faint low thump under it. */
function screenshot() {
  const out = new Float64Array(seconds(0.15))
  click(out, 0, 3200, 1.2, 0.007, 1.0, 1)
  note(out, 0, 150, 0.018, 0.35)
  click(out, 0.06, 2300, 1.4, 0.011, 0.8, 2)
  return wav(out, SHUTTER_PEAK_DB)
}

/** A save that didn't work: the chime's two notes the other way round
 *  and an octave down (A4 then D4), so it reads as "no" without a buzz. */
function saveFailed() {
  const out = new Float64Array(seconds(0.3))
  note(out, 0, 440.0, 0.06, 1.0)
  note(out, 0.1, 293.66, 0.08, 0.9)
  return wav(out, CHIME_PEAK_DB)
}

const dir = join(dirname(fileURLToPath(import.meta.url)), '..', 'app', 'src-tauri', 'sounds')
for (const [name, make] of [['clip-saved.wav', clipSaved], ['screenshot.wav', screenshot], ['save-failed.wav', saveFailed]]) {
  const data = make()
  writeFileSync(join(dir, name), data)
  console.log(`${name}: ${((data.length - 44) / 2 / RATE * 1000).toFixed(0)} ms, ${data.length} bytes`)
}
