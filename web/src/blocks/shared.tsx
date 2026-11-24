import { useState, type ReactNode } from 'react'
import { ago } from '@/api/clock'
import { ROUTES, type Command, type Comment, type EvidenceFile, type Person } from '@/api/contract'
import { errorText } from '@/api/store'
import { cn } from '@/ui/cn'
import { Spinner } from '@/ui/display'
import { PersonAvatar, personName } from '@/ui/domain'
import { Icon } from '@/ui/icon'
import { MarkdownPreview } from '@/ui/markdownProse'
import { Collapse, motion, tween } from '@/ui/motion'
import { ChatView } from '@/chat/ChatView'
import { useBlockHost } from './context'

export function RichText({ text }: { text: string }) {
  return <MarkdownPreview text={text} className="text-base [&_p]:my-0.5" />
}

export function Author({ person, size = 22 }: { person: Person; size?: number }) {
  return <PersonAvatar person={person} size={size} />
}

export function CommentItem({ c, compact }: { c: Comment; compact?: boolean }) {
  if (c.kind === 'approved') {
    return (
      <div className="flex items-center gap-2.5 py-1 text-base">
        <span className="flex size-[22px] items-center justify-center rounded-full bg-success text-white"><Icon name="check" size={13} strokeWidth={2.4} /></span>
        <span className="font-medium text-fg">{personName(c.author)}</span>
        <span className="text-muted">aprovou estas mudanças</span>
        <span className="ml-auto text-xs tabular-nums text-faint">{ago(c.at)}</span>
      </div>
    )
  }
  return (
    <div className={cn('flex gap-3', compact ? 'py-1.5' : 'py-2')}>
      <Author person={c.author} />
      <div className="min-w-0 flex-1">
        <div className="mb-0.5 flex items-center gap-2 text-sm">
          <span className="font-medium text-fg">{personName(c.author)}</span>
          {c.file && !compact && <span className="truncate font-mono text-xs text-muted">{c.file}:{c.line}</span>}
          <span className="ml-auto shrink-0 text-xs tabular-nums text-faint">{ago(c.at)}</span>
        </div>
        <RichText text={c.body} />
      </div>
    </div>
  )
}

export function Thread({ comments }: { comments: Comment[] }) {
  return <div className="divide-y">{comments.map((c) => <div key={c.id} className="py-1"><CommentItem c={c} /></div>)}</div>
}

export function Muted({ children }: { children: ReactNode }) {
  return <p className="text-base text-muted">{children}</p>
}

export function Loading({ error }: { error?: string | null }) {
  if (error) return <p role="alert" className="flex items-center gap-1.5 text-base text-danger-fg"><Icon name="alert" size={15} />{error}</p>
  return <div className="flex justify-center py-6"><Spinner /></div>
}

const commandTone = (st: Command['state']) => cn('flex size-[18px] shrink-0 items-center justify-center rounded-full', st === 'done' && 'bg-success text-white', st === 'failed' && 'bg-danger text-white', st === 'running' && 'bg-info-soft text-info', st === 'todo' && 'bg-inset text-faint')

function CommandLog({ log }: { log: string }) {
  return (
    <pre className="mx-1.5 mb-2 mt-1 overflow-x-auto rounded-control bg-code p-3 font-mono text-xs leading-5 text-code-fg">
      {log.split('\n').map((l, k) => (
        <span key={k} className={cn('block', /✗|error|fail$|Received/.test(l) && !/0 fail/.test(l) && 'text-code-err', /✓|pass$|No issues|0 errors|successfully/.test(l) && 'text-code-ok', l.startsWith('$') && 'text-white')}>{l || ' '}</span>
      ))}
    </pre>
  )
}

const IMAGE = /\.(png|jpe?g|gif|webp)$/i
const VIDEO = /\.(webm|mp4)$/i

function Evidence({ file }: { file: EvidenceFile }) {
  const url = ROUTES.attachment(file.id)
  if (IMAGE.test(file.name)) return <a href={url} target="_blank" rel="noreferrer"><img src={url} alt={file.name} className="max-h-[360px] rounded-control border" /></a>
  if (VIDEO.test(file.name)) return <video src={url} controls preload="metadata" className="max-h-[360px] rounded-control border" />
  return <a href={url} target="_blank" rel="noreferrer" className="flex items-center gap-1.5 font-mono text-sm text-fg2 hover:text-fg"><Icon name="file" size={14} />{file.name}</a>
}

function CommandBody({ command }: { command: Command }) {
  const files = command.files ?? []
  if (!command.session && files.length > 0) {
    return (
      <div>
        {command.log && <CommandLog log={command.log} />}
        <div className="mx-1.5 mb-2 mt-1 flex flex-wrap items-start gap-2">{files.map((f) => <Evidence key={f.id} file={f} />)}</div>
      </div>
    )
  }
  if (!command.session) return <CommandLog log={command.log} />
  return (
    <div className="mx-1.5 mb-2 mt-1 h-[440px] overflow-hidden rounded-control border bg-bg">
      <ChatView sessionId={command.session} composer={false} />
    </div>
  )
}

const opened = (commands: Command[]) => new Set(commands.filter((c) => c.state === 'failed' || (c.state === 'running' && c.session)).map((c) => c.name))

export function Commands({ commands }: { commands: Command[] }) {
  const [open, setOpen] = useState<Set<string>>(() => opened(commands))
  const toggle = (name: string) =>
    setOpen((now) => {
      const next = new Set(now)
      if (!next.delete(name)) next.add(name)
      return next
    })
  return (
    <ul className="space-y-0.5">
      {commands.map((c) => {
        const can = c.state !== 'todo'
        const isOpen = open.has(c.name) && can
        return (
          <li key={c.name}>
            <button disabled={!can} aria-expanded={isOpen} onClick={() => toggle(c.name)} className={cn('flex w-full items-center gap-3 rounded-control-sm px-1.5 py-1.5 text-left transition-colors', can && 'hover:bg-hover')}>
              <span className={commandTone(c.state)}>
                {c.state === 'done' && <Icon name="check" size={11} strokeWidth={2.4} />}
                {c.state === 'failed' && <Icon name="close" size={11} strokeWidth={2.4} />}
                {c.state === 'running' && <Spinner size={11} className="text-info" />}
              </span>
              {c.session && <Icon name="chat" size={14} className="shrink-0 text-muted" />}
              <span className={cn('min-w-0 flex-1 font-mono text-sm', c.state === 'todo' ? 'text-muted' : 'text-fg2')}>{c.name}</span>
              {c.seconds !== null && <span className="text-xs tabular-nums text-faint">{c.seconds}s</span>}
              {can && (
                <motion.span initial={false} animate={{ rotate: isOpen ? 0 : -90 }} transition={tween} className="flex text-faint">
                  <Icon name="down" size={14} />
                </motion.span>
              )}
            </button>
            <Collapse open={isOpen}>
              <CommandBody command={c} />
            </Collapse>
          </li>
        )
      })}
    </ul>
  )
}

export function useSend() {
  const host = useBlockHost()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const send = (action: string, payload: unknown) => {
    setBusy(true)
    setError(null)
    return host
      .send(action, payload)
      .catch((e: unknown) => setError(errorText(e)))
      .finally(() => setBusy(false))
  }
  return { busy, error, send }
}

export function SendError({ error }: { error: string | null }) {
  if (!error) return null
  return <p role="alert" className="mt-2 flex items-center gap-1.5 text-sm text-danger-fg"><Icon name="alert" size={14} />{error}</p>
}
