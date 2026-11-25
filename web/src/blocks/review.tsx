import { Fragment, useCallback, useEffect, useState } from 'react'
import type { Block, Comment, FileDiff, Person } from '@/api/contract'
import { now } from '@/api/clock'
import { useCall } from '@/api/store'
import { ChatView } from '@/chat/ChatView'
import { Button } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Checkbox, SegmentedControl, Textarea } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { MarkdownEditor } from '@/ui/markdown'
import { AnimatePresence, Collapse, motion, tween } from '@/ui/motion'
import { currentLanguage, plural, translate, useT, type MessageKey } from '@/i18n'
import { useBlockHost } from './context'
import { CommentItem, Loading, SendError, Thread, useSend } from './shared'
import { useViewed } from './viewed'

type Of<T extends Block['type']> = Extract<Block, { type: T }>

export interface Note {
  id: string
  file: string
  line: number
  body: string
}

const me = (): Person => ({ id: 'you', name: translate(currentLanguage(), 'blocks.you'), avatar: null, you: true })

export const noteComment = (n: Note): Comment => ({ id: n.id, author: me(), body: n.body, at: new Date(now()).toISOString(), kind: 'comment' })

export function NoteForm({ onSave, onCancel }: { onSave: (body: string) => void; onCancel: () => void }) {
  const t = useT()
  const [text, setText] = useState('')
  return (
    <motion.div initial={{ opacity: 0, y: -4 }} animate={{ opacity: 1, y: 0 }} transition={tween} className="mx-3 my-2 font-sans">
      <MarkdownEditor autoFocus compact minHeight={110} label={t('blocks.lineCommentLabel')} placeholder={t('blocks.lineCommentPlaceholder')} value={text} onChange={setText} saveLabel={t('blocks.comment')} saveDisabled={!text.trim()} onSave={() => text.trim() && onSave(text.trim())} onCancel={onCancel} />
    </motion.div>
  )
}

function LineThread({ comments, editable, resolved, onResolve, onReply }: { comments: Comment[]; editable: boolean; resolved: boolean; onResolve: () => void; onReply: () => void }) {
  const t = useT()
  const [open, setOpen] = useState(!resolved)
  useEffect(() => setOpen(!resolved), [resolved])
  return (
    <div className="mx-3 my-2 overflow-hidden rounded-control border bg-surface font-sans">
      <div className="flex h-9 items-center gap-2 border-b bg-subtle px-3">
        <button onClick={() => setOpen(!open)} className="flex min-w-0 flex-1 items-center gap-2 text-left text-sm">
          <motion.span initial={false} animate={{ rotate: open ? 0 : -90 }} transition={tween} className="flex text-faint"><Icon name="down" size={13} /></motion.span>
          {resolved ? (
            <span className="flex shrink-0 items-center gap-1.5 whitespace-nowrap text-success-fg"><Icon name="checkCircle" size={14} />{t('blocks.threadResolved')}</span>
          ) : (
            <span className="shrink-0 whitespace-nowrap text-muted">{t(`blocks.threadComments_${plural(comments.length)}`, { n: comments.length })}</span>
          )}
        </button>
        {editable && <Button size="sm" variant="ghost" className="h-7" onClick={onResolve}>{t(resolved ? 'blocks.threadReopen' : 'blocks.threadResolve')}</Button>}
      </div>
      <Collapse open={open}>
        <div className="px-3 py-1">{comments.map((c) => <CommentItem key={c.id} c={c} compact />)}</div>
        {editable && !resolved && (
          <div className="border-t px-3 py-2">
            <button onClick={onReply} className="w-full rounded-control bg-inset px-3 py-1.5 text-left text-sm text-muted transition-colors hover:bg-hover">{t('blocks.threadReply')}</button>
          </div>
        )}
      </Collapse>
    </div>
  )
}

