import type { AskQuestion, Session, SessionEvent, ToolCall } from '../contract'
import { AGENTS, PEOPLE } from './fixtures'
import { say } from './say'
import { IDX, REPO, STEPS, diffOf, fileOf, isPaused, isReviewWaiting, moduleOf, store, type Attempt, type SimItem } from './sim'
import { iso } from './view'

const you = PEOPLE['u-you']

function devCalls(it: SimItem, again: boolean): ToolCall[] {
  const d = diffOf(it)
  const mod = moduleOf(it)
  return [
    { id: 'r', kind: 'read', title: say('readFile', { file: fileOf(it) }), tag: 'file', state: 'ok', durationMs: 40 },
    { id: 's', kind: 'search', title: say('searchedUses', { module: mod }), tag: 'search', state: 'ok', durationMs: 320 },
    { id: 'e', kind: 'edit', title: say('edited', { file: fileOf(it) }), tag: 'file', state: 'ok', durationMs: 30, diff: again ? '-  if (expired) return session\n+  if (expired) return refresh(session)' : `-  // TODO\n+  export function ${mod}() {\n+    return handle()\n+  }` },
    { id: 't', kind: 'command', title: say('ranTests', { module: mod }), tag: 'script', state: 'ok', durationMs: 6400, output: `${18 + (it.h % 30)} passed` },
    { id: 'c', kind: 'command', title: say('committed'), tag: 'git', logo: 'github', state: 'ok', durationMs: 300, output: `${d.files} files changed, ${d.additions} insertions(+), ${d.deletions} deletions(-)` },
  ]
}

function planCalls(it: SimItem): ToolCall[] {
  return [
    { id: 'pr', kind: 'search', title: say('explored', { repo: REPO }), tag: 'search', state: 'ok', durationMs: 900 },
    { id: 'pj', kind: 'call', title: say('searchedIssues'), tag: 'tracker', logo: 'jira', state: 'ok', durationMs: 700, input: { query: it.title } },
    { id: 'pf', kind: 'read', title: say('readFile', { file: fileOf(it) }), tag: 'file', state: 'ok', durationMs: 40 },
  ]
}

function progress(calls: ToolCall[], a: Attempt | undefined, dur: number): { calls: ToolCall[]; live: boolean } {
  if (!a) return { calls: [], live: false }
  if (a.state !== 'running') return { calls, live: false }
  const n = Math.min(calls.length - 1, Math.floor((a.ticks / dur) * calls.length))
  return { calls: [...calls.slice(0, n), { ...calls[n], state: 'running' }], live: true }
}

const ask = (): AskQuestion[] => [
  { question: say('askQuestion'), header: say('askHeader'), multiSelect: false, options: [{ label: say('attempts3'), description: say('attempts3Detail') }, { label: say('attempts5'), description: say('attempts5Detail') }] },
]

function planEvents(it: SimItem, ev: SessionEvent[], at: (t: number) => string) {
  if (!it.attempts.plan.length) return
  ev.push({ id: 'p-u', at: at(it.attempts.plan[0].start), type: 'user', author: you, text: it.linked ? `${say('planIssue', { key: it.key, title: it.title })}${it.instructions ? `\n\n${it.instructions}` : ''}` : it.title, attachments: 0 })
  it.attempts.plan.forEach((a, i) => {
    const revised = it.plans[i - 1]
    if (i > 0 && revised?.feedback) ev.push({ id: `p${i}-f`, at: at(a.start), type: 'user', author: you, text: revised.feedback, attachments: 0 })
    const { calls, live } = progress(planCalls(it), a, STEPS[IDX.plan].dur)
    ev.push({ id: `p${i}-t`, at: at(a.start + 1), type: 'thinking', seconds: 8, live: live && calls.length < 2, steps: [{ text: say('understanding'), done: true }, { text: say('lookingWhere'), done: !live }] })
    if (calls.length) ev.push({ id: `p${i}-c`, at: at(a.start + 2), type: 'tools', calls, seconds: 40, live, agent: 'planner' })
    const plan = it.plans[i]
    if (plan) ev.push({ id: `p${i}-plan`, at: at(plan.tick), type: 'plan', text: plan.text, resolution: plan.resolution, feedback: plan.feedback, agent: 'planner' })
  })
  it.planChat.forEach((c) => ev.push(c.role === 'user' ? { id: c.id, at: at(c.tick), type: 'user', author: you, text: c.text, attachments: 0 } : { id: c.id, at: at(c.tick), type: 'assistant', text: c.text, agent: c.agent }))
  if (it.states.plan === 'passed') ev.push({ id: 'p-a', at: at(it.attempts.plan[it.attempts.plan.length - 1].start + 7), type: 'assistant', agent: 'planner', text: it.linked ? say('issueUpdated', { key: it.key }) : say('issueCreated', { key: it.key }) })
}

