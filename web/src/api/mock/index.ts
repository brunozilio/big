import { setClock } from '../clock'
import { BTW_WORKFLOW } from '../contract'
import { currentLanguage } from '@/i18n'
import type { Api, Application, BindingOption, Branding, CallAction, CallArgs, CallResult, Connection, Connections, Item, LiveEvent, Models, OnboardingState, SessionEvent, SignInPoll, SignInStart } from '../contract'
import { addIssueComment, changeData, issueData, pipelineRun, searchIssues, trackerProjects } from './calls'
import { CLOUD_ACCOUNTS, CODE_REPOS, checkedApplication, cloudFieldOptions, cloudOptions, codeBranches, openPulls, initialApplications, listDirs } from './applications'
import { initialConnections, MCP_TOOLS, MODELS, providers as allProviders, repositoryKinds, usageFixture, usageLimitsFixture } from './fixtures'
import { mockLibrary, stepsOf } from './library'
import { say } from './say'
import { mockUsage } from './usage'
import { itemOfSession, sessionById } from './sessions'
import {
  approveDeploy, approvePlan, archiveItem, cancelItem, chat, connect, createItem, disconnect, isStepId, mergePr, onChange, rejectDeploy, rerunFrom, reset, resolvePause, revisePlan, store, submitReview, tick,
  type Note, type SimItem, triageFindings } from './sim'
import { qualityReportOf, scanStatusOf, scannedFindings, type Triage } from './security'
import { EPOCH, TICK_MS, originOf, stepView, toItem } from './view'

export interface MockOptions {
  tickMs?: number
  delayMs?: number
}

function wireConnections(): Connections {
  const providers = allProviders().filter((p) => !p.hidden).map(({ capabilities, accounts: _a, choose: _h, paste: _p, hidden: _x, ...p }) => ({ ...p, ai: capabilities.includes('ai') }))
  return { providers, connections: store.connections.map((c) => ({ ...c, status: c.status ?? 'ok' })), repositoryKinds: repositoryKinds() }
}

export function wireModels(): Models {
  const connected = new Set(store.connections.map((c) => c.provider))
  const served = MODELS.filter((m) => connected.has(m.provider))
  if (!served.length) return { models: [], default: null }
  return { models: served, default: served[0]?.id ?? null }
}

interface Flow { provider: string; polls: number; chosen: string | null; started: number; fields: Record<string, string> }

