import type { AskQuestion, Session, SessionEvent, ToolCall } from '../contract'
import { AGENTS, PEOPLE } from './fixtures'
import { IDX, REPO, STEPS, diffOf, fileOf, isPaused, isReviewWaiting, moduleOf, store, type Attempt, type SimItem } from './sim'
import { iso } from './view'

const you = PEOPLE['u-you']

function devCalls(it: SimItem, again: boolean): ToolCall[] {
  const d = diffOf(it)
  const mod = moduleOf(it)
  return [
    { id: 'r', kind: 'read', title: `Leu ${fileOf(it)}`, tag: 'file', state: 'ok', durationMs: 40 },
    { id: 's', kind: 'search', title: `Buscou usos de ${mod}`, tag: 'search', state: 'ok', durationMs: 320 },
    { id: 'e', kind: 'edit', title: `Editou ${fileOf(it)}`, tag: 'file', state: 'ok', durationMs: 30, diff: again ? '-  if (expired) return session\n+  if (expired) return refresh(session)' : `-  // TODO\n+  export function ${mod}() {\n+    return handle()\n+  }` },
    { id: 't', kind: 'command', title: `Rodou bun test ${mod}`, tag: 'script', state: 'ok', durationMs: 6400, output: `${18 + (it.h % 30)} passed` },
    { id: 'c', kind: 'command', title: 'Fez o commit', tag: 'git', logo: 'github', state: 'ok', durationMs: 300, output: `${d.files} files changed, ${d.additions} insertions(+), ${d.deletions} deletions(-)` },
  ]
}

function planCalls(it: SimItem): ToolCall[] {
  return [
    { id: 'pr', kind: 'search', title: `Explorou ${REPO}`, tag: 'search', state: 'ok', durationMs: 900 },
    { id: 'pj', kind: 'call', title: 'Buscou issues parecidas', tag: 'tracker', logo: 'jira', state: 'ok', durationMs: 700, input: { query: it.title } },
    { id: 'pf', kind: 'read', title: `Leu ${fileOf(it)}`, tag: 'file', state: 'ok', durationMs: 40 },
  ]
}

function progress(calls: ToolCall[], a: Attempt | undefined, dur: number): { calls: ToolCall[]; live: boolean } {
  if (!a) return { calls: [], live: false }
  if (a.state !== 'running') return { calls, live: false }
  const n = Math.min(calls.length - 1, Math.floor((a.ticks / dur) * calls.length))
  return { calls: [...calls.slice(0, n), { ...calls[n], state: 'running' }], live: true }
}

const ASK: AskQuestion[] = [
  { question: 'Qual limite de tentativas o retry da cobrança deve usar?', header: 'Tentativas', multiSelect: false, options: [{ label: '3 tentativas', description: 'Segue o padrão de src/lib/retry.ts' }, { label: '5 tentativas', description: 'Igual ao checkout' }] },
]

function planEvents(it: SimItem, ev: SessionEvent[], at: (t: number) => string) {
  if (!it.attempts.plan.length) return
  ev.push({ id: 'p-u', at: at(it.attempts.plan[0].start), type: 'user', author: you, text: it.linked ? `Planeje ${it.key}: ${it.title}${it.instructions ? `\n\n${it.instructions}` : ''}` : it.title, attachments: 0 })
  it.attempts.plan.forEach((a, i) => {
    const revised = it.plans[i - 1]
    if (i > 0 && revised?.feedback) ev.push({ id: `p${i}-f`, at: at(a.start), type: 'user', author: you, text: revised.feedback, attachments: 0 })
    const { calls, live } = progress(planCalls(it), a, STEPS[IDX.plan].dur)
    ev.push({ id: `p${i}-t`, at: at(a.start + 1), type: 'thinking', seconds: 8, live: live && calls.length < 2, steps: [{ text: 'Entendendo o pedido', done: true }, { text: 'Procurando onde mexer', done: !live }] })
    if (calls.length) ev.push({ id: `p${i}-c`, at: at(a.start + 2), type: 'tools', calls, seconds: 40, live, agent: 'planner' })
    const plan = it.plans[i]
    if (plan) ev.push({ id: `p${i}-plan`, at: at(plan.tick), type: 'plan', text: plan.text, resolution: plan.resolution, feedback: plan.feedback, agent: 'planner' })
  })
  it.planChat.forEach((c) => ev.push(c.role === 'user' ? { id: c.id, at: at(c.tick), type: 'user', author: you, text: c.text, attachments: 0 } : { id: c.id, at: at(c.tick), type: 'assistant', text: c.text, agent: c.agent }))
  if (it.states.plan === 'passed') ev.push({ id: 'p-a', at: at(it.attempts.plan[it.attempts.plan.length - 1].start + 7), type: 'assistant', agent: 'planner', text: it.linked ? `Atualizei a issue **${it.key}** com o plano.` : `Criei a issue **${it.key}** com o plano.` })
}

