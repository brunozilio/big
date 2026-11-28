import { motion } from 'framer-motion'
import type { ReactNode } from 'react'
import type { TooltipProps } from 'recharts'
import type { UsageDay } from '@/api/contract'
import { currentLanguage, currentLocale, translate, useT } from '@/i18n'
import { money } from '@/ui/cn'
import { Card, EmptyState } from '@/ui/display'
import { Icon, type IconName } from '@/ui/icon'
import { SectionTitle, sectionTitleClass } from '@/ui/layout'
import { stagger, tween } from '@/ui/motion'

export const palette = ['rgb(var(--cat-2))', 'rgb(var(--cat-1))', 'rgb(var(--cat-6))', 'rgb(var(--cat-4))', 'rgb(var(--cat-5))', 'rgb(var(--cat-3))']

export const axis = { stroke: 'rgb(var(--text-muted))', fontSize: 12, tickLine: false, axisLine: false }

export const usd = (v: number) => money(v, 'USD')

export function withUnpriced(cost: string, unpriced?: number): string {
  return unpriced ? translate(currentLanguage(), 'settings.usage_withUnpriced', { cost, n: unpriced }) : cost
}

export const axisUsd = (v: number) => (Number.isInteger(v) || v >= 10 ? `$${Math.round(v)}` : `$${v.toFixed(v < 1 ? 2 : 1)}`)

export function compact(n: number): string {
  return new Intl.NumberFormat(currentLocale(), { notation: 'compact', maximumFractionDigits: 1 }).format(n)
}

export interface Series {
  key: string
  name: string
  color: string
}

export function seriesOf(rows: { id: string; name: string }[], daily: UsageDay[], pick: (d: UsageDay) => Record<string, number>, max = 5) {
  const top = rows.slice(0, rows.length > max + 1 ? max : max + 1)
  const rest = rows.length > top.length
  const series: Series[] = top.map((r, i) => ({ key: `s${i}`, name: r.name, color: palette[i % palette.length] }))
  if (rest) series.push({ key: 'rest', name: translate(currentLanguage(), 'settings.usage_others'), color: 'rgb(var(--text-faint))' })
  const fmt = (d: string) => new Date(`${d}T12:00:00`).toLocaleDateString(currentLocale(), { day: '2-digit', month: 'short' })
  const data = daily.map((d) => {
    const values = pick(d)
    const point: Record<string, number | string> = { date: fmt(d.date) }
    top.forEach((r, i) => {
      point[`s${i}`] = values[r.id] ?? 0
    })
    if (rest) point.rest = Object.entries(values).filter(([id]) => !top.some((r) => r.id === id)).reduce((n, [, v]) => n + v, 0)
    return point
  })
  return { series, data }
}

export function ChartTooltip({ active, payload, label }: TooltipProps<number, string>) {
  const t = useT()
  if (!active || !payload?.length) return null
  const total = payload.reduce((n, p) => n + Number(p.value ?? 0), 0)
  return (
    <div className="min-w-48 rounded-card bg-surface p-3 text-sm shadow-pop">
      <p className="mb-2 text-muted">{label}</p>
      {payload.map((p) => (
        <div key={String(p.dataKey)} className="flex items-center gap-2 py-0.5">
          <span className="size-2 rounded-full" style={{ background: p.color }} />
          <span className="flex-1 text-fg2">{p.name}</span>
          <span className="tabular-nums text-fg">{usd(Number(p.value ?? 0))}</span>
        </div>
      ))}
      <div className="mt-2 flex justify-between border-t pt-2 font-medium">
        <span>{t('settings.usage_total')}</span>
        <span className="tabular-nums">{usd(total)}</span>
      </div>
    </div>
  )
}

function Rise({ index, children, className }: { index: number; children: ReactNode; className?: string }) {
  return (
    <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ ...tween, delay: stagger(index) }} className={className}>
      {children}
    </motion.div>
  )
}

export function StatCard({ icon, label, value, index, hint }: { icon: IconName; label: string; value: ReactNode; index: number; hint?: ReactNode }) {
  return (
    <Rise index={index} className="rounded-card bg-inset/70 px-5 py-4">
      <p className="flex items-center gap-2 text-base text-muted">
        <Icon name={icon} size={16} />
        {label}
      </p>
      <p className="mt-2.5 text-2xl font-medium tabular-nums tracking-tightish text-fg">{value}</p>
      {hint && <p className="mt-0.5 text-sm text-muted">{hint}</p>}
    </Rise>
  )
}

export function ChartCard({ title, legend, children, index }: { title: string; legend: Series[]; children: ReactNode; index: number }) {
  return (
    <Rise index={index}>
      <Card className="h-full p-5">
        <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
          <h2 className={sectionTitleClass}>{title}</h2>
          <div className="flex flex-wrap gap-3">
            {legend.map((l) => (
              <span key={l.key} className="flex items-center gap-1.5 text-xs text-muted">
                <span className="size-2 rounded-full" style={{ background: l.color }} />
                {l.name}
              </span>
            ))}
          </div>
        </div>
        <div className="h-64">{children}</div>
      </Card>
    </Rise>
  )
}

export interface RankRow {
  id: string
  name: string
  detail?: string
  cost: number
  unpriced?: number
  avatar: ReactNode
}

export function RankCard({ title, rows, index }: { title: string; rows: RankRow[]; index: number }) {
  const t = useT()
  const max = Math.max(...rows.map((r) => r.cost), 0.01)
  return (
    <Rise index={index}>
      <Card className="h-full p-5">
        <SectionTitle className="mb-4">{title}</SectionTitle>
        <ul className="space-y-3.5">
          {rows.map((r) => (
            <li key={r.id} className="space-y-1.5">
              <div className="flex items-center gap-2.5 text-base">
                {r.avatar}
                <span className="min-w-0 flex-1 truncate text-fg2" title={r.detail ? `${r.name} · ${r.detail}` : r.name}>
                  {r.name}
                  {r.detail && <span className="ml-1.5 text-sm text-faint">{r.detail}</span>}
                </span>
                <span className="tabular-nums text-fg">{withUnpriced(usd(r.cost), r.unpriced)}</span>
              </div>
              <div className="h-1 overflow-hidden rounded-full bg-inset">
                <motion.div className="h-full origin-left rounded-full bg-fg/70" initial={false} animate={{ scaleX: r.cost / max }} transition={tween} />
              </div>
            </li>
          ))}
          {rows.length === 0 && (
            <li>
              <EmptyState size="inline" align="start" title={t('settings.usage_noneInPeriod')} />
            </li>
          )}
        </ul>
      </Card>
    </Rise>
  )
}