function mcpServer(fields: Record<string, string>) {
  const url = (fields.url ?? '').trim()
  if (!url) throw new Error(say('mcpUrlRequired'))
  const local = /^http:\/\/(localhost|127\.0\.0\.1)(:|\/|$)/.test(url)
  if (!local && !url.startsWith('https://')) throw new Error(say('httpsOnly'))
  return { open: local, host: url.replace(/^\w+:\/\//, '').split(/[/:?#]/)[0] ?? url }
}

const SCANNED: BindingOption[] = [
  { value: 'a1b2c3d4-0000-4000-8000-000000000001', label: 'Checkout · a1b2c3d4' },
  { value: 'a1b2c3d4-0000-4000-8000-000000000002', label: 'Infra · a1b2c3d4' },
]

const CHANGE_OPTIONS: Record<string, string[]> = {
  company: ['ACME', 'ACME Pagamentos'],
  requester_group: ['Squad Checkout', 'Squad Pix'],
  environment: ['HST', 'PRD'],
  template: ['Deploy HST', 'Deploy PRD', 'Emergencial'],
  classification: ['Normal', 'Padrão', 'Emergencial'],
  owner: ['Produção N1', 'DBA N1', 'MIDDLE N1', 'Time de infra'],
}

const ENVIRONMENTS: BindingOption[] = [
  { value: 'develop', label: 'Develop' },
  { value: 'hint', label: 'Homologação interna' },
  { value: 'hext', label: 'Homologação externa' },
  { value: 'prod', label: 'Produção' },
]

function optionsOf(capability: string, query: string, field = '', chosen: Record<string, string> = {}): BindingOption[] {
  const wanted = query.toLowerCase()
  const found = (() => {
    if (capability === 'changes') return (CHANGE_OPTIONS[field] ?? []).map((value) => ({ value, label: value }))
    if (capability === 'code') return CODE_REPOS.map((r) => ({ value: r.ref, label: r.ref }))
    if (capability === 'cloud') return field === 'environment' ? ENVIRONMENTS : (cloudFieldOptions(field, chosen) ?? cloudOptions())
    if (capability === 'security') return SCANNED
    if (capability === 'quality') return [{ value: 'acme_checkout', label: 'acme_checkout' }, { value: 'acme_infra', label: 'acme_infra' }]
    return trackerProjects('').map((p) => ({ value: p.key, label: p.name && p.name !== p.key ? `${p.name} · ${p.key}` : p.key }))
  })()
  return found.filter((o) => o.label.toLowerCase().includes(wanted) || o.value.toLowerCase().includes(wanted))
}

export function mockApi(options: MockOptions = {}): Api {
  const tickMs = options.tickMs ?? 700
  const delay = options.delayMs ?? 250
  reset(initialConnections())
  let lastTick = Date.now()
  setClock(() => EPOCH + store.clock * TICK_MS + Math.min(TICK_MS, ((Date.now() - lastTick) / tickMs) * TICK_MS))
  let branding: Branding = { name: 'Entregas', logo: '/api/branding/logo', favicon: '/api/branding/favicon', editor: 'VS Code', theme: 'system', language: currentLanguage(), output_style: null, output_styles: [{ key: 'concise', name: 'Concise', description: say('shortReplies') }] }
  let applications: Application[] = initialApplications()
  let onboarding: OnboardingState = { skipped: false }
  const flows = new Map<string, Flow>()
  const library = mockLibrary()
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
    if (!it) throw new Error(say('itemNotFound', { id }))
    return it
  }

  const call = <A extends CallAction>(_connection: string, action: A, args: CallArgs<A>): Promise<CallResult<A>> =>
    wait(() => {
      const a = args as Record<string, string>
      if (action.endsWith('.options')) return optionsOf(action.slice(0, -'.options'.length), a.query ?? '', a.field ?? '', (args as { chosen?: Record<string, string> }).chosen ?? {}) as CallResult<A>
      if (action === 'security.findings') return scannedFindings(a.project ?? '') as CallResult<A>
      if (action === 'security.status') return scanStatusOf(a.project ?? '') as CallResult<A>
      if (action === 'quality.report') return qualityReportOf(a.project ?? '') as unknown as CallResult<A>
      switch (action) {
        case 'tracker.search':
          return searchIssues(a.query ?? '', a.project) as CallResult<A>
        case 'tracker.projects':
        case 'tests.projects':
          return trackerProjects(a.query ?? '') as CallResult<A>
        case 'code.repos':
          return CODE_REPOS.filter((r) => r.ref.includes((a.query ?? '').toLowerCase())) as CallResult<A>
        case 'code.pulls':
          return openPulls(a.repo) as CallResult<A>
        case 'code.branches':
          return codeBranches() as CallResult<A>
        case 'tracker.issue':
          return issueData(a.key) as CallResult<A>
        case 'tracker.comment':
          return addIssueComment(a.key, a.body) as CallResult<A>
        case 'tracker.create_issue':
          return { key: `${a.project ?? 'APP'}-${100 + Math.floor(Math.random() * 900)}`, url: null } as CallResult<A>
        case 'code.change':
          return changeData(a.change) as CallResult<A>
        case 'ci.run':
          return pipelineRun(a.run) as CallResult<A>
        case 'cloud.accounts_and_roles':
          return CLOUD_ACCOUNTS as CallResult<A>
        case 'mcp.tools':
          return MCP_TOOLS as CallResult<A>
      }
      throw new Error(say('unknownAction', { action }))
    }, 120)

  const startSignIn = (provider: string, fields: Record<string, string>, method?: string): Promise<SignInStart> =>
    wait(() => {
      const p = allProviders().find((x) => x.key === provider)
      if (!p) throw new Error(say('unknownProvider', { provider }))
      const server = provider === 'mcp' ? mcpServer(fields) : null
      const flow = `f-${provider}-${Date.now()}`
      flows.set(flow, { provider, polls: 0, chosen: null, started: Date.now(), fields })
      if (method === 'api_token' || method === 'access_keys' || server?.open) return { flow, kind: 'token' }
      if (p.signIn === 'device' || p.signIn === 'sso' || method === 'sso') {
        const url = provider === 'aws' ? 'https://device.sso.us-east-1.amazonaws.com/' : provider === 'grok' ? 'https://accounts.x.ai/device' : 'https://github.com/login/device'
        return { flow, kind: 'device', url, code: provider === 'aws' ? 'KQTX-WMPB' : 'WDJB-MJHT', expiresIn: 900, interval: 1 }
      }
      return { flow, kind: 'redirect', url: provider === 'jira' ? 'https://auth.atlassian.com/authorize' : `https://${provider}.example/oauth/authorize`, paste: !!p.paste }
    }, 500)

  const finish = (f: Flow, choice: string | null): Connection => {
    const p = allProviders().find((x) => x.key === f.provider)
    const option = p?.choose?.options.find((o) => o.id === choice)
    const n = store.connections.filter((c) => c.provider === f.provider).length
    const named = f.fields.label?.trim() || (f.provider === 'mcp' ? mcpServer(f.fields).host : '')
    const label = option?.label ?? (named || p?.name || f.provider)
    const c = { id: `c-${f.provider}${n ? `-${n + 1}` : ''}`, provider: f.provider, label, account: option ? `${option.label} · ${option.detail}` : named || (p?.accounts[0] ?? ''), capabilities: p?.capabilities ?? [] }
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
      if (!f) return { state: 'failed', message: say('signInExpired') }
      const p = allProviders().find((x) => x.key === f.provider)
      f.polls++
      const ready = req.code !== undefined || f.polls >= 3
      if (!ready && req.choice === undefined) return { state: 'pending' }
      if (failing()) {
        flows.delete(req.flow)
        return { state: 'failed', message: say('signInRefused', { name: p?.name ?? f.provider }) }
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
      else if (payload.action === 'findings.triage') triageFindings(it.id, (payload.payload as { decisions: Triage }).decisions)
      else throw new Error(say('noActionAtStep', { action: payload.action, step }))
    })

  return {
    items: () => wait(() => {
      start()
      return store.items.map(item)
    }),
    createItem: (input) => wait(() => {
      const app = applications.find((x) => x.id === input.application)
      if (input.application && !app) throw new Error(say('appGone'))
      const space = app?.bindings?.tracker?.value ?? null
      const issueKey = input.issue?.key ?? input.sources?.find((s) => s.kind === 'issue')?.subject ?? null
      const pull = input.sources?.find((s) => s.kind === 'pull_request')
      const text = input.text || (pull && !issueKey ? `Pull request ${pull.subject}` : '')
      const made = createItem(text, issueKey, input.model, space)
      made.origin = originOf(input, app)
      if (app) made.application = { id: app.id, name: app.name }
      return item(made)
    }, 600),
    rerun: (id, step) => wait(() => {
      const it = findItem(id)
      const target = step && isStepId(step) ? step : 'develop'
      rerunFrom(it.id, target)
    }),
    cancel: (id) => wait(() => cancelItem(findItem(id).id)),
    baseBranches: () => wait(() => ({ branches: ['main', 'release'], default: 'main' })),
    chooseBase: (id) => wait(() => rerunFrom(findItem(id).id, 'develop')),
    archive: (id) => wait(() => archiveItem(findItem(id).id, true)),
    unarchive: (id) => wait(() => archiveItem(findItem(id).id, false)),
    step: (id, step, attempt) => wait(() => {
      if (!isStepId(step)) throw new Error(say('stepMissing', { step }))
      return stepView(findItem(id), step, attempt)
    }, 80),
    input,
    itemUsage: (id) => wait(() => mockUsage(findItem(id)), 120),
    call,
    session: (id) => wait(() => {
      const s = sessionById(id)
      if (!s) throw new Error(say('sessionNotFound', { id }))
      sessionSigs.set(id, new Map(s.events.map((e) => [e.id, JSON.stringify(e)])))
      sessionStatus.set(id, s.status)
      return s
    }, 80),
    send: (id, message) => wait(() => {
      const found = itemOfSession(id)
      if (found) chat(found.it.id, found.kind, message.text)
    }, 80),
    side: (id, question) => wait(() => {
      const found = itemOfSession(id)
      if (!found) throw new Error(say('sessionNotFound', { id }))
      if (!question.text.trim()) throw new Error(say('writeQuestion'))
      return { answer: say('sideAnswer', { key: found.it.key, question: question.text.trim() }) }
    }, 600),
    btw: (question) => wait(() => {
      if (!question.text.trim()) throw new Error(say('writeQuestion'))
      if (!question.application) throw new Error(say('chooseApp'))
      return item(createItem(question.text.trim(), null, question.model ?? 'auto', null, BTW_WORKFLOW))
    }, 200),
    respond: (id, event, response) => wait(() => {
      const found = itemOfSession(id)
      if (!found) throw new Error(say('sessionNotFound', { id }))
      if (response.kind === 'plan') {
        if (response.decision === 'approve') approvePlan(found.it.id)
        else revisePlan(found.it.id, response.feedback ?? '')
      } else if (response.kind === 'ask') resolvePause(found.it.id, response.answers, null)
      else resolvePause(found.it.id, null, response.decision)
      return event
    }, 120).then(() => undefined),
    stop: () => wait(() => undefined),
    stopJob: () => wait(() => undefined),
    openFile: () => wait(() => undefined),
    openFolders: () => wait(() => undefined),
    connections: () => wait(wireConnections),
    startSignIn: (provider, fields, method) => startSignIn(provider, fields, method),
    pollSignIn,
    disconnect: (id) => wait(() => {
      disconnect(id)
      publish({ type: 'connections', connections: wireConnections() })
    }),
    models: () => wait(wireModels),
    usage: (days) => wait(() => usageFixture(days, new Date())),
    usageLimits: () => wait(() => usageLimitsFixture(Date.now()), delay * 4),
    branding: () => wait(() => branding),
    saveBranding: (patch) => wait(() => {
      branding = { ...branding, ...patch }
      publish({ type: 'branding', branding })
      return branding
    }),
    onboarding: () => wait(() => onboarding),
    saveOnboarding: (state) => wait(() => (onboarding = { skipped: state.skipped })),
    applications: () => wait(() => applications),
    createApplication: (draft) => wait(() => {
      const made = checkedApplication(draft, `app-${Date.now()}-${applications.length}`)
      applications = [...applications, made]
      return made
    }),
    saveApplication: (application) => wait(() => {
      if (!applications.some((x) => x.id === application.id)) throw new Error(say('noSuchApp'))
      const saved = checkedApplication(application, application.id)
      applications = applications.map((x) => (x.id === saved.id ? saved : x))
      return saved
    }),
    deleteApplication: (id) => wait(() => {
      applications = applications.filter((x) => x.id !== id)
    }),
    dirs: (path) => wait(() => listDirs(path), 60),
    library: () => wait(() => library.list()),
    libraryItem: (path) => wait(() => library.shown(path)),
    saveLibraryItem: (path, change) => wait(() => library.save(path, change)),
    createLibraryItem: (section, name) => wait(() => library.create(section, name)),
    restoreLibraryItem: (path) => wait(() => library.restore(path)),
    createLibraryFile: (item, name) => wait(() => library.createFile(item, name)),
    outlineLibrary: (_path, source) => wait(() => stepsOf(source), 30),
    assistLibrary: async (path, draft, messages, ask) => {
      const answer = await wait(() => library.assist(path, draft, messages), delay * 2)
      const words = answer.reply.split(' ')
      for (let at = 1; at <= words.length && delay > 0; at += 1) {
        await wait(() => null, delay / 6)
        ask?.onReply?.(words.slice(0, at).join(' '))
      }
      return answer
    },
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
