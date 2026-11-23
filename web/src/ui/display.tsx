import { motion } from 'framer-motion'
import { useState, type HTMLAttributes, type ReactNode } from 'react'
import { cn } from './cn'
import { AvatarImg, Icon } from "./icon"
import { AnimatePresence, IconSwap, springSnap, stagger, tween, tweenClose } from './motion'

export function Spinner({ className, size = 16 }: { className?: string; size?: number }) {
  return (
    <svg viewBox="0 0 16 16" width={size} height={size} className={cn('shrink-0 animate-spin text-muted', className)} aria-label="carregando">
      <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" strokeOpacity="0.2" strokeWidth="1.8" />
      <path d="M14 8a6 6 0 0 0-6-6" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  )
}

export function DotsSpinner({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 16 16" width={16} height={16} className={cn('shrink-0 animate-spin text-muted [animation-duration:1.4s]', className)}>
      {Array.from({ length: 8 }, (_, i) => (
        <circle key={i} cx={8 + 5.5 * Math.cos((i * Math.PI) / 4)} cy={8 + 5.5 * Math.sin((i * Math.PI) / 4)} r="1.1" fill="currentColor" opacity={0.25 + i * 0.1} />
      ))}
    </svg>
  )
}

export function Kbd({ children, className }: { children: ReactNode; className?: string }) {
  return <kbd className={cn('inline-flex h-[20px] min-w-[20px] items-center justify-center rounded-[5px] bg-inset px-1 font-sans text-2xs font-medium tabular-nums text-muted shadow-[inset_0_-1px_0_rgb(0_0_0/0.06)]', className)}>{children}</kbd>
}

type Tone = 'neutral' | 'info' | 'success' | 'warning' | 'danger' | 'accent' | 'new' | 'dashed' | 'outline'

const tones: Record<Tone, string> = {
  neutral: 'bg-inset text-muted',
  info: 'bg-info-soft text-info-fg',
  success: 'bg-success-soft text-success-fg',
  warning: 'bg-warning-soft text-warning-fg',
  danger: 'bg-danger-soft text-danger-fg',
  accent: 'bg-accent-soft text-accent-fg',
  new: 'bg-info-soft text-info-fg',
  dashed: 'border border-dashed border-accent/50 text-accent-fg bg-accent-soft/40',
  outline: 'border bg-surface text-fg2',
}

export function Badge({ tone = 'neutral', children, className, icon }: { tone?: Tone; children: ReactNode; className?: string; icon?: ReactNode }) {
  return (
    <span className={cn('inline-flex h-[22px] items-center gap-1 whitespace-nowrap rounded-sm px-1.5 text-xs font-medium tabular-nums', tones[tone], className)}>
      {icon}
      {children}
    </span>
  )
}

export function Chip({ children, className, onRemove }: { children: ReactNode; className?: string; onRemove?: () => void }) {
  return (
    <span className={cn('inline-flex h-[24px] items-center gap-1 whitespace-nowrap rounded-control-sm bg-inset px-2 text-sm text-fg2', className)}>
      {children}
      {onRemove && (
        <button aria-label="remover" className="-mr-1 rounded p-0.5 text-faint hover:text-fg" onClick={onRemove}>
          <Icon name="close" size={12} />
        </button>
      )}
    </span>
  )
}

export function Dot({ className }: { className?: string }) {
  return <span className={cn('inline-block size-[7px] shrink-0 rounded-full', className)} />
}

export function Avatar({ name, photo, size = 24, agent, className }: { name: string; photo?: string | null; size?: number; agent?: boolean; className?: string }) {
  return (
    <span title={name} className={cn('relative inline-flex shrink-0', className)} style={{ width: size, height: size }}>
      {photo ? <img src={photo} alt="" className="size-full rounded-full object-cover" /> : <AvatarImg seed={name} size={size} />}
      {agent && (
        <span className="absolute -bottom-0.5 -right-0.5 flex items-center justify-center rounded-full bg-primary text-primary-fg ring-2 ring-bg" style={{ width: Math.max(12, size * 0.5), height: Math.max(12, size * 0.5) }}>
          <Icon name="asterisk" size={Math.max(9, Math.round(size * 0.36))} strokeWidth={2.4} />
        </span>
      )}
    </span>
  )
}

