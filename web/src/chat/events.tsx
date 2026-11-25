import { createContext, useContext, useEffect, useState } from 'react'
import { now } from '@/api/clock'
import type { Agent, SessionEvent, ThinkingStep, ToolCall } from '@/api/contract'
import { AgentAvatar } from '@/ui/agentAvatar'
import { ChatMarkdown } from '@/ui/chatMarkdown'
import { cn } from '@/ui/cn'
import { plural, useT } from '@/i18n'
import { CodeBlock, DiffView, Spinner } from '@/ui/display'
import { PersonAvatar, personName, ProviderLogo } from '@/ui/domain'
import { Icon, type IconName } from '@/ui/icon'
import { AnimatePresence, Collapse, IconSwap, motion, tween } from '@/ui/motion'
import { Tooltip } from '@/ui/overlay'
import { JobBadge, LiveDot, StopJobButton } from './jobs'
import { NoticeLine, NoticeMessage } from './Notices'
import { parseNotices, statusNotice } from './noticeText'

const rail = 'flex h-6 w-[22px] shrink-0 items-center justify-center'

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
  const notices = parseNotices(event.text)
  if (notices) return <NoticeMessage notices={notices} />
  return <PersonMessage event={event} />
}

function Sender({ event }: { event: Ev<'user'> }) {
  const agent = useAgent(event.agent)
  if (!event.author.agent) return <>{personName(event.author)}<PersonAvatar person={event.author} size={20} /></>
  return <>{agent?.name ?? event.author.name}<AgentMark size={18} agent={event.agent} /></>
}

function PersonMessage({ event }: { event: Ev<'user'> }) {
  const t = useT()
  return (
    <div className="group flex flex-col items-end gap-2">
      <div className="flex items-center gap-2 text-base text-muted">
        <Sender event={event} />
      </div>
      {event.attachments > 0 && (
        <span className="inline-flex h-8 items-center gap-1.5 rounded-full bg-surface px-3 text-base text-fg2 shadow-card">
          <Icon name="attach" size={15} className="text-muted" />
          <span className="tabular-nums">{event.attachments}</span> {t(`plan.attachment_${plural(event.attachments)}`)}
        </span>
      )}
      <div className="prose-chat prose-user max-w-[85%] rounded-overlay bg-primary px-4 py-3 text-md leading-6 text-primary-fg shadow-[0_1px_2px_rgb(0_0_0/0.12)] sm:max-w-[75%]">
        <ChatMarkdown text={event.text} />
      </div>
    </div>
  )
}

export function AssistantMessage({ event }: { event: Ev<'assistant'> }) {
  const t = useT()
  const agent = useAgent(event.agent)
  return (
    <div className="group flex gap-3">
      <span className="mt-[3px]">
        <Tooltip content={agent?.name ?? t('plan.agent')}><span className="flex"><AgentMark size={18} agent={event.agent} /></span></Tooltip>
      </span>
      <div className="min-w-0 flex-1">
        <div className="prose-chat">
          <ChatMarkdown text={event.text} />
        </div>
      </div>
    </div>
  )
}

export function thoughtText(steps: ThinkingStep[], live: boolean): string {
  const written = visibleSteps(steps).map((s) => s.text.trim())
  if (!live) return written.join('\n\n')
  return written[written.length - 1] ?? ''
}

export function ThinkingSteps({ event }: { event: Ev<'thinking'> }) {
  const t = useT()
  const text = thoughtText(event.steps, event.live)
  return (
    <div className="flex gap-3.5">
      <motion.span animate={event.live ? { opacity: [0.45, 1, 0.45] } : { opacity: 1 }} transition={event.live ? { duration: 1.6, repeat: Infinity, ease: 'easeInOut' } : tween} className={cn(rail, event.live ? 'text-warning-fg' : 'text-muted')}>
        <Icon name="idea" size={18} />
      </motion.span>
      <div className="min-w-0 flex-1 text-md leading-6 text-muted">
        {text ? (
          <div className={cn('prose-chat prose-thought break-words', event.live && 'thought-tail')}>
            <ChatMarkdown key={event.live ? `live-${visibleSteps(event.steps).length}` : 'done'} text={text} />
          </div>
        ) : (
          <p className="text-shimmer">{t('plan.thinking')}</p>
        )}
      </div>
    </div>
  )
}

