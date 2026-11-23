import { useState } from 'react'
import { EFFORTS, type Effort } from '@/api/contract'

const MODEL = 'composer.model'
const TUNING = 'composer.tuning'

export interface RememberedTuning {
  effort: Effort
  fast: boolean
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

export function useRememberedModel() {
  const [model, setModel] = useState<string | null>(() => rememberedModel())
  const choose = (id: string | null) => {
    setModel(id)
    write(MODEL, id)
  }
  return [model, choose] as const
}

export function useRememberedTuning(fallback: RememberedTuning) {
  const [tuning, setTuning] = useState<RememberedTuning>(() => rememberedTuning(fallback))
  const choose = (next: RememberedTuning) => {
    setTuning(next)
    write(TUNING, JSON.stringify(next))
  }
  return [tuning, choose] as const
}
