import type { UsageWindow } from '@/api/contract'

export interface Pace {
  elapsed: number
  rate: number
  projected: number
  exhaustsAt: string | null
}

const SETTLING = 0.05

export function paceOf(w: UsageWindow, now: number): Pace | null {
  if (!w.startsAt || !w.resetsAt) return null
  const start = new Date(w.startsAt).getTime()
  const reset = new Date(w.resetsAt).getTime()
  if (!(reset > start) || !(now > start)) return null
  const elapsed = Math.min(1, (now - start) / (reset - start))
  if (elapsed < SETTLING) return null
  const used = Math.max(0, w.usedPercent)
  const projected = used / elapsed
  const exhaustsAt = projected > 100 && used < 100 && used > 0 ? new Date(start + ((now - start) * 100) / used).toISOString() : null
  return { elapsed, rate: projected / 100, projected, exhaustsAt }
}
