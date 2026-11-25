import type { MouseEvent } from 'react'
import type { Connection, Provider } from '@/api/contract'
import { Button } from '@/ui/button'
import { LogoTile } from '@/ui/domain'
import { Icon } from '@/ui/icon'
import { Tooltip } from '@/ui/overlay'
import { useT } from '@/i18n'

function CardStatus({ accounts, onConnect }: { accounts: Connection[]; onConnect: () => void }) {
  const t = useT()
  const press = (e: MouseEvent) => {
    e.stopPropagation()
    onConnect()
  }
  if (accounts.some((c) => c.status === 'error')) return <Button size="sm" variant="secondary" icon={<Icon name="alert" size={14} className="text-danger-fg" />} onClick={press}>{t('settings.reconnect')}</Button>
  if (accounts.length === 0) return <Button size="sm" variant="secondary" onClick={press}>{t('settings.connect')}</Button>
  const label = accounts.length === 1 ? t('settings.connected') : t('settings.connectedCount', { n: accounts.length })
  return (
    <Tooltip content={label}>
      <span aria-label={label} className="flex size-6 items-center justify-center rounded-full bg-success-soft text-success-fg"><Icon name="check" size={13} strokeWidth={2.2} /></span>
    </Tooltip>
  )
}

export function ConnectionCard({ provider: p, accounts, onConnect }: { provider: Provider; accounts: Connection[]; onConnect: (provider: string) => void }) {
  return (
    <div role="button" tabIndex={0} data-provider={p.key} onClick={() => onConnect(p.key)} onKeyDown={(e) => e.key === 'Enter' && onConnect(p.key)} className="flex h-14 cursor-pointer items-center gap-3 rounded-card bg-surface px-3 shadow-card transition-shadow hover:shadow-pop">
      <LogoTile logo={p.logo} name={p.name} size={32} />
      <div className="min-w-0 flex-1">
        <p className="truncate text-base font-medium text-fg">{p.name}</p>
        <p className="truncate text-sm text-muted">{p.description}</p>
      </div>
      <CardStatus accounts={accounts} onConnect={() => onConnect(p.key)} />
    </div>
  )
}