interface ReviewState {
  notes: Note[]
  resolved: string[]
  addNote: (file: string, line: number, body: string) => void
  toggleResolved: (key: string) => void
}

export interface DiffRow {
  sign: ' ' | '+' | '-' | '@'
  code: string
  n: number | null
  at: number | null
}

const HUNK = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/

export function diffRows(file: Pick<FileDiff, 'diff' | 'firstLine'>): DiffRow[] {
  let oldN = file.firstLine
  let newN = file.firstLine
  return file.diff.split('\n').map((raw): DiffRow => {
    const hunk = HUNK.exec(raw)
    if (hunk) {
      oldN = Number(hunk[1])
      newN = Number(hunk[2])
      return { sign: '@', code: raw, n: null, at: null }
    }
    if (raw[0] === '-') return { sign: '-', code: raw.slice(1), n: oldN++, at: null }
    if (raw[0] === '+') return { sign: '+', code: raw.slice(1), n: newN, at: newN++ }
    oldN++
    return { sign: ' ', code: raw.slice(1), n: newN, at: newN++ }
  })
}

function DiffLines({ file, comments, editable, review }: { file: FileDiff; comments: Comment[]; editable: boolean; review: ReviewState }) {
  const t = useT()
  const [openLine, setOpenLine] = useState<number | null>(null)
  const box = useSize()
  const rows = diffRows(file)
  return (
    <div ref={box.ref} className="overflow-x-auto font-mono text-[13px] leading-6" style={{ '--diff-w': `${box.width}px` } as React.CSSProperties}>
      <div className="w-max min-w-full">
      {rows.map((r, i) => {
        if (r.sign === '@') return <div key={i} className="select-none bg-inset px-3 text-faint">{r.code}</div>
        const at = r.at
        const here = at === null ? [] : [...comments.filter((c) => c.line === at), ...review.notes.filter((n) => n.file === file.path && n.line === at).map(noteComment)]
        const key = `${file.path}:${at ?? `old${r.n}`}`
        const commentable = editable && at !== null
        return (
          <Fragment key={i}>
            <div onDoubleClick={() => commentable && setOpenLine(at)} className={cn('group/line flex', r.sign === '+' && 'bg-success-soft text-success-fg', r.sign === '-' && 'bg-danger-soft text-danger-fg')}>
              <span className="flex w-7 shrink-0 items-center justify-center">
                {commentable && (
                  <button aria-label={t('blocks.lineCommentPlaceholder')} onClick={() => setOpenLine(at)} className="flex size-[18px] items-center justify-center rounded-[5px] bg-info text-white opacity-0 shadow-card transition-opacity group-hover/line:opacity-100 focus-visible:opacity-100">
                    <Icon name="plus" size={12} strokeWidth={2.4} />
                  </button>
                )}
              </span>
              <span className="w-9 shrink-0 select-none pr-3 text-right text-faint">{r.n}</span>
              <span className="w-5 shrink-0 select-none text-center">{r.sign === ' ' ? '' : r.sign}</span>
              <span className="whitespace-pre pr-6">{r.code}</span>
            </div>
            {here.length > 0 && <div className="sticky left-0 w-[var(--diff-w)]"><LineThread comments={here} editable={editable} resolved={review.resolved.includes(key)} onResolve={() => review.toggleResolved(key)} onReply={() => setOpenLine(at)} /></div>}
            <div className="sticky left-0 w-[var(--diff-w)]"><AnimatePresence>{at !== null && openLine === at && <NoteForm onCancel={() => setOpenLine(null)} onSave={(body) => { review.addNote(file.path, at, body); setOpenLine(null) }} />}</AnimatePresence></div>
          </Fragment>
        )
      })}
      </div>
    </div>
  )
}

