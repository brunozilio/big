import { createContext, useContext, useEffect, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { now } from '@/api/clock'
import type { Agent, SessionEvent, ThinkingStep, ToolCall } from '@/api/contract'
import { AgentAvatar } from '@/ui/agentAvatar'
import { cn, plural } from '@/ui/cn'
import { CodeBlock, DiffView, Spinner } from '@/ui/display'
import { PersonAvatar, personName, ProviderLogo } from '@/ui/domain'
import { Icon, type IconName } from '@/ui/icon'
import { AnimatePresence, Collapse, IconSwap, motion, springSnap, stagger, tween } from '@/ui/motion'
import { Tooltip } from '@/ui/overlay'

export type Ev<T extends SessionEvent['type']> = Extract<SessionEvent, { type: T }>

export const AgentsContext = createContext<Agent[]>([])

export function useAgent(id: string | undefined): Agent | null {
  const agents = useContext(AgentsContext)
  return agents.find((a) => a.id === id) ?? null
}

export function AgentMark({ size = 18, spinning, className, agent }: { size?: number; spinning?: boolean; className?: string; agent?: string }) {
  const found = useAgent(agent)
  const look = found?.avatar?.kind === 'agent' ? found.avatar : null
  if (found?.avatar?.kind === 'image') return <img src={found.avatar.url} alt={found.name} width={size + 2} height={size + 2} className={cn('shrink-0 rounded-full', className)} />
  return <AgentAvatar look={look} seed={agent ?? 'agente'} name={found?.name} size={size + 2} working={spinning} className={className} />
}

export function UserMessage({ event }: { event: Ev<'user'> }) {
  const [expanded, setExpanded] = useState(false)
  const long = event.text.length > 320
  return (
    <div className="group flex flex-col items-end gap-2">
      <div className="flex items-center gap-2 text-base text-muted">
        {personName(event.author)}
        <PersonAvatar person={event.author} size={20} />
      </div>
      {event.attachments > 0 && (
        <span className="inline-flex h-8 items-center gap-1.5 rounded-full bg-surface px-3 text-base text-fg2 shadow-card">
          <Icon name="attach" size={15} className="text-muted" />
          <span className="tabular-nums">{event.attachments}</span> {plural(event.attachments, 'anexo', 'anexos')}
        </span>
      )}
      <div className="max-w-[85%] whitespace-pre-wrap rounded-overlay bg-primary px-4 py-3 text-md leading-6 text-primary-fg shadow-[0_1px_2px_rgb(0_0_0/0.12)] sm:max-w-[75%]">
        {long && !expanded ? `${event.text.slice(0, 320)}…` : event.text}
        {long && (
          <button className="ml-2 text-sm underline opacity-70" onClick={() => setExpanded(!expanded)}>
            {expanded ? 'mostrar menos' : 'mostrar mais'}
          </button>
        )}
      </div>
    </div>
  )
}

export function AssistantMessage({ event }: { event: Ev<'assistant'> }) {
  const agent = useAgent(event.agent)
  return (
    <div className="group flex gap-3">
      <span className="mt-[3px]">
        <Tooltip content={agent?.name ?? 'Agente'}><span className="flex"><AgentMark size={18} agent={event.agent} /></span></Tooltip>
      </span>
      <div className="min-w-0 flex-1">
        <div className="prose-chat">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{event.text}</ReactMarkdown>
        </div>
      </div>
    </div>
  )
}

export function ThinkingSteps({ event }: { event: Ev<'thinking'> }) {
  const [open, setOpen] = useState(event.live)
  const shown = open || event.live
  const written = visibleSteps(event.steps)
  return (
    <div>
      <motion.button layout whileTap={{ scale: 0.97 }} transition={springSnap} onClick={() => setOpen(!open)} className="inline-flex h-8 items-center gap-2 rounded-full bg-surface pl-2.5 pr-3 text-base shadow-card transition-colors hover:bg-hover">
        <IconSwap k={event.live ? 'l' : 'd'}>
          {event.live ? (
            <Spinner size={15} className="text-fg2" />
          ) : (
            <span className="flex size-[15px] items-center justify-center rounded-full bg-fg2 text-bg">
              <Icon name="check" size={10} strokeWidth={3} />
            </span>
          )}
        </IconSwap>
        <span className={cn(event.live ? 'text-shimmer font-medium' : 'text-fg2')}>
          {event.live ? 'Pensando…' : (
            <>
              Pensou <span className="tabular-nums">{event.seconds}s</span>
            </>
          )}
        </span>
        <motion.span animate={{ rotate: shown ? 180 : 0 }} transition={tween} className="flex text-muted">
          <Icon name="down" size={14} />
        </motion.span>
      </motion.button>
      <Collapse open={shown && written.length > 0}>
        <StepList steps={written} live={event.live} />
      </Collapse>
    </div>
  )
}

export const visibleSteps = (steps: ThinkingStep[]) => steps.filter((s) => s.text.trim().length > 0)

function StepList({ steps, live }: { steps: ThinkingStep[]; live: boolean }) {
  return (
    <ol className="relative ml-[13px] mt-3 space-y-3 pb-1 pl-5">
      <motion.span className="absolute left-0 top-1.5 w-px origin-top bg-line-strong" initial={{ scaleY: 0 }} animate={{ scaleY: 1 }} style={{ bottom: 6 }} transition={tween} />
      <AnimatePresence initial={false}>
        {steps.map((s, i) => (
          <motion.li key={i} initial={{ opacity: 0, x: -6, filter: 'blur(2px)' }} animate={{ opacity: 1, x: 0, filter: 'blur(0px)' }} transition={tween} className="relative text-md leading-5 text-muted">
            <span className="absolute -left-[24px] top-[3px] flex size-[9px] items-center justify-center rounded-full bg-bg">
              {s.done || !live ? <motion.span initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={tween} className="size-[7px] rounded-full bg-line-strong" /> : <Spinner size={11} className="text-fg2" />}
            </span>
            <span className={cn(!s.done && live && 'text-shimmer')}>{s.text}</span>
          </motion.li>
        ))}
      </AnimatePresence>
    </ol>
  )
}

const toolIcons: Record<ToolCall['kind'], IconName> = { command: 'terminal', edit: 'edit', read: 'read', search: 'search', call: 'plug', subagent: 'users' }

const toolVerb: Record<ToolCall['kind'], string> = { command: 'Rodando comando', edit: 'Editando arquivo', read: 'Lendo arquivo', search: 'Buscando', call: 'Chamando conexão', subagent: 'Delegando' }

function summarize(calls: ToolCall[]): string {
  const edits = calls.filter((c) => c.kind === 'edit').length
  const commands = calls.filter((c) => c.kind === 'command').length
  const parts = [`${calls.length} ${plural(calls.length, 'ferramenta', 'ferramentas')}`]
  if (edits) parts.push(`editou ${edits} ${plural(edits, 'arquivo', 'arquivos')}`)
  if (commands) parts.push(`rodou ${commands} ${plural(commands, 'comando', 'comandos')}`)
  return parts.join(', ')
}

export function duration(seconds: number) {
  if (seconds < 60) return `${seconds}s`
  return `${Math.floor(seconds / 60)}m ${seconds % 60}s`
}

export function ToolRunSummary({ event }: { event: Ev<'tools'> }) {
  const [open, setOpen] = useState(false)
  const shown = open || event.live
  const current = event.calls.find((c) => c.state === 'running')
  return (
    <div>
      <button onClick={() => setOpen(!open)} className="group flex w-full items-center gap-3 text-left">
        <AgentMark size={20} spinning={event.live} agent={event.agent} />
        <span className="min-w-0 truncate text-body text-fg2">{event.live && current ? <span className="text-shimmer">{toolVerb[current.kind]}…</span> : summarize(event.calls)}</span>
        <motion.span animate={{ rotate: shown ? 180 : 0 }} transition={tween} className="flex text-muted transition-colors group-hover:text-fg">
          <Icon name="down" size={16} />
        </motion.span>
        <span className="ml-auto text-md tabular-nums text-muted">{duration(event.seconds)}</span>
      </button>
      <Collapse open={shown}>
        <ToolTimeline calls={event.calls} done={!event.live} />
      </Collapse>
    </div>
  )
}

export function ToolTimeline({ calls, done }: { calls: ToolCall[]; done?: boolean }) {
  return (
    <ol className="relative mt-4 pl-[2px]">
      <AnimatePresence initial={false}>
        {calls.map((c, i) => (
          <motion.div key={c.id} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ ...tween, delay: stagger(i) }}>
            <ToolCallItem call={c} last={!done && i === calls.length - 1} />
          </motion.div>
        ))}
      </AnimatePresence>
      {done && (
        <motion.li initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={tween} className="flex items-center gap-3.5 pt-1 text-body text-fg2">
          <span className="flex size-[22px] items-center justify-center text-muted">
            <Icon name="checkCircle" size={22} />
          </span>
          Concluído
        </motion.li>
      )}
    </ol>
  )
}

