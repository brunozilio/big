import type { ChangeData, Comment, FileDiff, IssueData, IssueSummary, Job, JobState, PipelineRun } from '../contract'
import { agentPerson, PEOPLE } from './fixtures'
import { ISSUES, IDX, REPO, STEPS, commentIssue, fileOf, moduleOf, store, type SimItem } from './sim'
import { commands, iso, issueUrl, prUrl } from './view'

function prFiles(it: SimItem): FileDiff[] {
  const mod = moduleOf(it)
  const main = fileOf(it)
  const files = [
    { path: main, diff: [` import { handle } from './${mod}'`, ` `, ` export async function ${mod}(input: Input) {`, `-  const result = await handle(input)`, `-  if (result.expired) return result`, `+  const result = await handle(input)`, `+  if (result.expired) {`, `+    return refresh(result, { maxAttempts: 3 })`, `+  }`, `   return result`, ` }`].join('\n') },
    { path: main.replace(/\.tsx?$/, '.test.ts'), diff: [` describe('${mod}', () => {`, `+  it('renova a sessão expirada', async () => {`, `+    const result = await ${mod}(expiredInput)`, `+    expect(result.state).toBe('Refreshed')`, `+  })`, `+`, `   it('carrega a configuração', async () => {`].join('\n') },
    { path: 'src/lib/retry.ts', diff: [` export function refresh<T>(value: T, opts: RetryOptions) {`, `-  return value`, `+  return withBackoff(() => renew(value), opts)`, ` }`].join('\n') },
  ]
  return files.map((f) => {
    const lines = f.diff.split('\n')
    return { ...f, firstLine: 41, additions: lines.filter((l) => l.startsWith('+')).length, deletions: lines.filter((l) => l.startsWith('-')).length }
  })
}

const minutesAgo = (m: number) => iso(store.clock - m)

function prComments(it: SimItem): Comment[] {
  const main = fileOf(it)
  const out: Comment[] = [{ id: 'c0', author: agentPerson('developer'), body: `Implementa ${it.key}.\n\n- Ajusta \`${main}\`\n- Cobre \`${moduleOf(it)}\` com teste\n\nCriado pelo workflow \`sdlc\`.`, at: minutesAgo(60), kind: 'comment' }]
  if (it.cur > IDX.ci || it.done) {
    out.push({ id: 'c1', author: PEOPLE['u-maria'], file: main, line: 7, body: 'Três tentativas é suficiente? No checkout a gente usa cinco.', at: minutesAgo(25), kind: 'comment' })
    out.push({ id: 'c2', author: agentPerson('developer'), file: main, line: 7, body: 'Três segue o padrão de `src/lib/retry.ts`. O backoff total fica abaixo de 2s, dentro do timeout do gateway.', at: minutesAgo(24), kind: 'comment' })
  }
  it.reviewComments.forEach((n, i) => out.push({ id: `n${i}`, author: PEOPLE['u-you'], body: n.body, at: minutesAgo(0), kind: 'comment', ...(n.file ? { file: n.file, line: n.line + 1 } : {}) }))
  if (it.approvedBy) out.push({ id: 'c3', author: PEOPLE[it.approvedBy] ?? PEOPLE['u-you'], body: 'LGTM', at: iso(it.approvedAt), kind: 'approved' })
  return out
}

export function changeData(key: string): ChangeData {
  const it = store.items.find((x) => `#${x.pr}` === key)
  if (!it?.pr) throw new Error(`mudança ${key} não encontrada`)
  const merged = it.states.merge === 'passed'
  const body = `## O que muda\n\n${it.title}.\n\n- Ajusta \`${fileOf(it)}\` para renovar a sessão expirada\n- Adiciona teste cobrindo o caso que falhava\n- Usa o backoff de \`src/lib/retry.ts\`\n\n## Como testar\n\n1. \`bun test ${moduleOf(it)}\`\n2. Conferir o fluxo no preview\n\nCloses [${it.key}](${issueUrl(it.key)})`
  return { key, number: it.pr, title: `${it.key}: ${it.title}`, url: prUrl(it.pr), state: merged ? 'merged' : 'open', author: agentPerson('developer'), head: it.branch, base: 'main', body, files: prFiles(it), comments: prComments(it) }
}

function description(it: SimItem) {
  return `## Contexto\n\n${it.title}. Relatado pelo suporte depois de reclamações de clientes no checkout.\n\n## Critérios de aceite\n\n- [ ] Ajustar \`${fileOf(it)}\`\n- [ ] Cobrir \`${moduleOf(it)}\` com teste automatizado\n- [ ] Sem regressão na suíte do módulo\n\n## Notas\n\nVer o print e os logs anexados.`
}

