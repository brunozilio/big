import { motion } from 'framer-motion'
import { useState } from 'react'
import { api } from '@/api'
import type { UsageLimitAccount, UsageWindow } from '@/api/contract'
import { useLoad } from '@/api/store'
import { cn, relTime } from '@/ui/cn'
import { Badge, Card, EmptyState, Skeleton } from '@/ui/display'
import { IconButton } from '@/ui/button'
import { LogoTile } from '@/ui/domain'
import { Icon } from '@/ui/icon'
import { SectionTitle } from '@/ui/layout'
import { stagger, tween } from '@/ui/motion'
import { paceOf } from './pace'
import { accountLabel } from './usageTidy'

export function limitTone(pct: number): 'critical' | 'warning' | 'normal' {
  return pct >= 95 ? 'critical' : pct >= 80 ? 'warning' : 'normal'
}

const barClass = { critical: 'bg-danger', warning: 'bg-warning', normal: 'bg-fg/80' }
const textClass = { critical: 'text-danger-fg', warning: 'text-warning-fg', normal: 'text-fg2' }

function LimitBar({ window: w }: { window: UsageWindow }) {
  const pct = Math.min(100, Math.max(0, w.usedPercent))
  const tone = limitTone(pct)
  const pace = paceOf(w, Date.now())
  return (
    <li className="space-y-1.5">
      <div className="flex items-baseline gap-2 text-sm">
        <span className="min-w-0 flex-1 truncate text-fg2">{w.label}</span>
        <span className={cn('font-medium tabular-nums', textClass[tone])}>{Math.round(pct)}%</span>
      </div>
      <div className="relative">
        <div role="meter" aria-label={w.label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(pct)} className="h-[5px] w-full overflow-hidden rounded-full bg-inset">
          <motion.div className={cn('h-full w-full origin-left rounded-full', barClass[tone])} initial={{ scaleX: 0 }} animate={{ scaleX: pct / 100 }} transition={tween} />
        </div>
        {pace && <span data-testid="elapsed-tick" title="Tempo decorrido da janela" className="absolute -top-[3px] h-[11px] w-[2px] -translate-x-1/2 rounded-full bg-fg/50" style={{ left: `${pace.elapsed * 100}%` }} />}
      </div>
      {w.resetsAt && <p className="text-xs text-faint">reinicia {relTime(w.resetsAt)}</p>}
    </li>
  )
}

function AccountBody({ account }: { account: UsageLimitAccount }) {
  if (account.status === 'unsupported') return <p className="text-sm text-faint">Este provedor não informa limites</p>
  if (account.status === 'error') {
    return (
      <p className="flex items-start gap-1.5 text-sm text-danger-fg">
        <Icon name="alert" size={14} className="mt-0.5 shrink-0" />
        <span>{account.error ?? 'Não foi possível consultar os limites'}</span>
      </p>
    )
  }
  if (!account.windows.length) return <p className="text-sm text-faint">Nenhum limite ativo</p>
  return (
    <ul className="space-y-3.5">
      {account.windows.map((w) => <LimitBar key={w.id} window={w} />)}
    </ul>
  )
}

function AccountCard({ account, index }: { account: UsageLimitAccount; index: number }) {
  return (
    <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ ...tween, delay: stagger(index) }}>
      <Card className="h-full p-5" data-testid="limit-card">
        <div className="mb-4 flex items-center gap-3">
          <LogoTile logo={account.logo} name={account.name} size={32} />
          <div className="min-w-0 flex-1">
            <p className="flex items-center gap-2 text-base font-medium text-fg">
              <span className="truncate">{account.name}</span>
              {account.plan && <Badge>{account.plan}</Badge>}
            </p>
            {accountLabel(account.account) && <p className="truncate text-sm text-muted">{accountLabel(account.account)}</p>}
          </div>
        </div>
        <AccountBody account={account} />
      </Card>
    </motion.div>
  )
}

function LimitsSkeleton() {
  return (
    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3" aria-busy="true">
      {Array.from({ length: 3 }, (_, i) => (
        <Card key={i} className="space-y-4 p-5">
          <div className="flex items-center gap-3">
            <Skeleton className="size-8 rounded-[9px]" />
            <div className="flex-1 space-y-2">
              <Skeleton className="w-24" />
              <Skeleton className="h-2.5 w-36" />
            </div>
          </div>
          <Skeleton className="h-[5px] w-full" />
          <Skeleton className="h-[5px] w-full" />
        </Card>
      ))}
    </div>
  )
}

export function LimitsSection() {
  const [nonce, setNonce] = useState(0)
  const limits = useLoad(() => api().usageLimits(), [nonce])
  const refresh = (
    <IconButton tooltip="Atualizar limites" size="sm" onClick={() => setNonce((n) => n + 1)} disabled={limits.loading}>
      <motion.span className="flex" animate={limits.loading ? { rotate: 360 } : { rotate: 0 }} transition={limits.loading ? { repeat: Infinity, duration: 0.9, ease: 'linear' } : { duration: 0 }}>
        <Icon name="refresh" size={15} />
      </motion.span>
    </IconButton>
  )
  return (
    <section className="mb-10">
      <SectionTitle className="mb-3" actions={refresh}>Limites do plano</SectionTitle>
      {limits.error && <p role="alert" className="mb-3 text-sm text-danger-fg">{limits.error}</p>}
      {!limits.data ? (
        limits.loading ? <LimitsSkeleton /> : null
      ) : limits.data.accounts.length === 0 ? (
        <EmptyState size="inline" align="start" title="Nenhuma conta de assinatura conectada" />
      ) : (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {limits.data.accounts.map((a, i) => <AccountCard key={`${a.provider}:${a.account}`} account={a} index={i} />)}
        </div>
      )}
    </section>
  )
}
