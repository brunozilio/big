import type { Agent, Model, Person, Provider, RepositoryKind, Usage, UsageDays, UsageLimits } from '../contract'
import { say } from './say'
import { SONAR_LOGO, VERACODE_LOGO } from './security'
import type { ConnectionState, Need } from './sim'

const MCP_LOGO = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor"><g fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M9 2v5M15 2v5"/><path d="M6 7h12v4a6 6 0 0 1-12 0z"/><path d="M12 17v5"/></g></svg>'

export const MCP_TOOLS = [{ name: 'search' }, { name: 'fetch' }]

export type MockProvider = Provider & { capabilities: string[]; accounts: string[]; paste?: boolean; hidden?: boolean; choose?: { title: string; options: { id: string; label: string; detail: string }[] } }

export const repositoryKinds = (): RepositoryKind[] => [
  { key: 'application', label: say('kindApplication') },
  { key: 'library', label: say('kindLibrary') },
  { key: 'configuration', label: say('kindConfiguration') },
  { key: 'infrastructure', label: say('kindInfrastructure') },
  { key: 'tests', label: say('kindTests') },
]

export const providers = (): MockProvider[] => [
  { key: 'github', name: 'GitHub', group: 'code', description: say('yourAccount', { name: 'GitHub' }), logo: 'github', button: say('signInWith', { name: 'GitHub' }), site: say('siteGithub'), signIn: 'device', fields: [], multiple: true, bindings: [{ capability: 'code', scope: 'repository', label: say('bindRepository'), section: say('sectionCode'), order: 30 }], capabilities: ['code'],
    alternatives: [{
      key: 'api_token', label: say('usePat'), button: say('connect'), site: say('siteGithub'), signIn: 'token',
      fields: [{ name: 'token', label: say('ghTokenLabel'), kind: 'text', mono: true, secret: true, help: say('ghTokenHelp') }],
    }], accounts: [say('ghAccount')], choose: { title: say('organization'), options: [{ id: 'acme', label: 'acme', detail: 'acme/checkout, acme/web, acme/infra' }] } },
  { key: 'jira', name: 'Jira', group: 'tracker', description: say('yourAccount', { name: 'Atlassian' }), logo: 'jira', button: say('continueAt', { name: 'Atlassian' }), site: say('siteAtlassian'), signIn: 'oauth', fields: [], multiple: true, bindings: [{ capability: 'tracker', scope: 'application', label: say('bindSpace'), section: say('sectionPlanning'), order: 10 }], capabilities: ['tracker'],
    alternatives: [{
      key: 'api_token', label: say('useApiToken'), button: say('connect'), site: say('siteAtlassian'), signIn: 'token',
      fields: [
        { name: 'email', label: say('atlassianEmail'), kind: 'text', mono: true },
        { name: 'token', label: say('atlassianToken'), kind: 'text', mono: true, secret: true, help: say('atlassianTokenHelp') },
      ],
    }], accounts: ['acme.atlassian.net'], choose: { title: say('jiraSite'), options: [{ id: 'acme', label: 'acme.atlassian.net', detail: say('projectsAppOps') }, { id: 'labs', label: 'acme-labs.atlassian.net', detail: say('projectLab') }] } },
  {
    key: 'aws', name: 'AWS', group: 'cloud', description: say('yourAccount', { name: 'AWS' }), logo: 'aws', button: say('signInWith', { name: 'AWS' }), site: say('siteAws'), signIn: 'oauth', fields: [], multiple: true, bindings: [{ capability: 'cloud', scope: 'application', label: say('bindCloud'), each: { key: 'environment', label: say('changesEnvironment'), fields: [{ key: 'account', label: say('bindCloudAccount'), required: true }, { key: 'role', label: say('bindCloudRole'), required: true }] }, section: say('sectionCloud'), order: 70 }], capabilities: ['cloud'], accounts: [say('awsAccount')],
    alternatives: [{
      key: 'sso', label: say('useCompanySso'), button: say('continue'), site: say('siteIdentityCenter'), signIn: 'sso',
      fields: [
        { name: 'start_url', label: say('startUrl'), kind: 'text', value: 'https://acme.awsapps.com/start', mono: true },
        { name: 'region', label: say('region'), kind: 'text', value: 'us-east-1', mono: true },
      ],
    }, {
      key: 'access_keys', label: say('useAccessKey'), button: say('connect'), site: say('siteAws'), signIn: 'token',
      fields: [
        { name: 'accessKeyId', label: 'Access key ID', kind: 'text', mono: true },
        { name: 'secretAccessKey', label: 'Secret access key', kind: 'text', mono: true, secret: true, help: say('accessKeyHelp') },
      ],
    }],
    choose: { title: say('accountAndRole'), options: [{ id: 'prod', label: 'acme-prod · 1234-5678-9012', detail: 'DeployRole' }, { id: 'stg', label: 'acme-staging · 2109-8765-4321', detail: 'DeployRole' }] },
  },
  {
    key: 'zephyr', name: 'Zephyr', group: 'tests', description: say('yourAccount', { name: 'Zephyr Scale' }), logo: 'jira', button: say('continueAt', { name: 'SmartBear' }), site: say('siteSmartBear'), signIn: 'oauth', fields: [], multiple: true, bindings: [{ capability: 'tests', scope: 'application', label: say('bindTests'), section: say('sectionQuality'), order: 60 }], capabilities: ['tests'], accounts: ['Zephyr Scale'],
    alternatives: [{
      key: 'api_token', label: say('useApiToken'), button: say('connect'), site: say('siteZephyr'), signIn: 'token',
      fields: [{ name: 'token', label: say('zephyrToken'), kind: 'text', mono: true, secret: true, help: say('zephyrTokenHelp') }],
    }],
  },
  {
    key: 'veracode', name: 'Veracode', group: 'security', description: say('yourAccount', { name: 'Veracode' }), logo: VERACODE_LOGO, button: say('connect'), site: 'Veracode', signIn: 'token', multiple: true, bindings: [{ capability: 'security', scope: 'repository', label: 'Veracode', section: say('sectionQuality'), order: 50 }], capabilities: ['security'], accounts: ['API · acme'],
    fields: [
      { name: 'region', label: say('region'), kind: 'text', value: 'com', mono: true },
      { name: 'apiId', label: 'API ID', kind: 'text', mono: true },
      { name: 'apiKey', label: 'API key', kind: 'text', mono: true, secret: true },
    ],
  },
  {
    key: 'sonar', name: 'SonarQube', group: 'quality', description: say('yourAccount', { name: 'SonarQube' }), logo: SONAR_LOGO, button: say('connect'), site: 'SonarQube', signIn: 'token', multiple: true, bindings: [{ capability: 'quality', scope: 'repository', label: say('bindQuality'), section: say('sectionQuality'), order: 40 }], capabilities: ['quality'], accounts: ['acme'],
    fields: [
      { name: 'url', label: 'URL', kind: 'text', value: 'https://sonarcloud.io', mono: true },
      { name: 'organization', label: 'Organization (SonarQube Cloud)', kind: 'text', mono: true, optional: true },
      { name: 'token', label: 'Token', kind: 'text', mono: true, secret: true },
    ],
  },
  {
    key: 'changes', name: say('changesName'), group: 'changes', description: say('changesDescription'), logo: MCP_LOGO, button: say('connect'), site: say('changesName'), signIn: 'token', fields: [], multiple: true, capabilities: ['changes'], accounts: [say('changesName')],
    bindings: [{
      capability: 'changes', scope: 'application', label: say('bindChanges'), section: say('sectionChanges'), order: 80,
      fields: [{ key: 'company', label: say('changesCompany') }, { key: 'requester_group', label: say('changesRequesterGroup') }],
      each: { key: 'environment', label: say('changesEnvironment'), fields: [{ key: 'template', label: say('changesTemplate'), required: true }, { key: 'classification', label: say('changesClassification') }] },
    }],
  },
  {
    key: 'mcp', name: say('mcpName'), group: 'mcp', hidden: true, description: say('mcpDescription'), logo: MCP_LOGO, button: say('connect'), site: say('siteMcp'), signIn: 'oauth', multiple: true, capabilities: ['mcp'], accounts: [],
    fields: [
      { name: 'url', label: say('mcpUrl'), kind: 'text', mono: true, help: say('mcpUrlHelp') },
      { name: 'label', label: say('name'), kind: 'text', mono: true, help: say('mcpNameHelp') },
    ],
  },
  { key: 'claude', name: 'Claude', group: 'ai', description: say('yourAccount', { name: 'Claude' }), logo: 'claude', signIn: 'browser', fields: [], multiple: false, capabilities: ['ai'], accounts: ['Claude Team · ti@acme.com.br'] },
  { key: 'chatgpt', name: 'ChatGPT', group: 'ai', description: say('yourAccount', { name: 'ChatGPT' }), logo: 'chatgpt', signIn: 'browser', paste: true, fields: [], multiple: false, capabilities: ['ai'], accounts: ['ChatGPT Business · ti@acme.com.br'] },
  { key: 'copilot', name: 'GitHub Copilot', group: 'ai', description: say('yourAccount', { name: 'GitHub Copilot' }), logo: 'copilot', signIn: 'device', fields: [], multiple: false, capabilities: ['ai'], accounts: ['Copilot Business · acme'] },
  { key: 'grok', name: 'Grok', group: 'ai', description: say('yourAccount', { name: 'Grok' }), logo: 'grok', signIn: 'device', fields: [], multiple: false, capabilities: ['ai'], accounts: ['SuperGrok · bruno@acme.com.br'] },
  { key: 'kiro', name: 'Kiro', group: 'ai', description: say('yourAccount', { name: 'Kiro' }), logo: 'kiro', signIn: 'browser', fields: [], multiple: false, capabilities: ['ai'], accounts: ['Kiro Pro · AWS IAM Identity Center'] },
  { key: 'antigravity', name: 'Antigravity', group: 'ai', description: say('yourAccount', { name: 'Antigravity' }), logo: 'antigravity', signIn: 'browser', fields: [], multiple: false, capabilities: ['ai'], accounts: ['Google One AI Pro · ti@acme.com.br'] },
]

