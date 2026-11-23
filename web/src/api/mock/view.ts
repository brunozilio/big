import type { Attempt as WireAttempt, Attention, Block, ChangeRef, Command, ExternalRef, Item, ItemStep, ProviderMark, StepState, StepView } from '../contract'
import { NEED_PROVIDER, PEOPLE, PROVIDERS } from './fixtures'
import {
  IDX, REPO, STEPS, diffOf, fileOf, isBlocked, isGate, isMergeReady, isPaused, isPlanWaiting, isReviewWaiting, store, testName, withYou,
  type SimItem, type SimState, type Step, type StepId,
} from './sim'

export const EPOCH = Date.UTC(2026, 10, 22, 13, 0, 0)
export const TICK_MS = 60_000

export const iso = (tick: number) => new Date(EPOCH + tick * TICK_MS).toISOString()

export const prUrl = (n: number) => `https://github.com/${REPO}/pull/${n}`
export const issueUrl = (key: string) => `https://acme.atlassian.net/browse/${key}`

export function wireState(s: SimState): StepState {
  if (s === 'waiting' || s === 'blocked') return 'waiting_human'
  return s
}

export function mark(provider: string): ProviderMark {
  const p = PROVIDERS.find((x) => x.key === provider)
  return { provider, name: p?.name ?? provider, logo: p?.logo ?? provider }
}

const connectionOf = (provider: string) => store.connections.find((c) => c.provider === provider)?.id ?? `c-${provider}`

function issueRef(it: SimItem): ExternalRef {
  return { ...mark('jira'), connection: connectionOf('jira'), key: it.key, url: issueUrl(it.key) }
}

function changeRef(it: SimItem): ChangeRef | null {
  if (!it.pr) return null
  const ci = it.states.ci
  const checks = ci === 'passed' ? 'passed' : ci === 'failed' ? 'failed' : ci === 'running' ? 'running' : 'pending'
  return { ...mark('github'), connection: connectionOf('github'), key: `#${it.pr}`, url: prUrl(it.pr), checks, merged: it.states.merge === 'passed' }
}

function steps(it: SimItem): ItemStep[] {
  return STEPS.map((s) => ({ id: s.id, label: s.label, icon: s.icon, logo: s.logo, state: wireState(it.states[s.id]), attempts: it.attempts[s.id].length, seconds: it.attempts[s.id].reduce((n, a) => n + a.ticks, 0) * 60 }))
}

function needMark(s: Step): ProviderMark | null {
  if (!s.needs) return null
  return mark(NEED_PROVIDER[s.needs])
}

function attention(it: SimItem): Attention | null {
  if (isPlanWaiting(it)) return { badge: { text: 'Plano pronto', tone: 'info', icon: 'sPlan' }, diff: null, action: { label: 'Revisar plano', icon: 'sPlan', step: 'plan' } }
  if (isPaused(it)) {
    const ask = it.pause?.kind === 'ask'
    return { badge: { text: ask ? 'Pergunta do agente' : 'Pede permissão', tone: 'warning', icon: ask ? 'help' : 'lock' }, diff: null, action: { label: ask ? 'Responder' : 'Ver pedido', icon: 'chat', step: 'develop' } }
  }
  if (isMergeReady(it)) return { badge: { text: 'Aprovado', tone: 'success', icon: 'check' }, diff: null, action: { label: 'Fazer merge', icon: 'sMerge', step: 'merge' } }
  if (isGate(it)) return { badge: it.rejected ? { text: 'Deploy rejeitado', tone: 'danger' } : { text: 'production aguardando', tone: 'warning', icon: 'clock' }, diff: null, action: { label: it.rejected ? 'Ver deploy' : 'Revisar deploy', icon: 'hRocket', step: 'release' } }
  if (isReviewWaiting(it)) {
    const d = diffOf(it)
    return { badge: null, diff: { ...d, url: it.pr ? `${prUrl(it.pr)}/files` : null }, action: { label: 'Revisar', icon: 'hGitCompare', step: 'review' } }
  }
  return null
}

export function toItem(it: SimItem): Omit<Item, 'updatedAt'> {
  const cur = STEPS[it.cur]
  const blocked = isBlocked(it)
  const need = cur && blocked ? needMark(cur) : null
  return {
    id: String(it.id),
    title: it.title,
    workflow: 'sdlc',
    group: it.done ? 'delivered' : withYou(it) ? 'you' : 'running',
    status: it.done ? 'Done' : it.cur >= IDX.review ? 'In Review' : 'In Progress',
    issue: issueRef(it),
    change: changeRef(it),
    steps: steps(it),
    current: cur?.id ?? null,
    live: !it.done && !it.bounce && !!cur && (cur.id === 'plan' || cur.id === 'develop') && it.states[cur.id] === 'running',
    activity: it.done ? null : it.bounce ? { text: 'Voltando ao develop', since: iso(store.clock), tone: 'danger' } : { text: cur.doing, since: iso(store.clock - it.elapsed) },
    impediment: blocked ? { reason: `Falta conectar ${need?.name ?? 'um provedor'}`, connect: need } : null,
    attention: attention(it),
    createdAt: iso(it.createdAt),
    doneAt: it.done ? iso(it.doneAt) : null,
  }
}

const muted = (text: string): Block => ({ type: 'markdown', text, muted: true })

function attemptsOf(it: SimItem, id: StepId): WireAttempt[] {
  return it.attempts[id].map((a, i) => ({ n: i + 1, state: wireState(a.state), startedAt: iso(a.start), endedAt: a.state === 'running' || a.state === 'waiting' ? null : iso(a.start + a.ticks) }))
}

