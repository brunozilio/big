import { api } from '@/api'
import { ago, mins, now } from '@/api/clock'
import { errorText, setItems } from '@/api/store'
import type { ReactNode } from 'react'
import { useT, type T } from '@/i18n'
import { spoken } from '@/i18n/server'
import type { ChangeRef, ExternalRef, Item, ItemSource, ItemStep, StepState } from '@/api/contract'
import { Button } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Badge, Spinner } from '@/ui/display'
import { ProviderLogo } from '@/ui/domain'
import { Icon, NamedIcon } from '@/ui/icon'
import { motion, springPop } from '@/ui/motion'
import { Tooltip } from '@/ui/overlay'
import { DiffStat } from '@/ui/pill'
import { toast } from '@/ui/toast'
import { useMinutesSince } from './ticker'

export type NodeState = StepState | 'blocked'

export function report(e: unknown) {
  toast.error(errorText(e))
}

export function archiveToggle(it: Item): Promise<boolean> {
  const before = it.archivedAt
  const after = before ? null : new Date(now()).toISOString()
  const put = (from: string | null, to: string | null) => setItems((all) => all.map((x) => (x.id === it.id && x.archivedAt === from ? { ...x, archivedAt: to } : x)))
  put(before, after)
  return (before ? api().unarchive(it.id) : api().archive(it.id)).then(
    () => true,
    (e: unknown) => {
      put(after, before)
      report(e)
      return false
    },
  )
}

export const stop = { onClick: (e: { stopPropagation: () => void }) => e.stopPropagation(), onPointerDown: (e: { stopPropagation: () => void }) => e.stopPropagation() }

export function stateLabel(t: T, state: NodeState): string {
  return t(`items.state_${state}`)
}

export function nodeState(it: Item, s: ItemStep): NodeState {
  return it.impediment && it.current === s.id && s.state === 'waiting_human' ? 'blocked' : s.state
}

export function LiveDot({ className }: { className?: string }) {
  return (
    <span className={cn('relative flex size-2', className)}>
      <span className="relative inline-flex size-2 rounded-full bg-teal" />
    </span>
  )
}

export function Node({ s, state, size = 22 }: { s: ItemStep; state: NodeState; size?: number }) {
  const glyph = state === 'passed' ? 'check' : state === 'failed' ? 'close' : state === 'blocked' ? 'lock' : null
  const inner = Math.round(size * 0.58)
  return (
    <motion.span
      key={state}
      initial={{ scale: 0.6, opacity: 0 }}
      animate={{ scale: 1, opacity: 1 }}
      transition={springPop}
      style={{ width: size, height: size }}
      className={cn(
        'relative flex shrink-0 items-center justify-center rounded-full',
        state === 'passed' && 'bg-success text-white',
        state === 'failed' && 'bg-danger text-white',
        state === 'running' && 'border-run bg-surface text-info shadow-card',
        state === 'waiting_human' && 'bg-warning-soft text-warning-fg shadow-[0_0_0_1.5px_rgb(var(--warning)/0.5)]',
        state === 'blocked' && 'bg-inset text-muted shadow-[0_0_0_1px_rgb(var(--border-strong))]',
        state === 'pending' && 'bg-inset text-muted shadow-[inset_0_0_0_1px_rgb(var(--border-strong))]',
        state === 'skipped' && 'border border-dashed border-line-strong text-faint',
      )}
    >
      {glyph ? <Icon name={glyph} size={inner} strokeWidth={glyph === 'lock' ? 1.7 : 2.2} /> : <NamedIcon name={s.icon} size={inner} strokeWidth={1.7} />}
    </motion.span>
  )
}

function Link({ from, to }: { from: NodeState; to: NodeState }) {
  if (from === 'passed' && (to === 'running' || to === 'waiting_human')) {
    return (
      <svg width="8" height="2" className="block shrink-0 overflow-visible" aria-hidden>
        <line x1="0" y1="1" x2="8" y2="1" stroke="rgb(var(--info))" strokeWidth="1.6" strokeLinecap="round" className="dash-flow" />
      </svg>
    )
  }
  return <span className={cn('block h-px w-2 shrink-0', from === 'passed' ? 'bg-success/60' : 'border-t border-dashed border-line-strong')} />
}

