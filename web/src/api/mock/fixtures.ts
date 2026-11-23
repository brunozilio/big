import type { Agent, Model, Person, Provider, Usage, UsageDays, UsageLimits } from '../contract'
import type { ConnectionState, Need } from './sim'

export const PROVIDERS: (Provider & { capabilities: string[]; accounts: string[]; paste?: boolean; choose?: { title: string; options: { id: string; label: string; detail: string }[] } })[] = [
  { key: 'github', name: 'GitHub', group: 'Código', description: 'Sua conta GitHub', logo: 'github', button: 'Entrar com GitHub', site: 'do GitHub', signIn: 'device', fields: [], multiple: true, capabilities: ['code'], accounts: ['acme · 1 repositório'], choose: { title: 'Organização', options: [{ id: 'acme', label: 'acme', detail: 'acme/checkout, acme/web, acme/infra' }] } },
  { key: 'jira', name: 'Jira', group: 'Tarefas', description: 'Sua conta Atlassian', logo: 'jira', button: 'Continuar na Atlassian', site: 'da Atlassian', signIn: 'oauth', fields: [], multiple: true, capabilities: ['tracker'], accounts: ['acme.atlassian.net'], choose: { title: 'Site do Jira', options: [{ id: 'acme', label: 'acme.atlassian.net', detail: 'Projetos APP, OPS' }, { id: 'labs', label: 'acme-labs.atlassian.net', detail: 'Projeto LAB' }] } },
  {
    key: 'aws', name: 'AWS', group: 'Nuvem', description: 'Sua conta AWS', logo: 'aws', button: 'Entrar com AWS', site: 'da AWS', signIn: 'oauth', fields: [], multiple: true, capabilities: ['cloud'], accounts: ['papel deploy · us-east-1'],
    alternative: {
      key: 'sso', label: 'Usar SSO da empresa', button: 'Continuar', site: 'do AWS IAM Identity Center', signIn: 'sso',
      fields: [
        { name: 'start_url', label: 'Start URL do IAM Identity Center', kind: 'text', value: 'https://acme.awsapps.com/start', mono: true },
        { name: 'region', label: 'Região', kind: 'text', value: 'us-east-1', mono: true },
      ],
    },
    choose: { title: 'Conta e papel', options: [{ id: 'prod', label: 'acme-prod · 1234-5678-9012', detail: 'DeployRole' }, { id: 'stg', label: 'acme-staging · 2109-8765-4321', detail: 'DeployRole' }] },
  },
  { key: 'claude', name: 'Claude', group: 'Provedores de IA', description: 'Sua conta Claude', logo: 'claude', signIn: 'browser', fields: [], multiple: false, capabilities: ['ai'], accounts: ['Claude Team · ti@acme.com.br'] },
  { key: 'chatgpt', name: 'ChatGPT', group: 'Provedores de IA', description: 'Sua conta ChatGPT', logo: 'chatgpt', signIn: 'browser', paste: true, fields: [], multiple: false, capabilities: ['ai'], accounts: ['ChatGPT Business · ti@acme.com.br'] },
  { key: 'copilot', name: 'GitHub Copilot', group: 'Provedores de IA', description: 'Sua conta GitHub Copilot', logo: 'copilot', signIn: 'device', fields: [], multiple: false, capabilities: ['ai'], accounts: ['Copilot Business · acme'] },
  { key: 'grok', name: 'Grok', group: 'Provedores de IA', description: 'Sua conta Grok', logo: 'grok', signIn: 'device', fields: [], multiple: false, capabilities: ['ai'], accounts: ['SuperGrok · bruno@acme.com.br'] },
  { key: 'kiro', name: 'Kiro', group: 'Provedores de IA', description: 'Sua conta Kiro', logo: 'kiro', signIn: 'browser', fields: [], multiple: false, capabilities: ['ai'], accounts: ['Kiro Pro · AWS IAM Identity Center'] },
  { key: 'antigravity', name: 'Antigravity', group: 'Provedores de IA', description: 'Sua conta Antigravity', logo: 'antigravity', signIn: 'browser', fields: [], multiple: false, capabilities: ['ai'], accounts: ['Google One AI Pro · ti@acme.com.br'] },
]

export const INITIAL_CONNECTIONS: ConnectionState[] = [
  { id: 'c-github', provider: 'github', label: 'acme', account: 'acme · 3 repositórios', capabilities: ['code'] },
  { id: 'c-jira', provider: 'jira', label: 'acme.atlassian.net', account: 'acme.atlassian.net · projeto APP', capabilities: ['tracker'] },
  { id: 'c-claude', provider: 'claude', label: 'Claude Team', account: 'Claude Team · ti@acme.com.br', capabilities: ['ai'] },
  { id: 'c-copilot', provider: 'copilot', label: 'Copilot Business', account: 'Copilot Business · acme', capabilities: ['ai'], status: 'error' },
]

