import { useEffect, useState } from 'react'
import type { Block, CaseStep, TestCaseView } from '@/api/contract'
import { ChatView } from '@/chat/ChatView'
import { Button } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Badge } from '@/ui/display'
import { Checkbox, SegmentedControl, Textarea } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { AnimatePresence, Collapse, motion, tween } from '@/ui/motion'
import { NoteForm, noteComment, type Note } from './review'
import { CommentItem, SendError, useSend } from './shared'

type Of<T extends Block['type']> = Extract<Block, { type: T }>

const KINDS = { api: { label: 'API', icon: 'server' }, ui: { label: 'Tela', icon: 'computer' } } as const

interface Row {
  line: number
  label: string
  text: string
  data: string | null
  expected: string | null
}

function rowsOf(entry: TestCaseView): Row[] {
  const head: Row[] = []
  if (entry.objective) head.push({ line: -2, label: 'Objetivo', text: entry.objective, data: null, expected: null })
  if (entry.precondition) head.push({ line: -1, label: 'Pré-condição', text: entry.precondition, data: null, expected: null })
  const steps = entry.steps.map((s: CaseStep, i): Row => ({ line: i, label: String(i + 1), text: s.action, data: s.data, expected: s.expected }))
  return [...head, ...steps]
}

const rowName = (row: Row) => (row.line < 0 ? row.label : `Passo ${row.label}`)

function KindBadge({ kind }: { kind: TestCaseView['kind'] }) {
  const meta = KINDS[kind]
  return <Badge tone={kind === 'ui' ? 'accent' : 'info'} icon={<Icon name={meta.icon} size={12} />}>{meta.label}</Badge>
}

interface Notes {
  editable: boolean
  notes: Note[]
  add: (file: string, line: number, body: string) => void
  remove: (id: string) => void
}

function CaseRow({ file, row, notes }: { file: string; row: Row; notes: Notes }) {
  const [writing, setWriting] = useState(false)
  const here = notes.notes.filter((n) => n.file === file && n.line === row.line)
  const step = row.line >= 0
  return (
    <>
      <div onDoubleClick={() => notes.editable && setWriting(true)} className={cn('group/line flex gap-3 py-2 pr-4', !step && 'bg-subtle')}>
        <span className="flex w-7 shrink-0 justify-center pt-0.5">
          {notes.editable && (
            <button aria-label={`Comentar ${rowName(row)}`} onClick={() => setWriting(true)} className="flex size-[18px] items-center justify-center rounded-[5px] bg-info text-white opacity-0 shadow-card transition-opacity group-hover/line:opacity-100 focus-visible:opacity-100">
              <Icon name="plus" size={12} strokeWidth={2.4} />
            </button>
          )}
        </span>
        <span className={cn('shrink-0 select-none pt-px text-right text-sm tabular-nums text-faint', step ? 'w-6' : 'w-[104px] whitespace-nowrap text-left text-xs font-medium uppercase tracking-wide')}>{row.label}</span>
        {step ? (
          <div className="grid min-w-0 flex-1 gap-x-6 gap-y-1.5 sm:grid-cols-2">
            <div className="min-w-0 space-y-1">
              <p className="text-base text-fg">{row.text}</p>
              {row.data && <p className="font-mono text-[13px] leading-5 text-fg2"><span className="font-sans text-muted">Dados · </span>{row.data}</p>}
            </div>
            <p className="flex min-w-0 gap-2 text-base text-fg2"><Icon name="checkCircle" size={15} className="mt-[3px] shrink-0 text-success-fg" />{row.expected}</p>
          </div>
        ) : (
          <p className="min-w-0 flex-1 text-base text-fg2">{row.text}</p>
        )}
      </div>
      {here.map((n) => (
        <div key={n.id} className="mx-3 my-2 flex items-start gap-2 rounded-control border bg-surface px-3 py-1">
          <div className="min-w-0 flex-1"><CommentItem c={noteComment(n)} compact /></div>
          {notes.editable && <Button size="sm" variant="ghost" className="mt-1.5 h-7" onClick={() => notes.remove(n.id)}>Remover</Button>}
        </div>
      ))}
      <AnimatePresence>{writing && <NoteForm onCancel={() => setWriting(false)} onSave={(body) => { notes.add(file, row.line, body); setWriting(false) }} />}</AnimatePresence>
    </>
  )
}

