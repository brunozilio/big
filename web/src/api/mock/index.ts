import { setClock } from '../clock'
import type { Api, Branding, CallAction, CallArgs, CallResult, Connection, Connections, Item, LiveEvent, Models, SessionEvent, SignInPoll, SignInStart } from '../contract'
import { addIssueComment, changeData, issueData, pipelineRun, searchIssues, trackerProjects } from './calls'
import { INITIAL_CONNECTIONS, MODELS, PROVIDERS } from './fixtures'
import { itemOfSession, sessionById } from './sessions'
import {
  approveDeploy, approvePlan, cancelItem, chat, connect, createItem, disconnect, isStepId, mergePr, onChange, rejectDeploy, rerunFrom, reset, resolvePause, revisePlan, store, submitReview, tick,
  type Note, type SimItem,
} from './sim'
import { EPOCH, TICK_MS, stepView, toItem } from './view'

export interface MockOptions {
  tickMs?: number
  delayMs?: number
}

function wireConnections(): Connections {
  const providers = PROVIDERS.map(({ capabilities, accounts: _a, choose: _h, paste: _p, ...p }) => ({ ...p, ai: capabilities.includes('ai') }))
  return { providers, connections: store.connections.map((c) => ({ ...c, status: c.status ?? 'ok' })) }
}

export function wireModels(): Models {
  const connected = new Set(store.connections.map((c) => c.provider))
  const served = MODELS.filter((m) => connected.has(m.provider))
  if (!served.length) return { models: [], default: null }
  return { models: MODELS.filter((m) => m.provider === 'auto' || connected.has(m.provider)), default: 'auto' }
}

interface Flow { provider: string; polls: number; chosen: string | null; started: number }