export const NEED_PROVIDER: Record<Need, string> = { ai: 'claude', tracker: 'jira', code: 'github', cloud: 'aws' }

export const AGENTS: Agent[] = [
  { id: 'planner', name: 'Planejador', avatar: { kind: 'agent', shape: 'triangle', color: '#e94b9b' } },
  { id: 'developer', name: 'Desenvolvedor', avatar: { kind: 'agent', shape: 'squircle', color: '#3b82f6' } },
  { id: 'reviewer', name: 'Revisor', avatar: { kind: 'agent', shape: 'circle', color: '#4fb39c' } },
  { id: 'release', name: 'Release', avatar: { kind: 'agent', shape: 'cloud', color: '#f0a235' } },
]

export const PEOPLE: Record<string, Person> = {
  'u-you': { id: 'u-you', name: 'Bruno Zilio', avatar: null, you: true },
  'u-maria': { id: 'u-maria', name: 'Maria Souza', avatar: null },
  'u-joao': { id: 'u-joao', name: 'João Lima', avatar: null },
}

export function agentPerson(id: string): Person {
  const a = AGENTS.find((x) => x.id === id) ?? AGENTS[1]
  return { id: a.id, name: a.name, avatar: a.avatar }
}

const ALL_EFFORTS: Model['efforts'] = ['low', 'medium', 'high', 'xhigh', 'max']

export const MODELS: Model[] = [
  { id: 'claude-opus', name: 'Claude Opus', provider: 'claude', logo: 'claude', efforts: ALL_EFFORTS, fast: false },
  { id: 'claude-sonnet', name: 'Claude Sonnet', provider: 'claude', logo: 'claude', efforts: ALL_EFFORTS, fast: false },
  { id: 'gpt-5', name: 'GPT-5', provider: 'copilot', logo: 'copilot', efforts: ['low', 'medium', 'high', 'xhigh'], fast: false },
  { id: 'kiro-subscription:auto', name: 'Auto', provider: 'kiro', logo: 'kiro', efforts: [], fast: false },
]

interface UsageSource { provider: string; name: string; logo: string; account: string; calls: number; models: [string, number, number][] }

const USAGE_SOURCES: UsageSource[] = [
  { provider: 'claude', name: 'Claude', logo: 'claude', account: 'bruno@acme.com.br', calls: 34, models: [['claude-opus-4-5', 0.55, 0.011], ['claude-sonnet-4-5', 0.35, 0.0042], ['claude-haiku-4-5', 0.1, 0.0011]] },
  { provider: 'claude', name: 'Claude', logo: 'claude', account: 'ti@acme.com.br', calls: 18, models: [['claude-sonnet-4-5', 0.7, 0.0042], ['claude-opus-4-5', 0.3, 0.011]] },
  { provider: 'chatgpt', name: 'ChatGPT', logo: 'chatgpt', account: 'bruno@acme.com.br', calls: 14, models: [['gpt-5', 0.6, 0.0035], ['gpt-5-codex', 0.4, 0.003]] },
  { provider: 'copilot', name: 'GitHub Copilot', logo: 'copilot', account: 'acme', calls: 9, models: [['gpt-5', 0.5, 0.0035], ['claude-sonnet-4-5', 0.5, 0.0042]] },
  { provider: 'grok', name: 'Grok', logo: 'grok', account: 'bruno@acme.com.br', calls: 3, models: [['grok-4', 1, 0.004]] },
]

const AVG_TOKENS = 38_000

function noise(seed: number): number {
  const x = Math.sin(seed * 12.9898 + 78.233) * 43758.5453
  return x - Math.floor(x)
}

