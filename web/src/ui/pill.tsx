import { motion } from 'framer-motion'
import { springSnap, tween } from './motion'
import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from 'react'
import { cn, plural } from './cn'
import { Avatar } from './display'
import { Icon, type IconName } from './icon'
import { Tooltip } from './overlay'

const shell = 'inline-flex items-center rounded-full bg-surface shadow-[0_0_0_1px_rgb(var(--border)),0_1px_2px_rgb(0_0_0/0.04)]'

export function PillBar({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn(shell, 'gap-0.5 p-1', className)}>{children}</div>
}

export function PillDivider() {
  return <span className="mx-1 h-5 w-px bg-line" />
}

export const PillIconButton = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement> & { icon: IconName; label: string; active?: boolean; layoutGroup?: string }>(function PillIconButton({ icon, label, active, layoutGroup, className, ...props }, ref) {
  return (
    <Tooltip content={label}>
      <button ref={ref} aria-label={label} aria-pressed={active} className={cn('relative flex size-9 items-center justify-center rounded-full text-muted transition-colors hover:text-fg', active && 'text-fg', className)} {...props}>
        {active && (layoutGroup ? <motion.span layoutId={layoutGroup} transition={springSnap} className="absolute inset-0 rounded-full bg-active" /> : <span className="absolute inset-0 rounded-full bg-active" />)}
        <span className="relative flex">
          <Icon name={icon} size={18} />
        </span>
      </button>
    </Tooltip>
  )
})

export function PillTextButton({ icon, children, onClick, label }: { icon: IconName; children: ReactNode; onClick?: () => void; label?: string }) {
  return (
    <motion.button whileTap={{ scale: 0.97 }} onClick={onClick} aria-label={label} className="flex h-9 items-center gap-2 rounded-full px-2.5 text-base text-fg transition-colors hover:bg-hover sm:pl-3 sm:pr-3.5">
      <Icon name={icon} size={17} className="text-fg2" />
      <span className="hidden sm:inline">{children}</span>
    </motion.button>
  )
}

export function PillSlider({ value, min, max, step = 1, onChange, label }: { value: number; min: number; max: number; step?: number; onChange: (v: number) => void; label: string }) {
  const pct = ((value - min) / (max - min)) * 100
  return (
    <span className="flex items-center gap-1">
      <PillIconButton icon="plus" label="Aumentar" onClick={() => onChange(Math.min(max, value + step))} className="size-8" />
      <span className="relative flex h-8 w-[96px] items-center">
        <span className="absolute inset-x-0 h-[5px] rounded-full bg-inset" />
        <motion.span className="absolute inset-x-0 h-[5px] origin-left rounded-full bg-fg/25" initial={false} animate={{ scaleX: pct / 100 }} transition={tween} />
        <motion.span className="pointer-events-none absolute size-[14px] -translate-x-1/2 rounded-full bg-white shadow-[0_1px_3px_rgb(0_0_0/0.3),0_0_0_0.5px_rgb(0_0_0/0.1)]" initial={false} animate={{ left: `${pct}%` }} transition={tween} />
        <input type="range" aria-label={label} min={min} max={max} step={step} value={value} onChange={(e) => onChange(Number(e.target.value))} className="absolute inset-0 cursor-pointer opacity-0" />
      </span>
      <PillIconButton icon="minus" label="Diminuir" onClick={() => onChange(Math.max(min, value - step))} className="size-8" />
    </span>
  )
}

export function DiffStat({ files, additions, deletions, href, className }: { files: number; additions: number; deletions: number; href?: string; className?: string }) {
  const body = (
    <>
      <Icon name="pr" size={15} className="text-accent" strokeWidth={2} />
      <span className="text-fg">
        {files} {plural(files, 'arquivo', 'arquivos')}
      </span>
      <span className="font-mono text-xs tabular-nums text-success-fg">+{additions}</span>
      <span className="font-mono text-xs tabular-nums text-danger-fg">−{deletions}</span>
    </>
  )
  const cls = cn(shell, 'h-8 gap-2 px-3 text-sm', href && 'transition-shadow hover:shadow-[0_0_0_1px_rgb(var(--border-strong)),0_1px_2px_rgb(0_0_0/0.06)]', className)
  return href ? (
    <a href={href} target="_blank" rel="noreferrer" onClick={(e) => e.stopPropagation()} className={cls}>
      {body}
    </a>
  ) : (
    <span className={cls}>{body}</span>
  )
}

