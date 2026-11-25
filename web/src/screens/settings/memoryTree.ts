export type MemoryKind = 'episodic' | 'semantic' | 'procedural'

export interface Memory {
  id: number
  kind: MemoryKind
  text: string
  tags: string[]
}

export interface Forgotten {
  memory: Memory
  at: number
}

export interface Kept {
  memories: Memory[]
  forgotten: Forgotten[]
}

export interface RepositoryMemory extends Kept {
  directory: string
  team: Memory[]
}

export interface ApplicationMemory extends Kept {
  id: string
  name: string
  repositories: RepositoryMemory[]
}

export interface MemoryTree {
  global: Kept
  applications: ApplicationMemory[]
}

export type Shelf = { scope: 'global' } | { scope: 'application'; id: string } | { scope: 'repository'; directory: string }

export interface Place {
  shelf: Shelf
  key: string
  label: string
}

export function keyOf(shelf: Shelf): string {
  if (shelf.scope === 'global') return 'global'
  return shelf.scope === 'application' ? `application:${shelf.id}` : `repository:${shelf.directory}`
}

export function folderName(directory: string): string {
  return directory.split('/').filter(Boolean).pop() ?? directory
}

export function placesOf(tree: MemoryTree, globalLabel: string): Place[] {
  const global: Shelf = { scope: 'global' }
  const places: Place[] = [{ shelf: global, key: keyOf(global), label: globalLabel }]
  for (const application of tree.applications) {
    const shelf: Shelf = { scope: 'application', id: application.id }
    places.push({ shelf, key: keyOf(shelf), label: application.name })
    for (const repository of application.repositories) {
      const inner: Shelf = { scope: 'repository', directory: repository.directory }
      if (!places.some((p) => p.key === keyOf(inner))) places.push({ shelf: inner, key: keyOf(inner), label: `${application.name} · ${folderName(repository.directory)}` })
    }
  }
  return places
}

function fold(text: string): string {
  return text.normalize('NFD').replace(/\p{Diacritic}/gu, '').toLowerCase()
}

export function matches(query: string) {
  const wanted = fold(query.trim())
  return (memory: Memory) => !wanted || fold(memory.text).includes(wanted) || memory.tags.some((tag) => fold(tag).includes(wanted))
}

export interface ForgottenAt {
  shelf: Shelf
  label: string
  forgotten: Forgotten
}

export function forgottenOf(tree: MemoryTree, globalLabel: string): ForgottenAt[] {
  const places = placesOf(tree, globalLabel)
  const label = (shelf: Shelf) => places.find((p) => p.key === keyOf(shelf))?.label ?? ''
  const all: ForgottenAt[] = tree.global.forgotten.map((forgotten) => ({ shelf: { scope: 'global' }, label: label({ scope: 'global' }), forgotten }))
  const seen = new Set<string>()
  for (const application of tree.applications) {
    const shelf: Shelf = { scope: 'application', id: application.id }
    all.push(...application.forgotten.map((forgotten) => ({ shelf, label: label(shelf), forgotten })))
    for (const repository of application.repositories) {
      const inner: Shelf = { scope: 'repository', directory: repository.directory }
      if (seen.has(repository.directory)) continue
      seen.add(repository.directory)
      all.push(...repository.forgotten.map((forgotten) => ({ shelf: inner, label: label(inner), forgotten })))
    }
  }
  return all.sort((a, b) => b.forgotten.at - a.forgotten.at)
}

export function countOf(application: ApplicationMemory): number {
  return application.memories.length + application.repositories.reduce((sum, r) => sum + r.memories.length + r.team.length, 0)
}
