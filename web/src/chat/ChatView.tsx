import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { api } from '@/api'
import type { FileRef, SessionEvent } from '@/api/contract'
import { useSession } from '@/api/store'
import { OpenFileContext } from '@/ui/chatMarkdown'
import { cn } from '@/ui/cn'
import { plural, useT, type T } from '@/i18n'
import { EmptyState, Spinner } from '@/ui/display'
import { Icon } from '@/ui/icon'
import { AnimatePresence, Collapse, motion, tween, tweenClose } from '@/ui/motion'
import { AskBlock, PermissionBlock, type Respond } from './actionBlocks'
import { ArtifactCard } from './ArtifactCard'
import { BackgroundWork, hasWork } from './BackgroundWork'
import { JobsContext } from './jobs'
import { Composer } from './Composer'
import { useRememberedModel } from './remembered'
import { PlanBlock } from './PlanCard'
import { SideAnswers, useSideQuestions } from './SideQuestions'
import { AgentMark, AgentsContext, emptyThinking, AssistantMessage, StatusLine, ThinkingSteps, ToolRunSummary, TurnHeader, UserMessage } from './events'

interface Turn {
  user: Extract<SessionEvent, { type: 'user' }> | null
  events: SessionEvent[]
}

export function toTurns(events: SessionEvent[]): Turn[] {
  const turns: Turn[] = []
  for (const e of events) {
    if (e.type === 'user') turns.push({ user: e, events: [] })
    else if (turns.length === 0) turns.push({ user: null, events: [e] })
    else turns[turns.length - 1].events.push(e)
  }
  return turns
}

const human = (e: SessionEvent) => e.type === 'assistant' || e.type === 'ask' || e.type === 'permission' || e.type === 'plan' || e.type === 'artifact'

const enter = { initial: { opacity: 0, y: 8, filter: 'blur(2px)' }, animate: { opacity: 1, y: 0, filter: 'blur(0px)' }, transition: tween }
const settled = { initial: false as const, animate: enter.animate }

function useHistoricalIds(sessionId: string, events: SessionEvent[] | undefined) {
  const seen = useRef<{ sessionId: string; ids: Set<string> } | null>(null)
  if (events && seen.current?.sessionId !== sessionId) seen.current = { sessionId, ids: new Set(events.map((e) => e.id)) }
  const ids = seen.current?.ids
  return (id: string | undefined) => (id && ids?.has(id) ? settled : enter)
}

