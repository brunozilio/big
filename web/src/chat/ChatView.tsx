import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { api } from '@/api'
import type { FileRef, Session, SessionEvent } from '@/api/contract'
import { errorText, useSession } from '@/api/store'
import { toast } from '@/ui/toast'
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
import { Composer, type ComposerSubmit } from './Composer'
import { useRememberedModel } from './remembered'
import { PlanBlock } from './PlanCard'
import { SideAnswers, useSideQuestions } from './SideQuestions'
import { AgentMark, AgentsContext, emptyThinking, AssistantMessage, StatusLine, ThinkingSteps, ToolRunSummary, TurnHeader, UserMessage } from './events'
import type { StopJob } from './jobs'

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

const enter = { initial: { opacity: 0, y: 8 }, animate: { opacity: 1, y: 0 }, transition: tween }
const settled = { initial: false as const, animate: enter.animate }
const never = () => false
const NEAR = 120

const Work = memo(BackgroundWork)
const Answers = memo(SideAnswers)
const Input = memo(Composer)

function useStable<A extends unknown[], R>(fn: (...args: A) => R): (...args: A) => R {
  const latest = useRef(fn)
  latest.current = fn
  return useCallback((...args: A) => latest.current(...args), [])
}

function useHistoricalIds(sessionId: string, events: SessionEvent[]) {
  const seen = useRef<{ sessionId: string; ids: Set<string> } | null>(null)
  if (seen.current?.sessionId !== sessionId) seen.current = { sessionId, ids: new Set(events.map((e) => e.id)) }
  return useCallback((id: string) => !!seen.current?.ids.has(id), [])
}

const sameEvents = (a: SessionEvent[], b: SessionEvent[]) => a.length === b.length && a.every((e, i) => e === b[i])

function useTurns(events: SessionEvent[], hidePlans: boolean | undefined): Turn[] {
  const previous = useRef<Turn[]>([])
  return useMemo(() => {
    const shown = hidePlans ? events.filter((e) => e.type !== 'plan') : withoutRepeatedPlans(events)
    const turns = toTurns(shown).map((turn, i) => {
      const old = previous.current[i]
      return old && old.user === turn.user && sameEvents(old.events, turn.events) ? old : turn
    })
    previous.current = turns
    return turns
  }, [events, hidePlans])
}

function useChatScroll(sessionId: string, running: boolean, last: unknown, count: number) {
  const scroller = useRef<HTMLDivElement>(null)
  const content = useRef<HTMLDivElement>(null)
  const [atBottom, setAtBottom] = useState(true)
  const pinned = useRef(true)
  const live = useRef(running)
  const frame = useRef(0)
  live.current = running

  const mark = useCallback((next: boolean) => {
    if (pinned.current === next) return
    pinned.current = next
    setAtBottom(next)
  }, [])
  const pin = useCallback(() => {
    if (frame.current) return
    frame.current = requestAnimationFrame(() => {
      frame.current = 0
      const el = scroller.current
      if (el && pinned.current) el.scrollTop = el.scrollHeight
    })
  }, [])
  const onScroll = useCallback(() => {
    const el = scroller.current
    if (el) mark(el.scrollHeight - el.scrollTop - el.clientHeight < NEAR)
  }, [mark])
  const stick = useCallback(() => {
    mark(true)
    pin()
  }, [mark, pin])
  const jump = useCallback(() => scroller.current?.scrollTo({ top: scroller.current.scrollHeight, behavior: 'smooth' }), [])

  useLayoutEffect(() => {
    const el = scroller.current
    if (!el) return
    pinned.current = true
    setAtBottom(true)
    el.scrollTop = el.scrollHeight
    pin()
  }, [sessionId, pin])
  useEffect(pin, [last, count, atBottom, pin])
  useEffect(() => {
    const el = content.current
    if (!el || typeof ResizeObserver === 'undefined') return
    const observer = new ResizeObserver(() => {
      if (live.current) pin()
    })
    observer.observe(el)
    return () => observer.disconnect()
  }, [pin])
  useEffect(() => () => cancelAnimationFrame(frame.current), [])

  return { scroller, content, atBottom, onScroll, stick, jump }
}

export function ChatView({ sessionId, composer, dock, hidePlans }: { sessionId: string; composer: boolean; dock?: ReactNode; hidePlans?: boolean }) {
  const t = useT()
  const { session, error } = useSession(sessionId)
  if (error && !session) return <EmptyState title={t('plan.sessionNotFound')} text={error} />
  if (!session) return <div className="flex h-full items-center justify-center"><Spinner /></div>
  return <Chat session={session} composer={composer} dock={dock} hidePlans={hidePlans} />
}