export function logFor(it: SimItem, cmd: string, bad: boolean, running: boolean): string {
  const mod = testName(it).split('::')[0]
  const tail = running ? '\n▍' : ''
  if (cmd.includes('lint')) return `$ ${cmd}\n\n  Checked 214 files in 1.8s\n  No issues found${tail}`
  if (cmd.includes('typecheck')) return `$ ${cmd}\n\n  tsc -b\n  Found 0 errors${tail}`
  if (cmd === 'bun test') return bad
    ? `$ bun test\n\n  ${mod}.test.ts:\n  ✓ ${mod} › carrega a configuração [3ms]\n  ✗ ${testName(it)} [12ms]\n\n  error: expect(received).toBe(expected)\n    Expected: "Refreshed"\n    Received: "Expired"\n\n      at ${fileOf(it)}:88:9\n\n  212 pass\n  1 fail`
    : `$ bun test\n\n  ${mod}.test.ts:\n  ✓ ${mod} › carrega a configuração [3ms]\n  ✓ ${testName(it)} [12ms]\n\n  213 pass\n  0 fail${tail}`
  if (cmd === 'build') return `$ bun run build\n\n  vite v6.0.5 building for production...\n  ✓ 950 modules transformed.\n  ✓ built in 2.56s${tail}`
  if (cmd.includes('canary') || cmd.includes('rollout') || cmd.includes('imagem')) return `$ ${cmd}\n\n  deployment.apps/checkout configured\n  Waiting for rollout to finish: 3 of 3 updated replicas are available...\n  deployment "checkout" successfully rolled out${tail}`
  return `$ ${cmd}\n\n  ok${tail}`
}

export function commands(it: SimItem, names: string[], state: SimState, ticks: number, dur: number, failAt = -1): Command[] {
  const done = state === 'passed' ? names.length : state === 'failed' ? failAt + 1 : Math.floor((ticks / dur) * names.length)
  return names.map((n, i) => {
    const st = state === 'failed' && i === failAt ? 'failed' : i < done ? 'done' : i === done && state === 'running' ? 'running' : 'todo'
    return { name: n, state: st, seconds: st === 'done' || st === 'failed' ? i * 17 + 8 : null, log: st === 'todo' ? '' : logFor(it, n, st === 'failed', st === 'running') }
  })
}

function pickAttempt(it: SimItem, id: StepId, attempt: number | null) {
  const n = it.attempts[id].length
  return attempt === null || attempt < 1 || attempt > n ? n : attempt
}

function blocksFor(it: SimItem, s: Step, attempt: number): Block[] {
  const st = it.states[s.id]
  const github = connectionOf('github')
  if (st === 'pending') return [muted('Ainda não começou.')]
  if (s.id === 'ticket') return [{ type: 'issue', connection: connectionOf('jira'), key: it.key, comment: true }]
  if (st === 'skipped') return [muted('Pulado. O item veio de uma issue.')]
  if (st === 'blocked') return [muted('Parado até a conexão existir.')]
  if (s.id === 'plan' || s.id === 'develop') return [{ type: 'chat', session: `s-${it.id}`, composer: s.id === 'plan' && (st === 'running' || st === 'waiting') }]
  if (s.id === 'checks') {
    const a = it.attempts.checks[attempt - 1]
    return [{ type: 'logs', commands: a ? commands(it, ['bun run lint', 'bun run typecheck', 'bun test'], a.state, a.ticks, s.dur, 2) : [] }]
  }
  if (s.id === 'pr') return [{ type: 'change', connection: github, change: `#${it.pr}` }]
  if (s.id === 'ci') return [{ type: 'pipeline', connection: github, run: `ci:${it.id}:${attempt}`, gate: null }]
  if (s.id === 'release') return [{ type: 'pipeline', connection: github, run: `release:${it.id}:${attempt}`, gate: { approve: 'deploy.approve', reject: 'deploy.reject' } }]
  if (s.id === 'review') return [{ type: 'review', connection: github, change: `#${it.pr}`, editable: st === 'waiting', session: `r-${it.id}`, submit: 'review' }]
  const approver = it.approvedBy ? PEOPLE[it.approvedBy] : undefined
  const checks = [
    { label: 'Aprovado', text: approver?.name ?? '—', person: approver },
    { label: 'Checks', text: 'CI passou' },
    { label: 'Conflitos', text: 'Nenhum conflito com main' },
  ]
  const result = st === 'passed' ? [
    { label: 'Aprovado por', text: approver?.name ?? '—', person: approver, note: `há ${Math.max(1, store.clock - it.approvedAt)}m` },
    { label: 'Mergeado por', text: PEOPLE['u-you'].name, person: PEOPLE['u-you'] },
    { label: 'Método', text: 'Squash and merge' },
    { label: 'Commit', text: it.h.toString(16).slice(0, 7), mono: true, url: it.pr ? prUrl(it.pr) : undefined },
    { label: 'Branch', text: it.branch, mono: true, note: 'removida' },
  ] : null
  return [{ type: 'merge', connection: github, change: `#${it.pr}`, ready: st === 'waiting', methods: ['squash', 'merge', 'rebase'], action: 'merge', checks, result }]
}

export function stepView(it: SimItem, step: StepId, attempt: number | null): StepView {
  const s = STEPS[IDX[step]]
  const n = pickAttempt(it, step, attempt)
  return { item: String(it.id), step, attempt: n, attempts: attemptsOf(it, step), blocks: blocksFor(it, s, n) }
}

