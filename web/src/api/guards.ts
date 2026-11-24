import { BOARD_GROUPS, STEP_STATES, type Block, type BlockType, type Item, type LiveEvent, type Session, type SessionEvent, type StepView } from './contract'

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
  form: (b) => Array.isArray(b.fields) && obj(b.submit) && str(b.submit.action),
  actions: (b) => arr(b.buttons, (x): x is Shape => obj(x) && str(x.action) && str(x.label)),
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
    case 'session.status':
      return str(v.session) && str(v.status)
    case 'session.suggestion':
      return str(v.session) && (v.suggestion === null || str(v.suggestion))
    case 'connections':
      return obj(v.connections) && Array.isArray(v.connections.providers) && Array.isArray(v.connections.connections)
    case 'branding':
      return obj(v.branding) && str(v.branding.name)
    case 'resync':
      return true
    default:
      return false
  }
}
