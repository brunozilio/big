import type { ItemUsage } from '../contract'
import type { SimItem } from './sim'

const WINDOW = 200_000

export function mockUsage(it: SimItem): ItemUsage {
  const used = Math.min(WINDOW - 8_000, 18_000 + it.cur * 21_000 + (it.h % 9) * 3_100 + Math.round(it.elapsed / 40))
  const input = 40_000 + it.cur * 55_000 + (it.h % 7) * 9_000
  const output = Math.round(input / 9)
  const cacheRead = input * 6
  const cost = input * 0.000005 + output * 0.000025 + cacheRead * 0.0000005
  return {
    spent: { costUsd: Math.round(cost * 100) / 100, input, output, cacheRead, cacheWrite: Math.round(input / 3), calls: 6 + it.cur * 11 },
    context: it.cur === 0 && it.elapsed === 0 ? null : { used, window: WINDOW, percent: Math.min(100, Math.floor((used * 100) / WINDOW)) },
  }
}
