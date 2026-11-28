import { useState } from 'react'
import { Link } from 'react-router-dom'
import { Area, AreaChart, Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { api } from '@/api'
import type { Connections, Usage, UsageDays } from '@/api/contract'
import { useLoad, useStore } from '@/api/store'
import { Button } from '@/ui/button'
import { Card, EmptyState, Skeleton } from '@/ui/display'
import { ProviderLogo } from '@/ui/domain'
import { SegmentedControl } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { pageTitleClass } from '@/ui/layout'
import { Counter } from '@/ui/motion'
import { useT } from '@/i18n'
import { LimitsSection } from './UsageLimits'
import { accountLabel, tidyUsage } from './usageTidy'
import { axis, axisUsd, ChartCard, ChartTooltip, compact, RankCard, seriesOf, StatCard, usd } from './usageCards'

type DaysOption = '7' | '30' | '90'

export function hasSubscription(data: Connections | null): boolean | null {
  if (!data) return null
  const ai = new Set(data.providers.filter((p) => p.ai).map((p) => p.key))
  return data.connections.some((c) => ai.has(c.provider))
}

export function UsageView() {
  const t = useT()
  const [days, setDays] = useState<DaysOption>('30')
  const connected = useStore((s) => hasSubscription(s.connections))
  const usage = useLoad(() => api().usage(Number(days) as UsageDays), [days])
  return (
    <div>
      <div className="mb-8 flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className={pageTitleClass}>{t('settings.usage_title')}</h1>
        </div>
        <SegmentedControl<DaysOption>
          size="sm"
          value={days}
          onChange={setDays}
          options={[
            { value: '7', label: t('settings.usage_days', { n: 7 }) },
            { value: '30', label: t('settings.usage_days', { n: 30 }) },
            { value: '90', label: t('settings.usage_days', { n: 90 }) },
          ]}
        />
      </div>
      {connected === false ? <NoSubscription /> : <LimitsSection />}
      {usage.error && <p role="alert" className="mb-4 text-sm text-danger-fg">{usage.error}</p>}
      {usage.data ? <UsageBody usage={usage.data} /> : usage.loading && <UsageSkeleton />}
    </div>
  )
}

function NoSubscription() {
  const t = useT()
  return (
    <Card className="mb-10">
      <EmptyState
        className="py-12"
        icon={<Icon name="plug" size={28} className="mb-4 text-faint" />}
        title={t('settings.usage_noSubscription')}
        text={t('settings.usage_noSubscriptionText')}
        action={
          <Button asChild variant="primary">
            <Link to="/settings/connections">{t('settings.usage_goConnections')}</Link>
          </Button>
        }
      />
    </Card>
  )
}

function UsageSkeleton() {
  return (
    <div className="space-y-6" aria-busy="true">
      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        {Array.from({ length: 4 }, (_, i) => <Skeleton key={i} className="h-[92px] rounded-card" />)}
      </div>
      <div className="grid gap-4 lg:grid-cols-2">
        <Skeleton className="h-[330px] rounded-card" />
        <Skeleton className="h-[330px] rounded-card" />
      </div>
    </div>
  )
}

function UsageBody({ usage: raw }: { usage: Usage }) {
  const tr = useT()
  const usage = tidyUsage(raw)
  if (usage.totals.calls === 0) {
    return (
      <Card>
        <EmptyState className="py-14" icon={<Icon name="chart" size={28} className="mb-4 text-faint" />} title={tr('settings.usage_none', { n: usage.days })} text={tr('settings.usage_noneText')} />
      </Card>
    )
  }
  const t = usage.totals
  const tokens = t.input + t.output + t.cacheRead + t.cacheWrite
  return (
    <>
      <div className="mb-6 grid grid-cols-2 gap-3 md:grid-cols-4">
        <StatCard index={0} icon="dollar" label={tr('settings.usage_cost')} value={<Counter value={t.costUsd} format={usd} />} hint={t.unpricedCalls ? tr('settings.usage_unpriced', { n: t.unpricedCalls }) : undefined} />
        <StatCard index={1} icon="cpu" label={tr('settings.usage_tokens')} value={<Counter value={tokens} format={compact} />} hint={tr('settings.usage_cache', { n: compact(t.cacheRead) })} />
        <StatCard index={2} icon="flash" label={tr('settings.usage_calls')} value={<Counter value={t.calls} />} />
        <StatCard index={3} icon="chat" label={tr('settings.usage_sessions')} value={<Counter value={t.sessions} />} />
      </div>
      <UsageCharts usage={usage} />
      <UsageRanks usage={usage} />
    </>
  )
}

function UsageCharts({ usage }: { usage: Usage }) {
  const t = useT()
  const providers = seriesOf(usage.byProvider, usage.daily, (d) => d.byProvider)
  const models = seriesOf(usage.byModel, usage.daily, (d) => d.byModel)
  return (
    <div className="mb-6 grid gap-4 lg:grid-cols-2">
      <ChartCard index={0} title={t('settings.usage_costBySubscription')} legend={providers.series}>
        <ResponsiveContainer width="100%" height="100%">
          <AreaChart data={providers.data} margin={{ left: -8, right: 4, top: 4 }}>
            <defs>
              {providers.series.map((s) => (
                <linearGradient key={s.key} id={`usage-grad-${s.key}`} x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor={s.color} stopOpacity={0.35} />
                  <stop offset="100%" stopColor={s.color} stopOpacity={0.02} />
                </linearGradient>
              ))}
            </defs>
            <CartesianGrid stroke="rgb(var(--border))" strokeDasharray="3 4" vertical={false} />
            <XAxis dataKey="date" {...axis} minTickGap={24} />
            <YAxis {...axis} width={44} tickFormatter={axisUsd} />
            <Tooltip content={<ChartTooltip />} cursor={{ stroke: 'rgb(var(--border-strong))' }} />
            {providers.series.map((s) => (
              <Area key={s.key} type="monotone" dataKey={s.key} name={s.name} stackId="1" stroke={s.color} strokeWidth={1.8} fill={`url(#usage-grad-${s.key})`} animationDuration={250} animationBegin={0} />
            ))}
          </AreaChart>
        </ResponsiveContainer>
      </ChartCard>
      <ChartCard index={1} title={t('settings.usage_costByModel')} legend={models.series}>
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={models.data} margin={{ left: -8, right: 4, top: 4 }} barCategoryGap="22%">
            <CartesianGrid stroke="rgb(var(--border))" strokeDasharray="3 4" vertical={false} />
            <XAxis dataKey="date" {...axis} minTickGap={24} />
            <YAxis {...axis} width={44} tickFormatter={axisUsd} />
            <Tooltip content={<ChartTooltip />} cursor={{ fill: 'rgb(var(--inset))' }} />
            {models.series.map((s, i) => (
              <Bar key={s.key} dataKey={s.key} name={s.name} stackId="1" fill={s.color} radius={i === models.series.length - 1 ? [3, 3, 0, 0] : 0} animationDuration={250} animationBegin={0} />
            ))}
          </BarChart>
        </ResponsiveContainer>
      </ChartCard>
    </div>
  )
}

function UsageRanks({ usage }: { usage: Usage }) {
  const t = useT()
  const provider = (id: string) => usage.byProvider.find((p) => p.id === id)
  const logo = (id: string) => {
    const p = provider(id)
    return p?.logo ? <ProviderLogo logo={p.logo} name={p.name} size={20} /> : <Icon name="dashedCircle" size={18} className="text-faint" />
  }
  return (
    <div className="grid gap-4 lg:grid-cols-3">
      <RankCard index={0} title={t('settings.usage_bySubscription')} rows={usage.byProvider.slice(0, 6).map((p) => ({ id: p.id, name: p.name, cost: p.costUsd, unpriced: p.unpricedCalls, avatar: logo(p.id) }))} />
      <RankCard
        index={1}
        title={t('settings.usage_byAccount')}
        rows={usage.byAccount.slice(0, 6).map((a) => ({ id: a.id, name: accountLabel(a.account) || provider(a.provider)?.name || a.provider, detail: accountLabel(a.account) ? provider(a.provider)?.name : undefined, cost: a.costUsd, unpriced: a.unpricedCalls, avatar: logo(a.provider) }))}
      />
      <RankCard
        index={2}
        title={t('settings.usage_byModel')}
        rows={usage.byModel.slice(0, 6).map((m) => ({ id: m.id, name: m.name, cost: m.costUsd, unpriced: m.unpricedCalls, avatar: <Icon name="cpu" size={18} className="text-muted" /> }))}
      />
      {usage.byAgent && usage.byAgent.length > 0 && (
        <RankCard
          index={3}
          title={t('settings.usage_byAgent')}
          rows={usage.byAgent.slice(0, 6).map((a) => ({ id: a.id, name: a.name, cost: a.costUsd, unpriced: a.unpricedCalls, avatar: <Icon name="bot" size={18} className="text-muted" /> }))}
        />
      )}
    </div>
  )
}