function CaseFile({ entry, index, viewed, onViewed, notes }: { entry: TestCaseView; index: number; viewed: boolean; onViewed: () => void; notes: Notes }) {
  const [open, setOpen] = useState(!viewed)
  useEffect(() => setOpen(!viewed), [viewed])
  const file = String(index)
  const count = notes.notes.filter((n) => n.file === file).length
  return (
    <div className={cn('overflow-hidden rounded-card bg-surface shadow-card transition-opacity', viewed && 'opacity-80')}>
      <div className="flex h-11 items-center gap-2.5 px-3.5">
        <button onClick={() => setOpen(!open)} className="flex min-w-0 flex-1 items-center gap-2.5 text-left">
          <motion.span initial={false} animate={{ rotate: open ? 0 : -90 }} transition={tween} className="flex text-faint"><Icon name="down" size={14} /></motion.span>
          <span className="shrink-0 text-sm tabular-nums text-faint">{index + 1}</span>
          <span className="min-w-0 truncate text-base font-medium text-fg" title={entry.title}>{entry.title}</span>
          <KindBadge kind={entry.kind} />
          <span className="shrink-0 text-xs tabular-nums text-muted">{entry.steps.length} {entry.steps.length === 1 ? 'passo' : 'passos'}</span>
          {count > 0 && <span className="flex items-center gap-1 text-xs text-muted"><Icon name="comment" size={13} />{count}</span>}
        </button>
        {entry.ref && (
          <a href={entry.url ?? undefined} target="_blank" rel="noreferrer" className={cn('flex items-center gap-1 text-sm text-muted', entry.url && 'hover:text-fg')}>
            {entry.ref}{entry.url && <Icon name="external" size={12} />}
          </a>
        )}
        {notes.editable && (
          <label className={cn('flex h-7 cursor-pointer items-center gap-2 rounded-control-sm px-2 text-sm transition-colors', viewed ? 'bg-info-soft text-info-fg' : 'text-muted hover:bg-hover')}>
            <Checkbox checked={viewed} onCheckedChange={onViewed} label={`Marcar ${entry.title} como visto`} />
            Visto
          </label>
        )}
      </div>
      <Collapse open={open}>
        <div className="divide-y border-t">{rowsOf(entry).map((row) => <CaseRow key={row.line} file={file} row={row} notes={notes} />)}</div>
      </Collapse>
    </div>
  )
}

export function feedback(cases: TestCaseView[], body: string, notes: Note[]): string {
  const parts = body ? [body] : []
  cases.forEach((entry, index) => {
    const mine = notes.filter((n) => n.file === String(index))
    if (mine.length === 0) return
    const lines = mine.map((n) => {
      const row = rowsOf(entry).find((r) => r.line === n.line)
      return `- ${row ? `${rowName(row)} (${row.text})` : 'Caso'}: ${n.body}`
    })
    parts.push(`### ${index + 1}. ${entry.title}\n${lines.join('\n')}`)
  })
  return parts.join('\n\n')
}

type Verdict = 'approve' | 'changes'

const options: { v: Verdict; t: string; d: string }[] = [
  { v: 'approve', t: 'Criar casos', d: 'Os casos são criados no gerenciador de testes.' },
  { v: 'changes', t: 'Pedir mudanças', d: 'Volta para o testador com seus comentários.' },
]

