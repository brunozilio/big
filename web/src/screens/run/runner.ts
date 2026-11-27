import type { Language } from '@/api/contract'

export type Localized = string | Partial<Record<string, string>>

export type StepState = 'waiting' | 'running' | 'done' | 'failed'
export type FieldKind = 'text' | 'password' | 'select' | 'info'

export interface RunStep {
  id: string
  title: Localized
  state: StepState
}

export interface FieldOption {
  value: string
  label: Localized
  hint?: Localized
}

export interface RunField {
  key: string
  label: Localized
  kind: FieldKind
  value?: string
  hint?: Localized
  required?: boolean
  options?: FieldOption[]
}

export interface RunAction {
  id: string
  label: Localized
  primary?: boolean
}

export interface RunForm {
  id: string
  title: Localized
  description?: Localized
  fields: RunField[]
  actions: RunAction[]
}

export interface RunFinish {
  next: string
  message?: Localized
}

export type RunEvent =
  | ({ type: 'step' } & RunStep)
  | ({ type: 'form' } & RunForm)
  | { type: 'error'; message: Localized }
  | { type: 'connect'; url: string }
  | ({ type: 'finish' } & RunFinish)
  | { type: 'exit'; code: number }

export type RunInput = RunEvent | { type: 'answered'; form: string } | { type: 'lost' } | { type: 'reset' }

export interface RunState {
  steps: RunStep[]
  form: RunForm | null
  answered: boolean
  errors: Localized[]
  url: string | null
  finish: RunFinish | null
  exit: number | null
  lost: boolean
  seq: number
}

export type RunPhase = 'running' | 'finished' | 'failed' | 'ended'

export const RUNNER = { events: '/api/runner/events', answer: '/api/runner/answer' }

export const EMPTY_RUN: RunState = { steps: [], form: null, answered: false, errors: [], url: null, finish: null, exit: null, lost: false, seq: 0 }

const STATES: readonly string[] = ['waiting', 'running', 'done', 'failed']
const KINDS: readonly string[] = ['text', 'password', 'select', 'info']

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as Record<string, unknown>) : null
}

function text(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined
}

function localizedOf(value: unknown): Localized | undefined {
  if (typeof value === 'string') return value
  const o = record(value)
  if (!o) return undefined
  const entries = Object.entries(o).filter((e): e is [string, string] => typeof e[1] === 'string')
  return entries.length > 0 ? Object.fromEntries(entries) : undefined
}

export function localize(value: Localized | undefined, language: Language): string {
  if (value === undefined) return ''
  if (typeof value === 'string') return value
  return value[language] ?? value.en ?? Object.values(value).find((v) => v !== undefined) ?? ''
}

function option(value: unknown): FieldOption | null {
  const o = record(value)
  const v = text(o?.value)
  if (!o || v === undefined) return null
  return { value: v, label: localizedOf(o.label) ?? v, hint: localizedOf(o.hint) }
}

function field(value: unknown): RunField | null {
  const f = record(value)
  const key = text(f?.key)
  const kind = text(f?.kind)
  if (!f || key === undefined || kind === undefined || !KINDS.includes(kind)) return null
  const options = Array.isArray(f.options) ? f.options.map(option).filter((o): o is FieldOption => o !== null) : undefined
  return { key, label: localizedOf(f.label) ?? key, kind: kind as FieldKind, value: text(f.value), hint: localizedOf(f.hint), required: f.required === true, options }
}

function action(value: unknown): RunAction | null {
  const a = record(value)
  const id = text(a?.id)
  if (!a || id === undefined) return null
  return { id, label: localizedOf(a.label) ?? id, primary: a.primary === true }
}

function form(e: Record<string, unknown>): RunEvent | null {
  const id = text(e.id)
  if (id === undefined) return null
  const fields = Array.isArray(e.fields) ? e.fields.map(field).filter((f): f is RunField => f !== null) : []
  const actions = Array.isArray(e.actions) ? e.actions.map(action).filter((a): a is RunAction => a !== null) : []
  return { type: 'form', id, title: localizedOf(e.title) ?? '', description: localizedOf(e.description), fields, actions }
}

function step(e: Record<string, unknown>): RunEvent | null {
  const id = text(e.id)
  const state = text(e.state)
  if (id === undefined || state === undefined || !STATES.includes(state)) return null
  return { type: 'step', id, title: localizedOf(e.title) ?? id, state: state as StepState }
}

export function toRunEvent(value: unknown): RunEvent | null {
  const e = record(value)
  if (!e) return null
  switch (e.type) {
    case 'step':
      return step(e)
    case 'form':
      return form(e)
    case 'error':
      return { type: 'error', message: localizedOf(e.message) ?? '' }
    case 'connect':
      return typeof e.url === 'string' ? { type: 'connect', url: e.url } : null
    case 'finish':
      return typeof e.next === 'string' ? { type: 'finish', next: e.next, message: localizedOf(e.message) } : null
    case 'exit':
      return typeof e.code === 'number' ? { type: 'exit', code: e.code } : null
    default:
      return null
  }
}

export function parseRunEvent(data: string): RunEvent | null {
  try {
    return toRunEvent(JSON.parse(data))
  } catch {
    return null
  }
}

function withStep(steps: RunStep[], next: RunStep): RunStep[] {
  return steps.some((s) => s.id === next.id) ? steps.map((s) => (s.id === next.id ? next : s)) : [...steps, next]
}

export function runReducer(state: RunState, input: RunInput): RunState {
  switch (input.type) {
    case 'reset':
      return EMPTY_RUN
    case 'step':
      return { ...state, steps: withStep(state.steps, { id: input.id, title: input.title, state: input.state }) }
    case 'form':
      return { ...state, form: { id: input.id, title: input.title, description: input.description, fields: input.fields, actions: input.actions }, answered: false, seq: state.seq + 1 }
    case 'answered':
      return state.form?.id === input.form ? { ...state, answered: true } : state
    case 'error':
      return { ...state, errors: [...state.errors, input.message] }
    case 'connect':
      return { ...state, url: input.url }
    case 'finish':
      return { ...state, finish: { next: input.next, message: input.message }, form: null }
    case 'exit':
      return { ...state, exit: input.code, form: null }
    case 'lost':
      return state.finish || state.exit !== null ? state : { ...state, lost: true }
  }
}

export function runPhase(state: RunState): RunPhase {
  if (state.finish) return 'finished'
  if (state.exit === null) return 'running'
  return state.exit === 0 ? 'ended' : 'failed'
}

export function localNext(next: string): string {
  return next.startsWith('/') && !next.startsWith('//') && !next.includes('\\') ? next : '/'
}

export function startingValues(form: RunForm): Record<string, string> {
  const values: Record<string, string> = {}
  for (const f of form.fields) if (f.kind !== 'info') values[f.key] = f.value ?? ''
  return values
}

export function missingRequired(form: RunForm, values: Record<string, string>): boolean {
  return form.fields.some((f) => f.kind !== 'info' && f.required === true && !(values[f.key] ?? '').trim())
}