export const visibleSteps = (steps: ThinkingStep[]) => steps.filter((s) => s.text.trim().length > 0)

export const emptyThinking = (e: SessionEvent, running = true) => e.type === 'thinking' && (!e.live || !running) && visibleSteps(e.steps).length === 0

const toolIcons: Record<ToolCall['kind'], IconName> = { command: 'terminal', edit: 'edit', read: 'read', search: 'search', call: 'plug', subagent: 'users' }

export function duration(seconds: number) {
  if (seconds < 60) return `${seconds}s`
  return `${Math.floor(seconds / 60)}m ${seconds % 60}s`
}

export function ToolRunSummary({ event }: { event: Ev<'tools'> }) {
  const last = event.calls.length - 1
  return (
    <ol className="relative">
      <AnimatePresence initial={false}>
        {event.calls.map((c, i) => (
          <motion.div key={c.id} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={tween}>
            <ToolCallItem call={c} last={i === last} single={i === last} />
          </motion.div>
        ))}
      </AnimatePresence>
    </ol>
  )
}

export function ToolCallItem({ call, last, single }: { call: ToolCall; last?: boolean; single?: boolean }) {
  const [open, setOpen] = useState(call.state === 'error')
  const hasDetail = call.input != null || call.output !== undefined || call.diff !== undefined || (call.steps?.length ?? 0) > 0
  return (
    <li className={cn('relative list-none', !single && 'pb-5')}>
      {!last && <span className="absolute bottom-0 left-[11px] top-[28px] w-px bg-line-strong" />}
      <div className="flex items-center gap-2">
      <button disabled={!hasDetail} onClick={() => setOpen(!open)} className="group flex min-w-0 flex-1 items-center gap-3.5 text-left disabled:cursor-default">
        <span className={cn(rail, 'relative text-muted', call.state === 'error' && 'text-danger-fg')}>
          <IconSwap k={call.state === 'running' ? 'r' : call.logo ? 'c' : call.state === 'error' ? 'e' : 'i'}>
            {call.state === 'running' ? <Spinner size={18} className="text-fg2" /> : call.logo ? <ProviderLogo logo={call.logo} size={18} /> : <Icon name={call.state === 'error' ? 'alert' : toolIcons[call.kind]} size={21} />}
          </IconSwap>
          {call.jobRunning && <LiveDot className="absolute -right-0.5 -top-0.5 ring-2 ring-bg rounded-full" />}
        </span>
        <span title={call.title} className={cn('min-w-0 flex-1 truncate text-body text-fg2 transition-colors', hasDetail && 'group-hover:text-fg', call.state === 'error' && 'text-danger-fg', call.state === 'running' && 'text-shimmer')}>{call.title}</span>
        <span className="hidden text-sm tabular-nums text-muted opacity-0 transition-opacity group-hover:opacity-100 sm:inline">{call.durationMs < 1000 ? `${call.durationMs}ms` : `${(call.durationMs / 1000).toFixed(1)}s`}</span>
        {call.job ? <JobBadge running={!!call.jobRunning} /> : scriptOf(call.input) === null && <span className={cn('inline-flex h-7 items-center rounded-control-sm bg-inset px-2.5 text-base text-muted', call.state === 'error' && 'bg-danger-soft text-danger-fg')}>{call.tag}</span>}
      </button>
      {call.job && call.jobStoppable && <StopJobButton job={call.job} />}
      </div>
      <Collapse open={open}>
        <div className="ml-[36px]">
          <ToolCallDetail call={call} />
        </div>
      </Collapse>
    </li>
  )
}

function SubagentSteps({ steps }: { steps: string[] }) {
  const t = useT()
  return (
    <ol aria-label={t('plan.subagentSteps')} className="space-y-1 border-l border-line-strong pl-3 text-sm text-muted">
      {steps.map((step, i) => (
        <li key={i} className="truncate" title={step}>{step}</li>
      ))}
    </ol>
  )
}

function scriptOf(input: unknown) {
  if (typeof input !== 'object' || input === null || !('code' in input)) return null
  return typeof input.code === 'string' ? input.code : null
}

function ToolCallInput({ input }: { input: unknown }) {
  const script = scriptOf(input)
  if (script !== null) return <CodeBlock language="Script" code={script} />
  return <CodeBlock language="json" code={JSON.stringify(input, null, 2)} />
}

