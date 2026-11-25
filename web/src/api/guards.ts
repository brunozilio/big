import { BOARD_GROUPS, STEP_STATES, type Application, type BindingChoice, type Block, type BlockType, type CloudAccount, type Item, type LiveEvent, type McpTool, type Session, type SessionEvent, type StepView } from './contract'

type Shape = Record<string, unknown>

const obj = (v: unknown): v is Shape => typeof v === 'object' && v !== null && !Array.isArray(v)
const str = (v: unknown): v is string => typeof v === 'string'
const num = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v)
const bool = (v: unknown): v is boolean => typeof v === 'boolean'
const strOrNull = (v: unknown) => v === null || str(v)
const arr = <T>(v: unknown, each: (x: unknown) => x is T): v is T[] => Array.isArray(v) && v.every(each)

export function isItem(v: unknown): v is Item {
  if (!obj(v)) return false
  return (
    str(v.id) &&
    str(v.title) &&
    str(v.workflow) &&
    BOARD_GROUPS.includes(v.group as Item['group']) &&
    strOrNull(v.status) &&
    strOrNull(v.current) &&
    bool(v.live) &&
    str(v.createdAt) &&
    str(v.updatedAt) &&
    strOrNull(v.doneAt) &&
    strOrNull(v.archivedAt) &&
    (v.issue === null || isRef(v.issue)) &&
    (v.change === null || (isRef(v.change) && bool((v.change as Shape).merged))) &&
    (v.impediment === null || (obj(v.impediment) && str(v.impediment.reason))) &&
    (v.attention === null || obj(v.attention)) &&
    strOrNull(v.recap) &&
    (v.activity === null || (obj(v.activity) && str(v.activity.text) && str(v.activity.since))) &&
    arr(v.sources, isSource) &&
    arr(v.git, isGit) &&
    arr(v.steps, isItemStep)
  )
}

function isSource(v: unknown): v is Item['sources'][number] {
  return obj(v) && str(v.kind) && str(v.key) && strOrNull(v.connection) && strOrNull(v.repository) && strOrNull(v.provider) && strOrNull(v.name) && strOrNull(v.logo) && strOrNull(v.url)
}

function isGit(v: unknown): v is Item['git'][number] {
  return obj(v) && str(v.repository) && str(v.name) && strOrNull(v.base) && strOrNull(v.head)
}

function isRef(v: unknown): boolean {
  return obj(v) && str(v.connection) && str(v.provider) && str(v.name) && str(v.logo) && str(v.key) && strOrNull(v.url)
}

function isItemStep(v: unknown): v is Item['steps'][number] {
  return obj(v) && str(v.id) && str(v.label) && str(v.icon) && STEP_STATES.includes(v.state as Item['steps'][number]['state']) && num(v.attempts) && num(v.seconds)
}

function isCase(v: unknown): v is Shape {
  return obj(v) && str(v.title) && (v.kind === 'api' || v.kind === 'ui') && strOrNull(v.objective) && strOrNull(v.ref) && strOrNull(v.url) && arr(v.given, str) && arr(v.when, str) && arr(v.then, str)
}

function isCall(v: unknown): v is Shape {
  return obj(v) && str(v.id) && str(v.title) && str(v.tag) && (v.state === 'running' || v.state === 'ok' || v.state === 'error') && num(v.durationMs)
}

function isFinding(v: unknown): v is Shape {
  return obj(v) && str(v.id) && str(v.title) && num(v.severity) && str(v.kind) && str(v.suggested) && bool(v.new) && bool(v.policy) && str(v.description)
}

const blockChecks: Record<BlockType, (b: Shape) => boolean> = {
  chat: (b) => str(b.session) && bool(b.composer),
  issue: (b) => str(b.connection) && str(b.key) && bool(b.comment),
  change: (b) => str(b.connection) && str(b.change),
  review: (b) => str(b.connection) && str(b.change) && bool(b.editable) && strOrNull(b.session) && str(b.submit),
  pipeline: (b) => str(b.connection) && str(b.run) && (b.gate === null || (obj(b.gate) && str(b.gate.approve) && str(b.gate.reject))),
  merge: (b) => str(b.connection) && str(b.change) && bool(b.ready) && Array.isArray(b.methods) && str(b.action) && Array.isArray(b.checks),
  markdown: (b) => str(b.text),
  cases: (b) => arr(b.cases, isCase) && strOrNull(b.session) && bool(b.editable) && strOrNull(b.approve) && strOrNull(b.changes),
  logs: (b) => Array.isArray(b.commands),
  calls: (b) => arr(b.calls, isCall),
  form: (b) => Array.isArray(b.fields) && obj(b.submit) && str(b.submit.action),
  actions: (b) => arr(b.buttons, (x): x is Shape => obj(x) && str(x.action) && str(x.label)),
  findings: (b) => str(b.connection) && str(b.project) && strOrNull(b.scannedAt) && strOrNull(b.url) && strOrNull(b.triage) && arr(b.findings, isFinding),
  quality: (b) => str(b.connection) && str(b.project) && str(b.branch) && strOrNull(b.analyzedAt) && strOrNull(b.url) && obj(b.report) && (b.report.gate === 'passed' || b.report.gate === 'failed') && Array.isArray(b.report.conditions) && Array.isArray(b.report.issues),
  stages: (b) => Array.isArray(b.stages) && bool(b.live),
  members: (b) => Array.isArray(b.items) && bool(b.removable),
  tasks: (b) => Array.isArray(b.tasks) && Array.isArray(b.kinds) && bool(b.locked) && strOrNull(b.environment) && (b.owners === undefined || Array.isArray(b.owners)),
}

