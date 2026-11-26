import type { Item } from '@/api/contract'
import { SETTINGS_GROUPS, type SettingsTab } from './settingsNav'

export interface PaletteEntry {
  id: string
  kind: 'page' | 'item'
  label: string
  hint?: string
  to: string
}

export interface PaletteLabels {
  deliveries: string
  security: string
  settings: string
  tab: (tab: SettingsTab) => string
  title: (item: Item) => string
}

export function itemPath(id: string, step?: string | null): string {
  const base = `/items/${encodeURIComponent(id)}`
  return step ? `${base}/${encodeURIComponent(step)}` : base
}

export function fold(text: string): string {
  return text.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase().trim()
}

export function paletteEntries(items: Item[], labels: PaletteLabels): PaletteEntry[] {
  const pages: PaletteEntry[] = [
    { id: 'page:items', kind: 'page', label: labels.deliveries, to: '/' },
    { id: 'page:security', kind: 'page', label: labels.security, to: '/security' },
    ...SETTINGS_GROUPS.flatMap((g) => g.tabs).map((tab): PaletteEntry => ({ id: `page:settings:${tab}`, kind: 'page', label: labels.tab(tab), hint: labels.settings, to: `/settings/${tab}` })),
  ]
  const open = items.filter((it) => !it.archivedAt).map((it): PaletteEntry => ({ id: `item:${it.id}`, kind: 'item', label: labels.title(it), hint: it.issue?.key, to: itemPath(it.id) }))
  return [...pages, ...open]
}

function rank(entry: PaletteEntry, words: string[]): number {
  const label = fold(entry.label)
  const hint = fold(entry.hint ?? '')
  const all = `${label} ${hint}`
  if (!words.every((w) => all.includes(w))) return -1
  const first = words[0]
  if (hint === words.join(' ') || label === words.join(' ')) return 0
  if (label.startsWith(first) || hint.startsWith(first)) return 1
  if (label.split(/\s+/).some((part) => part.startsWith(first))) return 2
  return 3
}

export function filterEntries(entries: PaletteEntry[], query: string, limit = 40): PaletteEntry[] {
  const words = fold(query).split(/\s+/).filter(Boolean)
  if (words.length === 0) return entries.slice(0, limit)
  return entries
    .map((entry, index) => ({ entry, index, score: rank(entry, words) }))
    .filter((x) => x.score >= 0)
    .sort((a, b) => a.score - b.score || a.index - b.index)
    .slice(0, limit)
    .map((x) => x.entry)
}