export function ToolCallItem({ call, last }: { call: ToolCall; last?: boolean }) {
  const [open, setOpen] = useState(call.state === 'error')
  const hasDetail = call.input !== undefined || call.output !== undefined || call.diff !== undefined || (call.steps?.length ?? 0) > 0
  return (
    <li className="relative list-none pb-5">
      {!last && <span className="absolute bottom-0 left-[11px] top-[28px] w-px bg-line-strong" />}
      <button disabled={!hasDetail} onClick={() => setOpen(!open)} className="group flex w-full items-center gap-3.5 text-left disabled:cursor-default">
        <span className={cn('relative flex size-[22px] shrink-0 items-center justify-center text-muted', call.state === 'error' && 'text-danger-fg')}>
          <IconSwap k={call.state === 'running' ? 'r' : call.logo ? 'c' : call.state === 'error' ? 'e' : 'i'}>
            {call.state === 'running' ? <Spinner size={18} className="text-fg2" /> : call.logo ? <ProviderLogo logo={call.logo} size={18} /> : <Icon name={call.state === 'error' ? 'alert' : toolIcons[call.kind]} size={21} />}
          </IconSwap>
        </span>
        <span title={call.title} className={cn('min-w-0 flex-1 truncate text-body text-fg2 transition-colors', hasDetail && 'group-hover:text-fg', call.state === 'error' && 'text-danger-fg', call.state === 'running' && 'text-shimmer')}>{call.title}</span>
        <span className="hidden text-sm tabular-nums text-muted opacity-0 transition-opacity group-hover:opacity-100 sm:inline">{call.durationMs < 1000 ? `${call.durationMs}ms` : `${(call.durationMs / 1000).toFixed(1)}s`}</span>
        <span className={cn('inline-flex h-7 items-center rounded-control-sm bg-inset px-2.5 text-base text-muted', call.state === 'error' && 'bg-danger-soft text-danger-fg')}>{call.tag}</span>
      </button>
      <Collapse open={open}>
        <div className="ml-[36px]">
          <ToolCallDetail call={call} />
        </div>
      </Collapse>
    </li>
  )
}