function localDate(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

type Tally = Map<string, { costUsd: number; tokens: number; calls: number }>

function tally(map: Tally, key: string, costUsd: number, tokens: number, calls: number) {
  const row = map.get(key) ?? { costUsd: 0, tokens: 0, calls: 0 }
  map.set(key, { costUsd: row.costUsd + costUsd, tokens: row.tokens + tokens, calls: row.calls + calls })
}

function ranked<T extends { costUsd: number; calls: number }>(rows: T[]): T[] {
  return rows.filter((r) => r.calls !== 0).sort((a, b) => b.costUsd - a.costUsd)
}

function usageDay(day: Date, providers: Tally, models: Tally, accounts: Tally) {
  const ordinal = Math.floor(day.getTime() / 86_400_000)
  const weekend = day.getDay() === 0 || day.getDay() === 6 ? 0.25 : 1
  const entry = { date: localDate(day), costUsd: 0, tokens: 0, calls: 0, byProvider: {} as Record<string, number>, byModel: {} as Record<string, number> }
  USAGE_SOURCES.forEach((s, si) => {
    s.models.forEach(([model, share, perK], mi) => {
      const calls = Math.round(s.calls * share * weekend * (0.35 + 1.3 * noise(ordinal * 31 + si * 7 + mi)))
      if (!calls) return
      const tokens = Math.round(calls * AVG_TOKENS * (0.6 + 0.8 * noise(ordinal + si + mi * 3)))
      const cost = (tokens / 1000) * perK
      entry.costUsd += cost
      entry.tokens += tokens
      entry.calls += calls
      entry.byProvider[s.provider] = (entry.byProvider[s.provider] ?? 0) + cost
      entry.byModel[model] = (entry.byModel[model] ?? 0) + cost
      tally(providers, s.provider, cost, tokens, calls)
      tally(models, model, cost, tokens, calls)
      tally(accounts, `${s.provider}:${s.account}`, cost, tokens, calls)
    })
  })
  return entry
}

export function usageFixture(days: UsageDays, today: Date): Usage {
  const providers: Tally = new Map()
  const models: Tally = new Map()
  const accounts: Tally = new Map()
  const daily = Array.from({ length: days }, (_, i) => usageDay(new Date(today.getFullYear(), today.getMonth(), today.getDate() - (days - 1 - i), 12), providers, models, accounts))
  const tokens = daily.reduce((n, d) => n + d.tokens, 0)
  const calls = daily.reduce((n, d) => n + d.calls, 0)
  return {
    days,
    from: daily[0]?.date ?? localDate(today),
    to: localDate(today),
    totals: {
      costUsd: daily.reduce((n, d) => n + d.costUsd, 0),
      input: Math.round(tokens * 0.08),
      output: Math.round(tokens * 0.04),
      cacheRead: Math.round(tokens * 0.83),
      cacheWrite: tokens - Math.round(tokens * 0.08) - Math.round(tokens * 0.04) - Math.round(tokens * 0.83),
      calls,
      sessions: Math.round(calls / 9),
    },
    daily,
    byProvider: ranked([...providers].map(([id, t]) => {
      const s = USAGE_SOURCES.find((x) => x.provider === id)
      return { id, name: s?.name ?? id, logo: s?.logo ?? '', ...t }
    })),
    byModel: ranked([...models].map(([id, t]) => ({ id, name: id, ...t }))),
    byAccount: ranked([...accounts].map(([id, t]) => ({ id, provider: id.slice(0, id.indexOf(':')), account: id.slice(id.indexOf(':') + 1), ...t }))),
  }
}

export function usageLimitsFixture(now: number): UsageLimits {
  const at = (hours: number) => new Date(now + hours * 3_600_000).toISOString()
  const span = (id: string, label: string, usedPercent: number, resetsIn: number, length: number) => ({ id, label, usedPercent, resetsAt: at(resetsIn), startsAt: at(resetsIn - length) })
  const week = 168
  const month = 720
  return {
    accounts: [
      { provider: 'claude', name: 'Claude', logo: 'claude', account: 'bruno@acme.com.br', status: 'ok', error: null, plan: 'Max', windows: [
        span('five_hour', 'Sessão (5 h)', 42, 2.2, 5),
        span('seven_day', 'Semana', 67, 76, week),
        span('seven_day_opus', 'Semana (Opus)', 84, 76, week),
      ] },
      { provider: 'claude', name: 'Claude', logo: 'claude', account: 'ti@acme.com.br', status: 'ok', error: null, plan: 'Max', windows: [
        span('five_hour', 'Sessão (5 h)', 97, 0.6, 5),
        span('seven_day', 'Semana', 81, 30, week),
      ] },
      { provider: 'chatgpt', name: 'ChatGPT', logo: 'chatgpt', account: 'bruno@acme.com.br', status: 'ok', error: null, plan: 'Plus', windows: [
        span('five_hour', 'Sessão (5 h)', 18, 3.5, 5),
        span('seven_day', 'Semana', 34, 120, week),
      ] },
      { provider: 'copilot', name: 'GitHub Copilot', logo: 'copilot', account: 'acme', status: 'ok', error: null, plan: 'Pro', windows: [
        span('premium_interactions', 'Requisições premium', 56, 190, month),
      ] },
      { provider: 'grok', name: 'Grok', logo: 'grok', account: 'bruno@acme.com.br', status: 'unsupported', error: null, plan: null, windows: [] },
      { provider: 'kiro', name: 'Kiro', logo: 'kiro', account: 'bruno@acme.com.br', status: 'ok', error: null, plan: 'KIRO PRO+', windows: [
        span('credit', 'Credits · 1240 de 2000', 62, 400, month),
        { id: 'credit_bonus_0', label: 'Welcome bonus · 120 de 500', usedPercent: 24, resetsAt: at(240), startsAt: null },
      ] },
      { provider: 'kiro', name: 'Kiro', logo: 'kiro', account: 'AWS IAM Identity Center', status: 'error', error: 'A sessão expirou. Entre de novo em Conexões.', plan: null, windows: [] },
    ],
  }
}