export const initialConnections = (): ConnectionState[] => [
  { id: 'c-github', provider: 'github', label: 'acme', account: say('ghConnection'), capabilities: ['code'] },
  { id: 'c-jira', provider: 'jira', label: 'acme.atlassian.net', account: say('jiraConnection'), capabilities: ['tracker'] },
  { id: 'c-sonar', provider: 'sonar', label: 'SonarQube Cloud', account: 'acme', capabilities: ['quality'] },
  { id: 'c-veracode', provider: 'veracode', label: 'Veracode', account: 'API · acme', capabilities: ['security'] },
  { id: 'c-claude', provider: 'claude', label: 'Claude Team', account: 'Claude Team · ti@acme.com.br', capabilities: ['ai'] },
  { id: 'c-copilot', provider: 'copilot', label: 'Copilot Business', account: 'Copilot Business · acme', capabilities: ['ai'], status: 'error' },
]

export const NEED_PROVIDER: Record<Need, string> = { ai: 'claude', tracker: 'jira', code: 'github', cloud: 'aws', security: 'veracode', quality: 'sonar' }

export const AGENTS: Agent[] = [
  { id: 'planner', name: 'Planner', avatar: { kind: 'agent', shape: 'triangle', color: '#e94b9b' } },
  { id: 'developer', name: 'Developer', avatar: { kind: 'agent', shape: 'squircle', color: '#3b82f6' } },
  { id: 'reviewer', name: 'Reviewer', avatar: { kind: 'agent', shape: 'circle', color: '#4fb39c' } },
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
        span('five_hour', say('limitSession'), 42, 2.2, 5),
        span('seven_day', say('limitWeek'), 67, 76, week),
        span('seven_day_opus', say('limitWeekOpus'), 84, 76, week),
      ] },
      { provider: 'claude', name: 'Claude', logo: 'claude', account: 'ti@acme.com.br', status: 'ok', error: null, plan: 'Max', windows: [
        span('five_hour', say('limitSession'), 97, 0.6, 5),
        span('seven_day', say('limitWeek'), 81, 30, week),
      ] },
      { provider: 'chatgpt', name: 'ChatGPT', logo: 'chatgpt', account: 'bruno@acme.com.br', status: 'ok', error: null, plan: 'Plus', windows: [
        span('five_hour', say('limitSession'), 18, 3.5, 5),
        span('seven_day', say('limitWeek'), 34, 120, week),
      ] },
      { provider: 'copilot', name: 'GitHub Copilot', logo: 'copilot', account: 'acme', status: 'ok', error: null, plan: 'Pro', windows: [
        span('premium_interactions', say('limitPremium'), 56, 190, month),
      ] },
      { provider: 'grok', name: 'Grok', logo: 'grok', account: 'bruno@acme.com.br', status: 'unsupported', error: null, plan: null, windows: [] },
      { provider: 'kiro', name: 'Kiro', logo: 'kiro', account: 'bruno@acme.com.br', status: 'ok', error: null, plan: 'KIRO PRO+', windows: [
        span('credit', say('limitCredits', { used: 1240, total: 2000 }), 62, 400, month),
        { id: 'credit_bonus_0', label: say('limitBonus', { used: 120, total: 500 }), usedPercent: 24, resetsAt: at(240), startsAt: null },
      ] },
      { provider: 'kiro', name: 'Kiro', logo: 'kiro', account: 'AWS IAM Identity Center', status: 'error', error: say('sessionExpired'), plan: null, windows: [] },
    ],
  }
}