export function isBlock(v: unknown): v is Block {
  if (!obj(v) || !str(v.type)) return false
  const check = blockChecks[v.type as BlockType]
  return !!check && check(v)
}

export function isStepView(v: unknown): v is StepView {
  return obj(v) && str(v.item) && str(v.step) && num(v.attempt) && Array.isArray(v.attempts) && arr(v.blocks, isBlock) && (v.headsUp === null || isHeadsUp(v.headsUp))
}

function isHeadsUp(v: unknown): boolean {
  return obj(v) && str(v.title) && str(v.explain) && bool(v.urgent)
}

const eventTypes: SessionEvent['type'][] = ['user', 'thinking', 'tools', 'assistant', 'status', 'ask', 'permission', 'plan']

export function isSessionEvent(v: unknown): v is SessionEvent {
  return obj(v) && str(v.id) && str(v.at) && eventTypes.includes(v.type as SessionEvent['type'])
}

export function isSession(v: unknown): v is Session {
  return obj(v) && str(v.id) && str(v.title) && str(v.status) && str(v.mode) && str(v.model) && Array.isArray(v.agents) && arr(v.events, isSessionEvent) && strOrNull(v.suggestion)
}

export function isLiveEvent(v: unknown): v is LiveEvent {
  if (!obj(v) || !str(v.type)) return false
  switch (v.type) {
    case 'item':
      return isItem(v.item)
    case 'item.removed':
      return str(v.id)
    case 'session':
      return str(v.session) && isSessionEvent(v.event)
    case 'session.event.removed':
      return str(v.session) && str(v.id)
    case 'session.status':
      return str(v.session) && str(v.status)
    case 'session.suggestion':
      return str(v.session) && (v.suggestion === null || str(v.suggestion))
    case 'connections':
      return obj(v.connections) && Array.isArray(v.connections.providers) && Array.isArray(v.connections.connections)
    case 'branding':
      return obj(v.branding) && str(v.branding.name) && (v.branding.language === undefined || ['en', 'es', 'pt-BR'].includes(String(v.branding.language)))
    case 'resync':
      return true
    default:
      return false
  }
}

const choice = (v: unknown): v is BindingChoice => obj(v) && str(v.value) && (v.label === undefined || str(v.label))

const chosen = (v: unknown): v is Record<string, BindingChoice> => obj(v) && Object.values(v).every(choice)

const bound = (v: unknown) => obj(v) && str(v.connection) && str(v.value) && (v.label === undefined || str(v.label)) && (v.more === undefined || arr(v.more, choice)) && (v.fields === undefined || chosen(v.fields)) && (v.profiles === undefined || arr(v.profiles, chosen))

const bindings = (v: unknown) => v === undefined || (obj(v) && Object.values(v).every(bound))

const variables = (v: unknown) => v === undefined || (obj(v) && Object.values(v).every(str))

function isRepository(v: unknown): v is Application['repositories'][number] {
  return obj(v) && str(v.id) && str(v.directory) && arr(v.run, str) && arr(v.verify, str) && (v.e2e === undefined || arr(v.e2e, str)) && (v.e2eDirectory === undefined || str(v.e2eDirectory)) && (v.flow === undefined || str(v.flow)) && bindings(v.bindings)
}

export function isApplication(v: unknown): v is Application {
  return obj(v) && str(v.id) && str(v.name) && bindings(v.bindings) && arr(v.repositories, isRepository) && variables(v.env)
}

export function isCloudAccount(v: unknown): v is CloudAccount {
  return obj(v) && str(v.id) && str(v.name) && str(v.email) && arr(v.roles, str)
}

export function isMcpTool(v: unknown): v is McpTool {
  return obj(v) && str(v.name) && v.name.length > 0
}