function Finish({ block, notes, onSent }: { block: Of<'cases'>; notes: Note[]; onSent: () => void }) {
  const [kind, setKind] = useState<Verdict>(notes.length > 0 ? 'changes' : 'approve')
  const [body, setBody] = useState('')
  const { busy, error, send } = useSend()
  useEffect(() => { if (notes.length > 0) setKind('changes') }, [notes.length])
  const text = feedback(block.cases, body.trim(), notes)
  const submit = () => {
    const sent = kind === 'approve' ? send(block.approve ?? '', null) : send(block.changes ?? '', { changes: text })
    void sent.then(() => { setBody(''); onSent() })
  }
  return (
    <div className="pt-3">
      <Textarea rows={3} value={body} onChange={(e) => setBody(e.target.value)} placeholder="Comentário geral sobre os casos" />
      <div className="mt-2 space-y-1">
        {options.map((o) => (
          <label key={o.v} className={cn('flex cursor-pointer items-start gap-3 rounded-control px-2.5 py-2 transition-colors', kind === o.v ? 'bg-inset' : 'hover:bg-hover')}>
            <input type="radio" name="cases-verdict" checked={kind === o.v} onChange={() => setKind(o.v)} className="mt-1 accent-[rgb(var(--text))]" />
            <span>
              <span className="block text-base font-medium text-fg">{o.t}</span>
              <span className="block text-sm text-muted">{o.d}</span>
            </span>
          </label>
        ))}
      </div>
      <div className="mt-2 flex justify-end">
        <Button size="sm" variant="primary" loading={busy} disabled={kind === 'changes' && !text} onClick={submit}>Enviar</Button>
      </div>
      <SendError error={error} />
    </div>
  )
}

function Header({ block, view, setView, seen, notes, onSent }: { block: Of<'cases'>; view: 'cases' | 'chat'; setView: (v: 'cases' | 'chat') => void; seen: number; notes: Note[]; onSent: () => void }) {
  const [open, setOpen] = useState(false)
  const tabs = [{ value: 'cases' as const, label: 'Casos', badge: <span className="text-xs tabular-nums text-muted">{block.editable ? `${seen}/` : ''}{block.cases.length}</span> }, ...(block.session ? [{ value: 'chat' as const, label: 'Conversa' }] : [])]
  return (
    <div className="sticky -top-4 z-10 -mx-4 -mt-4 border-b bg-surface px-4 py-3">
      <div className="flex items-center gap-3">
        <SegmentedControl<'cases' | 'chat'> size="sm" value={view} onChange={setView} options={tabs} />
        {view === 'cases' && notes.length > 0 && <span className="flex items-center gap-1 text-sm text-muted"><Icon name="comment" size={14} />{notes.length}</span>}
        {block.editable && (
          <Button size="sm" variant="primary" className="ml-auto" onClick={() => setOpen(!open)}>
            Finalizar revisão
            <motion.span initial={false} animate={{ rotate: open ? 180 : 0 }} transition={tween} className="flex"><Icon name="down" size={14} /></motion.span>
          </Button>
        )}
      </div>
      <Collapse open={open && block.editable}><Finish block={block} notes={notes} onSent={() => { setOpen(false); onSent() }} /></Collapse>
    </div>
  )
}

export function CasesBlock({ block }: { block: Of<'cases'> }) {
  const [view, setView] = useState<'cases' | 'chat'>('cases')
  const [notes, setNotes] = useState<Note[]>([])
  const [viewed, setViewed] = useState<number[]>([])
  const kit: Notes = {
    editable: block.editable,
    notes,
    add: (file, line, body) => setNotes((n) => [...n, { id: `n${n.length}-${file}-${line}`, file, line, body }]),
    remove: (id) => setNotes((n) => n.filter((x) => x.id !== id)),
  }
  const toggle = (i: number) => setViewed((v) => (v.includes(i) ? v.filter((x) => x !== i) : [...v, i]))
  if (block.cases.length === 0) return <p className="text-base text-muted">Nenhum caso de teste.</p>
  return (
    <div className={cn(view === 'chat' ? 'flex h-full flex-col' : 'space-y-4')}>
      <Header block={block} view={view} setView={setView} seen={viewed.length} notes={notes} onSent={() => { setNotes([]); setViewed([]) }} />
      {view === 'chat' && block.session ? (
        <div className="-mx-4 -mb-4 min-h-0 flex-1"><ChatView sessionId={block.session} composer={block.editable} /></div>
      ) : (
        <div className="space-y-2">
          {block.cases.map((c, i) => <CaseFile key={`${i}-${c.title}`} entry={c} index={i} viewed={viewed.includes(i)} onViewed={() => toggle(i)} notes={kit} />)}
        </div>
      )}
    </div>
  )
}