function pauseEvents(it: SimItem, a: Attempt, ev: SessionEvent[], at: (t: number) => string) {
  const p = it.pause
  const waiting = isPaused(it)
  if (!p || a.ticks < p.at || (!waiting && p.answer === null && p.decision === null)) return
  if (p.kind === 'ask') ev.push({ id: 'd-ask', at: at(a.start + p.at), type: 'ask', questions: ask(), answers: waiting ? null : p.answer ?? [''], agent: 'developer' })
  else ev.push({ id: 'd-perm', at: at(a.start + p.at), type: 'permission', tool: 'exec', detail: say('migrationDetail'), risk: 'high', resolution: waiting ? 'pending' : p.decision ?? 'allowed' })
}

function developEvents(it: SimItem, ev: SessionEvent[], at: (t: number) => string) {
  it.attempts.develop.forEach((a, i) => {
    const reason = i > 0 ? it.reasons.filter((r) => r.kind !== 'plan')[i - 1] : undefined
    const hasPlan = it.attempts.plan.length > 0
    if (i === 0) ev.push({ id: `d${i}-u`, at: at(a.start), type: 'user', author: you, text: hasPlan ? say('implementPlan', { key: it.key }) : say('implementIssue', { key: it.key, title: it.title }), attachments: 0 })
    else if (reason?.kind === 'review') ev.push({ id: `d${i}-u`, at: at(a.start), type: 'user', author: you, text: reason.text, attachments: 0 })
    else if (reason) ev.push({ id: `d${i}-s`, at: at(a.start), type: 'status', status: 'error', text: reason.text })
    const { calls, live } = progress(devCalls(it, i > 0), a.state === 'waiting' ? { ...a, state: 'running' } : a, STEPS[IDX.develop].dur)
    if (calls.length) ev.push({ id: `d${i}-c`, at: at(a.start + 1), type: 'tools', calls, seconds: 60 + i * 20, live: live && a.state === 'running', agent: 'developer' })
    if (i === it.attempts.develop.length - 1) pauseEvents(it, a, ev, at)
    if (a.state === 'passed') ev.push({ id: `d${i}-a`, at: at(a.start + a.ticks), type: 'assistant', agent: 'developer', text: i === 0 ? say('devDone', { file: fileOf(it), module: moduleOf(it) }) : say('devFixed', { what: reason?.kind === 'review' ? say('appliedRequest') : say('failingTestPasses') }) })
  })
}

function workSession(it: SimItem): Session {
  const ev: SessionEvent[] = []
  planEvents(it, ev, iso)
  developEvents(it, ev, iso)
  const running = !it.done && !it.bounce && (it.states.plan === 'running' || it.states.develop === 'running')
  const waiting = it.states.plan === 'waiting' || isPaused(it)
  return { id: `s-${it.id}`, title: `${it.key} · ${it.title}`, status: running ? 'running' : waiting ? 'waiting' : 'idle', mode: it.states.plan === 'running' || it.states.plan === 'waiting' ? 'plan' : 'auto', model: it.model, agents: AGENTS, events: ev, suggestion: !running && !waiting && it.states.develop === 'passed' ? say('explainChange', { file: fileOf(it) }) : null }
}

function reviewSession(it: SimItem): Session {
  const d = diffOf(it)
  const ev: SessionEvent[] = [{ id: 'r-a', at: iso(store.clock - 20), type: 'assistant', agent: 'reviewer', text: say('reviewSummary', { pr: it.pr ?? '', files: d.files, additions: d.additions, deletions: d.deletions, file: fileOf(it) }) }]
  it.reviewChat.forEach((c) => ev.push(c.role === 'user' ? { id: c.id, at: iso(c.tick), type: 'user', author: you, text: c.text, attachments: 0 } : { id: c.id, at: iso(c.tick), type: 'assistant', text: c.text, agent: c.agent }))
  const replying = it.reviewChat.length > 0 && it.reviewChat[it.reviewChat.length - 1].role === 'user'
  return { id: `r-${it.id}`, title: `Review · ${it.key}`, status: replying ? 'running' : isReviewWaiting(it) ? 'idle' : 'idle', mode: 'auto', model: it.model, agents: AGENTS, events: ev, suggestion: replying ? null : say('whichTests') }
}

export function sessionById(id: string): Session | null {
  const [kind, raw] = id.split('-')
  const it = store.items.find((x) => String(x.id) === raw)
  if (!it) return null
  if (kind === 's') return workSession(it)
  if (kind === 'r' && (it.done || it.cur >= IDX.review)) return reviewSession(it)
  return null
}

export function itemOfSession(id: string) {
  const [kind, raw] = id.split('-')
  const it = store.items.find((x) => String(x.id) === raw)
  return it ? { it, kind: kind === 'r' ? ('review' as const) : ('plan' as const) } : null
}
