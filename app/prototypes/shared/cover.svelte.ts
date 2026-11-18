// "Set cover frame": the clip's card shows a frame the user chose instead of
// the one at 1 s (often a loading screen or a menu).
//
// Prototype only: the frame is drawn from the playing video into a picture
// held in memory for this page. The real version would keep the moment in
// the clip's sidecar (`"cover": 7.25`) and have the thumbnailer grab that
// frame (an accurate seek, not a keyframe one), so it survives restarts and
// moves with the folder like the title and stars do.
const covers = $state<Record<string, { src: string; at: number }>>({})

export const coverOf = (path: string) => covers[path] ?? null

/** Take the frame on screen as the clip's cover. False when the frame can't
 *  be read (a video served without CORS taints the canvas). */
export function setCover(path: string, video: HTMLVideoElement): boolean {
  if (!video.videoWidth) return false
  const w = 480
  const h = Math.round((w * video.videoHeight) / video.videoWidth)
  const canvas = document.createElement('canvas')
  canvas.width = w
  canvas.height = h
  try {
    canvas.getContext('2d')!.drawImage(video, 0, 0, w, h)
    covers[path] = { src: canvas.toDataURL('image/jpeg', 0.86), at: video.currentTime }
    return true
  } catch {
    return false
  }
}

export function clearCover(path: string) {
  delete covers[path]
}
