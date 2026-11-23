export type StepId = 'plan' | 'ticket' | 'develop' | 'checks' | 'pr' | 'ci' | 'review' | 'merge' | 'release'
export type SimState = 'pending' | 'running' | 'waiting' | 'passed' | 'failed' | 'skipped' | 'blocked'
export type Need = 'ai' | 'tracker' | 'code' | 'cloud'

export interface Step { id: StepId; label: string; icon: string; logo?: string; dur: number; needs?: Need; doing: string }

export const STEPS: Step[] = [
  { id: 'plan', label: 'Plano', icon: 'sPlan', dur: 7, needs: 'ai', doing: 'Planejando' },
  { id: 'ticket', label: 'Jira', icon: 'sTicket', logo: 'jira', dur: 3, needs: 'tracker', doing: 'Criando issue' },
  { id: 'develop', label: 'Develop', icon: 'sCode', dur: 14, needs: 'ai', doing: 'Desenvolvendo' },
  { id: 'checks', label: 'Checks', icon: 'sTest', dur: 6, doing: 'Rodando checks' },
  { id: 'pr', label: 'Pull request', icon: 'sPr', logo: 'github', dur: 3, needs: 'code', doing: 'Abrindo PR' },
  { id: 'ci', label: 'CI', icon: 'sCi', logo: 'github', dur: 12, needs: 'code', doing: 'Aguardando CI' },
  { id: 'review', label: 'Review', icon: 'sReview', dur: Infinity, doing: 'Esperando você' },
  { id: 'merge', label: 'Merge', icon: 'sMerge', logo: 'github', dur: 3, needs: 'code', doing: 'Mergeando' },
  { id: 'release', label: 'Release', icon: 'sRelease', dur: 11, needs: 'cloud', doing: 'Deploy' },
]
export const IDX = Object.fromEntries(STEPS.map((s, i) => [s.id, i])) as Record<StepId, number>
export const isStepId = (s: string): s is StepId => s in IDX

export const REPO = 'acme/checkout'
export const GATE_AT = 5

export interface Attempt { state: SimState; ticks: number; start: number }
export interface Reason { kind: 'checks' | 'review' | 'plan'; text: string }
export interface Pause { kind: 'ask' | 'permission'; at: number; answer: string[] | null; decision: 'allowed' | 'always' | 'denied' | null }
export interface ChatLine { id: string; tick: number; role: 'user' | 'assistant'; text: string; agent?: string }
export interface Note { file: string; line: number; body: string }

export interface SimItem {
  id: number
  key: string
  title: string
  branch: string
  pr: number | null
  states: Record<StepId, SimState>
  attempts: Record<StepId, Attempt[]>
  cur: number
  elapsed: number
  fail: Partial<Record<StepId, number>>
  bounce: number
  done: boolean
  reasons: Reason[]
  h: number
  waitedAt: number
  doneAt: number
  createdAt: number
  approvedBy: string | null
  approvedAt: number
  gate: boolean
  deployApproved: boolean
  rejected: boolean
  linked: boolean
  instructions: string
  plans: { text: string; resolution: 'pending' | 'approved' | 'revised'; feedback?: string; tick: number }[]
  pause: Pause | null
  planChat: ChatLine[]
  reviewChat: ChatLine[]
  issueComments: { id: string; body: string; tick: number }[]
  reviewComments: Note[]
  model: string
}

export interface ConnectionState { id: string; provider: string; label: string; account: string; capabilities: string[]; status?: 'ok' | 'error' }

export const store = { clock: 0, items: [] as SimItem[], connections: [] as ConnectionState[] }

let seq = 0
let prSeq = 1180

const listeners = new Set<() => void>()
export function onChange(l: () => void) {
  listeners.add(l)
  return () => listeners.delete(l)
}
function emit() {
  listeners.forEach((l) => l())
}