function pauseEvents(it: SimItem, a: Attempt, ev: SessionEvent[], at: (t: number) => string) {
  const p = it.pause
  const waiting = isPaused(it)
  if (!p || a.ticks < p.at || (!waiting && p.answer === null && p.decision === null)) return
  if (p.kind === 'ask') ev.push({ id: 'd-ask', at: at(a.start + p.at), type: 'ask', questions: ASK, answers: waiting ? null : p.answer ?? [''], agent: 'developer' })
  else ev.push({ id: 'd-perm', at: at(a.start + p.at), type: 'permission', tool: 'exec', detail: 'Rodar a migração no banco de staging: bun run db:migrate --env staging', risk: 'high', resolution: waiting ? 'pending' : p.decision ?? 'allowed' })
}

function developEvents(it: SimItem, ev: SessionEvent[], at: (t: number) => string) {
  it.attempts.develop.forEach((a, i) => {
    const reason = i > 0 ? it.reasons.filter((r) => r.kind !== 'plan')[i - 1] : undefined
    const hasPlan = it.attempts.plan.length > 0
    if (i === 0) ev.push({ id: `d${i}-u`, at: at(a.start), type: 'user', author: you, text: hasPlan ? `Implemente ${it.key} seguindo o plano.` : `Implemente ${it.key}: ${it.title}`, attachments: 0 })
    else if (reason?.kind === 'review') ev.push({ id: `d${i}-u`, at: at(a.start), type: 'user', author: you, text: reason.text, attachments: 0 })
    else if (reason) ev.push({ id: `d${i}-s`, at: at(a.start), type: 'status', status: 'error', text: reason.text })
    const { calls, live } = progress(devCalls(it, i > 0), a.state === 'waiting' ? { ...a, state: 'running' } : a, STEPS[IDX.develop].dur)
    if (calls.length) ev.push({ id: `d${i}-c`, at: at(a.start + 1), type: 'tools', calls, seconds: 60 + i * 20, live: live && a.state === 'running', agent: 'developer' })
    if (i === it.attempts.develop.length - 1) pauseEvents(it, a, ev, at)
    if (a.state === 'passed') ev.push({ id: `d${i}-a`, at: at(a.start + a.ticks), type: 'assistant', agent: 'developer', text: i === 0 ? `Pronto. Alterei \`${fileOf(it)}\` e os testes de \`${moduleOf(it)}\` passam. Os checks e o PR seguem pelo workflow.` : `Corrigi: ${reason?.kind === 'review' ? 'apliquei o que você pediu' : 'o teste que falhava agora passa'}. Mandei de volta para os checks.` })
  })
}

function workSession(it: SimItem): Session {
  const ev: SessionEvent[] = []
  planEvents(it, ev, iso)
  developEvents(it, ev, iso)
  const running = !it.done && !it.bounce && (it.states.plan === 'running' || it.states.develop === 'running')
  const waiting = it.states.plan === 'waiting' || isPaused(it)
  return { id: `s-${it.id}`, title: `${it.key} · ${it.title}`, status: running ? 'running' : waiting ? 'waiting' : 'idle', mode: it.pause?.kind === 'permission' ? 'bypass' : it.states.plan === 'running' || it.states.plan === 'waiting' ? 'plan' : 'accept_edits', model: it.model, agents: AGENTS, events: ev }
}

function reviewSession(it: SimItem): Session {
  const d = diffOf(it)
  const ev: SessionEvent[] = [{ id: 'r-a', at: iso(store.clock - 20), type: 'assistant', agent: 'reviewer', text: `Resumo do **PR #${it.pr}** (${d.files} arquivos, +${d.additions} −${d.deletions}):\n\n- \`${fileOf(it)}\`: renova a sessão expirada em vez de devolvê-la\n- Teste novo cobre o caso que falhava\n- \`src/lib/retry.ts\`: usa o backoff existente\n\nPergunte o que quiser sobre as mudanças.` }]
  it.reviewChat.forEach((c) => ev.push(c.role === 'user' ? { id: c.id, at: iso(c.tick), type: 'user', author: you, text: c.text, attachments: 0 } : { id: c.id, at: iso(c.tick), type: 'assistant', text: c.text, agent: c.agent }))
  const replying = it.reviewChat.length > 0 && it.reviewChat[it.reviewChat.length - 1].role === 'user'
  return { id: `r-${it.id}`, title: `Review · ${it.key}`, status: replying ? 'running' : isReviewWaiting(it) ? 'idle' : 'idle', mode: 'ask', model: it.model, agents: AGENTS, events: ev }
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
