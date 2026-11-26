import { useSyncExternalStore } from 'react'
import { api } from '@/api'
import type { Application, ApplicationDraft } from '@/api/contract'
import { getState, setApplications } from '@/api/store'

const PENDING = 'pending:'

let pending: ReadonlySet<string> = new Set()
let serial = 0
const listeners = new Set<() => void>()

function mark(id: string, on: boolean) {
  const next = new Set(pending)
  if (on) next.add(id)
  else next.delete(id)
  pending = next
  listeners.forEach((l) => l())
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

export function usePending(id: string): boolean {
  return useSyncExternalStore(subscribe, () => pending.has(id), () => false)
}

export function isUnsaved(id: string): boolean {
  return id.startsWith(PENDING)
}

function sent(draft: ApplicationDraft, original: Application | null): Promise<Application> {
  return original ? api().saveApplication({ ...draft, id: original.id }) : api().createApplication(draft)
}

function placed(list: Application[], saved: Application): Application[] {
  return list.some((x) => x.id === saved.id) ? list.map((x) => (x.id === saved.id ? saved : x)) : [...list, saved]
}

export function saveApplication(draft: ApplicationDraft, original: Application | null): Promise<Application> {
  return sent(draft, original).then((saved) => {
    setApplications((list) => placed(list, saved))
    return saved
  })
}

export function saveOptimistically(draft: ApplicationDraft, original: Application | null): Promise<Application> {
  serial += 1
  const id = original?.id ?? `${PENDING}${serial}`
  const shown: Application = { ...draft, id }
  mark(id, true)
  setApplications((list) => placed(list, shown))
  return sent(draft, original)
    .then((saved) => {
      setApplications((list) => placed(list.filter((x) => x.id !== id || x.id === saved.id), saved))
      return saved
    })
    .catch((e: unknown) => {
      setApplications((list) => (original ? list.map((x) => (x.id === id ? original : x)) : list.filter((x) => x.id !== id)))
      throw e
    })
    .finally(() => mark(id, false))
}

export function removeOptimistically(application: Application): Promise<void> {
  const at = getState().applications?.findIndex((x) => x.id === application.id) ?? -1
  setApplications((list) => list.filter((x) => x.id !== application.id))
  return api()
    .deleteApplication(application.id)
    .then(() => undefined)
    .catch((e: unknown) => {
      setApplications((list) => (list.some((x) => x.id === application.id) ? list : [...list.slice(0, Math.max(at, 0)), application, ...list.slice(Math.max(at, 0))]))
      throw e
    })
}
