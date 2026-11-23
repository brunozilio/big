import type { Usage } from '@/api/contract'

const UNKNOWN_PROVIDER = 'unknown'
const NOT_MODELS = new Set(['auto', 'unknown', ''])

export function accountLabel(account: string): string {
  const trimmed = account.trim()
  return trimmed.startsWith('arn:') ? '' : trimmed
}

function without(map: Record<string, number>, drop: (key: string) => boolean): Record<string, number> {
  return Object.fromEntries(Object.entries(map).filter(([key]) => !drop(key)))
}

export function tidyUsage(usage: Usage): Usage {
  const badProvider = (id: string) => id === UNKNOWN_PROVIDER
  const badModel = (id: string) => NOT_MODELS.has(id.toLowerCase())
  return {
    ...usage,
    daily: usage.daily.map((d) => ({ ...d, byProvider: without(d.byProvider, badProvider), byModel: without(d.byModel, badModel) })),
    byProvider: usage.byProvider.filter((p) => !badProvider(p.id)),
    byModel: usage.byModel.filter((m) => !badModel(m.id)),
    byAccount: usage.byAccount.filter((a) => !badProvider(a.provider)),
  }
}
