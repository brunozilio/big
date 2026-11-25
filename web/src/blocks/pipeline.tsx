import { useEffect, useRef, useState } from 'react'
import { ago } from '@/api/clock'
import type { Block, Job, JobState, PipelineRun } from '@/api/contract'
import { useCall, useProviderOf } from '@/api/store'
import { Button } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Spinner } from '@/ui/display'
import { ProviderLogo } from '@/ui/domain'
import { Checkbox, Textarea } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { motion, tween } from '@/ui/motion'
import { useT, type MessageKey } from '@/i18n'
import { useBlockHost } from './context'
import { Commands, Loading, SendError, useSend } from './shared'

type Of<T extends Block['type']> = Extract<Block, { type: T }>

export function JobIcon({ state, size = 16 }: { state: JobState; size?: number }) {
  if (state === 'running') return <Spinner size={size} className="text-warning" />
  if (state === 'passed') return <span className="flex items-center justify-center rounded-full bg-success text-white" style={{ width: size, height: size }}><Icon name="check" size={size * 0.65} strokeWidth={2.6} /></span>
  if (state === 'failed' || state === 'rejected') return <span className="flex items-center justify-center rounded-full bg-danger text-white" style={{ width: size, height: size }}><Icon name="close" size={size * 0.65} strokeWidth={2.6} /></span>
  if (state === 'waiting') return <span className="flex items-center justify-center rounded-full bg-warning-soft text-warning-fg ring-1 ring-warning/60" style={{ width: size, height: size }}><Icon name="clock" size={size * 0.7} /></span>
  if (state === 'skipped') return <span className="flex items-center justify-center rounded-full border border-dashed border-line-strong text-faint" style={{ width: size, height: size }}><Icon name="minus" size={size * 0.6} /></span>
  return <span className="rounded-full border-[1.5px] border-line-strong" style={{ width: size, height: size }} />
}

function jobDuration(j: Job) {
  if (j.seconds === null) return ''
  return j.seconds < 60 ? `${j.seconds}s` : `${Math.floor(j.seconds / 60)}m ${j.seconds % 60}s`
}

const CARD_W = 176
const CARD_H = 40
const COL_GAP = 56
const ROW_GAP = 14

function JobGraph({ jobs, sel, onSel }: { jobs: Job[]; sel: string; onSel: (id: string) => void }) {
  const stages = [...new Set(jobs.map((j) => j.stage))].sort((a, b) => a - b)
  const maxRows = Math.max(...stages.map((st) => jobs.filter((j) => j.stage === st).length))
  const height = maxRows * CARD_H + (maxRows - 1) * ROW_GAP
  const pos: Record<string, { x: number; y: number }> = {}
  stages.forEach((st, si) => {
    const col = jobs.filter((j) => j.stage === st)
    const colH = col.length * CARD_H + (col.length - 1) * ROW_GAP
    col.forEach((j, ri) => {
      pos[j.id] = { x: si * (CARD_W + COL_GAP), y: (height - colH) / 2 + ri * (CARD_H + ROW_GAP) }
    })
  })
  const width = stages.length * CARD_W + (stages.length - 1) * COL_GAP
  const state = Object.fromEntries(jobs.map((j) => [j.id, j.state]))
  const edges = jobs.flatMap((j) => j.needs.filter((n) => pos[n]).map((n) => ({ from: n, to: j.id })))
  return (
    <div className="relative" style={{ width, height }}>
      <svg className="absolute inset-0 overflow-visible" width={width} height={height} aria-hidden>
        {edges.map(({ from, to }) => {
          const a = pos[from]
          const b = pos[to]
          const x1 = a.x + CARD_W
          const y1 = a.y + CARD_H / 2
          const x2 = b.x
          const y2 = b.y + CARD_H / 2
          const mx = (x1 + x2) / 2
          const done = state[from] === 'passed' && state[to] !== 'queued' && state[to] !== 'skipped'
          const stroke = done ? 'rgb(var(--text-muted))' : 'rgb(var(--border-strong))'
          return (
            <g key={`${from}-${to}`}>
              <path d={`M${x1} ${y1} C${mx} ${y1} ${mx} ${y2} ${x2} ${y2}`} fill="none" stroke={stroke} strokeWidth={1.5} />
              <circle cx={x1} cy={y1} r={3} fill="rgb(var(--surface))" stroke={stroke} strokeWidth={1.5} />
              <circle cx={x2} cy={y2} r={3} fill="rgb(var(--surface))" stroke={stroke} strokeWidth={1.5} />
            </g>
          )
        })}
      </svg>
      {jobs.map((j) => {
        const p = pos[j.id]
        return (
          <button key={j.id} data-sel={sel === j.id} onClick={() => onSel(j.id)} style={{ left: p.x, top: p.y, width: CARD_W, height: CARD_H }} className={cn('absolute flex items-center gap-2 rounded-control border bg-surface px-2.5 text-left transition-shadow hover:shadow-pop', sel === j.id ? 'border-info shadow-[0_0_0_1px_rgb(var(--info))]' : 'border-line-strong', j.state === 'waiting' && sel !== j.id && 'border-warning', j.state === 'skipped' && 'border-dashed')}>
            <JobIcon state={j.state} size={15} />
            <span className={cn('min-w-0 flex-1 truncate text-sm', j.state === 'skipped' ? 'text-muted' : 'text-fg')}>{j.name}</span>
            <span className="text-xs tabular-nums text-faint">{jobDuration(j)}</span>
          </button>
        )
      })}
    </div>
  )
}

