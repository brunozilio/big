import { api } from '@/api'
import type { LibraryAsk, LibraryAssist, LibraryCard, LibraryDraft, LibraryField, LibraryItem, LibraryTurn, LibraryValue } from '@/api/contract'

export type { Library, LibraryCard, LibraryField, LibraryItem, LibrarySection, LibraryStep } from '@/api/contract'

export type FieldValue = LibraryValue

export type Draft = LibraryDraft

export const libraryApi = {
  list: () => api().library(),
  open: (path: string) => api().libraryItem(path),
  save: (item: LibraryItem, draft: Draft) => api().saveLibraryItem(item.path, item.mode === 'code' ? { source: draft.source } : { values: draft.values, body: draft.body }),
  create: (section: string, name: string) => api().createLibraryItem(section, name),
  restore: (path: string) => api().restoreLibraryItem(path),
  assist: (path: string, draft: Draft, messages: LibraryTurn[], ask?: LibraryAsk) => api().assistLibrary(path, draft, messages, ask),
  createFile: (item: string, name: string) => api().createLibraryFile(item, name),
  outline: (path: string, source: string) => api().outlineLibrary(path, source),
}

export function applied(draft: Draft, answer: LibraryAssist): Draft {
  return { values: { ...draft.values, ...answer.values }, body: answer.body ?? draft.body, source: answer.source ?? draft.source }
}

export function touches(answer: LibraryAssist): boolean {
  return answer.body !== null || answer.source !== null || Object.keys(answer.values).length > 0
}

export function draftOf(item: LibraryItem): Draft {
  const values = Object.fromEntries((item.fields ?? []).map((field) => [field.key, field.value]))
  return { values, body: item.body?.text ?? null, source: item.source }
}

function same(a: FieldValue, b: FieldValue): boolean {
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((value, index) => value === b[index])
  return a === b
}

export function changed(item: LibraryItem, draft: Draft): boolean {
  if (item.mode === 'code') return draft.source !== item.source
  const start = draftOf(item)
  return draft.body !== start.body || Object.keys(start.values).some((key) => !same(start.values[key], draft.values[key]))
}

export function matching(items: LibraryCard[], query: string): LibraryCard[] {
  const wanted = query.trim().toLowerCase()
  if (!wanted) return items
  return items.filter((item) => `${item.title} ${item.description ?? ''} ${item.path}`.toLowerCase().includes(wanted))
}

export function fileName(path: string): string {
  return path.split('/').pop() ?? path
}

export function toggled(values: string[], value: string): string[] {
  return values.includes(value) ? values.filter((other) => other !== value) : [...values, value]
}

export interface ChatTurn extends LibraryTurn {
  before?: Draft
  undone?: boolean
}

const TALK = 'library.talk:'
const MODEL = 'library.model'
const KEPT = 40

function stored(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

function store(key: string, value: string | null) {
  try {
    if (value === null) localStorage.removeItem(key)
    else localStorage.setItem(key, value)
  } catch {
    return
  }
}

export function talkOf(path: string): ChatTurn[] {
  try {
    const kept: unknown = JSON.parse(stored(TALK + path) ?? '[]')
    return Array.isArray(kept) ? (kept as ChatTurn[]).filter((turn) => (turn.role === 'user' || turn.role === 'assistant') && typeof turn.text === 'string') : []
  } catch {
    return []
  }
}

export function keepTalk(path: string, turns: ChatTurn[]) {
  store(TALK + path, turns.length === 0 ? null : JSON.stringify(turns.slice(-KEPT).map(({ role, text }) => ({ role, text }))))
}

export const chosenModel = () => stored(MODEL)
export const chooseModel = (id: string) => store(MODEL, id)

const waiting = new Map<string, string>()

export function askOnOpen(path: string, text: string) {
  if (!text.trim()) return
  waiting.set(path, text.trim())
  store('library.chat', 'open')
}

export function askedOnOpen(path: string): string | null {
  const text = waiting.get(path) ?? null
  waiting.delete(path)
  return text
}

export type Problem = { kind: 'required' } | { kind: 'long'; size: number; max: number } | { kind: 'shape'; hint: string }

export function sizeOf(value: string): number {
  return [...value].length
}

export function problemOf(field: LibraryField, value: FieldValue | undefined): Problem | null {
  if (typeof value !== 'string') return null
  if (field.required && value.trim() === '') return { kind: 'required' }
  if (field.max !== undefined && sizeOf(value) > field.max) return { kind: 'long', size: sizeOf(value), max: field.max }
  if (field.pattern && value !== '' && !new RegExp(field.pattern).test(value)) return { kind: 'shape', hint: field.hint ?? field.pattern }
  return null
}

export function blocked(item: LibraryItem, draft: Draft): boolean {
  if (item.mode === 'code') return false
  return (item.fields ?? []).some((field) => draft.values[field.key] !== field.value && problemOf(field, draft.values[field.key]) !== null)
}