function issueComments(it: SimItem): Comment[] {
  const out: Comment[] = [{ id: 'j0', author: PEOPLE['u-joao'], body: 'Prioridade alta, cliente reclamou ontem.', at: minutesAgo(140), kind: 'comment' }]
  if (it.plans.some((p) => p.resolution === 'approved')) out.push({ id: 'j1', author: agentPerson('planner'), body: `Plano:\n1. Ajustar \`${fileOf(it)}\`\n2. Cobrir \`${moduleOf(it)}\` com teste`, at: minutesAgo(80), kind: 'comment' })
  if (it.pr) out.push({ id: 'j2', author: agentPerson('developer'), body: `PR #${it.pr} aberto em ${REPO}.`, at: minutesAgo(50), kind: 'comment' })
  if (it.cur >= IDX.review || it.done) out.push({ id: 'j3', author: agentPerson('developer'), body: 'Status: In Progress → In Review', at: minutesAgo(30), kind: 'comment' })
  if (it.done) out.push({ id: 'j4', author: agentPerson('release'), body: 'Em produção. Status: In Review → Done', at: iso(it.doneAt), kind: 'comment' })
  it.issueComments.forEach((c) => out.push({ id: c.id, author: PEOPLE['u-you'], body: c.body, at: iso(c.tick), kind: 'comment' }))
  return out
}

export function issueData(key: string): IssueData {
  const it = store.items.find((x) => x.key === key)
  if (!it) {
    const seed = ISSUES.find((i) => i.key === key)
    if (!seed) throw new Error(`issue ${key} não encontrada`)
    return { key, url: issueUrl(key), title: seed.title, status: 'To Do', assignee: PEOPLE['u-you'], description: '', attachments: [], subtasks: [], comments: [] }
  }
  const devDone = it.states.develop === 'passed'
  const [p, n] = it.key.split('-')
  const sub = (d: number, title: string, done: boolean, byAgent: boolean) => ({ key: `${p}-${Number(n) + d}`, title, done, byAgent })
  return {
    key, url: issueUrl(key), title: it.title, status: it.done ? 'Done' : it.cur >= IDX.review ? 'In Review' : 'In Progress', assignee: PEOPLE['u-you'], description: description(it),
    attachments: [{ name: 'erro-no-checkout.png', size: '184 KB' }, { name: 'logs-producao.txt', size: '12 KB' }],
    subtasks: [sub(100, `Ajustar ${fileOf(it)}`, devDone, true), sub(101, `Teste para ${moduleOf(it)}`, devDone, true), sub(102, 'Avisar o suporte quando estiver em produção', it.done, false)],
    comments: issueComments(it),
  }
}

export function searchIssues(query: string): IssueSummary[] {
  const q = query.toLowerCase()
  return ISSUES.filter((i) => !i.used && `${i.key} ${i.title}`.toLowerCase().includes(q)).map((i) => ({ key: i.key, title: i.title, status: 'To Do' }))
}

export function addIssueComment(key: string, body: string): Comment {
  const c = commentIssue(key, body)
  return { id: c?.id ?? `jc${Date.now()}`, author: PEOPLE['u-you'], body, at: iso(store.clock), kind: 'comment' }
}

interface JobSeed { id: string; name: string; stage: number; from: number; to: number; skip?: string; gate?: string; steps: string[]; needs?: string[] }