function ReviewFile({ file, comments, editable, viewed, onViewed, review }: { file: FileDiff; comments: Comment[]; editable: boolean; viewed: boolean; onViewed: () => void; review: ReviewState }) {
  const t = useT()
  const [open, setOpen] = useState(!viewed)
  useEffect(() => setOpen(!viewed), [viewed])
  const count = comments.length + review.notes.filter((n) => n.file === file.path).length
  return (
    <div className={cn('overflow-clip rounded-card bg-surface shadow-card transition-opacity', viewed && 'opacity-80')}>
      <div className={cn('sticky top-[calc(var(--review-bar,0px)-1rem-1px)] z-[5] flex h-11 items-center gap-2.5 bg-surface px-3.5', open && 'border-b')}>
        <button onClick={() => setOpen(!open)} className="flex min-w-0 flex-1 items-center gap-2.5 text-left">
          <motion.span initial={false} animate={{ rotate: open ? 0 : -90 }} transition={tween} className="flex text-faint"><Icon name="down" size={14} /></motion.span>
          <span className="min-w-0 truncate font-mono text-sm text-fg">{file.path}</span>
          <span className="font-mono text-xs tabular-nums text-success-fg">+{file.additions}</span>
          <span className="font-mono text-xs tabular-nums text-danger-fg">−{file.deletions}</span>
          {count > 0 && <span className="flex items-center gap-1 text-xs text-muted"><Icon name="comment" size={13} />{count}</span>}
        </button>
        {editable && (
          <label className={cn('flex h-7 cursor-pointer items-center gap-2 rounded-control-sm px-2 text-sm transition-colors', viewed ? 'bg-info-soft text-info-fg' : 'text-muted hover:bg-hover')}>
            <Checkbox checked={viewed} onCheckedChange={onViewed} label={t('blocks.markViewed', { name: file.path })} />
            {t('blocks.viewed')}
          </label>
        )}
      </div>
      <Collapse open={open}>
        <div className="py-2"><DiffLines file={file} comments={comments} editable={editable} review={review} /></div>
      </Collapse>
    </div>
  )
}

type Verdict = 'comment' | 'approve' | 'changes'

const options: { v: Verdict; t: MessageKey; d: MessageKey }[] = [
  { v: 'comment', t: 'blocks.comment', d: 'blocks.reviewCommentHint' },
  { v: 'approve', t: 'blocks.reviewApprove', d: 'blocks.reviewApproveHint' },
  { v: 'changes', t: 'blocks.requestChanges', d: 'blocks.reviewChangesHint' },
]

function useSize() {
  const [size, setSize] = useState({ width: 0, height: 0 })
  const [node, setNode] = useState<HTMLElement | null>(null)
  useEffect(() => {
    if (!node) return
    const ro = new ResizeObserver(() => setSize({ width: node.clientWidth, height: node.offsetHeight }))
    ro.observe(node)
    return () => ro.disconnect()
  }, [node])
  return { ...size, ref: useCallback((el: HTMLElement | null) => setNode(el), []) }
}