function ToolCallDetail({ call }: { call: ToolCall }) {
  const t = useT()
  const output = call.output ?? ''
  const isJson = output.trim().startsWith('{') || output.trim().startsWith('[')
  return (
    <div className="mt-3 space-y-2.5">
      {call.input != null && <ToolCallInput input={call.input} />}
      {scriptOf(call.input) === null && call.steps && call.steps.length > 0 && <SubagentSteps steps={call.steps} />}
      {call.diff && <DiffView diff={call.diff} />}
      {output && <CodeBlock language={isJson ? 'json' : call.kind === 'command' ? 'Terminal' : t('plan.output')} code={isJson ? prettyJson(output) : output} className={cn(call.state === 'error' && 'ring-1 ring-danger/30')} />}
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

export function backgroundNotice(text: string): { command: string; ok: boolean; code: number | null } | null {
  const found = /^\[background [^\]]+\] `([\s\S]+)` (finished|failed|was killed|was stopped by the user|stopped)(?: \(exit code (-?\d+)\))?\.?$/.exec(text.trim())
  if (!found) return null
  const code = found[3] === undefined ? null : Number(found[3])
  const command = found[1].replace(/'[^']*\/worktrees\/[^/']+\/([^']*)'/g, '$1').replace(/\S*\/worktrees\/[^/\s]+\/(\S*)/g, '$1').replace(/\s+/g, ' ').trim()
  return { command, ok: found[2] === 'finished' && code === 0, code }
}

function BackgroundDone({ notice }: { notice: NonNullable<ReturnType<typeof backgroundNotice>> }) {
  const t = useT()
  return (
    <p className="flex min-w-0 items-center gap-3.5 text-sm text-muted">
      <span className={rail}>
        <span className={cn('flex size-4 items-center justify-center rounded-full', notice.ok ? 'bg-success text-white' : 'bg-danger text-white')}>
          <Icon name={notice.ok ? 'check' : 'close'} size={10} strokeWidth={2.6} />
        </span>
      </span>
      <span className="shrink-0">{notice.ok ? t('plan.background_done') : notice.code === null ? t('plan.background_stopped') : t('plan.background_failed', { code: notice.code })}</span>
      <code className="min-w-0 truncate rounded-[5px] bg-inset px-1.5 py-0.5 font-mono text-xs text-fg2" title={notice.command}>{notice.command}</code>
    </p>
  )
}

export function observerReport(text: string): { observer: string; message: string } | null {
  const found = /^\[observer ([^\]]+)\] Your background observer \([^)]*\) sent a report:\n([\s\S]*?)(?:\n\nThis is one-way advice from an agent watching[\s\S]*)?$/.exec(text.trim())
  if (!found) return null
  return { observer: found[1], message: found[2].trim() }
}

function ObserverReport({ report }: { report: NonNullable<ReturnType<typeof observerReport>> }) {
  const t = useT()
  return (
    <div className="flex gap-3.5">
      <Tooltip content={`${t('plan.notice_observer')}: ${report.observer}`}>
        <span className={cn(rail, 'text-muted')}>
          <Icon name="eye" size={16} />
        </span>
      </Tooltip>
      <div className="min-w-0 flex-1 text-md leading-6 text-muted">
        <div className="prose-chat prose-thought break-words">
          <ChatMarkdown text={report.message} />
        </div>
      </div>
    </div>
  )
}

export function StatusLine({ event }: { event: Ev<'status'> }) {
  const report = observerReport(event.text)
  if (report) return <ObserverReport report={report} />
  const notice = backgroundNotice(event.text)
  if (notice) return <BackgroundDone notice={notice} />
  const sent = statusNotice(event.text)
  if (sent) return <NoticeLine notice={sent} />
  if (event.status !== 'error') return <p className="pl-9 text-sm text-muted">{event.text}</p>
  return (
    <p className="flex items-center gap-3.5 text-sm text-muted">
      <span className={rail}>
        <Icon name="stop" size={14} />
      </span>
      {event.text}
    </p>
  )
}

export function TurnHeader({ startedAt, endedAt, live }: { startedAt: string; endedAt: string; live: boolean }) {
  const t = useT()
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
          {t('plan.workingFor')} <span className="tabular-nums">{duration(secs)}</span>
        </span>
      ) : (
        <span className="text-muted">
          {t('plan.workedFor')} <span className="tabular-nums">{duration(secs)}</span>
        </span>
      )}
    </span>
  )
}