function SubagentSteps({ steps }: { steps: string[] }) {
  return (
    <ol aria-label="Passos do subagente" className="space-y-1 border-l border-line-strong pl-3 text-sm text-muted">
      {steps.map((step, i) => (
        <li key={i} className="truncate" title={step}>{step}</li>
      ))}
    </ol>
  )
}

function ToolCallDetail({ call }: { call: ToolCall }) {
  if (call.steps && call.steps.length > 0 && call.output === undefined) {
    return <div className="mt-3"><SubagentSteps steps={call.steps} /></div>
  }
  const output = call.output ?? ''
  const isJson = output.trim().startsWith('{') || output.trim().startsWith('[')
  return (
    <div className="mt-3 space-y-2.5">
      {call.input !== undefined && <CodeBlock language="json" code={JSON.stringify(call.input, null, 2)} />}
      {call.diff && <DiffView diff={call.diff} />}
      {output && <CodeBlock language={isJson ? 'json' : call.kind === 'command' ? 'Terminal' : 'Saída'} code={isJson ? prettyJson(output) : output} className={cn(call.state === 'error' && 'ring-1 ring-danger/30')} />}
    </div>
  )
}

function prettyJson(s: string) {
  try {
    return JSON.stringify(JSON.parse(s), null, 2)
  } catch {
    return s
  }
}

export function StatusLine({ event }: { event: Ev<'status'> }) {
  if (event.status !== 'error') return <p className="text-sm text-muted">{event.text}</p>
  return (
    <p className="flex items-center gap-2 text-sm text-muted">
      <Icon name="stop" size={14} />
      {event.text}
    </p>
  )
}

export function TurnHeader({ startedAt, endedAt, live }: { startedAt: string; endedAt: string; live: boolean }) {
  const [tick, setTick] = useState(() => now())
  useEffect(() => {
    if (!live) return
    const t = setInterval(() => setTick(now()), 1000)
    return () => clearInterval(t)
  }, [live])
  const end = live ? tick : new Date(endedAt).getTime()
  const secs = Math.max(1, Math.round((end - new Date(startedAt).getTime()) / 1000))
  return (
    <span className="text-md">
      {live ? (
        <span className="text-shimmer">
          Trabalhando há <span className="tabular-nums">{duration(secs)}</span>
        </span>
      ) : (
        <span className="text-muted">
          Trabalhou por <span className="tabular-nums">{duration(secs)}</span>
        </span>
      )}
    </span>
  )
}
