import { currentLanguage, translate, type MessageKey } from '@/i18n'

function word(key: MessageKey, vars?: Record<string, string | number>): string {
  return translate(currentLanguage(), key, vars)
}

export interface TextEdit {
  text: string
  start: number
  end: number
}

export type LineKind = 'ul' | 'ol' | 'task' | 'quote' | 'h1' | 'h2' | 'h3'

const linePrefixPattern = /^(\s*)(#{1,6} |[-*+] \[[ xX]\] |[-*+] |\d+[.)] |> ?)/

function lineBounds(text: string, start: number, end: number) {
  const lineStart = text.lastIndexOf('\n', start - 1) + 1
  const nextBreak = text.indexOf('\n', end)
  const lineEnd = nextBreak === -1 ? text.length : nextBreak
  return { lineStart, lineEnd }
}

function prefixFor(kind: LineKind, index: number) {
  switch (kind) {
    case 'ul':
      return '- '
    case 'ol':
      return `${index + 1}. `
    case 'task':
      return '- [ ] '
    case 'quote':
      return '> '
    case 'h1':
      return '# '
    case 'h2':
      return '## '
    case 'h3':
      return '### '
  }
}

export function lineKind(line: string): LineKind | null {
  const m = line.match(linePrefixPattern)
  if (!m) return null
  const p = m[2]
  if (p.startsWith('#')) {
    const level = p.trim().length
    return level === 1 ? 'h1' : level === 2 ? 'h2' : level === 3 ? 'h3' : null
  }
  if (/\[[ xX]\]/.test(p)) return 'task'
  if (/^\d/.test(p)) return 'ol'
  if (p.startsWith('>')) return 'quote'
  return 'ul'
}

export function toggleWrap(text: string, start: number, end: number, marker: string, placeholder: string): TextEdit {
  const m = marker.length
  const selected = text.slice(start, end)
  if (start >= m && text.slice(start - m, start) === marker && text.slice(end, end + m) === marker) {
    const next = text.slice(0, start - m) + selected + text.slice(end + m)
    return { text: next, start: start - m, end: end - m }
  }
  if (selected.length >= m * 2 && selected.startsWith(marker) && selected.endsWith(marker)) {
    const inner = selected.slice(m, selected.length - m)
    return { text: text.slice(0, start) + inner + text.slice(end), start, end: start + inner.length }
  }
  const body = selected || placeholder
  const next = text.slice(0, start) + marker + body + marker + text.slice(end)
  return { text: next, start: start + m, end: start + m + body.length }
}

export function toggleLinePrefix(text: string, start: number, end: number, kind: LineKind): TextEdit {
  const { lineStart, lineEnd } = lineBounds(text, start, end)
  const lines = text.slice(lineStart, lineEnd).split('\n')
  const all = lines.every((l) => lineKind(l) === kind)
  const replaced = lines
    .map((l, i) => {
      const m = l.match(linePrefixPattern)
      const indent = m ? m[1] : ''
      const body = m ? l.slice(m[0].length) : l
      return all ? indent + body : indent + prefixFor(kind, i) + body
    })
    .join('\n')
  const next = text.slice(0, lineStart) + replaced + text.slice(lineEnd)
  if (lines.length === 1 && start === end) {
    const caret = Math.max(lineStart, start + replaced.length - (lineEnd - lineStart))
    return { text: next, start: caret, end: caret }
  }
  return { text: next, start: lineStart, end: lineStart + replaced.length }
}

export function cycleHeading(text: string, start: number, end: number): TextEdit {
  const { lineStart } = lineBounds(text, start, end)
  const current = lineKind(text.slice(lineStart, text.indexOf('\n', lineStart) === -1 ? text.length : text.indexOf('\n', lineStart)))
  const order: (LineKind | null)[] = [null, 'h1', 'h2', 'h3']
  const at = order.indexOf(current && current.startsWith('h') ? current : null)
  const nextKind = order[(at + 1) % order.length]
  if (nextKind) return toggleLinePrefix(text, start, end, nextKind)
  return toggleLinePrefix(text, start, end, current as LineKind)
}

export function continueList(text: string, start: number, end: number): TextEdit | null {
  const lineStart = text.lastIndexOf('\n', start - 1) + 1
  const line = text.slice(lineStart, start)
  const m = line.match(/^(\s*)([-*+] \[[ xX]\] |[-*+] |(\d+)([.)]) |> ?)/)
  if (!m) return null
  const restOfLine = text.slice(start, text.indexOf('\n', start) === -1 ? text.length : text.indexOf('\n', start))
  if (line.trim() === m[2].trim() && !restOfLine.trim()) {
    const next = text.slice(0, lineStart) + text.slice(end)
    return { text: next, start: lineStart, end: lineStart }
  }
  const marker = m[3] ? `${Number(m[3]) + 1}${m[4]} ` : m[2].replace(/\[[xX]\]/, '[ ]')
  const insert = `\n${m[1]}${marker}`
  const next = text.slice(0, start) + insert + text.slice(end)
  const pos = start + insert.length
  return { text: next, start: pos, end: pos }
}