export const hash = (s: string) => {
  let h = 2166136261
  for (const c of s) {
    h ^= c.charCodeAt(0)
    h = Math.imul(h, 16777619)
  }
  return Math.abs(h)
}
const slug = (t: string) => t.toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '').replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 24)
const durOf = (s: Step) => (Number.isFinite(s.dur) ? s.dur : 4)
export const last = (it: SimItem, id: StepId) => it.attempts[id][it.attempts[id].length - 1]
export const testName = (it: SimItem) => ['auth::session::refresh_expired', 'billing::retry::backoff_caps_at_max', 'startup::cold::loads_config_once', 'invoices::list::paginates_by_cursor', 'settings::a11y::labels_toggles'][it.h % 5]
export const fileOf = (it: SimItem) => ['src/auth/session.ts', 'src/billing/retry.ts', 'src/startup/cold.ts', 'src/invoices/list.ts', 'src/settings/view.tsx'][it.h % 5]
export const moduleOf = (it: SimItem) => testName(it).split('::')[0]
export const diffOf = (it: SimItem) => ({ additions: 40 + (it.h % 380), deletions: 6 + ((it.h >> 4) % 120), files: 2 + ((it.h >> 8) % 9) })

const needCapability: Record<Need, string> = { ai: 'ai', tracker: 'tracker', code: 'code', cloud: 'cloud' }
export function connectionFor(n?: Need) {
  if (!n) return undefined
  return store.connections.find((c) => c.capabilities.includes(needCapability[n]))
}
export function hasNeed(n?: Need) {
  return !n || !!connectionFor(n)
}

export const isBlocked = (it: SimItem) => !it.done && !it.bounce && it.states[STEPS[it.cur].id] === 'blocked'
export const isPlanWaiting = (it: SimItem) => !it.done && STEPS[it.cur]?.id === 'plan' && it.states.plan === 'waiting'
export const isReviewWaiting = (it: SimItem) => !it.done && !it.bounce && STEPS[it.cur]?.id === 'review' && it.states.review === 'waiting'
export const isMergeReady = (it: SimItem) => !it.done && !it.bounce && STEPS[it.cur]?.id === 'merge' && it.states.merge === 'waiting'
export const isGate = (it: SimItem) => !it.done && STEPS[it.cur]?.id === 'release' && (it.gate || it.rejected)
export const isPaused = (it: SimItem) => !it.done && STEPS[it.cur]?.id === 'develop' && it.states.develop === 'waiting'
export const withYou = (it: SimItem) => isBlocked(it) || isPlanWaiting(it) || isReviewWaiting(it) || isMergeReady(it) || isGate(it) || isPaused(it)

export function planText(it: SimItem, revision: number) {
  const extra = revision > 0 ? `\n4. ${it.reasons.filter((r) => r.kind === 'plan')[revision - 1]?.text ?? 'Ajustar conforme pedido'}` : ''
  return `Plano para **${it.title}**:\n\n1. Ajustar \`${fileOf(it)}\`\n2. Cobrir \`${moduleOf(it)}\` com teste\n3. Rodar a suíte do módulo${extra}`
}

interface Seed { key: string; title: string; at?: number; elapsed?: number; fail?: Partial<Record<StepId, number>>; skipPlan?: boolean; done?: boolean; pause?: Pause['kind']; planWaiting?: boolean }