export function ChatView({ sessionId, composer, dock, hidePlans }: { sessionId: string; composer: boolean; dock?: ReactNode; hidePlans?: boolean }) {
  const t = useT()
  const { session, error } = useSession(sessionId)
  const enterFor = useHistoricalIds(sessionId, session?.events)
  const scroller = useRef<HTMLDivElement>(null)
  const [atBottom, setAtBottom] = useState(true)
  const [sendError, setSendError] = useState<string | null>(null)
  const [model, setModel] = useRememberedModel()
  const side = useSideQuestions(sessionId, (text) => api().side(sessionId, { text, model: model ?? session?.model ?? null }))
  const eventCount = session?.events.length ?? 0
  const lastSignature = session ? JSON.stringify(session.events[session.events.length - 1] ?? null).length : 0

  const placed = useRef<string | null>(null)
  const loaded = !!session

  useLayoutEffect(() => {
    const el = scroller.current
    if (!el || !loaded || placed.current === sessionId) return
    placed.current = sessionId
    el.scrollTop = el.scrollHeight
    requestAnimationFrame(() => {
      el.scrollTop = el.scrollHeight
    })
  }, [loaded, sessionId])

  useEffect(() => {
    const el = scroller.current
    if (el && atBottom && placed.current === sessionId) el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' })
  }, [eventCount, lastSignature, atBottom, sessionId, side.asked])

  if (error && !session) return <EmptyState title={t('plan.sessionNotFound')} text={error} />
  if (!session) return <div className="flex h-full items-center justify-center"><Spinner /></div>

  const running = session.status === 'running'
  const turns = toTurns(hidePlans ? session.events.filter((e) => e.type !== 'plan') : withoutRepeatedPlans(session.events))
  const respond: Respond = (event, response) => api().respond(session.id, event, response)
  const openFile = (file: FileRef) => void api().openFile(session.id, file).catch(() => undefined)
  const stopJob = (job: string) => api().stopJob(session.id, job)

  return (
    <AgentsContext.Provider value={session.agents}>
    <JobsContext.Provider value={stopJob}>
    <OpenFileContext.Provider value={openFile}>
      <div className="flex h-full min-h-0 flex-col">
        <div ref={scroller} onScroll={(e) => {
          const el = e.currentTarget
          setAtBottom(el.scrollHeight - el.scrollTop - el.clientHeight < 120)
        }} className="scrollbar-thin relative min-h-0 flex-1 overflow-y-auto">
          <div role="log" aria-live="polite" aria-label={t('plan.messages')} className="mx-auto w-full max-w-[760px] px-5 pb-10 pt-8 sm:px-8">
            {turns.map((turn, ti) => {
              const last = ti === turns.length - 1
              const live = last && running
              const started = turn.user?.at ?? turn.events[0]?.at
              const ended = turn.events[turn.events.length - 1]?.at ?? started
              return (
                <section key={turn.user?.id ?? `t${ti}`} className="mb-10">
                  {turn.user && (
                    <motion.div {...enterFor(turn.user.id)} className="mb-8">
                      <UserMessage event={turn.user} />
                    </motion.div>
                  )}
                  <TurnWork live={live} started={started} ended={ended ?? started} events={turn.events} render={(e) => {
                    const node = <EventView event={e} running={running} respond={respond} />
                    return <motion.div key={e.id} data-event={e.type} {...enterFor(e.id)}>{node}</motion.div>
                  }} />
                </section>
              )
            })}
            {hasWork(session.work) && <BackgroundWork work={session.work} />}
            <SideAnswers asked={side.asked} onDismiss={side.dismiss} />
          </div>
          <AnimatePresence>
            {!atBottom && (
              <motion.button initial={{ opacity: 0, y: 8, scale: 0.96 }} animate={{ opacity: 1, y: 0, scale: 1 }} exit={{ opacity: 0, y: 8, scale: 0.96, transition: tweenClose }} transition={tween} onClick={() => scroller.current?.scrollTo({ top: scroller.current.scrollHeight, behavior: 'smooth' })} className="sticky bottom-4 mx-auto flex size-9 items-center justify-center rounded-full bg-surface text-fg2 shadow-pop" aria-label={t('plan.jumpToEnd')}>
                <Icon name="down" size={17} />
              </motion.button>
            )}
          </AnimatePresence>
        </div>
        {(composer || dock) && (
          <div className="relative px-4 pb-4 sm:px-8">
            <div className="pointer-events-none absolute inset-x-0 -top-8 h-8 bg-gradient-to-t from-bg to-transparent" />
            <div className="mx-auto max-w-[760px]">
              {dock && <div data-testid="chat-dock" className={cn('flex flex-wrap items-center gap-x-4 gap-y-2 rounded-card bg-surface px-4 py-3 shadow-card [&>*:first-child]:min-w-0 [&>*:first-child]:flex-1', composer && 'mb-2')}>{dock}</div>}
              {composer && <Composer
                running={running}
                onStop={() => void api().stop(session.id)}
                placeholder={running ? t('plan.composerRunning') : t('plan.composerIdle')}
                suggestion={session.suggestion}
                onAside={(text) => {
                  setAtBottom(true)
                  side.ask(text)
                }}
                onBtw={(text) => {
                  setAtBottom(true)
                  side.ask(text)
                }}
                model={model ?? session.model}
                onModel={setModel}
                onSubmit={(input) => {
                  setAtBottom(true)
                  setSendError(null)
                  api().send(session.id, { text: input.text, model: input.model, effort: input.effort, fast: input.fast, attachments: input.attachments }).catch((e: unknown) => setSendError(e instanceof Error ? e.message : t('plan.sendFailed')))
                }}
              />}
              {sendError && <p role="alert" className="mt-2 px-2 text-sm text-danger-fg">{sendError}</p>}
            </div>
          </div>
        )}
      </div>
    </OpenFileContext.Provider>
    </JobsContext.Provider>
    </AgentsContext.Provider>
  )
}

