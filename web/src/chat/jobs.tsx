import { createContext, useContext, useState } from 'react'
import { cn } from '@/ui/cn'
import { Spinner } from '@/ui/display'
import { Icon } from '@/ui/icon'
import { useT } from '@/i18n'

export type StopJob = (job: string) => Promise<void>

export const JobsContext = createContext<StopJob | null>(null)

export function LiveDot({ className }: { className?: string }) {
  return (
    <span className={cn('relative flex size-2 shrink-0', className)}>
      <span className="absolute inline-flex size-full rounded-full bg-success opacity-60 motion-safe:animate-ping" />
      <span className="relative inline-flex size-2 rounded-full bg-success" />
    </span>
  )
}

export function JobBadge({ running }: { running: boolean }) {
  const t = useT()
  if (!running) return <span className="inline-flex h-7 shrink-0 items-center rounded-control-sm bg-inset px-2.5 text-base text-muted">{t('plan.job_ended')}</span>
  return (
    <span className="inline-flex h-7 shrink-0 items-center gap-2 rounded-control-sm bg-success-soft px-2.5 text-base text-success-fg">
      <LiveDot />
      {t('plan.job_running')}
    </span>
  )
}

export function StopJobButton({ job, compact }: { job: string; compact?: boolean }) {
  const t = useT()
  const stop = useContext(JobsContext)
  const [state, setState] = useState<'idle' | 'stopping' | 'failed'>('idle')
  if (!stop) return null
  const press = () => {
    setState('stopping')
    stop(job).then(() => setState('idle'), () => setState('failed'))
  }
  const label = state === 'failed' ? t('plan.job_stop_failed') : t('plan.job_stop')
  return (
    <button type="button" title={t('plan.job_stop_hint')} onClick={press} disabled={state === 'stopping'} aria-label={`${t('plan.job_stop_hint')} (${job})`} className={cn('inline-flex h-7 shrink-0 items-center gap-1.5 rounded-control-sm px-2.5 text-base text-danger-fg shadow-[0_0_0_1px_rgb(var(--border-strong))] transition-colors hover:bg-danger-soft disabled:opacity-60', compact && 'h-6 px-2 text-sm')}>
      {state === 'stopping' ? <Spinner size={12} className="text-danger-fg" /> : <Icon name="stop" size={12} />}
      {label}
    </button>
  )
}
