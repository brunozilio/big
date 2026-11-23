import { useEffect, useState } from 'react'
import { ago, mins, minutesSince } from '@/api/clock'
import type { ChangeRef, ExternalRef, Item, ItemStep, StepState } from '@/api/contract'
import { Button } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Badge, Spinner } from '@/ui/display'
import { ProviderLogo } from '@/ui/domain'
import { Icon, NamedIcon } from '@/ui/icon'
import { motion, springPop } from '@/ui/motion'
import { Tooltip } from '@/ui/overlay'
import { DiffStat } from '@/ui/pill'

export type NodeState = StepState | 'blocked'

export const stop = { onClick: (e: { stopPropagation: () => void }) => e.stopPropagation(), onPointerDown: (e: { stopPropagation: () => void }) => e.stopPropagation() }

export const stateLabel: Record<NodeState, string> = { pending: 'Pendente', running: 'Rodando', waiting_human: 'Esperando você', passed: 'Concluído', failed: 'Falhou', skipped: 'Pulado', blocked: 'Sem conexão' }

export function nodeState(it: Item, s: ItemStep): NodeState {
  return it.impediment && it.current === s.id && s.state === 'waiting_human' ? 'blocked' : s.state
}

export function useMinuteTick(on = true) {
  const [, setN] = useState(0)
  useEffect(() => {
    if (!on) return
    const t = setInterval(() => setN((n) => n + 1), 1000)
    return () => clearInterval(t)
  }, [on])
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
        state === 'pending' && 'bg-inset text-faint',
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
      <svg width="12" height="2" className="block shrink-0 overflow-visible" aria-hidden>
        <line x1="0" y1="1" x2="12" y2="1" stroke="rgb(var(--info))" strokeWidth="1.6" strokeLinecap="round" className="dash-flow" />
      </svg>
    )
  }
  return <span className={cn('block h-px w-3 shrink-0', from === 'passed' ? 'bg-success/50' : 'border-t border-dashed border-line-strong')} />
}

export function Pipeline({ it, onStep }: { it: Item; onStep: (s: string) => void }) {
  return (
    <span className="hidden items-center md:flex">
      {it.steps.map((s, i) => {
        const st = nodeState(it, s)
        const next = it.steps[i + 1]
        return (
          <span key={s.id} className="flex items-center">
            <Tooltip content={`${s.label} · ${stateLabel[st]}`}>
              <button aria-label={`${s.label}: ${stateLabel[st]}`} onClick={() => onStep(s.id)} className="flex rounded-full transition-transform duration-150 hover:scale-110 active:scale-95">
                <Node s={s} state={st} />
              </button>
            </Tooltip>
            {next && <Link from={st} to={nodeState(it, next)} />}
          </span>
        )
      })}
    </span>
  )
}

export function IssueKey({ issue }: { issue: ExternalRef }) {
  const inner = (
    <>
      <ProviderLogo logo={issue.logo} name={issue.name} size={15} />
      <span className="leading-none">{issue.key}</span>
    </>
  )
  if (!issue.url) return <span className="flex w-[88px] shrink-0 items-center gap-2 text-base text-fg2">{inner}</span>
  return (
    <Tooltip content={`Abrir no ${issue.name}`}>
      <a href={issue.url} target="_blank" rel="noreferrer" {...stop} className="flex w-[88px] shrink-0 items-center gap-2 text-base text-fg2 transition-colors hover:text-fg">
        {inner}
      </a>
    </Tooltip>
  )
}

export function ChangeChip({ change }: { change: ChangeRef | null }) {
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
    <Tooltip content={`Abrir no ${change.name}`}>
      <a href={change.url} target="_blank" rel="noreferrer" {...stop} className={cls}>{body}</a>
    </Tooltip>
  )
}

function StepText({ it }: { it: Item }) {
  useMinuteTick(!!it.activity)
  if (!it.activity) return null
  if (it.activity.tone === 'danger') return <span className="hidden w-[136px] justify-end text-sm text-danger-fg lg:flex">{it.activity.text}</span>
  return (
    <span className="hidden w-[136px] items-center justify-end gap-1.5 text-sm lg:flex">
      <span className="text-shimmer">{it.activity.text}</span>
      <span className="tabular-nums text-faint">{mins(minutesSince(it.activity.since))}</span>
    </span>
  )
}

export function RowRight({ it, onStep, onConnect }: { it: Item; onStep: (s: string) => void; onConnect: (provider: string) => void }) {
  useMinuteTick(!!it.doneAt)
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
        <Badge className="min-w-0" icon={<Icon name="lock" size={12} className="shrink-0" />}><span className="truncate" title={it.impediment.reason}>{it.impediment.reason}</span></Badge>
        {c && <Button size="sm" variant="secondary" icon={<ProviderLogo logo={c.logo} name={c.name} size={15} />} onClick={() => onConnect(c.provider)}>Conectar</Button>}
      </>
    )
  }
  const a = it.attention
  if (a) {
    return (
      <>
        <ChangeChip change={it.change} />
        {a.badge && <Badge tone={a.badge.tone === 'accent' ? 'neutral' : a.badge.tone} icon={a.badge.icon ? <NamedIcon name={a.badge.icon} size={12} /> : undefined}>{a.badge.text}</Badge>}
        {a.diff && <span className="hidden md:inline-flex"><DiffStat files={a.diff.files} additions={a.diff.additions} deletions={a.diff.deletions} href={a.diff.url ?? undefined} /></span>}
        {a.action && <Button size="sm" variant="primary" icon={a.action.icon ? <NamedIcon name={a.action.icon} size={15} /> : undefined} onClick={() => a.action && onStep(a.action.step)}>{a.action.label}</Button>}
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