export function trailPosition(it: Item): { top: ItemStep[]; at: number } {
  const top = it.steps.filter((s) => !s.parent)
  let found = it.steps.find((s) => s.id === it.current)
  while (found?.parent) {
    const parent = found.parent
    found = it.steps.find((s) => s.id === parent)
  }
  const current = found ? top.findIndex((s) => s.id === found.id) : -1
  if (current >= 0) return { top, at: current }
  const open = top.findIndex((s) => s.state !== 'passed' && s.state !== 'skipped')
  return { top, at: open >= 0 ? open : top.length - 1 }
}

function TrailStep({ s, state, current, onStep }: { s: ItemStep; state: NodeState; current: boolean; onStep: (s: string) => void }) {
  const t = useT()
  const locked = state === 'pending'
  return (
    <Tooltip content={`${s.label} · ${stateLabel(t, state)}`}>
      <button
        aria-label={`${s.label}: ${stateLabel(t, state)}`}
        aria-current={current ? 'step' : undefined}
        aria-disabled={locked}
        onClick={() => !locked && onStep(s.id)}
        className={cn('flex items-center rounded-full', current && 'gap-1.5 bg-inset pr-2.5', locked ? 'cursor-default' : 'transition-transform duration-150 active:scale-95', !locked && !current && 'hover:scale-110')}
      >
        <Node s={s} state={state} size={20} />
        {current && <span className="max-w-[128px] truncate text-sm leading-none text-fg">{s.label}</span>}
      </button>
    </Tooltip>
  )
}

export function Pipeline({ it, onStep }: { it: Item; onStep: (s: string) => void }) {
  const t = useT()
  const { top, at } = trailPosition(it)
  const current = top[at]
  if (!current) return null
  const state = nodeState(it, current)
  return (
    <>
      <span className="hidden items-center lg:flex">
        {top.map((s, i) => {
          const st = nodeState(it, s)
          const next = top[i + 1]
          return (
            <span key={s.id} className="flex items-center">
              <TrailStep s={s} state={st} current={i === at} onStep={onStep} />
              {next && <Link from={st} to={nodeState(it, next)} />}
            </span>
          )
        })}
      </span>
      <button
        aria-label={t('items.stepOf', { n: at + 1, total: top.length, label: current.label })}
        title={`${current.label} · ${stateLabel(t, state)}`}
        onClick={() => state !== 'pending' && onStep(current.id)}
        className="flex min-w-0 items-center gap-1.5 rounded-full bg-inset py-0.5 pl-0.5 pr-2.5 text-sm leading-none lg:hidden"
      >
        <Node s={current} state={state} size={20} />
        <span className="max-w-[112px] truncate text-fg">{current.label}</span>
        <span className="shrink-0 tabular-nums text-muted">· {at + 1}/{top.length}</span>
      </button>
    </>
  )
}

export function OriginCell({ children }: { children?: ReactNode }) {
  return <span className="flex w-[84px] shrink-0 items-center overflow-hidden sm:w-[116px]">{children}</span>
}

export function IssueKey({ issue, className }: { issue: ExternalRef; className?: string }) {
  const t = useT()
  const inner = (
    <>
      <ProviderLogo logo={issue.logo} name={issue.name} size={15} />
      <span className="truncate leading-none" title={issue.key}>{issue.key}</span>
    </>
  )
  if (!issue.url) return <span className={cn('flex min-w-[88px] shrink-0 items-center gap-2 text-base text-fg2', className)}>{inner}</span>
  return (
    <Tooltip content={t('items.openIn', { name: issue.name })}>
      <a href={issue.url} target="_blank" rel="noreferrer" {...stop} className={cn('flex min-w-[88px] shrink-0 items-center gap-2 text-base text-fg2 transition-colors hover:text-fg', className)}>
        {inner}
      </a>
    </Tooltip>
  )
}

export function ChangeChip({ change }: { change: ChangeRef | null }) {
  const t = useT()
  if (!change) return null
  const body = (
    <>
      <ProviderLogo logo={change.logo} name={change.name} size={13} className="text-fg" />
      <span className="tabular-nums">{change.key}</span>
      {change.merged ? <Icon name="hGitMerge" size={13} className="text-accent" /> : change.checks === 'passed' ? <Icon name="check" size={13} strokeWidth={2.2} className="text-success" /> : change.checks === 'failed' ? <Icon name="close" size={13} strokeWidth={2.2} className="text-danger" /> : change.checks === 'running' ? <Spinner size={11} className="text-warning" /> : null}
    </>
  )
  const cls = 'inline-flex h-6 shrink-0 items-center gap-1.5 rounded-control-sm bg-inset px-2 text-sm leading-none text-fg2 transition-colors hover:bg-active hover:text-fg'
  if (!change.url) return <span className={cls}>{body}</span>
  return (
    <Tooltip content={t('items.openIn', { name: change.name })}>
      <a href={change.url} target="_blank" rel="noreferrer" {...stop} className={cls}>{body}</a>
    </Tooltip>
  )
}

