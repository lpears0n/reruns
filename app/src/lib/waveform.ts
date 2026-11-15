// A clip's audio as peaks, for the studio's timeline. Decoded in the
// webview from the clip itself (its first audio track: the mix, or the
// game), once per clip.
import { videoUrl } from './api'

/** shortcut: decodes the whole file in memory, so bigger clips get no waveform; move to peaks made with the thumbnails when long clips need one */
const MAX_BYTES = 300e6
const BUCKETS = 600

const cache = new Map<string, Promise<Float32Array | null>>()

/** Loudest sample in each of 600 slices of the clip, 0 to 1; null when the
 *  audio can't be read here (no track, an undecodable codec, a huge file). */
export function waveform(path: string, sizeBytes: number): Promise<Float32Array | null> {
  let made = cache.get(path)
  if (!made) {
    made = sizeBytes > MAX_BYTES ? Promise.resolve(null) : decode(path).catch(() => null)
    cache.set(path, made)
  }
  return made
}

async function decode(path: string): Promise<Float32Array | null> {
  const file = await (await fetch(videoUrl(path))).arrayBuffer()
  // Offline: nothing is played, so no audio device is opened.
  const audio = await new OfflineAudioContext(1, 1, 48000).decodeAudioData(file)
  return peaks(Array.from({ length: audio.numberOfChannels }, (_, c) => audio.getChannelData(c)))
}

export function peaks(channels: Float32Array[], buckets = BUCKETS): Float32Array | null {
  const length = channels[0]?.length ?? 0
  if (!length) return null
  const out = new Float32Array(buckets)
  const per = length / buckets
  // Every 8th sample: a peak is wide enough at 48 kHz not to be missed.
  for (const data of channels) {
    for (let b = 0; b < buckets; b++) {
      const to = Math.min(length, Math.floor((b + 1) * per))
      let max = out[b]
      for (let i = Math.floor(b * per); i < to; i += 8) {
        const v = Math.abs(data[i])
        if (v > max) max = v
      }
      out[b] = max
    }
  }
  return out
}
