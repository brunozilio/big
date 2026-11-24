import { useSyncExternalStore } from 'react'
import { EFFORTS, type Effort } from '@/api/contract'

const MODEL = 'composer.model'
const TUNING = 'composer.tuning'

export interface RememberedTuning {
  effort: Effort
  fast: boolean
}

const listeners = new Set<() => void>()

function notify(): void {
  for (const listener of listeners) listener()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  const onStorage = (e: StorageEvent) => {
    if (e.key === null || e.key === MODEL || e.key === TUNING) listener()
  }
  globalThis.addEventListener?.('storage', onStorage)
  return () => {
    listeners.delete(listener)
    globalThis.removeEventListener?.('storage', onStorage)
  }
}

function read(key: string): string | null {
  try {
    return globalThis.localStorage?.getItem(key) || null
  } catch {
    return null
  }
}

function write(key: string, value: string | null): void {
  try {
    const kept = globalThis.localStorage
    if (!kept) return
    if (value) kept.setItem(key, value)
    else kept.removeItem(key)
  } catch {
    return
  } finally {
    notify()
  }
}

export function rememberedModel(): string | null {
  return read(MODEL)
}

export function rememberedTuning(fallback: RememberedTuning): RememberedTuning {
  try {
    const parsed: unknown = JSON.parse(read(TUNING) ?? 'null')
    if (!parsed || typeof parsed !== 'object') return fallback
    const { effort, fast } = parsed as Record<string, unknown>
    return {
      effort: EFFORTS.includes(effort as Effort) ? (effort as Effort) : fallback.effort,
      fast: typeof fast === 'boolean' ? fast : fallback.fast,
    }
  } catch {
    return fallback
  }
}

function chooseModel(id: string | null): void {
  write(MODEL, id)
}

function chooseTuning(next: RememberedTuning): void {
  write(TUNING, JSON.stringify(next))
}

export function useRememberedModel() {
  const model = useSyncExternalStore(subscribe, rememberedModel, () => null)
  return [model, chooseModel] as const
}

export function useRememberedTuning(fallback: RememberedTuning) {
  const raw = useSyncExternalStore(subscribe, () => read(TUNING), () => null)
  const tuning = raw === null ? fallback : rememberedTuning(fallback)
  return [tuning, chooseTuning] as const
}
