export type PlanKind = 'story' | 'criteria' | 'subtasks' | 'context' | 'steps' | 'files' | 'risks' | 'checks' | 'other'

export interface PlanSection {
  kind: PlanKind
  heading: string
  body: string
  items: string[]
}

export interface PlanDoc {
  title: string | null
  intro: string
  sections: PlanSection[]
}

const kinds: [PlanKind, RegExp][] = [
  ['story', /^(descri[çc][ãa]o|hist[óo]ria|user story)/i],
  ['criteria', /^(crit[ée]rios de aceite|acceptance criteria)/i],
  ['subtasks', /^(subtarefas|subtasks)/i],
  ['context', /^(contexto|context|problema|objetivo|resumo|summary)/i],
  ['steps', /^(abordagem|approach|passos|steps|implementa|plano de implementa)/i],
  ['files', /^(arquivos|critical files|files)/i],
  ['risks', /^(riscos|risks)/i],
  ['checks', /^(verifica|verification|testes|como verificar)/i],
]

const kindOf = (heading: string): PlanKind => kinds.find(([, re]) => re.test(heading))?.[0] ?? 'other'

const headingOf = (line: string): { heading: string; rest: string } | null => {
  const hashed = /^#{2,4}\s+(.+?)\s*#*\s*$/.exec(line)
  if (hashed) return { heading: hashed[1].replace(/\*\*/g, '').replace(/:$/, '').trim(), rest: '' }
  const bold = /^\*\*([^*]+?):?\*\*:?\s*(.*)$/.exec(line.trim())
  if (!bold) return null
  const heading = bold[1].replace(/:$/, '').trim()
  if (bold[2] && kindOf(heading) === 'other') return null
  return { heading, rest: bold[2] }
}

const bullet = /^\s{0,3}(?:[-*+]|\d+[.)])\s+(.*)$/

export function listItems(body: string): string[] {
  const items: string[] = []
  let loose = false
  for (const line of body.split('\n')) {
    const hit = bullet.exec(line)
    if (hit) items.push(hit[1].trim())
    else if (line.trim() && items.length > 0 && /^\s+/.test(line)) items[items.length - 1] += `\n${line.trim()}`
    else if (line.trim()) loose = true
  }
  return loose ? [] : items
}

export function parsePlan(text: string): PlanDoc {
  const lines = text.replace(/\r\n/g, '\n').split('\n')
  let title: string | null = null
  const intro: string[] = []
  const sections: { heading: string; lines: string[] }[] = []
  let fence = false
  for (const line of lines) {
    if (/^\s*```/.test(line)) fence = !fence
    const top = fence ? null : /^#\s+(.+)$/.exec(line)
    if (top && title === null && sections.length === 0) {
      title = top[1].trim()
      continue
    }
    const open = sections[sections.length - 1]
    const nested = open && kindOf(open.heading) === 'subtasks' && /^###\s+/.test(line)
    const heading = fence || nested ? null : headingOf(line)
    if (heading) sections.push({ heading: heading.heading, lines: heading.rest ? [heading.rest] : [] })
    else if (sections.length > 0) sections[sections.length - 1].lines.push(line)
    else intro.push(line)
  }
  return {
    title,
    intro: intro.join('\n').trim(),
    sections: sections.map((s) => {
      const body = s.lines.join('\n').trim()
      return { kind: kindOf(s.heading), heading: s.heading, body, items: listItems(body) }
    }),
  }
}

export const structured = (doc: PlanDoc) => doc.sections.filter((s) => s.kind !== 'other').length >= 2

export interface Subtask {
  title: string
  details: string
}

export function subtasksOf(body: string): Subtask[] {
  const titled = body.split(/^###\s+/m).map((part) => part.trim()).filter(Boolean)
  if (titled.length > 0 && /^###\s+/m.test(body)) {
    return titled.map((part) => {
      const [first, ...rest] = part.split('\n')
      return { title: first.trim(), details: rest.join('\n').trim() }
    })
  }
  return listItems(body).map((item) => {
    const [first, ...rest] = item.split('\n')
    return { title: first.replace(/\*\*/g, '').trim(), details: rest.join('\n').trim() }
  })
}

export function stressed(criterion: string): string {
  return criterion.replace(/(^|[,;.]\s*)(dado que|dado|quando|então|e)(?=\s)/gi, (_, lead: string, word: string) => `${lead}**${word.charAt(0).toUpperCase()}${word.slice(1)}**`)
}

const capital = (word: string) => `**${word.charAt(0).toUpperCase()}${word.slice(1)}**`

export function storied(story: string): string {
  const lead = story.replace(/^(\s*)(como)(?=\s)/i, (_, space: string, word: string) => `${space}${capital(word)}`)
  const want = /(,?\s*)(eu quero)(?=\s)/i.exec(lead)
  if (!want) return lead
  const cut = want.index + want[0].length
  const head = `${lead.slice(0, want.index)}${want[1]}${capital(want[2])}`
  const tail = lead.slice(cut).replace(/(\s)(para que|para)(?=\s)/i, (_, space: string, word: string) => `${space}${capital(word)}`)
  return `${head}${tail}`
}

export function fileEntry(item: string): { path: string; note: string } {
  const hit = /^\*{0,2}`?([^`\s*]+)`?\*{0,2}:?(?:\s+(?:[—–-]\s+)?(.*))?$/s.exec(item.trim())
  if (!hit) return { path: item.trim(), note: '' }
  return { path: hit[1], note: (hit[2] ?? '').trim() }
}
