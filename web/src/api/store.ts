import { useEffect, useRef, useState, useSyncExternalStore } from 'react'
import { api } from '.'
import type { Api, Branding, CallAction, CallArgs, CallResult, Connections, Item, LiveEvent, Models, Session, SessionEvent, StepView } from './contract'

export interface State {
  ready: boolean
  connected: boolean
  error: string | null
  items: Item[]
  connections: Connections | null
  models: Models | null
  branding: Branding | null
  sessions: Record<string, Session>
}

const initial = (): State => ({ ready: false, connected: false, error: null, items: [], connections: null, models: null, branding: null, sessions: {} })

let state: State = initial()
const listeners = new Set<() => void>()
let unsubscribe: (() => void) | null = null

function set(patch: Partial<State> | ((s: State) => Partial<State>)) {
  state = { ...state, ...(typeof patch === 'function' ? patch(state) : patch) }
  listeners.forEach((l) => l())
}

export function getState(): State {
  return state
}

export function useStore<T>(select: (s: State) => T): T {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
    () => select(state),
    () => select(state),
  )
}

export function errorText(e: unknown): string {
  return e instanceof Error ? e.message : 'Algo deu errado'
}

function upsertItem(items: Item[], item: Item): Item[] {
  const i = items.findIndex((x) => x.id === item.id)
  if (i < 0) return [item, ...items]
  const next = items.slice()
  next[i] = item
  return next
}

function upsertEvent(session: Session, event: SessionEvent): Session {
  const i = session.events.findIndex((e) => e.id === event.id)
  const events = session.events.slice()
  if (i < 0) events.push(event)
  else events[i] = event
  return { ...session, events }
}

export function applyLive(e: LiveEvent) {
  switch (e.type) {
    case 'item':
      set((s) => ({ items: upsertItem(s.items, e.item) }))
      return
    case 'item.removed':
      set((s) => ({ items: s.items.filter((x) => x.id !== e.id) }))
      return
    case 'session':
      set((s) => (s.sessions[e.session] ? { sessions: { ...s.sessions, [e.session]: upsertEvent(s.sessions[e.session], e.event) } } : {}))
      return
    case 'session.status':
      set((s) => (s.sessions[e.session] ? { sessions: { ...s.sessions, [e.session]: { ...s.sessions[e.session], status: e.status } } } : {}))
      return
    case 'connections':
      set({ connections: e.connections })
      return
    case 'branding':
      set({ branding: e.branding })
      return
    case 'resync':
      void reload(api())
  }
}

async function reload(a: Api) {
  try {
    const [items, connections, models, branding] = await Promise.all([a.items(), a.connections(), a.models(), a.branding()])
    set({ items, connections, models, branding, ready: true, error: null })
    const ids = Object.keys(state.sessions)
    const sessions = await Promise.all(ids.map((id) => a.session(id).catch(() => null)))
    set((s) => ({ sessions: { ...s.sessions, ...Object.fromEntries(sessions.filter((x): x is Session => !!x).map((x) => [x.id, x])) } }))
  } catch (e) {
    set({ error: errorText(e), ready: true })
  }
}

export async function boot(a: Api) {
  unsubscribe?.()
  state = initial()
  unsubscribe = a.live(applyLive, (connected) => set({ connected }))
  await reload(a)
}

export function stop() {
  unsubscribe?.()
  unsubscribe = null
}

export async function retry() {
  await reload(api())
}

export function setItems(update: (items: Item[]) => Item[]) {
  set((s) => ({ items: update(s.items) }))
}

export function setBranding(branding: Branding) {
  set({ branding })
}

export function setConnections(connections: Connections) {
  set({ connections })
}

export function useSession(id: string | null): { session: Session | null; error: string | null } {
  const session = useStore((s) => (id ? s.sessions[id] ?? null : null))
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    if (!id || state.sessions[id]) return
    let alive = true
    api()
      .session(id)
      .then((loaded) => alive && set((s) => ({ sessions: { ...s.sessions, [id]: loaded } })))
      .catch((e) => alive && setError(errorText(e)))
    return () => {
      alive = false
    }
  }, [id])
  return { session, error }
}

export interface Loaded<T> {
  data: T | null
  error: string | null
  loading: boolean
}

export function useLoad<T>(load: (() => Promise<T>) | null, deps: unknown[]): Loaded<T> {
  const [result, setResult] = useState<Loaded<T>>({ data: null, error: null, loading: !!load })
  const latest = useRef(load)
  latest.current = load
  useEffect(() => {
    const run = latest.current
    if (!run) return
    let alive = true
    setResult((r) => ({ ...r, loading: true }))
    run()
      .then((data) => alive && setResult({ data, error: null, loading: false }))
      .catch((e) => alive && setResult((r) => ({ data: r.data, error: errorText(e), loading: false })))
    return () => {
      alive = false
    }
  }, deps)
  return result
}

export function useStepView(item: Item | null, step: string | null, attempt: number | null): Loaded<StepView> {
  return useLoad(item && step ? () => api().step(item.id, step, attempt) : null, [item?.id, item?.updatedAt, step, attempt])
}

export function useCall<A extends CallAction>(connection: string, action: A, args: CallArgs<A>, version: string | undefined): Loaded<CallResult<A>> {
  const key = JSON.stringify(args)
  return useLoad(() => api().call(connection, action, args), [connection, action, key, version])
}

export function useProviderOf(connection: string | null) {
  return useStore((s) => {
    const c = s.connections?.connections.find((x) => x.id === connection)
    return c ? s.connections?.providers.find((p) => p.key === c.provider) ?? null : null
  })
}
