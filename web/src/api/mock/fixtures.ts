import type { Agent, Model, Person, Provider } from '../contract'
import type { ConnectionState, Need } from './sim'

export const PROVIDERS: (Provider & { capabilities: string[]; accounts: string[]; choose?: { title: string; options: { id: string; label: string; detail: string }[] } })[] = [
  { key: 'github', name: 'GitHub', group: 'Código', description: 'Sua conta GitHub', logo: 'github', signIn: 'device', fields: [], multiple: true, capabilities: ['code'], accounts: ['acme · 1 repositório'], choose: { title: 'Organização', options: [{ id: 'acme', label: 'acme', detail: 'acme/checkout, acme/web, acme/infra' }] } },
  { key: 'jira', name: 'Jira', group: 'Tarefas', description: 'Sua conta Atlassian', logo: 'jira', signIn: 'oauth', fields: [], multiple: true, capabilities: ['tracker'], accounts: ['acme.atlassian.net'], choose: { title: 'Site do Jira', options: [{ id: 'acme', label: 'acme.atlassian.net', detail: 'Projetos APP, OPS' }, { id: 'labs', label: 'acme-labs.atlassian.net', detail: 'Projeto LAB' }] } },
  {
    key: 'aws', name: 'AWS', group: 'Nuvem', description: 'Sua conta AWS', logo: 'aws', signIn: 'sso', multiple: true, capabilities: ['cloud'], accounts: ['papel deploy · us-east-1'],
    fields: [
      { name: 'startUrl', label: 'Start URL do IAM Identity Center', kind: 'text', value: 'https://acme.awsapps.com/start', mono: true },
      { name: 'region', label: 'Região', kind: 'text', value: 'us-east-1', mono: true },
    ],
    choose: { title: 'Conta e papel', options: [{ id: 'prod', label: 'acme-prod · 1234-5678-9012', detail: 'DeployRole' }, { id: 'stg', label: 'acme-staging · 2109-8765-4321', detail: 'DeployRole' }] },
  },
  { key: 'claude', name: 'Claude', group: 'Provedores de IA', description: 'Sua conta Claude', logo: 'claude', signIn: 'browser', fields: [], multiple: false, capabilities: ['ai'], accounts: ['Claude Team · ti@acme.com.br'] },
  { key: 'chatgpt', name: 'ChatGPT', group: 'Provedores de IA', description: 'Sua conta ChatGPT', logo: 'chatgpt', signIn: 'browser', fields: [], multiple: false, capabilities: ['ai'], accounts: ['ChatGPT Business · ti@acme.com.br'] },
  { key: 'copilot', name: 'GitHub Copilot', group: 'Provedores de IA', description: 'Sua conta GitHub Copilot', logo: 'copilot', signIn: 'device', fields: [], multiple: false, capabilities: ['ai'], accounts: ['Copilot Business · acme'] },
  { key: 'grok', name: 'Grok', group: 'Provedores de IA', description: 'Sua conta Grok', logo: 'grok', signIn: 'device', fields: [], multiple: false, capabilities: ['ai'], accounts: ['SuperGrok · bruno@acme.com.br'] },
  { key: 'kiro', name: 'Kiro', group: 'Provedores de IA', description: 'Sua conta Kiro', logo: 'kiro', signIn: 'browser', fields: [], multiple: false, capabilities: ['ai'], accounts: ['Kiro Pro · AWS IAM Identity Center'] },
  { key: 'antigravity', name: 'Antigravity', group: 'Provedores de IA', description: 'Sua conta Antigravity', logo: 'antigravity', signIn: 'browser', fields: [], multiple: false, capabilities: ['ai'], accounts: ['Google One AI Pro · ti@acme.com.br'] },
]

export const INITIAL_CONNECTIONS: ConnectionState[] = [
  { id: 'c-github', provider: 'github', label: 'acme', account: 'acme · 3 repositórios', capabilities: ['code'] },
  { id: 'c-jira', provider: 'jira', label: 'acme.atlassian.net', account: 'acme.atlassian.net · projeto APP', capabilities: ['tracker'] },
  { id: 'c-claude', provider: 'claude', label: 'Claude Team', account: 'Claude Team · ti@acme.com.br', capabilities: ['ai'] },
  { id: 'c-copilot', provider: 'copilot', label: 'Copilot Business', account: 'Copilot Business · acme', capabilities: ['ai'] },
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

export const MODELS: Model[] = [
  { id: 'auto', name: 'Auto', provider: 'auto', logo: '', description: 'Escolhe o modelo por etapa' },
  { id: 'claude-opus', name: 'Claude Opus', provider: 'claude', logo: 'claude' },
  { id: 'claude-sonnet', name: 'Claude Sonnet', provider: 'claude', logo: 'claude' },
  { id: 'gpt-5', name: 'GPT-5', provider: 'copilot', logo: 'copilot' },
]
