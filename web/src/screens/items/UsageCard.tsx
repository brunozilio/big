import { api } from '@/api'
import type { Item, ItemUsage } from '@/api/contract'
import { useLoad } from '@/api/store'
import { useT } from '@/i18n'
import { money } from '@/ui/cn'
import { Card, ProgressBar } from '@/ui/display'
import { Icon } from '@/ui/icon'
import { compact as short } from '../settings/usageCards'

const LIVE_REFRESH_MS = 15_000

export function refreshKey(it: Pick<Item, 'live' | 'updatedAt'>): string {
  if (!it.live) return it.updatedAt
  const at = Date.parse(it.updatedAt)
  return Number.isNaN(at) ? it.updatedAt : `live-${Math.floor(at / LIVE_REFRESH_MS)}`
}

export function UsageCard({ it }: { it: Item }) {
  const t = useT()
  const { data } = useLoad<ItemUsage>(() => api().itemUsage(it.id), [it.id, refreshKey(it)])
  if (!data) return null
  const { spent, context } = data
  return (
    <Card variant="inset" className="space-y-3 p-3" aria-label={t('items.usageTitle')}>
      <div className="space-y-1.5">
        <div className="flex items-baseline justify-between gap-2 text-sm">
          <span className="text-fg2">{t('items.contextLabel')}</span>
          {context && <span className="tabular-nums text-xs text-muted">{t('items.contextOf', { used: short(context.used), window: short(context.window) })}</span>}
        </div>
        {context ? <ProgressBar value={context.percent} onInset /> : <p className="text-xs text-faint">{t('items.noContextYet')}</p>}
      </div>
      <div className="space-y-0.5">
        <div className="flex items-baseline justify-between gap-2 text-sm">
          <span className="text-fg2">{t('items.costLabel')}</span>
          <span className="tabular-nums text-fg">{spent.costUsd === null ? t('items.costUnknown') : money(spent.costUsd, 'USD')}</span>
        </div>
        <p className="flex items-center gap-3 truncate text-xs tabular-nums text-faint">
          <span className="inline-flex items-center gap-1" title={t('items.tokensIn')}>
            <Icon name="upload" size={16} />
            <span className="sr-only">{t('items.tokensIn')}</span>
            {short(spent.input + spent.cacheRead + spent.cacheWrite)}
          </span>
          <span className="inline-flex items-center gap-1" title={t('items.tokensOut')}>
            <Icon name="download" size={16} />
            <span className="sr-only">{t('items.tokensOut')}</span>
            {short(spent.output)}
          </span>
        </p>
      </div>
    </Card>
  )
}