function makeItem(o: Seed): SimItem {
  const at = o.at ?? 0
  const id = ++seq
  const it: SimItem = {
    id, key: o.key, title: o.title, branch: `feat/${o.key.toLowerCase()}-${slug(o.title)}`, pr: null,
    states: {} as SimItem['states'], attempts: {} as SimItem['attempts'], cur: at, elapsed: 0, fail: { ...(o.fail ?? {}) }, bounce: 0, done: false, reasons: [],
    h: hash(o.key), waitedAt: 0, doneAt: 0, createdAt: store.clock - 120, approvedBy: null, approvedAt: 0, gate: false, deployApproved: false, rejected: false,
    linked: false, instructions: '', plans: [], pause: o.pause ? { kind: o.pause, at: 4, answer: null, decision: null } : null, planChat: [], reviewChat: [], issueComments: [], reviewComments: [], model: 'auto',
  }
  let t = store.clock - 90
  STEPS.forEach((s, i) => {
    const sk = !!o.skipPlan && i < 2
    it.states[s.id] = sk ? 'skipped' : i < at ? 'passed' : 'pending'
    it.attempts[s.id] = !sk && i < at ? [{ state: 'passed', ticks: durOf(s), start: t }] : []
    if (!sk && i < at) t += durOf(s)
  })
  if (at > IDX.plan && !o.skipPlan) it.plans.push({ text: planText(it, 0), resolution: 'approved', tick: store.clock - 80 })
  if (at > IDX.pr) it.pr = ++prSeq
  if (o.done) {
    STEPS.forEach((s) => {
      if (it.states[s.id] !== 'skipped') it.states[s.id] = 'passed'
      if (!it.attempts[s.id].length && it.states[s.id] === 'passed') it.attempts[s.id] = [{ state: 'passed', ticks: durOf(s), start: store.clock - 200 }]
    })
    if (!it.plans.length) it.plans.push({ text: planText(it, 0), resolution: 'approved', tick: store.clock - 200 })
    it.done = true
    it.cur = STEPS.length
    it.pr = it.pr ?? ++prSeq
    it.doneAt = store.clock - (20 + (it.h % 140))
    it.approvedBy = it.h % 2 ? 'u-maria' : 'u-joao'
    it.approvedAt = it.doneAt - 18
    return it
  }
  startStep(it, at)
  if (it.states[STEPS[at].id] === 'running') {
    it.elapsed = o.elapsed ?? 0
    const a = last(it, STEPS[at].id)
    a.ticks = it.elapsed
    a.start = store.clock - it.elapsed
  }
  if (o.planWaiting) finishPlan(it)
  it.waitedAt = store.clock - (o.elapsed ?? 0) - (it.h % 9)
  return it
}

function startStep(it: SimItem, i: number) {
  const s = STEPS[i]
  it.cur = i
  it.elapsed = 0
  if (!hasNeed(s.needs)) {
    it.states[s.id] = 'blocked'
    it.waitedAt = store.clock
    return
  }
  it.states[s.id] = s.id === 'review' || s.id === 'merge' ? 'waiting' : 'running'
  it.attempts[s.id].push({ state: it.states[s.id], ticks: 0, start: store.clock })
  if (s.id === 'review') it.waitedAt = store.clock
}

function finishPlan(it: SimItem) {
  it.states.plan = 'waiting'
  const a = last(it, 'plan')
  if (a) a.state = 'waiting'
  it.plans.push({ text: planText(it, it.plans.length), resolution: 'pending', tick: store.clock })
  it.waitedAt = store.clock
}

function pass(it: SimItem, s: Step) {
  it.states[s.id] = 'passed'
  const a = last(it, s.id)
  if (a) a.state = 'passed'
  if (s.id === 'pr') it.pr = it.pr ?? ++prSeq
  let next = IDX[s.id] + 1
  while (next < STEPS.length && it.states[STEPS[next].id] === 'skipped') next++
  if (next >= STEPS.length) {
    it.done = true
    it.cur = STEPS.length
    it.doneAt = store.clock
  } else startStep(it, next)
}

function fail(it: SimItem, s: Step) {
  it.states[s.id] = 'failed'
  last(it, s.id).state = 'failed'
  it.reasons.push({ kind: 'checks', text: `cargo nextest run falhou em ${testName(it)}: esperado Refreshed, veio Expired` })
  it.bounce = 3
}

function advance(it: SimItem) {
  if (it.done || isBlocked(it)) return
  if (it.bounce) {
    if (--it.bounce <= 0) {
      for (let i = IDX.develop; i <= it.cur; i++) it.states[STEPS[i].id] = 'pending'
      startStep(it, IDX.develop)
    }
    return
  }
  const s = STEPS[it.cur]
  if (s.id === 'review' || it.states[s.id] === 'waiting') return
  if (s.id === 'release' && (it.rejected || (!it.deployApproved && it.elapsed >= GATE_AT))) {
    if (!it.gate && !it.rejected) {
      it.gate = true
      it.waitedAt = store.clock
    }
    return
  }
  if (s.id === 'develop' && it.pause && it.pause.answer === null && it.pause.decision === null && it.elapsed >= it.pause.at) {
    it.states.develop = 'waiting'
    last(it, 'develop').state = 'waiting'
    it.waitedAt = store.clock
    return
  }
  it.elapsed++
  last(it, s.id).ticks = it.elapsed
  if (it.elapsed < s.dur) return
  if (s.id === 'plan') {
    finishPlan(it)
    return
  }
  const f = it.fail[s.id] ?? 0
  if (f > 0) {
    it.fail[s.id] = f - 1
    fail(it, s)
  } else pass(it, s)
}