function Chat({ session, composer, dock, hidePlans }: { session: Session; composer: boolean; dock?: ReactNode; hidePlans?: boolean }) {
  const t = useT()
  const id = session.id
  const historical = useHistoricalIds(id, session.events)
  const [model, setModel] = useRememberedModel()
  const side = useSideQuestions(id, (text) => api().side(id, { text, model: model ?? session.model ?? null }))
  const running = session.status === 'running'
  const scroll = useChatScroll(id, running, session.events[session.events.length - 1], session.events.length + side.asked.length)
  const respond = useCallback<Respond>((event, response) => api().respond(id, event, response), [id])
  const openFile = useCallback((file: FileRef) => void api().openFile(id, file).catch((e: unknown) => toast.error(errorText(e))), [id])
  const stopJob = useCallback<StopJob>((job) => api().stopJob(id, job), [id])
  const dismiss = useStable(side.dismiss)

  return (
    <AgentsContext.Provider value={session.agents}>
    <JobsContext.Provider value={stopJob}>
    <OpenFileContext.Provider value={openFile}>
      <div className="flex h-full min-h-0 flex-col">
        <div ref={scroll.scroller} onScroll={scroll.onScroll} className="scrollbar-thin relative min-h-0 flex-1 overflow-y-auto">
          <div ref={scroll.content} role="log" aria-live="polite" aria-label={t('plan.messages')} className="mx-auto w-full max-w-[760px] px-5 pb-10 pt-8 sm:px-8">
            <Transcript events={session.events} running={running} respond={respond} hidePlans={hidePlans} historical={historical} />
            {hasWork(session.work) && <Work work={session.work} />}
            <Answers asked={side.asked} onDismiss={dismiss} />
          </div>
          <AnimatePresence>
            {!scroll.atBottom && (
              <motion.button initial={{ opacity: 0, y: 8, scale: 0.96 }} animate={{ opacity: 1, y: 0, scale: 1 }} exit={{ opacity: 0, y: 8, scale: 0.96, transition: tweenClose }} transition={tween} onClick={scroll.jump} className="sticky bottom-4 mx-auto flex size-9 items-center justify-center rounded-full bg-surface text-fg2 shadow-pop" aria-label={t('plan.jumpToEnd')}>
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
              {composer && <ChatComposer session={session} running={running} model={model} onModel={setModel} ask={side.ask} stick={scroll.stick} />}
            </div>
          </div>
        )}
      </div>
    </OpenFileContext.Provider>
    </JobsContext.Provider>
    </AgentsContext.Provider>
  )
}

function ChatComposer({ session, running, model, onModel, ask, stick }: { session: Session; running: boolean; model: string | null; onModel: (id: string | null) => void; ask: (text: string) => void; stick: () => void }) {
  const t = useT()
  const id = session.id
  const [sendError, setSendError] = useState<string | null>(null)
  const onStop = useCallback(() => void api().stop(id), [id])
  const onAside = useStable((text: string) => {
    stick()
    ask(text)
  })
  const onSubmit = useStable((input: ComposerSubmit) => {
    stick()
    setSendError(null)
    api().send(id, { text: input.text, model: input.model, effort: input.effort, fast: input.fast, attachments: input.attachments }).catch((e: unknown) => setSendError(e instanceof Error ? e.message : t('plan.sendFailed')))
  })
  return (
    <>
      <Input running={running} onStop={onStop} placeholder={running ? t('plan.composerRunning') : t('plan.composerIdle')} suggestion={session.suggestion} onAside={onAside} onBtw={onAside} model={model ?? session.model} onModel={onModel} onSubmit={onSubmit} />
      {sendError && <p role="alert" className="mt-2 px-2 text-sm text-danger-fg">{sendError}</p>}
    </>
  )
}

export const Transcript = memo(function Transcript({ events, running, respond, hidePlans, historical = never }: { events: SessionEvent[]; running: boolean; respond: Respond; hidePlans?: boolean; historical?: (id: string) => boolean }) {
  const turns = useTurns(events, hidePlans)
  const render = useCallback((e: SessionEvent) => <EventRow key={e.id} event={e} running={running} respond={respond} historical={historical(e.id)} />, [running, respond, historical])
  return (
    <>
      {turns.map((turn, i) => (
        <TurnSection key={turn.user?.id ?? `t${i}`} turn={turn} live={running && i === turns.length - 1} historical={!!turn.user && historical(turn.user.id)} render={render} />
      ))}
    </>
  )
})

const TurnSection = memo(function TurnSection({ turn, live, historical, render }: { turn: Turn; live: boolean; historical: boolean; render: (e: SessionEvent) => ReactNode }) {
  const started = turn.user?.at ?? turn.events[0]?.at
  const ended = turn.events[turn.events.length - 1]?.at ?? started
  return (
    <section className="mb-10">
      {turn.user && (
        <motion.div {...(historical ? settled : enter)} className="mb-8">
          <UserMessage event={turn.user} />
        </motion.div>
      )}
      <TurnWork live={live} started={started} ended={ended} events={turn.events} render={render} />
    </section>
  )
})

const EventRow = memo(function EventRow({ event, running, respond, historical }: { event: SessionEvent; running: boolean; respond: Respond; historical: boolean }) {
  return (
    <motion.div data-event={event.type} {...(historical ? settled : enter)}>
      <EventView event={event} running={running} respond={respond} />
    </motion.div>
  )
})

export const EventView = memo(function EventView({ event, running = true, respond }: { event: SessionEvent; running?: boolean; respond: Respond }) {
  switch (event.type) {
    case 'user':
      return <UserMessage event={event} />
    case 'assistant':
      return <AssistantMessage event={event} />
    case 'thinking':
      return <ThinkingSteps event={event} running={running} />
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
})

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
  const later = new Set<string>()
  const kept: SessionEvent[] = []
  for (let i = events.length - 1; i >= 0; i--) {
    const e = events[i]
    if (e.type === 'plan') {
      if (later.has(e.text)) continue
      later.add(e.text)
    }
    kept.push(e)
  }
  return kept.reverse()
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
  const segments = useMemo(() => segmentsOf(events, live), [events, live])
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