export function indentLines(text: string, start: number, end: number, outdent: boolean): TextEdit | null {
  const { lineStart, lineEnd } = lineBounds(text, start, end)
  const lines = text.slice(lineStart, lineEnd).split('\n')
  const listish = lines.every((l) => !l.trim() || ['ul', 'ol', 'task'].includes(lineKind(l) ?? ''))
  if (!listish) return null
  let firstDelta = 0
  let total = 0
  const replaced = lines.map((l, i) => {
    if (!l.trim()) return l
    const next = outdent ? l.replace(/^ {1,2}/, '') : `  ${l}`
    const delta = next.length - l.length
    if (i === 0) firstDelta = delta
    total += delta
    return next
  })
  const joined = replaced.join('\n')
  const next = text.slice(0, lineStart) + joined + text.slice(lineEnd)
  return { text: next, start: Math.max(lineStart, start + firstDelta), end: Math.max(lineStart, end + total) }
}

export function tableMarkdown(cols: number, rows: number) {
  const header = `| ${Array.from({ length: cols }, (_, i) => word('editor.column', { n: i + 1 })).join(' | ')} |`
  const rule = `| ${Array.from({ length: cols }, () => '---').join(' | ')} |`
  const body = Array.from({ length: rows }, () => `| ${Array.from({ length: cols }, () => '   ').join(' | ')} |`)
  return [header, rule, ...body].join('\n')
}

export function insertBlock(text: string, start: number, block: string): TextEdit {
  const lineStart = text.lastIndexOf('\n', start - 1) + 1
  const onEmptyLine = !text.slice(lineStart, start).trim()
  const lead = start === 0 ? '' : onEmptyLine ? (text[lineStart - 2] === '\n' || lineStart === 0 ? '' : '\n') : '\n\n'
  const trail = text.slice(start).startsWith('\n') ? '\n' : '\n\n'
  const next = text.slice(0, start) + lead + block + trail + text.slice(start)
  const pos = start + lead.length + block.length + trail.length
  return { text: next, start: pos, end: pos }
}

export function insertTable(text: string, start: number, cols = 2, rows = 2): TextEdit {
  const block = tableMarkdown(cols, rows)
  const r = insertBlock(text, start, block)
  const headerAt = r.text.indexOf(block, start) + 2
  return { text: r.text, start: headerAt, end: headerAt + word('editor.column', { n: 1 }).length }
}

export function insertLink(text: string, start: number, end: number, image = false): TextEdit {
  const selected = text.slice(start, end)
  const label = selected || (image ? word('editor.sampleImage') : word('editor.sampleLink'))
  const open = image ? '![' : '['
  const snippet = `${open}${label}](https://)`
  const next = text.slice(0, start) + snippet + text.slice(end)
  if (selected) {
    const urlAt = start + open.length + label.length + 2
    return { text: next, start: urlAt, end: urlAt + 'https://'.length }
  }
  return { text: next, start: start + open.length, end: start + open.length + label.length }
}

export function markdownStats(text: string) {
  const trimmed = text.trim()
  return {
    words: trimmed ? trimmed.split(/\s+/).length : 0,
    chars: text.length,
    lines: text ? text.split('\n').length : 0,
  }
}

export function syncedScrollTop(from: { top: number; height: number; client: number }, to: { height: number; client: number }) {
  const range = from.height - from.client
  const ratio = range > 0 ? from.top / range : 0
  return Math.round(ratio * Math.max(0, to.height - to.client))
}

export type TokenKind = 'plain' | 'heading' | 'mark' | 'bold' | 'italic' | 'strike' | 'code' | 'link' | 'quote' | 'fence' | 'rule'