export function tick() {
  store.clock++
  store.items.forEach(advance)
  emit()
}

const find = (id: number) => store.items.find((x) => x.id === id)

export function approvePlan(id: number) {
  const it = find(id)
  if (!it || !isPlanWaiting(it)) return
  const p = it.plans[it.plans.length - 1]
  if (p) p.resolution = 'approved'
  pass(it, STEPS[IDX.plan])
  emit()
}

export function revisePlan(id: number, feedback: string) {
  const it = find(id)
  if (!it || !isPlanWaiting(it)) return
  const p = it.plans[it.plans.length - 1]
  if (p) {
    p.resolution = 'revised'
    p.feedback = feedback
  }
  it.reasons.push({ kind: 'plan', text: feedback || 'Ajustar o plano' })
  it.states.plan = 'pending'
  startStep(it, IDX.plan)
  emit()
}

export function resolvePause(id: number, answer: string[] | null, decision: Pause['decision']) {
  const it = find(id)
  if (!it?.pause || !isPaused(it)) return
  it.pause.answer = answer
  it.pause.decision = decision
  it.states.develop = 'running'
  last(it, 'develop').state = 'running'
  emit()
}

export function submitReview(id: number, verdict: 'comment' | 'approve' | 'changes', body: string, notes: Note[]) {
  const it = find(id)
  if (!it || !isReviewWaiting(it)) return
  it.reviewComments.push(...notes)
  if (body && verdict !== 'changes') it.reviewComments.push({ file: '', line: -1, body })
  if (verdict === 'approve') {
    it.approvedBy = 'u-you'
    it.approvedAt = store.clock
    pass(it, STEPS[IDX.review])
  } else if (verdict === 'changes') {
    it.states.review = 'failed'
    last(it, 'review').state = 'failed'
    const lines = notes.map((n) => `${n.file}:${n.line}: ${n.body}`)
    it.reasons.push({ kind: 'review', text: [body, ...lines].filter(Boolean).join('\n') || 'Mudanças pedidas na revisão' })
    it.bounce = 2
  }
  emit()
}

export function approveDeploy(id: number) {
  const it = find(id)
  if (!it) return
  it.deployApproved = true
  it.gate = false
  emit()
}

export function rejectDeploy(id: number) {
  const it = find(id)
  if (!it) return
  it.gate = false
  it.rejected = true
  it.states.release = 'failed'
  const a = last(it, 'release')
  if (a) a.state = 'failed'
  emit()
}

export function mergePr(id: number) {
  const it = find(id)
  if (!it || !isMergeReady(it)) return
  pass(it, STEPS[IDX.merge])
  emit()
}

export function rerunFrom(id: number, step: StepId) {
  const it = find(id)
  if (!it) return
  for (let i = IDX[step]; i < STEPS.length; i++) if (it.states[STEPS[i].id] !== 'skipped') it.states[STEPS[i].id] = 'pending'
  it.done = false
  it.bounce = 0
  it.gate = false
  it.deployApproved = false
  it.rejected = false
  startStep(it, IDX[step])
  emit()
}

export function cancelItem(id: number) {
  store.items = store.items.filter((x) => x.id !== id)
  emit()
}

export function chat(id: number, where: 'plan' | 'review', text: string) {
  const it = find(id)
  if (!it) return
  const lines = where === 'plan' ? it.planChat : it.reviewChat
  lines.push({ id: `u${Date.now()}`, tick: store.clock, role: 'user', text })
  emit()
  setTimeout(() => {
    lines.push({ id: `a${Date.now()}`, tick: store.clock, role: 'assistant', agent: where === 'plan' ? 'planner' : 'reviewer', text: where === 'plan' ? 'Anotado. Vou levar isso em conta no plano.' : `Boa pergunta. A mudança em \`${fileOf(it)}\` segue o padrão de \`src/lib/retry.ts\`.` })
    emit()
  }, 1400)
}