export function AvatarStack({ people, max = 4, size = 22 }: { people: { name: string; id?: string; photo?: string | null }[]; max?: number; size?: number }) {
  const shown = people.slice(0, max)
  const rest = people.length - shown.length
  return (
    <span className="flex items-center -space-x-1.5">
      {shown.map((p) => (
        <span key={p.id ?? p.name}>
          <Avatar name={p.name} photo={p.photo} size={size} />
        </span>
      ))}
      {rest > 0 && (
        <span className="inline-flex items-center justify-center rounded-full bg-inset text-2xs font-medium tabular-nums text-muted ring-2 ring-bg" style={{ width: size, height: size }}>
          +{rest}
        </span>
      )}
    </span>
  )
}

export function Card({ className, variant = 'default', ...props }: HTMLAttributes<HTMLDivElement> & { variant?: 'default' | 'interactive' | 'inset' | 'flat' }) {
  return (
    <div
      className={cn(
        'rounded-card',
        variant === 'default' && 'bg-surface shadow-card',
        variant === 'interactive' && 'cursor-pointer bg-surface shadow-card transition-shadow duration-150 ease-out hover:shadow-pop',
        variant === 'inset' && 'bg-inset',
        variant === 'flat' && 'border bg-surface',
        className,
      )}
      {...props}
    />
  )
}

export function Skeleton({ className }: { className?: string }) {
  return <div className={cn('skeleton h-3', className)} />
}

export function SkeletonList({ rows = 4 }: { rows?: number }) {
  return (
    <div className="space-y-3.5">
      {Array.from({ length: rows }, (_, i) => (
        <div key={i} className="flex items-center gap-3">
          <Skeleton className="size-6" />
          <Skeleton className="h-2.5 flex-1" />
          <Skeleton className="h-2.5 w-12" />
        </div>
      ))}
    </div>
  )
}

export function ProgressBar({ value, tone }: { value: number; tone?: 'auto' | 'info' }) {
  const pct = Math.min(100, Math.max(0, value))
  const color = tone === 'info' ? 'bg-info' : pct >= 100 ? 'bg-danger' : pct >= 80 ? 'bg-warning' : 'bg-fg'
  return (
    <div className="h-[5px] w-full overflow-hidden rounded-full bg-inset">
      <motion.div className={cn('h-full w-full origin-left rounded-full', color)} initial={false} animate={{ scaleX: pct / 100 }} transition={tween} />
    </div>
  )
}

export function SegmentBar({ total, filled }: { total: number; filled: number }) {
  return (
    <div className="flex gap-[3px]">
      {Array.from({ length: total }, (_, i) => (
        <span key={i} className={cn('h-[14px] w-[7px] rounded-[2px] transition-colors duration-[250ms]', i < filled ? 'bg-info' : 'bg-inset')} />
      ))}
    </div>
  )
}

export function EmptyState({ icon, title, text, action, size = 'page', align = 'center', className }: { icon?: ReactNode; title: ReactNode; text?: ReactNode; action?: ReactNode; size?: 'page' | 'inline'; align?: 'center' | 'start'; className?: string }) {
  if (size === 'inline') {
    return (
      <div className={cn('flex items-center gap-2.5 py-6 text-muted', align === 'center' ? 'justify-center text-center' : 'text-left', className)}>
        {icon ?? <Icon name="dashedCircle" size={16} className="shrink-0 text-faint" />}
        <div className="min-w-0">
          <p className="text-base text-muted">{title}</p>
          {text && <p className="mt-0.5 text-sm text-faint">{text}</p>}
        </div>
        {action && <div className="ml-1 shrink-0">{action}</div>}
      </div>
    )
  }
  return (
    <div className={cn('mx-auto flex max-w-sm flex-col items-center py-20 text-center', className)}>
      {icon ?? <Icon name="inbox" size={28} className="mb-4 text-faint" />}
      <p className="text-md font-medium text-fg">{title}</p>
      {text && <p className="mt-1.5 text-base text-muted">{text}</p>}
      {action && <div className="mt-7">{action}</div>}
    </div>
  )
}

export function CopyButton({ text, className }: { text: string; className?: string }) {
  const [done, setDone] = useState(false)
  return (
    <button
      aria-label="copiar"
      className={cn('relative inline-flex size-7 items-center justify-center rounded-control-sm text-muted transition-colors hover:bg-hover hover:text-fg', className)}
      onClick={() => {
        navigator.clipboard?.writeText(text).catch(() => undefined)
        setDone(true)
        setTimeout(() => setDone(false), 1400)
      }}
    >
      <IconSwap k={done ? 'y' : 'n'}>
        {done ? <Icon name="check" size={15} className="text-success" /> : <Icon name="copy" size={15} />}
      </IconSwap>
      <AnimatePresence>
        {done && (
          <motion.span initial={{ opacity: 0, y: 4 }} animate={{ opacity: 1, y: -2 }} exit={{ opacity: 0, transition: tweenClose }} transition={tween} className="pointer-events-none absolute -top-8 whitespace-nowrap rounded-control-sm bg-primary px-2 py-1 text-xs text-primary-fg shadow-pop">
            Copiado
          </motion.span>
        )}
      </AnimatePresence>
    </button>
  )
}