export interface Token {
  text: string
  kind: TokenKind
}

const inlinePattern = /(`[^`\n]+`)|(\*\*[^*\n]+\*\*|__[^_\n]+__)|(~~[^~\n]+~~)|(\*[^*\s][^*\n]*\*|_[^_\s][^_\n]*_)|(!?\[[^\]\n]*\]\([^)\n]*\))/g

function inlineTokens(line: string): Token[] {
  const out: Token[] = []
  let last = 0
  for (const m of line.matchAll(inlinePattern)) {
    const at = m.index ?? 0
    if (at > last) out.push({ text: line.slice(last, at), kind: 'plain' })
    const kind: TokenKind = m[1] ? 'code' : m[2] ? 'bold' : m[3] ? 'strike' : m[4] ? 'italic' : 'link'
    out.push({ text: m[0], kind })
    last = at + m[0].length
  }
  if (last < line.length) out.push({ text: line.slice(last), kind: 'plain' })
  return out
}

export function highlightMarkdown(text: string): Token[][] {
  let fenced = false
  return text.split('\n').map((line) => {
    if (/^\s*(```|~~~)/.test(line)) {
      fenced = !fenced
      return [{ text: line, kind: 'fence' }]
    }
    if (fenced) return [{ text: line, kind: 'code' }]
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) return [{ text: line, kind: 'rule' }]
    const heading = line.match(/^(#{1,6} )(.*)$/)
    if (heading) return [{ text: heading[1], kind: 'mark' }, { text: heading[2], kind: 'heading' }]
    const quote = line.match(/^(\s*> ?)(.*)$/)
    if (quote) return [{ text: quote[1], kind: 'mark' }, ...inlineTokens(quote[2]).map((t) => (t.kind === 'plain' ? { ...t, kind: 'quote' as const } : t))]
    const list = line.match(/^(\s*(?:[-*+] \[[ xX]\] |[-*+] |\d+[.)] ))(.*)$/)
    if (list) return [{ text: list[1], kind: 'mark' }, ...inlineTokens(list[2])]
    return inlineTokens(line)
  })
}

function sentences(paragraph: string) {
  return paragraph.split(/(?<=[.!?])\s+/).map((s) => s.trim()).filter(Boolean)
}

const GOAL_HEADINGS = ['Objetivo', 'Goal']
const CRITERIA_HEADINGS = ['Critérios de aceite', 'Acceptance criteria', 'Criterios de aceptación', 'Criterios de aceptacion']

function hasHeading(src: string, names: string[]): boolean {
  return names.some((name) => new RegExp(`^#{1,3} ${name}`, 'mi').test(src))
}

function improveParts(src: string) {
  const prose: string[] = []
  const items: string[] = []
  const checks: string[] = []
  for (const raw of src.split('\n')) {
    const line = raw.trim()
    if (!line || /^#{1,6} /.test(line) || /^(```|---)/.test(line)) continue
    const task = line.match(/^[-*+] \[([ xX])\] (.*)$/)
    if (task) {
      checks.push(`- [${task[1].toLowerCase() === 'x' ? 'x' : ' '}] ${task[2]}`)
      continue
    }
    const bullet = line.match(/^(?:[-*+]|\d+[.)]) (.*)$/)
    if (bullet) {
      items.push(bullet[1])
      continue
    }
    prose.push(...sentences(line.replace(/^> ?/, '')))
  }
  return { prose, items, checks }
}

export function improveMarkdown(text: string): string {
  const src = text.trim()
  if (hasHeading(src, GOAL_HEADINGS) && hasHeading(src, CRITERIA_HEADINGS)) return src
  const { prose, items, checks } = improveParts(src)
  const goal = prose[0] ?? word('editor.goalPlaceholder')
  const scope = [...prose.slice(1), ...items]
  const criteria = checks.length ? checks : (items.length ? items : [goal.replace(/[.!?]$/, '')]).map((i) => `- [ ] ${i}`)
  return [
    `## ${word('editor.goalHeading')}`,
    '',
    goal,
    '',
    `## ${word('editor.scopeHeading')}`,
    '',
    ...(scope.length ? scope.map((s) => `- ${s}`) : [`- ${word('editor.toDefine')}`]),
    '',
    `## ${word('editor.criteriaHeading')}`,
    '',
    ...criteria,
  ].join('\n')
}