export function ReviewBlock({ block }: { block: Of<'review'> }) {
  const t = useT()
  const { version, item } = useBlockHost()
  const bar = useSize()
  const { data: change, error } = useCall(block.connection, 'code.change', { change: block.change }, version)
  const [open, setOpen] = useState(false)
  const [view, setView] = useState<'files' | 'chat'>('files')
  const [kind, setKind] = useState<Verdict>('approve')
  const [body, setBody] = useState('')
  const [notes, setNotes] = useState<Note[]>([])
  const { viewed, toggle: toggleViewed, clear: clearViewed } = useViewed(`viewed:review:${item}:${block.connection}:${block.change}`)
  const [resolved, setResolved] = useState<string[]>([])
  const { busy, error: sendError, send } = useSend()
  if (!change) return <Loading error={error} />
  const review: ReviewState = {
    notes,
    resolved,
    addNote: (file, line, text) => setNotes((n) => [...n, { id: `n${n.length}-${line}`, file, line, body: text }]),
    toggleResolved: (key) => setResolved((r) => (r.includes(key) ? r.filter((k) => k !== key) : [...r, key])),
  }
  const waiting = block.editable
  const files = change.files
  const seen = files.filter((f) => viewed.includes(f.path)).length
  const general = change.comments.filter((c) => !c.file && c.kind !== 'approved')
  const approval = change.comments.find((c) => c.kind === 'approved')
  const lineNotes = notes.length
  const submit = () => {
    void send(block.submit, { verdict: kind, body: body.trim(), notes: notes.map(({ file, line, body: text }) => ({ file, line, body: text })) }).then(() => {
      setBody('')
      setNotes([])
      clearViewed()
      setOpen(false)
    })
  }
  return (
    <div className={cn(view === 'chat' ? 'flex h-full flex-col' : 'space-y-4')} style={{ '--review-bar': `${bar.height}px` } as React.CSSProperties}>
      <div ref={bar.ref} className="sticky -top-4 z-10 -mx-4 -mt-4 border-b bg-surface px-4 py-3">
        <div className="flex items-center gap-3">
          <SegmentedControl<'files' | 'chat'> size="sm" value={view} onChange={setView} options={[{ value: 'files', label: t('blocks.reviewFiles'), badge: <span className="text-xs tabular-nums text-muted">{seen}/{files.length}</span> }, ...(block.session ? [{ value: 'chat' as const, label: t('blocks.conversation') }] : [])]} />
          {view === 'files' && lineNotes > 0 && <span className="flex items-center gap-1 text-sm text-muted"><Icon name="comment" size={14} />{lineNotes}</span>}
          {waiting && (
            <Button size="sm" variant="primary" className="ml-auto" onClick={() => setOpen(!open)}>
              {t('blocks.reviewFinish')}
              <motion.span initial={false} animate={{ rotate: open ? 180 : 0 }} transition={tween} className="flex"><Icon name="down" size={14} /></motion.span>
            </Button>
          )}
        </div>
        <Collapse open={open && waiting}>
          <div className="pt-3">
            <Textarea rows={3} value={body} onChange={(e) => setBody(e.target.value)} placeholder={t('blocks.reviewPlaceholder')} />
            <div className="mt-2 space-y-1">
              {options.map((o) => (
                <label key={o.v} className={cn('flex cursor-pointer items-start gap-3 rounded-control px-2.5 py-2 transition-colors', kind === o.v ? 'bg-inset' : 'hover:bg-hover')}>
                  <input type="radio" name={`review-${item}`} checked={kind === o.v} onChange={() => setKind(o.v)} className="mt-1 accent-[rgb(var(--text))]" />
                  <span>
                    <span className="block text-base font-medium text-fg">{t(o.t)}</span>
                    <span className="block text-sm text-muted">{t(o.d)}</span>
                  </span>
                </label>
              ))}
            </div>
            <div className="mt-2 flex justify-end">
              <Button size="sm" variant="primary" loading={busy} disabled={kind === 'comment' ? !body.trim() : kind === 'changes' ? !body.trim() && lineNotes === 0 : false} onClick={submit}>{t('blocks.reviewSend')}</Button>
            </div>
            <SendError error={sendError} />
          </div>
        </Collapse>
      </div>
      {view === 'chat' && block.session ? (
        <div className="-mx-4 -mb-4 min-h-0 flex-1"><ChatView sessionId={block.session} composer={waiting} /></div>
      ) : (
        <>
          {!waiting && approval && <CommentItem c={approval} />}
          {general.length > 0 && <Thread comments={general} />}
          <div className="space-y-2">
            {files.map((f) => (
              <ReviewFile key={f.path} file={f} comments={change.comments.filter((c) => c.file === f.path)} editable={waiting} viewed={viewed.includes(f.path)} onViewed={() => toggleViewed(f.path)} review={review} />
            ))}
          </div>
        </>
      )}
    </div>
  )
}