const kw = /\b(import|from|const|await|return|async|function|let|new|export|def|print)\b/g

function highlight(line: string, json: boolean): ReactNode[] {
  if (json) {
    return line.split(/("(?:[^"\\]|\\.)*"\s*:?)/g).map((p, i) =>
      p.startsWith('"') ? (
        <span key={i} className={p.trimEnd().endsWith(':') ? 'text-fg' : 'text-success-fg'}>
          {p}
        </span>
      ) : (
        <span key={i}>{p}</span>
      ),
    )
  }
  if (line.trim().startsWith('//') || line.trim().startsWith('#')) return [<span key="c" className="text-muted">{line}</span>]
  return line.split(/("[^"]*"|'[^']*')/g).map((p, i) => {
    if (/^["']/.test(p)) return <span key={i} className="text-success-fg">{p}</span>
    const parts = p.split(kw)
    return (
      <span key={i}>
        {parts.map((q, j) => (j % 2 === 1 ? <span key={j} className="text-cat-5">{q}</span> : <span key={j}>{q.split(/(\b[A-Za-z_]+(?=\())/g).map((r, n) => (n % 2 === 1 ? <span key={n} className="text-info-fg">{r}</span> : r))}</span>))}
      </span>
    )
  })
}

export function CodeBlock({ code, language, tabs, className }: { code?: string; language?: string; tabs?: { label: string; code: string; icon?: ReactNode }[]; className?: string }) {
  const [active, setActive] = useState(0)
  const current = tabs ? tabs[active].code : code ?? ''
  return (
    <div className={cn('overflow-hidden rounded-card bg-surface shadow-card', className)}>
      <div className="flex items-center justify-between border-b px-4">
        {tabs ? (
          <div className="flex gap-5">
            {tabs.map((t, i) => (
              <button key={t.label} onClick={() => setActive(i)} className={cn('relative flex h-11 items-center gap-1.5 text-base transition-colors', i === active ? 'text-fg' : 'text-muted hover:text-fg')}>
                {t.icon}
                {t.label}
                {i === active && <motion.span layoutId={`code-tab-${tabs.map((x) => x.label).join()}`} transition={springSnap} className="absolute inset-x-0 -bottom-px h-[2px] rounded-full bg-fg" />}
              </button>
            ))}
          </div>
        ) : (
          <span className="flex h-10 items-center text-sm text-muted">{language === 'json' ? 'JSON' : language ?? 'Texto'}</span>
        )}
        <CopyButton text={current} />
      </div>
      <AnimatePresence mode="wait" initial={false}>
        <motion.pre key={active} initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.15 }} className="overflow-x-auto px-4 py-3.5 font-mono text-sm leading-[1.75] text-fg2">
          {current.split('\n').map((line, i) => (
            <div key={i}>{highlight(line, language === 'json') }{line === '' ? ' ' : null}</div>
          ))}
        </motion.pre>
      </AnimatePresence>
    </div>
  )
}

export function DiffView({ diff }: { diff: string }) {
  return (
    <pre className="overflow-x-auto rounded-card bg-surface py-2 font-mono text-sm leading-[1.7] shadow-card">
      {diff.split('\n').map((line, i) => (
        <motion.div key={i} initial={{ opacity: 0, x: -4 }} animate={{ opacity: 1, x: 0 }} transition={{ ...tween, delay: stagger(i) }} className={cn('px-4', line.startsWith('+') && 'bg-success-soft text-success-fg', line.startsWith('-') && 'bg-danger-soft text-danger-fg')}>
          {line || ' '}
        </motion.div>
      ))}
    </pre>
  )
}

export function Stat({ label, value, hint, icon }: { label: string; value: ReactNode; hint?: ReactNode; icon?: ReactNode }) {
  return (
    <div>
      <p className="flex items-center gap-1.5 text-base text-muted">
        {icon}
        {label}
      </p>
      <p className="mt-2 text-xl font-medium tabular-nums tracking-tightish text-fg">{value}</p>
      {hint && <p className="mt-0.5 text-xs tabular-nums text-muted">{hint}</p>}
    </div>
  )
}
