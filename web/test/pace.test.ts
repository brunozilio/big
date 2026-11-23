import { describe, expect, test } from 'bun:test'
import { paceOf } from '@/screens/settings/pace'

const hour = 3_600_000
const start = Date.UTC(2026, 10, 1)
const w = (usedPercent: number, hours: number, startsAt: string | null = new Date(start).toISOString()) => ({ id: 'w', label: 'W', usedPercent, startsAt, resetsAt: new Date(start + hours * hour).toISOString() })

describe('pace', () => {
  test('usage ahead of time projects past the limit and says when it runs out', () => {
    const pace = paceOf(w(60, 10), start + 4 * hour)
    expect(pace?.elapsed).toBeCloseTo(0.4)
    expect(pace?.projected).toBeCloseTo(150)
    expect(pace?.rate).toBeCloseTo(1.5)
    expect(pace?.exhaustsAt).toBe(new Date(start + (4 * hour * 100) / 60).toISOString())
  })

  test('usage behind time projects under the limit and never runs out', () => {
    const pace = paceOf(w(20, 10), start + 5 * hour)
    expect(pace?.projected).toBeCloseTo(40)
    expect(pace?.exhaustsAt).toBeNull()
  })

  test('without a start, or too early in the window, there is no pace', () => {
    expect(paceOf(w(10, 10, null), start + 5 * hour)).toBeNull()
    expect(paceOf(w(10, 100), start + hour)).toBeNull()
  })
})

describe('tidy usage', () => {
  test('drops unknown providers, non-models and ARN account names', async () => {
    const { tidyUsage, accountLabel } = await import('@/screens/settings/usageTidy')
    const t = { costUsd: 1, tokens: 1, calls: 1 }
    const usage = tidyUsage({
      days: 7, from: 'a', to: 'b',
      totals: { costUsd: 2, input: 0, output: 0, cacheRead: 0, cacheWrite: 0, calls: 2, sessions: 1 },
      daily: [{ date: 'a', costUsd: 2, tokens: 0, calls: 2, byProvider: { unknown: 1, kiro: 1 }, byModel: { auto: 1, 'claude-opus': 1 } }],
      byProvider: [{ id: 'unknown', name: 'Sem registro', logo: '', ...t }, { id: 'kiro', name: 'Kiro', logo: '', ...t }],
      byModel: [{ id: 'auto', name: 'auto', ...t }, { id: 'claude-opus', name: 'claude-opus', ...t }],
      byAccount: [{ id: 'unknown:', provider: 'unknown', account: '', ...t }, { id: 'kiro:x', provider: 'kiro', account: 'x', ...t }],
    })
    expect(usage.byProvider.map((p) => p.id)).toEqual(['kiro'])
    expect(usage.byModel.map((m) => m.id)).toEqual(['claude-opus'])
    expect(usage.byAccount.map((a) => a.id)).toEqual(['kiro:x'])
    expect(usage.daily[0].byModel).toEqual({ 'claude-opus': 1 })
    expect(accountLabel('arn:aws:codewhisperer:us-east-1:1:profile/X')).toBe('')
  })
})