export function EventView({ event, running = true, respond }: { event: SessionEvent; running?: boolean; respond: Respond }) {
  switch (event.type) {
    case 'user':
      return <UserMessage event={event} />
    case 'assistant':
      return <AssistantMessage event={event} />
    case 'thinking':
      return <ThinkingSteps event={running ? event : { ...event, live: false }} />
    case 'tools':
      return <ToolRunSummary event={event} />
    case 'status':
      return <StatusLine event={event} />
    case 'ask':
      return <AskBlock event={event} respond={respond} />
    case 'permission':
      return <PermissionBlock event={event} respond={respond} />
    case 'plan':
      return <PlanBlock event={event} respond={respond} />
    case 'artifact':
      return <ArtifactCard event={event} />
  }
}

export type Segment = { kind: 'work'; events: SessionEvent[] } | { kind: 'human'; event: SessionEvent }

export function segmentsOf(events: SessionEvent[], live: boolean): Segment[] {
  const segments: Segment[] = []
  for (const e of events) {
    if (e.type === 'permission' && e.resolution !== 'pending') continue
    if (human(e)) {
      segments.push({ kind: 'human', event: e })
      continue
    }
    if (emptyThinking(e, live)) continue
    const last = segments[segments.length - 1]
    if (last?.kind === 'work') last.events.push(e)
    else segments.push({ kind: 'work', events: [e] })
  }
  return segments
}

export function withoutRepeatedPlans(events: SessionEvent[]): SessionEvent[] {
  return events.filter((e, i) => e.type !== 'plan' || !events.slice(i + 1).some((later) => later.type === 'plan' && later.text === e.text))
}

function workLabel(events: SessionEvent[], t: T): string {
  const tools = events.reduce((n, e) => n + (e.type === 'tools' ? e.calls.length : 0), 0)
  const thought = events.some((e) => e.type === 'thinking')
  const only = tools === 1 ? events.flatMap((e) => (e.type === 'tools' ? e.calls : []))[0]?.title : undefined
  const parts = [thought ? t('plan.thought') : '', only ?? (tools ? t(`plan.tools_${plural(tools)}`, { n: tools }) : '')].filter(Boolean)
  return parts.length > 0 ? parts.join(' · ') : t(`plan.steps_${plural(events.length)}`, { n: events.length })
}

function WorkGroup({ events, live, render }: { events: SessionEvent[]; live: boolean; render: (e: SessionEvent) => ReactNode }) {
  const t = useT()
  const [open, setOpen] = useState(live)
  useEffect(() => setOpen(live), [live])
  return (
    <div>
      <button onClick={() => setOpen(!open)} className="group flex max-w-full items-center gap-1.5 text-md text-muted transition-colors hover:text-fg">
        <span className="min-w-0 truncate">{workLabel(events, t)}</span>
        <motion.span initial={false} animate={{ rotate: open ? 90 : 0 }} transition={tween} className="flex text-faint group-hover:text-fg"><Icon name="right" size={14} /></motion.span>
      </button>
      <Collapse open={open}>
        <div className="mt-4 space-y-5 border-l pl-4">
          <AnimatePresence initial={false}>{events.map(render)}</AnimatePresence>
        </div>
      </Collapse>
    </div>
  )
}

function TurnWork({ live, started, ended, events, render }: { live: boolean; started?: string; ended: string | undefined; events: SessionEvent[]; render: (e: SessionEvent) => ReactNode }) {
  const t = useT()
  const segments = segmentsOf(events, live)
  const working = segments.some((segment) => segment.kind === 'work')
  return (
    <div className="space-y-5">
      {(working || live) && started && (
        <div className="text-md text-muted"><TurnHeader startedAt={started} endedAt={ended ?? started} live={live} /></div>
      )}
      {segments.map((segment, i) =>
        segment.kind === 'human' ? (
          <AnimatePresence key={segment.event.id} initial={false}>{render(segment.event)}</AnimatePresence>
        ) : segment.events.length === 1 ? (
          <AnimatePresence key={segment.events[0].id} initial={false}>{render(segment.events[0])}</AnimatePresence>
        ) : (
          <WorkGroup key={segment.events[0].id} events={segment.events} live={live && i === segments.length - 1} render={render} />
        ),
      )}
      {live && segments[segments.length - 1]?.kind !== 'work' && (
        <div className="flex items-center gap-3 text-md">
          <AgentMark size={18} spinning />
          <span className="text-shimmer">{t('plan.working')}</span>
        </div>
      )}
    </div>
  )
}
