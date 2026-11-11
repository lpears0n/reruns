// Release notes arrive as the Markdown written on the release. The update
// prompt shows them with the little structure they use (headings, bullets,
// bold, links as their text), built as elements, never as HTML.

export interface NoteLine {
  kind: 'heading' | 'bullet' | 'text'
  parts: { text: string; bold: boolean }[]
}

/** `**bold**` runs; links become their text. */
function parts(text: string): NoteLine['parts'] {
  const plain = text.replace(/\[([^\]]+)\]\([^)]*\)/g, '$1').replace(/`([^`]+)`/g, '$1')
  return plain
    .split(/\*\*(.+?)\*\*/g)
    .map((t, i) => ({ text: t, bold: i % 2 === 1 }))
    .filter((p) => p.text)
}

export function noteLines(markdown: string): NoteLine[] {
  const lines: NoteLine[] = []
  for (const raw of markdown.split(/\r?\n/)) {
    const line = raw.trim()
    if (!line) continue
    const heading = /^#{1,6}\s+(.*)$/.exec(line)
    const bullet = /^[-*]\s+(.*)$/.exec(line)
    if (heading) lines.push({ kind: 'heading', parts: parts(heading[1]) })
    else if (bullet) lines.push({ kind: 'bullet', parts: parts(bullet[1]) })
    else lines.push({ kind: 'text', parts: parts(line) })
  }
  return lines
}