const CI_JOBS: JobSeed[] = [
  { id: 'lint', name: 'lint', stage: 0, from: 0, to: 0.3, steps: ['bun run lint'] },
  { id: 'types', name: 'typecheck', stage: 0, from: 0, to: 0.35, steps: ['bun run typecheck'] },
  { id: 'test-ubuntu', name: 'test (ubuntu)', needs: ['lint', 'types'], stage: 1, from: 0.35, to: 0.85, steps: ['bun test'] },
  { id: 'test-macos', name: 'test (macos)', needs: ['lint', 'types'], stage: 1, from: 0.35, to: 0.95, steps: ['bun test'] },
  { id: 'build', name: 'build', needs: ['test-ubuntu', 'test-macos'], stage: 2, from: 0.85, to: 1, steps: ['build'] },
  { id: 'preview', name: 'deploy preview', needs: ['test-ubuntu', 'test-macos'], stage: 2, from: 0, to: 0, skip: "github.event_name == 'push'", steps: [] },
]
const RELEASE_JOBS: JobSeed[] = [
  { id: 'image', name: 'build image', stage: 0, from: 0, to: 0.2, steps: ['build da imagem'] },
  { id: 'staging', name: 'deploy staging', needs: ['image'], stage: 1, from: 0.2, to: 0.38, steps: ['deploy canary 5%'] },
  { id: 'smoke-staging', name: 'smoke staging', needs: ['staging'], stage: 2, from: 0.38, to: 0.45, steps: ['bun test'] },
  { id: 'production', name: 'deploy production', needs: ['smoke-staging'], stage: 3, from: 0.5, to: 0.9, gate: 'production', steps: ['rollout 100%'] },
  { id: 'smoke', name: 'smoke production', needs: ['production'], stage: 4, from: 0.9, to: 1, steps: ['bun test'] },
  { id: 'rollback', name: 'rollback', needs: ['production'], stage: 4, from: 0, to: 0, skip: 'failure()', steps: [] },
]
const SETUP = ['Set up job', 'Checkout', 'Setup Bun', 'Install dependencies']

function jobState(it: SimItem, kind: 'ci' | 'release', j: JobSeed, ai: number): JobState {
  const atts = it.attempts[kind]
  const isLast = ai >= atts.length - 1
  const a = atts[ai]
  const st = isLast ? it.states[kind] : a?.state ?? 'pending'
  if (j.skip) return st === 'pending' || st === 'blocked' ? 'queued' : 'skipped'
  if (st === 'pending' || st === 'blocked' || st === 'skipped') return 'queued'
  if (st === 'passed') return 'passed'
  const p = a ? a.ticks / STEPS[IDX[kind]].dur : 0
  if (kind === 'release' && j.gate) {
    if (it.rejected) return 'rejected'
    if (!it.deployApproved) return it.gate ? 'waiting' : 'queued'
  }
  if (kind === 'release' && it.rejected && j.stage > 3) return 'skipped'
  if (st === 'failed' && kind === 'ci') return j.id === 'test-ubuntu' ? 'failed' : j.stage > 1 ? 'skipped' : 'passed'
  if (p >= j.to) return 'passed'
  if (p >= j.from) return 'running'
  return 'queued'
}

function jobSeconds(j: JobSeed, st: JobState, it: SimItem) {
  if (st === 'skipped' || st === 'queued' || st === 'waiting' || st === 'rejected') return null
  return Math.round((j.to - j.from) * 180 + (it.h % 40))
}

export function pipelineRun(run: string): PipelineRun {
  const [kind, id, n] = run.split(':')
  const it = store.items.find((x) => String(x.id) === id)
  if (!it || (kind !== 'ci' && kind !== 'release')) throw new Error(`execução ${run} não encontrada`)
  const ai = Math.max(0, Number(n) - 1)
  const seeds = kind === 'ci' ? CI_JOBS : RELEASE_JOBS
  const jobs: Job[] = seeds.map((j) => {
    const state = jobState(it, kind, j, ai)
    const names = [...SETUP, ...j.steps, 'Complete job']
    const steps = state === 'passed' || state === 'failed' || state === 'running' ? commands(it, names, state === 'passed' ? 'passed' : state === 'failed' ? 'failed' : 'running', state === 'running' ? 4 : 0, 7, state === 'failed' ? SETUP.length : -1) : []
    return { id: j.id, name: j.name, stage: j.stage, needs: j.needs ?? [], state, seconds: jobSeconds(j, state, it), condition: j.skip ?? null, gate: j.gate ?? null, waitingSince: state === 'waiting' ? iso(it.waitedAt) : null, steps }
  })
  const isLast = ai >= it.attempts[kind].length - 1
  const st = isLast ? it.states[kind] : it.attempts[kind][ai]?.state
  const overall: JobState = st === 'passed' ? 'passed' : st === 'failed' ? 'failed' : jobs.some((j) => j.state === 'waiting') ? 'waiting' : st === 'running' ? 'running' : 'queued'
  const runId = 9000000000 + (it.h % 99999999) + (kind === 'release' ? 1 : 0)
  return {
    id: run, title: kind === 'ci' ? 'CI' : 'Release', file: kind === 'ci' ? 'ci.yml' : 'release.yml', number: 4800 + (it.h % 300) + ai,
    event: kind === 'ci' ? 'pull_request' : 'push', ref: kind === 'ci' ? it.branch : 'main', url: `https://github.com/${REPO}/actions/runs/${runId}`, state: overall, jobs,
  }
}