function DeployApproval({ job, gate }: { job: Job; gate: { approve: string; reject: string } }) {
  const t = useT()
  const [note, setNote] = useState('')
  const { busy, error, send } = useSend()
  const env = job.gate ?? ''
  return (
    <motion.div initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} transition={tween} className="rounded-card bg-warning-soft p-4">
      <div className="mb-3 flex items-center gap-2 text-base font-medium text-warning-fg">
        <Icon name="hSecurityCheck" size={17} />
        {t('blocks.deployReview', { env })}
      </div>
      <label className="mb-3 flex items-center gap-2.5 rounded-control bg-surface px-3 py-2.5 text-base shadow-card">
        <Checkbox checked onCheckedChange={() => undefined} label={env} />
        <span className="font-medium text-fg">{env}</span>
        {job.waitingSince && <span className="text-sm text-muted">{t('blocks.deployWaiting', { time: ago(job.waitingSince) })}</span>}
      </label>
      <Textarea rows={2} value={note} onChange={(e) => setNote(e.target.value)} placeholder={t('blocks.deployNote')} className="bg-surface" />
      <div className="mt-3 flex justify-end gap-2">
        <Button size="sm" variant="secondary" disabled={busy} onClick={() => void send(gate.reject, { env, note })}>{t('blocks.deployReject')}</Button>
        <Button size="sm" variant="primary" disabled={busy} icon={<Icon name="hRocket" size={15} />} onClick={() => void send(gate.approve, { env, note })}>{t('blocks.deployApprove')}</Button>
      </div>
      <SendError error={error} />
    </motion.div>
  )
}

const runLabel: Record<JobState, MessageKey> = { queued: 'blocks.runQueued', running: 'blocks.runRunning', passed: 'blocks.runPassed', failed: 'blocks.runFailed', skipped: 'blocks.runSkipped', waiting: 'blocks.runWaiting', rejected: 'blocks.runRejected' }

function JobDetail({ job }: { job: Job }) {
  const t = useT()
  if (job.state === 'skipped') return <p className="px-2 py-2 text-sm text-muted">{t('blocks.jobSkippedBefore')} <span className="font-mono text-xs">if: {job.condition ?? 'needs.*.result'}</span> {t('blocks.jobSkippedAfter')}</p>
  if (job.state === 'waiting') return <p className="px-2 py-2 text-sm text-muted">{t('blocks.jobWaiting')} <span className="font-medium text-fg">{job.gate}</span>.</p>
  if (job.state === 'rejected') return <p className="px-2 py-2 text-sm text-danger-fg">{t('blocks.jobRejected')}</p>
  if (job.state === 'queued') return <p className="px-2 py-2 text-sm text-muted">{t('blocks.jobQueued')}</p>
  return <Commands key={`${job.id}-${job.state}`} commands={job.steps} />
}

function Run({ run, gate, connection }: { run: PipelineRun; gate: Of<'pipeline'>['gate']; connection: string }) {
  const t = useT()
  const provider = useProviderOf(connection)
  const firstOpen = run.jobs.find((j) => j.state === 'waiting' || j.state === 'running' || j.state === 'failed')?.id ?? run.jobs[0]?.id ?? ''
  const [sel, setSel] = useState(firstOpen)
  const graph = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const el = graph.current?.querySelector<HTMLElement>('[data-sel="true"]')
    el?.scrollIntoView?.({ block: 'nearest', inline: 'center', behavior: 'smooth' })
  }, [sel])
  const job = run.jobs.find((j) => j.id === sel) ?? run.jobs[0]
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <JobIcon state={run.state} size={20} />
        <span className="text-md font-medium text-fg">{run.title}</span>
        <span className="font-mono text-sm text-muted">{run.file} #{run.number}</span>
        <span className="text-sm text-muted">{t(runLabel[run.state])}</span>
        <span className="flex items-center gap-1.5 rounded-control-sm bg-inset px-2 py-0.5 font-mono text-xs text-fg2">{run.event} · {run.ref}</span>
        {run.url && (
          <Button asChild size="sm" variant="secondary" className="ml-auto">
            <a href={run.url} target="_blank" rel="noreferrer">
              {provider && <ProviderLogo logo={provider.logo} name={provider.name} size={15} />}
              {provider ? t('blocks.runViewIn', { name: provider.name }) : t('blocks.runOpen')}
              <Icon name="arrowUpRight" size={14} className="text-muted" />
            </a>
          </Button>
        )}
      </div>
      <div ref={graph} className="scrollbar-thin overflow-x-auto rounded-card border bg-subtle p-5 [background-image:radial-gradient(rgb(var(--border-strong))_1px,transparent_1.2px)] [background-size:16px_16px]">
        <JobGraph jobs={run.jobs} sel={sel} onSel={setSel} />
      </div>
      {job && job.state === 'waiting' && job.gate && gate && <DeployApproval job={job} gate={gate} />}
      {job && (
        <div className="overflow-hidden rounded-card bg-surface shadow-card">
          <div className="flex h-11 items-center gap-2.5 border-b px-4">
            <JobIcon state={job.state} />
            <span className="text-base font-medium text-fg">{job.name}</span>
            <span className="ml-auto text-xs tabular-nums text-faint">{jobDuration(job)}</span>
          </div>
          <div className="p-2">
            <JobDetail job={job} />
          </div>
        </div>
      )}
    </div>
  )
}

export function PipelineBlock({ block }: { block: Of<'pipeline'> }) {
  const { version } = useBlockHost()
  const { data: run, error } = useCall(block.connection, 'ci.run', { run: block.run }, version)
  if (!run) return <Loading error={error} />
  return <Run key={block.run} run={run} gate={block.gate} connection={block.connection} />
}