export function mockApi(options: MockOptions = {}): Api {
  const tickMs = options.tickMs ?? 700
  const delay = options.delayMs ?? 250
  reset(INITIAL_CONNECTIONS)
  let lastTick = Date.now()
  setClock(() => EPOCH + store.clock * TICK_MS + Math.min(TICK_MS, ((Date.now() - lastTick) / tickMs) * TICK_MS))
  let branding: Branding = { name: 'Entregas', logo: null, theme: 'system' }
  const flows = new Map<string, Flow>()
  const listeners = new Set<(e: LiveEvent) => void>()
  const itemSigs = new Map<string, { sig: string; updatedAt: string }>()
  const sessionSigs = new Map<string, Map<string, string>>()
  const sessionStatus = new Map<string, string>()
  let timer: ReturnType<typeof setInterval> | null = null
  let version = 0

  const wait = <T>(value: () => T, ms = delay) => new Promise<T>((resolve, reject) => setTimeout(() => {
    try {
      resolve(value())
    } catch (e) {
      reject(e)
    }
  }, delay === 0 ? 0 : ms))

  const item = (it: SimItem): Item => {
    const base = toItem(it)
    const sig = JSON.stringify(base)
    const known = itemSigs.get(base.id)
    if (known?.sig === sig) return { ...base, updatedAt: known.updatedAt }
    version = (version + 1) % 60_000
    const updatedAt = new Date(EPOCH + store.clock * TICK_MS + version).toISOString()
    itemSigs.set(base.id, { sig, updatedAt })
    return { ...base, updatedAt }
  }

  const publish = (e: LiveEvent) => listeners.forEach((l) => l(e))

  const broadcast = () => {
    const seen = new Set<string>()
    for (const it of store.items) {
      const id = String(it.id)
      seen.add(id)
      const before = itemSigs.get(id)?.updatedAt
      const next = item(it)
      if (next.updatedAt !== before) publish({ type: 'item', item: next })
    }
    for (const id of [...itemSigs.keys()]) {
      if (!seen.has(id)) {
        itemSigs.delete(id)
        publish({ type: 'item.removed', id })
      }
    }
    for (const [id, known] of sessionSigs) {
      const s = sessionById(id)
      if (!s) continue
      s.events.forEach((e: SessionEvent) => {
        const sig = JSON.stringify(e)
        if (known.get(e.id) !== sig) {
          known.set(e.id, sig)
          publish({ type: 'session', session: id, event: e })
        }
      })
      if (sessionStatus.get(id) !== s.status) {
        sessionStatus.set(id, s.status)
        publish({ type: 'session.status', session: id, status: s.status })
      }
    }
  }

  onChange(broadcast)

  const start = () => {
    if (timer || tickMs <= 0) return
    timer = setInterval(() => {
      lastTick = Date.now()
      tick()
    }, tickMs)
  }

  const findItem = (id: string) => {
    const it = store.items.find((x) => String(x.id) === id)
    if (!it) throw new Error(`item ${id} não encontrado`)
    return it
  }

  const call = <A extends CallAction>(_connection: string, action: A, args: CallArgs<A>): Promise<CallResult<A>> =>
    wait(() => {
      const a = args as Record<string, string>
      switch (action) {
        case 'tracker.search':
          return searchIssues(a.query ?? '', a.project) as CallResult<A>
        case 'tracker.projects':
          return trackerProjects(a.query ?? '') as CallResult<A>
        case 'tracker.issue':
          return issueData(a.key) as CallResult<A>
        case 'tracker.comment':
          return addIssueComment(a.key, a.body) as CallResult<A>
        case 'code.change':
          return changeData(a.change) as CallResult<A>
        case 'ci.run':
          return pipelineRun(a.run) as CallResult<A>
      }
      throw new Error(`ação ${action} desconhecida`)
    }, 120)

  const startSignIn = (provider: string, method?: string): Promise<SignInStart> =>
    wait(() => {
      const p = PROVIDERS.find((x) => x.key === provider)
      if (!p) throw new Error(`provedor ${provider} desconhecido`)
      const flow = `f-${provider}-${Date.now()}`
      flows.set(flow, { provider, polls: 0, chosen: null, started: Date.now() })
      if (p.signIn === 'device' || p.signIn === 'sso' || method === 'sso') {
        const url = provider === 'aws' ? 'https://device.sso.us-east-1.amazonaws.com/' : provider === 'grok' ? 'https://accounts.x.ai/device' : 'https://github.com/login/device'
        return { flow, kind: 'device', url, code: provider === 'aws' ? 'KQTX-WMPB' : 'WDJB-MJHT', expiresIn: 900, interval: 1 }
      }
      return { flow, kind: 'redirect', url: provider === 'jira' ? 'https://auth.atlassian.com/authorize' : `https://${provider}.example/oauth/authorize`, paste: !!p.paste }
    }, 500)

  const finish = (f: Flow, choice: string | null): Connection => {
    const p = PROVIDERS.find((x) => x.key === f.provider)
    const option = p?.choose?.options.find((o) => o.id === choice)
    const n = store.connections.filter((c) => c.provider === f.provider).length
    const c = { id: `c-${f.provider}${n ? `-${n + 1}` : ''}`, provider: f.provider, label: option?.label ?? p?.name ?? f.provider, account: option ? `${option.label} · ${option.detail}` : p?.accounts[0] ?? '', capabilities: p?.capabilities ?? [] }
    connect(c)
    publish({ type: 'connections', connections: wireConnections() })
    return { ...c, status: 'ok' }
  }

  const failing = () => {
    try {
      return globalThis.localStorage?.getItem('mock:signIn') === 'failed'
    } catch {
      return false
    }
  }

  const pollSignIn = (req: { flow: string; code?: string; choice?: string }): Promise<SignInPoll> =>
    wait(() => {
      const f = flows.get(req.flow)
      if (!f) return { state: 'failed', message: 'Entrada expirou. Comece de novo.' }
      const p = PROVIDERS.find((x) => x.key === f.provider)
      f.polls++
      const ready = req.code !== undefined || f.polls >= 3
      if (!ready && req.choice === undefined) return { state: 'pending' }
      if (failing()) {
        flows.delete(req.flow)
        return { state: 'failed', message: `A entrada no ${p?.name ?? f.provider} foi recusada.` }
      }
      if (p?.choose && req.choice === undefined) return { state: 'choose', title: p.choose.title, choices: p.choose.options }
      flows.delete(req.flow)
      return { state: 'done', connection: finish(f, req.choice ?? null) }
    }, 400)

  const input = (itemId: string, step: string, payload: { action: string; payload: unknown }) =>
    wait(() => {
      const it = findItem(itemId)
      const p = (payload.payload ?? {}) as { verdict?: 'comment' | 'approve' | 'changes'; body?: string; notes?: Note[]; method?: string }
      if (payload.action === 'review') submitReview(it.id, p.verdict ?? 'comment', p.body ?? '', p.notes ?? [])
      else if (payload.action === 'merge') mergePr(it.id)
      else if (payload.action === 'deploy.approve') approveDeploy(it.id)
      else if (payload.action === 'deploy.reject') rejectDeploy(it.id)
      else throw new Error(`ação ${payload.action} não existe na etapa ${step}`)
    })

  return {
    items: () => wait(() => {
      start()
      return store.items.map(item)
    }),
    createItem: (input) => wait(() => item(createItem(input.text, input.issue?.key ?? null, input.model, input.project?.key ?? null)), 600),
    rerun: (id, step) => wait(() => {
      const it = findItem(id)
      const target = step && isStepId(step) ? step : 'develop'
      rerunFrom(it.id, target)
    }),
    cancel: (id) => wait(() => cancelItem(findItem(id).id)),
    step: (id, step, attempt) => wait(() => {
      if (!isStepId(step)) throw new Error(`etapa ${step} não existe`)
      return stepView(findItem(id), step, attempt)
    }, 80),
    input,
    call,
    session: (id) => wait(() => {
      const s = sessionById(id)
      if (!s) throw new Error(`sessão ${id} não encontrada`)
      sessionSigs.set(id, new Map(s.events.map((e) => [e.id, JSON.stringify(e)])))
      sessionStatus.set(id, s.status)
      return s
    }, 80),
    send: (id, message) => wait(() => {
      const found = itemOfSession(id)
      if (found) chat(found.it.id, found.kind, message.text)
    }, 80),
    respond: (id, event, response) => wait(() => {
      const found = itemOfSession(id)
      if (!found) throw new Error(`sessão ${id} não encontrada`)
      if (response.kind === 'plan') {
        if (response.decision === 'approve') approvePlan(found.it.id)
        else revisePlan(found.it.id, response.feedback ?? '')
      } else if (response.kind === 'ask') resolvePause(found.it.id, response.answers, null)
      else resolvePause(found.it.id, null, response.decision)
      return event
    }, 120).then(() => undefined),
    stop: () => wait(() => undefined),
    connections: () => wait(wireConnections),
    startSignIn: (provider, _fields, method) => startSignIn(provider, method),
    pollSignIn,
    disconnect: (id) => wait(() => {
      disconnect(id)
      publish({ type: 'connections', connections: wireConnections() })
    }),
    models: () => wait(wireModels),
    branding: () => wait(() => branding),
    saveBranding: (patch) => wait(() => {
      branding = { ...branding, ...patch }
      publish({ type: 'branding', branding })
      return branding
    }),
    upload: (file) => wait(() => ({ id: `a-${Date.now()}`, name: file.name, size: file.size })),
    live: (onEvent, onState) => {
      listeners.add(onEvent)
      setTimeout(() => onState(true), 0)
      return () => {
        listeners.delete(onEvent)
      }
    },
  }
}