export function commentIssue(key: string, body: string) {
  const it = store.items.find((x) => x.key === key)
  if (!it) return null
  const c = { id: `jc${Date.now()}`, body, tick: store.clock }
  it.issueComments.push(c)
  emit()
  return c
}

export interface IssueSeed { key: string; title: string; kind: 'story' | 'bug' | 'task'; used?: boolean }
export const ISSUES: IssueSeed[] = [
  { key: 'APP-251', title: 'Exportar faturas em CSV', kind: 'story' },
  { key: 'APP-249', title: 'Timeout no webhook de pagamento', kind: 'bug' },
  { key: 'APP-247', title: 'Filtro por data na lista de clientes', kind: 'story' },
  { key: 'APP-246', title: 'Remover feature flag do checkout antigo', kind: 'task' },
  { key: 'APP-244', title: 'Mensagem de erro do cartão recusado', kind: 'bug' },
  { key: 'OPS-31', title: 'Alerta de fila parada no worker', kind: 'task' },
  { key: 'OPS-28', title: 'Rotacionar chaves do bucket de backups', kind: 'task' },
]

export const PROJECTS = [
  { id: '10000', key: 'APP', name: 'Aplicativo', avatar: null },
  { id: '10001', key: 'OPS', name: 'Operações', avatar: null },
]

export function createItem(text: string, issueKey: string | null, model: string, project: string | null = null) {
  const issue = issueKey ? ISSUES.find((i) => i.key === issueKey) : undefined
  const it = issue ? makeItem({ key: issue.key, title: issue.title }) : makeItem({ key: `${project ?? 'APP'}-${252 + seq}`, title: text.replace(/^./, (c) => c.toUpperCase()) })
  it.createdAt = store.clock
  it.model = model
  if (issue) {
    it.states.ticket = 'skipped'
    it.linked = true
    it.instructions = text
    issue.used = true
  }
  store.items.unshift(it)
  emit()
  return it
}

export function connect(c: ConnectionState) {
  store.connections.push(c)
  store.items.forEach((it) => {
    if (isBlocked(it) && hasNeed(STEPS[it.cur].needs)) startStep(it, it.cur)
  })
  emit()
}

export function disconnect(id: string) {
  store.connections = store.connections.filter((c) => c.id !== id)
  emit()
}

export function reset(connections: ConnectionState[]) {
  seq = 0
  prSeq = 1180
  store.clock = 0
  store.connections = connections.map((c) => ({ ...c }))
  ISSUES.forEach((i) => (i.used = false))
  store.items = [
    makeItem({ key: 'APP-228', title: 'Atualizar dependências', at: IDX.review }),
    makeItem({ key: 'APP-253', title: 'Paginação na lista de faturas', at: 0, planWaiting: true }),
    makeItem({ key: 'APP-242', title: 'Acessibilidade na tela de configurações', at: 0, elapsed: 2 }),
    makeItem({ key: 'APP-240', title: 'Retry de cobrança com backoff', at: IDX.develop, elapsed: 5, pause: 'ask' }),
    makeItem({ key: 'APP-238', title: 'Índice na tabela de pedidos', at: IDX.develop, elapsed: 5, pause: 'permission' }),
    makeItem({ key: 'APP-234', title: 'Crash no cold start', at: IDX.checks, elapsed: 2, fail: { checks: 1 } }),
    makeItem({ key: 'APP-219', title: 'Migração de auth para OIDC', at: IDX.ci, elapsed: 7, skipPlan: true }),
    makeItem({ key: 'APP-231', title: 'Teste flaky de auth', at: IDX.release }),
    makeItem({ key: 'APP-210', title: 'Revisão da documentação', done: true }),
    makeItem({ key: 'APP-205', title: 'Limite de taxa na API pública', done: true }),
  ]
  const first = store.items[0]
  first.attempts.checks.unshift({ state: 'failed', ticks: 6, start: store.clock - 60 })
  first.attempts.develop.unshift({ state: 'passed', ticks: 14, start: store.clock - 80 })
  first.reasons.push({ kind: 'checks', text: 'cargo nextest run falhou em deps::lock::resolves_semver: versão 2.x incompatível' })
  emit()
}
