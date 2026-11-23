import { useEffect, useRef, useState, type ReactNode } from 'react'
import { api } from '@/api'
import type { Session, SessionEvent } from '@/api/contract'
import { useSession } from '@/api/store'
import { cn } from '@/ui/cn'
import { EmptyState, Spinner } from '@/ui/display'
import { Icon } from '@/ui/icon'
import { AnimatePresence, Collapse, motion, tween, tweenClose } from '@/ui/motion'
import { AskBlock, PermissionBlock, type Respond } from './actionBlocks'
import { BackgroundWork, hasWork } from './BackgroundWork'
import { Composer } from './Composer'
import { PlanBlock } from './PlanCard'
import { AgentMark, AgentsContext, AssistantMessage, StatusLine, ThinkingSteps, ToolRunSummary, TurnHeader, UserMessage } from './events'

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

const human = (e: SessionEvent) => e.type === 'assistant' || e.type === 'ask' || e.type === 'permission' || e.type === 'plan'

const enter = { initial: { opacity: 0, y: 8, filter: 'blur(2px)' }, animate: { opacity: 1, y: 0, filter: 'blur(0px)' }, transition: tween }
const settled = { initial: false as const, animate: enter.animate }

function useHistoricalIds(sessionId: string, events: SessionEvent[] | undefined) {
  const seen = useRef<{ sessionId: string; ids: Set<string> } | null>(null)
  if (events && seen.current?.sessionId !== sessionId) seen.current = { sessionId, ids: new Set(events.map((e) => e.id)) }
  const ids = seen.current?.ids
  return (id: string | undefined) => (id && ids?.has(id) ? settled : enter)
}

export function ChatView({ sessionId, composer, dock }: { sessionId: string; composer: boolean; dock?: ReactNode }) {
  const { session, error } = useSession(sessionId)
  const enterFor = useHistoricalIds(sessionId, session?.events)
  const scroller = useRef<HTMLDivElement>(null)
  const [atBottom, setAtBottom] = useState(true)
  const [sendError, setSendError] = useState<string | null>(null)
  const [model, setModel] = useState<string | null>(null)
  const eventCount = session?.events.length ?? 0
  const lastSignature = session ? JSON.stringify(session.events[session.events.length - 1] ?? null).length : 0

  useEffect(() => {
    const el = scroller.current
    if (el && atBottom) el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' })
  }, [eventCount, lastSignature, atBottom])

  if (error && !session) return <EmptyState title="Sessão não encontrada" text={error} />
  if (!session) return <div className="flex h-full items-center justify-center"><Spinner /></div>

  const running = session.status === 'running'
  const turns = toTurns(session.events)
  const respond: Respond = (event, response) => api().respond(session.id, event, response)

  return (
    <AgentsContext.Provider value={session.agents}>
      <div className="flex h-full min-h-0 flex-col">
        <div ref={scroller} onScroll={(e) => {
          const el = e.currentTarget
          setAtBottom(el.scrollHeight - el.scrollTop - el.clientHeight < 120)
        }} className="scrollbar-thin relative min-h-0 flex-1 overflow-y-auto">
          <div role="log" aria-live="polite" aria-label="Mensagens" className="mx-auto w-full max-w-[760px] px-5 pb-10 pt-8 sm:px-8">
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
                    const node = <EventView session={session} event={e} respond={respond} />
                    return <motion.div key={e.id} data-event={e.type} {...enterFor(e.id)}>{node}</motion.div>
                  }} />
                </section>
              )
            })}
            {hasWork(session.work) && <BackgroundWork work={session.work} />}
          </div>
          <AnimatePresence>
            {!atBottom && (
              <motion.button initial={{ opacity: 0, y: 8, scale: 0.96 }} animate={{ opacity: 1, y: 0, scale: 1 }} exit={{ opacity: 0, y: 8, scale: 0.96, transition: tweenClose }} transition={tween} onClick={() => scroller.current?.scrollTo({ top: scroller.current.scrollHeight, behavior: 'smooth' })} className="sticky bottom-4 mx-auto flex size-9 items-center justify-center rounded-full bg-surface text-fg2 shadow-pop" aria-label="Ir para o fim">
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
                placeholder="Responder ou pedir o próximo passo…"
                model={model ?? session.model}
                onModel={setModel}
                onSubmit={(input) => {
                  setAtBottom(true)
                  setSendError(null)
                  api().send(session.id, { text: input.text, model: input.model, effort: input.effort, fast: input.fast, attachments: input.attachments }).catch((e: unknown) => setSendError(e instanceof Error ? e.message : 'Não foi possível enviar'))
                }}
              />}
              {sendError && <p role="alert" className="mt-2 px-2 text-sm text-danger-fg">{sendError}</p>}
            </div>
          </div>
        )}
      </div>
    </AgentsContext.Provider>
  )
}

export function EventView({ session, event, respond }: { session: Session; event: SessionEvent; respond: Respond }) {
  switch (event.type) {
    case 'user':
      return <UserMessage event={event} />
    case 'assistant':
      return <AssistantMessage event={event} />
    case 'thinking':
      return <ThinkingSteps event={event} />
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
  }
}

function TurnWork({ live, started, ended, events, render }: { live: boolean; started?: string; ended: string | undefined; events: SessionEvent[]; render: (e: SessionEvent) => ReactNode }) {
  const work = events.filter((e) => !human(e))
  const finals = events.filter(human)
  const [open, setOpen] = useState(live)
  useEffect(() => setOpen(live), [live])
  return (
    <div className="space-y-5">
      {(work.length > 0 || live) && started && (
        <div>
          <button onClick={() => setOpen(!open)} className="group flex items-center gap-1.5 text-md text-muted transition-colors hover:text-fg">
            <TurnHeader startedAt={started} endedAt={ended ?? started} live={live} />
            <motion.span initial={false} animate={{ rotate: open ? 90 : 0 }} transition={tween} className="flex text-faint group-hover:text-fg"><Icon name="right" size={14} /></motion.span>
          </button>
          <Collapse open={open}>
            <div className={cn('mt-4 space-y-5 border-l pl-4')}>
              <AnimatePresence initial={false}>{work.map(render)}</AnimatePresence>
              {live && work.length === 0 && (
                <div className="flex items-center gap-3 text-md">
                  <AgentMark size={18} spinning />
                  <span className="text-shimmer">Preparando…</span>
                </div>
              )}
            </div>
          </Collapse>
        </div>
      )}
      <AnimatePresence initial={false}>{finals.map(render)}</AnimatePresence>
    </div>
  )
}