export function ApprovalPill({ people, state, className }: { people: string[]; state: 'approved' | 'changes' | 'pending'; className?: string }) {
  const meta = { approved: { label: 'Aprovado', icon: 'check' as IconName, cls: 'text-accent-fg' }, changes: { label: 'Alterações pedidas', icon: 'alert' as IconName, cls: 'text-warning-fg' }, pending: { label: 'Aguardando revisão', icon: 'clock' as IconName, cls: 'text-muted' } }[state]
  return (
    <span className={cn(shell, 'h-8 gap-2 pl-1 pr-3 text-sm', people.length === 0 && 'pl-3', className)}>
      {people.length > 0 && (
        <span className="flex -space-x-1.5">
          {people.slice(0, 3).map((p) => (
            <span key={p} className="rounded-full ring-2 ring-surface">
              <Avatar name={p} size={22} />
            </span>
          ))}
        </span>
      )}
      <span className="text-fg">{meta.label}</span>
      <Icon name={meta.icon} size={15} strokeWidth={2.2} className={meta.cls} />
    </span>
  )
}

export type RiskLevel = 'low' | 'medium' | 'high'

const riskMeta: Record<RiskLevel, { label: string; cls: string }> = {
  low: { label: 'Baixo risco', cls: 'text-success-fg' },
  medium: { label: 'Risco médio', cls: 'text-warning-fg' },
  high: { label: 'Alto risco', cls: 'text-danger-fg' },
}

export function riskOf(tool: string): RiskLevel {
  if (/delete|drop|apply|merge|push|deploy|destroy|rm\b|force/i.test(tool)) return 'high'
  if (/create|write|update|transition|post|send|comment|edit/i.test(tool)) return 'medium'
  return 'low'
}

export function RiskPill({ level, tag, className }: { level: RiskLevel; tag?: string; className?: string }) {
  const m = riskMeta[level]
  return (
    <span className={cn(shell, 'h-8 gap-2 pl-2.5 pr-1 text-sm', !tag && 'pr-3', className)}>
      <RiskGlyph level={level} className={m.cls} />
      <span className="text-fg">{m.label}</span>
      {tag && <span className="rounded-full px-2 py-0.5 font-mono text-2xs text-muted shadow-[inset_0_0_0_1px_rgb(var(--border))]">{tag}</span>}
    </span>
  )
}

function RiskGlyph({ level, className }: { level: RiskLevel; className?: string }) {
  const name: IconName = level === 'high' ? 'riskHigh' : level === 'medium' ? 'riskMedium' : 'riskLow'
  return <Icon name={name} size={16} className={className} />
}

export type PrPhase = 'merged' | 'failing' | 'approved' | 'review' | 'draft' | 'closed'

export function prPhase(pr: { state: 'draft' | 'open' | 'merged' | 'closed'; checks: 'passing' | 'failing' | 'pending'; reviews: number }): PrPhase {
  if (pr.state === 'merged') return 'merged'
  if (pr.state === 'closed') return 'closed'
  if (pr.checks === 'failing') return 'failing'
  if (pr.state === 'draft') return 'draft'
  if (pr.reviews > 0 && pr.checks === 'passing') return 'approved'
  return 'review'
}

const phaseMeta: Record<PrPhase, { icon: IconName; cls: string; label: string }> = {
  merged: { icon: 'merge', cls: 'text-accent-fg', label: 'Mergeado' },
  failing: { icon: 'pr', cls: 'text-danger-fg', label: 'CI falhou' },
  approved: { icon: 'pr', cls: 'text-success-fg', label: 'Aprovado, pronto para merge' },
  review: { icon: 'clock', cls: 'text-warning-fg', label: 'Aguardando revisão' },
  draft: { icon: 'pr', cls: 'text-muted', label: 'Rascunho' },
  closed: { icon: 'pr', cls: 'text-muted', label: 'Fechado sem merge' },
}

export function PrStatusPill({ pr, additions, deletions, className }: { pr: { number: number; repo: string; state: 'draft' | 'open' | 'merged' | 'closed'; checks: 'passing' | 'failing' | 'pending'; reviews: number }; additions: number; deletions: number; className?: string }) {
  const m = phaseMeta[prPhase(pr)]
  return (
    <Tooltip content={`${m.label} · ${pr.repo}`}>
      <a
        href={`https://github.com/${pr.repo}/pull/${pr.number}`}
        target="_blank"
        rel="noreferrer"
        onClick={(e) => e.stopPropagation()}
        className={cn(shell, 'h-8 gap-2 px-3 text-sm transition-shadow hover:shadow-[0_0_0_1px_rgb(var(--border-strong)),0_1px_2px_rgb(0_0_0/0.06)]', className)}
      >
        <Icon name={m.icon} size={15} strokeWidth={2} className={m.cls} />
        <span className="font-medium tabular-nums text-fg">#{pr.number}</span>
        <span className="font-mono text-xs tabular-nums text-success-fg">+{additions}</span>
        <span className="font-mono text-xs tabular-nums text-danger-fg">−{deletions}</span>
      </a>
    </Tooltip>
  )
}
