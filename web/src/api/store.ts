import { useEffect, useRef, useState, useSyncExternalStore } from 'react'
import { api } from '.'
import { currentLanguage, translate } from '@/i18n'
import type { Api, Application, Branding, OnboardingState, CallAction, CallArgs, CallResult, Connections, Item, LiveEvent, Models, Session, SessionEvent, StepView } from './contract'

export interface State {
  ready: boolean
  connected: boolean
  error: string | null
  items: Item[]
  connections: Connections | null
  models: Models | null
  branding: Branding | null
  applications: Application[] | null
  onboarding: OnboardingState | null
  sessions: Record<string, Session>
}

const initial = (): State => ({ ready: false, connected: false, error: null, items: [], connections: null, models: null, branding: null, applications: null, onboarding: null, sessions: {} })

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
  return e instanceof Error ? e.message : translate(currentLanguage(), 'app.somethingWrong')
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
    case 'session.event.removed':
      set((s) => (s.sessions[e.session] ? { sessions: { ...s.sessions, [e.session]: { ...s.sessions[e.session], events: s.sessions[e.session].events.filter((x) => x.id !== e.id) } } } : {}))
      return
    case 'session.work':
      set((s) => (s.sessions[e.session] ? { sessions: { ...s.sessions, [e.session]: { ...s.sessions[e.session], work: e.work } } } : {}))
      return
    case 'session.status':
      applyStatus(e.session, e.status)
      return
    case 'session.suggestion':
      set((s) => (s.sessions[e.session] ? { sessions: { ...s.sessions, [e.session]: { ...s.sessions[e.session], suggestion: e.suggestion } } } : {}))
      return
    case 'connections':
      set({ connections: e.connections })
      refreshModels()
      return
    case 'branding':
      set({ branding: e.branding })
      return
    case 'resync':
      void reload(api())
  }
}

function applyStatus(id: string, status: Session['status']) {
  const before = state.sessions[id]
  if (!before) return
  const suggestion = status === 'idle' ? before.suggestion : null
  set((s) => ({ sessions: { ...s.sessions, [id]: { ...before, status, suggestion } } }))
}

async function reload(a: Api) {
  try {
    const [items, connections, models, branding, applications, onboarding] = await Promise.all([a.items(), a.connections(), a.models(), a.branding(), a.applications().catch(() => []), a.onboarding().catch(() => ({ skipped: false }))])
    set({ items, connections, models, branding, applications, onboarding, ready: true, error: null })
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

export function setOnboarding(onboarding: OnboardingState) {
  set({ onboarding })
}

export function setApplications(update: (applications: Application[]) => Application[]) {
  set((s) => ({ applications: update(s.applications ?? []) }))
}

function refreshModels() {
  api()
    .models()
    .then((models) => set({ models }))
    .catch(() => undefined)
}

const SESSION_TRIES = 8
const SESSION_RETRY_MS = 1000

export function useSession(id: string | null): { session: Session | null; error: string | null } {
  const session = useStore((s) => (id ? s.sessions[id] ?? null : null))
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    setError(null)
    if (!id || state.sessions[id]) return
    let alive = true
    let timer: ReturnType<typeof setTimeout> | undefined
    const fetchOnce = (tries: number) => {
      api()
        .session(id)
        .then((loaded) => alive && set((s) => ({ sessions: { ...s.sessions, [id]: loaded } })))
        .catch((e) => {
          if (!alive) return
          if (tries < SESSION_TRIES) timer = setTimeout(() => fetchOnce(tries + 1), SESSION_RETRY_MS)
          else setError(errorText(e))
        })
    }
    fetchOnce(1)
    return () => {
      alive = false
      clearTimeout(timer)
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

const seenSteps = new Map<string, StepView>()

function stepKey(item: string, step: string, attempt: number | null) {
  return `${item}/${step}/${attempt ?? ''}`
}

const STEP_SETTLE_MS = 400
const STEP_SECONDS_BUCKET = 20

export function stepSignature(item: Item, step: string): string {
  const found = item.steps.find((s) => s.id === step)
  const here = item.current === step
  const stuck = here && item.impediment ? item.impediment.reason : ''
  const asked = here && item.attention ? `${item.attention.action?.step ?? ''}:${item.attention.badge?.text ?? ''}` : ''
  const change = here && item.change ? `${item.change.key}:${item.change.checks ?? ''}:${item.change.merged}` : ''
  return [found?.state ?? '', found?.attempts ?? 0, Math.floor((found?.seconds ?? 0) / STEP_SECONDS_BUCKET), item.current ?? '', here && item.live, stuck, asked, change, item.doneAt ?? ''].join('|')
}

export function useSettled(value: string, scope: string, ms: number): string {
  const [kept, setKept] = useState({ scope, value })
  useEffect(() => {
    if (kept.scope !== scope) {
      setKept({ scope, value })
      return
    }
    if (kept.value === value) return
    const timer = setTimeout(() => setKept({ scope, value }), ms)
    return () => clearTimeout(timer)
  }, [scope, value, ms, kept])
  return kept.scope === scope ? kept.value : value
}

export function useStepView(item: Item | null, step: string | null, attempt: number | null, nonce = 0): Loaded<StepView> {
  const key = item && step ? stepKey(item.id, step, attempt) : ''
  const signature = useSettled(item && step ? stepSignature(item, step) : '', key, STEP_SETTLE_MS)
  const loaded = useLoad(item && step ? () => api().step(item.id, step, attempt).then((view) => ({ key, view })) : null, [key, signature, nonce])
  const fresh = loaded.data?.key === key ? loaded.data.view : null
  useEffect(() => {
    if (fresh) seenSteps.set(key, fresh)
  }, [key, fresh])
  const data = fresh ?? seenSteps.get(key) ?? loaded.data?.view ?? null
  return { data, error: loaded.error, loading: loaded.loading && !data }
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
