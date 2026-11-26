import type { Application, ApplicationDraft, ApplicationRepository, Bindings } from '@/api/contract'

export const FALLBACK_KINDS = ['application', 'library', 'configuration', 'infrastructure', 'tests'] as const

export type FallbackKind = (typeof FALLBACK_KINDS)[number]

export function isFallbackKind(name: string): name is FallbackKind {
  return (FALLBACK_KINDS as readonly string[]).includes(name)
}

export function folderName(directory: string): string {
  return directory.split('/').filter(Boolean).pop() ?? directory
}

export function shortPath(directory: string): string {
  return directory.replace(/^\/(Users|home)\/[^/]+(?=\/|$)/, '~')
}

export function parentOf(directory: string): string | null {
  const at = directory.replace(/\/+$/, '').lastIndexOf('/')
  return at > 0 ? directory.slice(0, at) : null
}

export function blankRepository(directory = '', kind: string = FALLBACK_KINDS[0]): ApplicationRepository {
  return { id: '', directory, kind, run: [], verify: [], e2e: [], e2eDirectory: '', bindings: {} }
}

export function withFolders(repositories: ApplicationRepository[], paths: string[], kind: string = FALLBACK_KINDS[0]): ApplicationRepository[] {
  const known = new Set(repositories.map((r) => r.directory).filter(Boolean))
  const fresh = [...new Set(paths)].filter((path) => !known.has(path))
  const next = repositories.map((r) => (r.directory || fresh.length === 0 ? r : { ...r, directory: fresh.shift() ?? '' }))
  return [...next, ...fresh.map((path) => blankRepository(path, kind))]
}

export function withAccess(repository: ApplicationRepository, readOnly: boolean): ApplicationRepository {
  const { readOnly: _, ...rest } = repository
  return readOnly ? { ...rest, readOnly: true } : rest
}

export type Variable = [string, string]

export interface Row {
  key: string
  value: ApplicationRepository
}

export interface Editing {
  name: string
  bindings: Bindings
  repositories: Row[]
  variables: Variable[]
}

let serial = 0

function rowOf(value: ApplicationRepository): Row {
  serial += 1
  return { key: `r${serial}`, value }
}

export function draftOf(application: Application | null, kind?: string): ApplicationDraft {
  return application ? structuredClone(application) : { name: '', bindings: {}, repositories: [blankRepository('', kind)], env: {} }
}

export function editingOf(draft: ApplicationDraft): Editing {
  return { name: draft.name, bindings: draft.bindings ?? {}, repositories: draft.repositories.map((r) => rowOf(structuredClone(r))), variables: Object.entries(draft.env ?? {}) }
}

function envOf(rows: Variable[]): Record<string, string> {
  return Object.fromEntries(rows.map(([name, value]): Variable => [name.trim(), value]).filter(([name]) => name))
}

const commands = (list: string[] | undefined) => (list ?? []).map((c) => c.trim()).filter(Boolean)

function cleanRepository(repository: ApplicationRepository): ApplicationRepository {
  const e2e = repository.e2e === undefined ? {} : { e2e: commands(repository.e2e) }
  const folder = repository.e2eDirectory === undefined ? {} : { e2eDirectory: repository.e2eDirectory.trim() }
  return { ...repository, run: commands(repository.run), verify: commands(repository.verify), ...e2e, ...folder }
}

export function cleanOf(editing: Editing, base: ApplicationDraft): ApplicationDraft {
  return { ...base, name: editing.name.trim(), bindings: editing.bindings, repositories: editing.repositories.map((r) => cleanRepository(r.value)), env: envOf(editing.variables) }
}

export function sameDraft(a: ApplicationDraft, b: ApplicationDraft): boolean {
  return JSON.stringify(a) === JSON.stringify(b)
}

export type Change =
  | { type: 'name'; name: string }
  | { type: 'named'; path: string }
  | { type: 'bindings'; bindings: Bindings }
  | { type: 'variables'; variables: Variable[] }
  | { type: 'repository'; key: string; change: (repository: ApplicationRepository) => ApplicationRepository }
  | { type: 'remove'; key: string }
  | { type: 'folders'; paths: string[]; kind?: string }
  | { type: 'suggested'; path: string; bindings: Bindings }

function withFolderRows(rows: Row[], paths: string[], kind?: string): Row[] {
  const next = withFolders(rows.map((r) => r.value), paths, kind)
  return next.map((value, at) => (at < rows.length ? (rows[at].value === value ? rows[at] : { key: rows[at].key, value }) : rowOf(value)))
}

export function reduce(state: Editing, change: Change): Editing {
  switch (change.type) {
    case 'name':
      return { ...state, name: change.name }
    case 'named':
      return state.name.trim() ? state : { ...state, name: folderName(change.path) }
    case 'bindings':
      return { ...state, bindings: change.bindings }
    case 'variables':
      return { ...state, variables: change.variables }
    case 'repository':
      return { ...state, repositories: state.repositories.map((r) => (r.key === change.key ? { key: r.key, value: change.change(r.value) } : r)) }
    case 'remove':
      return { ...state, repositories: state.repositories.filter((r) => r.key !== change.key) }
    case 'folders':
      return { ...state, repositories: withFolderRows(state.repositories, change.paths, change.kind) }
    case 'suggested':
      return { ...state, repositories: state.repositories.map((r) => (r.value.directory === change.path ? { key: r.key, value: { ...r.value, bindings: { ...change.bindings, ...r.value.bindings } } } : r)) }
  }
}