function StepText({ it }: { it: Item }) {
  const t = useT()
  const waited = useMinutesSince(it.activity?.since)
  if (!it.activity) return null
  if (it.activity.tone === 'danger') return <span className="hidden w-[136px] justify-end text-sm text-danger-fg xl:flex">{spoken(t, it.activity.said, it.activity.text)}</span>
  return (
    <span className="hidden w-[136px] items-center justify-end gap-1.5 text-sm xl:flex">
      <span className="text-shimmer">{spoken(t, it.activity.said, it.activity.text)}</span>
      <span className="tabular-nums text-faint">{mins(waited)}</span>
    </span>
  )
}

export function RowRight({ it, onStep, onConnect }: { it: Item; onStep: (s: string) => void; onConnect: (provider: string) => void }) {
  const t = useT()
  useMinutesSince(it.doneAt)
  if (it.doneAt) {
    return (
      <>
        <ChangeChip change={it.change} />
        <span className="w-[72px] text-right text-sm tabular-nums text-muted">{ago(it.doneAt)}</span>
      </>
    )
  }
  if (it.impediment) {
    const c = it.impediment.connect
    return (
      <>
        <ChangeChip change={it.change} />
        <Badge className="min-w-0" icon={<Icon name="lock" size={12} className="shrink-0" />}><span className="truncate" title={spoken(t, it.impediment.said, it.impediment.reason)}>{spoken(t, it.impediment.said, it.impediment.reason)}</span></Badge>
        {c && <Button size="sm" variant="secondary" icon={<ProviderLogo logo={c.logo} name={c.name} size={15} />} onClick={() => onConnect(c.provider)}>{t('items.connect')}</Button>}
      </>
    )
  }
  const a = it.attention
  if (a) {
    return (
      <>
        <ChangeChip change={it.change} />
        {a.badge && <Badge tone={a.badge.tone === 'accent' ? 'neutral' : a.badge.tone} icon={a.badge.icon ? <NamedIcon name={a.badge.icon} size={12} /> : undefined}>{spoken(t, a.badge.said, a.badge.text)}</Badge>}
        {a.diff && <span className="hidden md:inline-flex"><DiffStat files={a.diff.files} additions={a.diff.additions} deletions={a.diff.deletions} href={a.diff.url ?? undefined} /></span>}
        {a.action && <Button size="sm" variant="primary" icon={a.action.icon ? <NamedIcon name={a.action.icon} size={15} /> : undefined} onClick={() => a.action && onStep(a.action.step)}>{spoken(t, a.action.said, a.action.label)}</Button>}
      </>
    )
  }
  return (
    <>
      <ChangeChip change={it.change} />
      <Pipeline it={it} onStep={onStep} />
      <StepText it={it} />
    </>
  )
}

const SOURCE_KINDS = ['issue', 'pull_request', 'branch'] as const

export function sourceLabel(t: T, source: ItemSource): string {
  const kind = SOURCE_KINDS.find((k) => k === source.kind)
  return `${kind ? t(`items.source_${kind}`) : source.kind} ${source.key}`
}

export function SourceChip({ source, className }: { source: ItemSource; className?: string }) {
  const t = useT()
  const glyph = source.kind === 'branch' ? 'branch' : source.kind === 'pull_request' ? 'hGitPullRequest' : 'sTicket'
  const body = (
    <>
      {source.logo && source.name ? <ProviderLogo logo={source.logo} name={source.name} size={13} className="text-fg" /> : <Icon name={glyph} size={13} />}
      <span className="truncate tabular-nums" title={source.key}>{source.key}</span>
    </>
  )
  const cls = cn('inline-flex h-6 min-w-0 max-w-[220px] shrink-0 items-center gap-1.5 rounded-control-sm bg-inset px-2 text-sm leading-none text-fg2', className)
  if (!source.url) return <span aria-label={sourceLabel(t, source)} className={cls}>{body}</span>
  return (
    <a href={source.url} target="_blank" rel="noreferrer" aria-label={sourceLabel(t, source)} {...stop} className={cn(cls, 'transition-colors hover:bg-active hover:text-fg')}>{body}</a>
  )
}

