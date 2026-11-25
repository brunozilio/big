import type { Application, Attempt as WireAttempt, Attention, Block, ChangeRef, Command, ExternalRef, HeadsUp, Item, ItemGit, ItemSource, ItemStep, NewItem, ProviderMark, StepState, StepView } from '../contract'
import { NEED_PROVIDER, PEOPLE, providers } from './fixtures'
import { say } from './say'
import { autoTriage, findingsFor, qualityFor } from './security'
import {
  IDX, REPO, STEPS, diffOf, fileOf, isBlocked, isGate, isMergeReady, isPaused, isTriageWaiting, isPlanWaiting, isReviewWaiting, store, testName, withYou,
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
  const p = providers().find((x) => x.key === provider)
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

export function originOf(input: NewItem, app: Application | undefined): { sources: ItemSource[]; git: ItemGit[] } {
  const sources = (input.sources ?? [])
    .filter((s) => s.kind !== 'issue')
    .map((s): ItemSource => {
      const provider = s.kind === 'pull_request' ? mark('github') : { provider: null, name: null, logo: null }
      return { ...provider, kind: s.kind, key: s.subject, connection: s.connection ?? (s.kind === 'pull_request' ? connectionOf('github') : null), repository: s.repository ?? null, url: null }
    })
  const git = Object.entries(input.git ?? {}).map(([repository, b]): ItemGit => {
    const repo = app?.repositories.find((r) => r.id === repository)
    return { repository, name: repo?.code?.repo ?? repo?.directory ?? repository, base: b.base ?? null, head: b.head ?? null }
  })
  return { sources, git }
}

function sourcesOf(it: SimItem): ItemSource[] {
  const linked = it.linked || it.states.ticket === 'passed'
  const issue: ItemSource[] = linked ? [{ ...issueRef(it), kind: 'issue', repository: null }] : []
  return [...issue, ...(it.origin?.sources ?? [])]
}

function steps(it: SimItem): ItemStep[] {
  return STEPS.map((s) => ({ id: s.id, label: s.label, icon: s.icon, logo: s.logo, requires: s.needs, provider: s.logo ? mark(s.logo).name : undefined, state: wireState(it.states[s.id]), attempts: it.attempts[s.id].length, seconds: it.attempts[s.id].reduce((n, a) => n + a.ticks, 0) * 60 }))
}

function needMark(s: Step): ProviderMark | null {
  if (!s.needs) return null
  return mark(NEED_PROVIDER[s.needs])
}

function attention(it: SimItem): Attention | null {
  if (isPlanWaiting(it)) return { badge: { text: say('planReady'), tone: 'info', icon: 'sPlan', said: { key: 'plan_ready', vars: {} } }, diff: null, action: { label: say('reviewPlan'), icon: 'sPlan', step: 'plan', said: { key: 'review_plan', vars: {} } } }
  if (isPaused(it)) {
    const ask = it.pause?.kind === 'ask'
    return { badge: { text: ask ? say('agentQuestion') : say('asksPermission'), tone: 'warning', icon: ask ? 'help' : 'lock', said: ask ? { key: 'agent_question', vars: {} } : null }, diff: null, action: { label: ask ? say('answer') : say('seeRequest'), icon: 'chat', step: 'develop', said: ask ? { key: 'answer', vars: {} } : null } }
  }
  if (isMergeReady(it)) return { badge: { text: say('approved'), tone: 'success', icon: 'check', said: null }, diff: null, action: { label: say('doMerge'), icon: 'sMerge', step: 'merge', said: null } }
  if (isGate(it)) return { badge: it.rejected ? { text: say('deployRejected'), tone: 'danger', said: null } : { text: say('productionWaiting'), tone: 'warning', icon: 'clock', said: null }, diff: null, action: { label: it.rejected ? say('seeDeploy') : say('reviewDeploy'), icon: 'hRocket', step: 'release', said: null } }
  if (isTriageWaiting(it)) return { badge: { text: say('findingsReady', { n: findingsFor(it, null).filter((f) => !f.held).length }), tone: 'danger', icon: 'shield', said: null }, diff: null, action: { label: say('triage'), icon: 'hSecurityCheck', step: 'security', said: null } }
  if (isReviewWaiting(it)) {
    const d = diffOf(it)
    return { badge: null, diff: { ...d, url: it.pr ? `${prUrl(it.pr)}/files` : null }, action: { label: say('review'), icon: 'hGitCompare', step: 'review', said: null } }
  }
  return null
}

function recap(it: SimItem): string | null {
  if (isPlanWaiting(it)) return say('recapPlan', { key: it.key })
  if (isPaused(it)) return it.pause?.kind === 'ask'
    ? say('recapAsk')
    : say('recapPermission')
  if (isReviewWaiting(it)) return say('recapReview', { pr: it.pr ?? '' })
  if (isMergeReady(it)) return say('recapMerge', { pr: it.pr ?? '' })
  if (isGate(it)) return it.rejected ? say('recapRejected') : say('recapGate')
  if (isTriageWaiting(it)) return say('recapTriage')
  return null
}

export function toItem(it: SimItem): Omit<Item, 'updatedAt'> {
  const cur = STEPS[it.cur]
  const blocked = isBlocked(it)
  const need = cur && blocked ? needMark(cur) : null
  return {
    id: String(it.id),
    title: it.title,
    workflow: it.workflow ?? 'sdlc',
    group: it.done ? 'delivered' : withYou(it) ? 'you' : 'running',
    status: it.done ? 'Done' : it.cur >= IDX.review ? 'In Review' : 'In Progress',
    issue: it.linked || it.states.ticket === 'passed' ? issueRef(it) : null,
    change: changeRef(it),
    sources: sourcesOf(it),
    git: it.origin?.git ?? [],
    steps: steps(it),
    current: cur?.id ?? null,
    live: !it.done && !it.bounce && !!cur && (cur.id === 'plan' || cur.id === 'develop') && it.states[cur.id] === 'running',
    activity: it.done ? null : it.bounce ? { text: say('backToDevelop'), since: iso(store.clock), tone: 'danger' } : { text: cur.doing, since: iso(store.clock - it.elapsed) },
    impediment: blocked ? { reason: say('missingConnection', { name: need?.name ?? say('aProvider') }), connect: need, said: need ? { key: 'connect_named', vars: { name: need.name } } : { key: 'connect_account', vars: {} } } : null,
    attention: attention(it),
    recap: withYou(it) ? recap(it) : null,
    createdAt: iso(it.createdAt),
    doneAt: it.done ? iso(it.doneAt) : null,
    archivedAt: it.archivedAt == null ? null : iso(it.archivedAt),
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
    ? `$ bun test\n\n  ${mod}.test.ts:\n  ✓ ${mod} › ${say('loadsConfig')} [3ms]\n  ✗ ${testName(it)} [12ms]\n\n  error: expect(received).toBe(expected)\n    Expected: "Refreshed"\n    Received: "Expired"\n\n      at ${fileOf(it)}:88:9\n\n  212 pass\n  1 fail`
    : `$ bun test\n\n  ${mod}.test.ts:\n  ✓ ${mod} › ${say('loadsConfig')} [3ms]\n  ✓ ${testName(it)} [12ms]\n\n  213 pass\n  0 fail${tail}`
  if (cmd === 'build') return `$ bun run build\n\n  vite v6.0.5 building for production...\n  ✓ 950 modules transformed.\n  ✓ built in 2.56s${tail}`
  if (cmd.includes('canary') || cmd.includes('rollout') || cmd === say('imageBuild')) return `$ ${cmd}\n\n  deployment.apps/checkout configured\n  Waiting for rollout to finish: 3 of 3 updated replicas are available...\n  deployment "checkout" successfully rolled out${tail}`
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
  if (st === 'pending') return [muted(say('notStarted'))]
  if (s.id === 'ticket') return [{ type: 'issue', connection: connectionOf('jira'), key: it.key, comment: true }]
  if (st === 'skipped') return [muted(say('skippedFromIssue'))]
  if (st === 'blocked') return [muted(say('waitingConnection'))]
  if (s.id === 'plan' || s.id === 'develop') return [{ type: 'chat', session: `s-${it.id}`, composer: s.id === 'plan' && (st === 'running' || st === 'waiting') }]
  if (s.id === 'checks') {
    const a = it.attempts.checks[attempt - 1]
    return [{ type: 'logs', commands: a ? commands(it, ['bun run lint', 'bun run typecheck', 'bun test'], a.state, a.ticks, s.dur, 2) : [] }]
  }
  if (s.id === 'pr') return [{ type: 'change', connection: github, change: `#${it.pr}` }]
  if (s.id === 'ci') return [{ type: 'pipeline', connection: github, run: `ci:${it.id}:${attempt}`, gate: null }]
  if (s.id === 'release') return [{ type: 'pipeline', connection: github, run: `release:${it.id}:${attempt}`, gate: { approve: 'deploy.approve', reject: 'deploy.reject' } }]
  if (s.id === 'security') {
    if (st === 'running') return [muted(say('doingSecurity'))]
    const at = it.done ? it.doneAt - 2 : store.clock - 1
    const waiting = isTriageWaiting(it)
    const triage = waiting ? null : it.triage ?? autoTriage(it)
    return [{ type: 'findings', connection: connectionOf('veracode'), project: 'checkout', scannedAt: iso(at), url: 'https://analysiscenter.veracode.com/', triage: waiting ? 'findings.triage' : null, findings: findingsFor(it, triage) }]
  }
  if (s.id === 'quality') {
    if (st === 'running') return [muted(say('doingQuality'))]
    const a = it.attempts.quality[attempt - 1]
    const done = a ? a.start + a.ticks : store.clock
    return [{ type: 'quality', connection: connectionOf('sonar'), project: 'acme_checkout', branch: 'main', analyzedAt: iso(done), url: 'https://sonarcloud.io/summary/new_code?id=acme_checkout', report: qualityFor(it, a?.state === 'failed') }]
  }
  if (s.id === 'review') return [{ type: 'review', connection: github, change: `#${it.pr}`, editable: st === 'waiting', session: `r-${it.id}`, submit: 'review' }]
  const approver = it.approvedBy ? PEOPLE[it.approvedBy] : undefined
  const checks = [
    { label: say('approved'), text: approver?.name ?? '—', person: approver },
    { label: 'Checks', text: say('ciPassed') },
    { label: say('conflicts'), text: say('noConflicts') },
  ]
  const result = st === 'passed' ? [
    { label: say('approvedBy'), text: approver?.name ?? '—', person: approver, note: say('minutesAgo', { n: Math.max(1, store.clock - it.approvedAt) }) },
    { label: say('mergedBy'), text: PEOPLE['u-you'].name, person: PEOPLE['u-you'] },
    { label: say('method'), text: 'Squash and merge' },
    { label: 'Commit', text: it.h.toString(16).slice(0, 7), mono: true, url: it.pr ? prUrl(it.pr) : undefined },
    { label: 'Branch', text: it.branch, mono: true, note: say('removed') },
  ] : null
  return [{ type: 'merge', connection: github, change: `#${it.pr}`, ready: st === 'waiting', methods: ['squash', 'merge', 'rebase'], action: 'merge', checks, result }]
}

export function stepView(it: SimItem, step: StepId, attempt: number | null): StepView {
  const s = STEPS[IDX[step]]
  const n = pickAttempt(it, step, attempt)
  return { item: String(it.id), step, attempt: n, attempts: attemptsOf(it, step), blocks: blocksFor(it, s, n), headsUp: headsUpFor(it, step) }
}

function headsUpFor(it: SimItem, step: StepId): HeadsUp | null {
  if (step === 'develop' && it.pause?.kind === 'permission' && isPaused(it)) return { title: say('stagingMigration'), explain: say('stagingMigrationExplain'), urgent: true }
  if (step === 'develop' && it.attempts.develop.length > 1) return { title: say('testFailedBefore'), explain: say('testFailedExplain', { test: testName(it) }), urgent: false }
  return null
}

