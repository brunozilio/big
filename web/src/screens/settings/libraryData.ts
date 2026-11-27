import { api } from '@/api'
import type { LibraryCard, LibraryItem, LibraryValue } from '@/api/contract'

export type { Library, LibraryCard, LibraryField, LibraryItem, LibrarySection } from '@/api/contract'

export type FieldValue = LibraryValue

export interface Draft {
  values: Record<string, FieldValue>
  body: string | null
  source: string
}

export const libraryApi = {
  list: () => api().library(),
  open: (path: string) => api().libraryItem(path),
  save: (item: LibraryItem, draft: Draft) => api().saveLibraryItem(item.path, item.mode === 'code' ? { source: draft.source } : { values: draft.values, body: draft.body }),
  create: (section: string, name: string) => api().createLibraryItem(section, name),
  restore: (path: string) => api().restoreLibraryItem(path),
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
