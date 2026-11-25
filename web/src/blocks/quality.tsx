import type { ReactNode } from 'react'
import { ago } from '@/api/clock'
import type { Block, QualityCondition, QualityGrade, QualityIssue } from '@/api/contract'
import { cn } from '@/ui/cn'
import { Icon } from '@/ui/icon'
import { useT, type MessageKey } from '@/i18n'
import { SourceCard } from './findings'

type Of<T extends Block['type']> = Extract<Block, { type: T }>

const GRADE: Record<QualityGrade, string> = {
  A: 'bg-success-soft text-success-fg',
  B: 'bg-success-soft text-success-fg',
  C: 'bg-warning-soft text-warning-fg',
  D: 'bg-danger-soft text-danger-fg',
  E: 'bg-danger text-white',
}

const METRIC: Record<string, MessageKey> = {
  new_coverage: 'blocks.qualityCoverage',
  new_duplicated_lines_density: 'blocks.qualityDuplication',
  new_security_hotspots_reviewed: 'blocks.qualityHotspots',
  new_reliability_rating: 'blocks.qualityReliability',
  new_security_rating: 'blocks.qualitySecurity',
  new_maintainability_rating: 'blocks.qualityMaintainability',
}

const TYPE: Record<QualityIssue['type'], { label: MessageKey; pill: string }> = {
  bug: { label: 'blocks.qualityTypeBug', pill: 'bg-warning-soft text-warning-fg' },
  vulnerability: { label: 'blocks.qualityTypeVulnerability', pill: 'bg-danger-soft text-danger-fg' },
  hotspot: { label: 'blocks.qualityTypeHotspot', pill: 'bg-danger-soft text-danger-fg' },
  code_smell: { label: 'blocks.qualityTypeCodeSmell', pill: 'bg-inset text-fg2' },
}

const percent = (n: number | null) => (n === null ? '—' : `${n.toLocaleString(undefined, { maximumFractionDigits: 1 })}%`)

function Gate({ block }: { block: Of<'quality'> }) {
  const t = useT()
  const { report } = block
  const failed = report.conditions.filter((c) => !c.passed).length
  const ok = report.gate === 'passed'
  return (
    <div className={cn('flex items-center gap-3.5 rounded-card px-4 py-3.5', ok ? 'bg-success-soft' : 'bg-danger-soft')}>
      <span className={cn('flex size-8 shrink-0 items-center justify-center rounded-[9px] text-white', ok ? 'bg-success' : 'bg-danger-solid')}>
        <Icon name={ok ? 'check' : 'close'} size={17} strokeWidth={2.4} />
      </span>
      <div className="flex min-w-0 flex-col">
        <span className={cn('text-base font-semibold', ok ? 'text-success-fg' : 'text-danger-fg')}>{t(ok ? 'blocks.qualityPassed' : 'blocks.qualityFailed')}</span>
        <span className={cn('text-sm', ok ? 'text-success-fg/80' : 'text-danger-fg/80')}>
          {ok ? t('blocks.qualityAllPassed', { total: report.conditions.length }) : t('blocks.qualityFailedCount', { n: failed, total: report.conditions.length })}
        </span>
      </div>
    </div>
  )
}

function Card({ children }: { children: ReactNode }) {
  return <div className="flex items-center gap-3 rounded-card bg-surface px-3.5 py-3 shadow-card">{children}</div>
}

function Rating({ grade, label, detail }: { grade: QualityGrade; label: string; detail: string }) {
  return (
    <Card>
      <span className={cn('flex size-8 shrink-0 items-center justify-center rounded-control-sm text-md font-semibold', GRADE[grade])}>{grade}</span>
      <span className="flex min-w-0 flex-col"><span className="truncate text-base font-medium text-fg">{label}</span><span className="text-sm text-muted">{detail}</span></span>
    </Card>
  )
}

function Measure({ value, label }: { value: string; label: string }) {
  return (
    <Card>
      <span className="flex min-w-0 flex-col"><span className="text-xl font-semibold tabular-nums text-fg">{value}</span><span className="truncate text-sm text-muted">{label}</span></span>
    </Card>
  )
}

function Measures({ block }: { block: Of<'quality'> }) {
  const t = useT()
  const { ratings, counts, coverage, duplication } = block.report
  return (
    <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
      <Rating grade={ratings.reliability} label={t('blocks.qualityReliability')} detail={t('blocks.qualityBugs', { n: counts.bugs })} />
      <Rating grade={ratings.security} label={t('blocks.qualitySecurity')} detail={t('blocks.qualityVulns', { n: counts.vulnerabilities })} />
      <Rating grade={ratings.maintainability} label={t('blocks.qualityMaintainability')} detail={t('blocks.qualitySmells', { n: counts.smells })} />
      <Measure value={percent(coverage)} label={t('blocks.qualityCoverage')} />
      <Measure value={percent(duplication)} label={t('blocks.qualityDuplication')} />
      <Measure value={`${counts.hotspotsReviewed}/${counts.hotspots}`} label={t('blocks.qualityHotspots')} />
    </div>
  )
}

function Condition({ c }: { c: QualityCondition }) {
  const t = useT()
  const label = METRIC[c.metric] ? t(METRIC[c.metric]) : c.metric
  return (
    <li className="grid min-h-11 grid-cols-[20px_minmax(0,1fr)_auto_120px] items-center gap-3 px-4 py-2 [&+&]:border-t">
      <Icon name={c.passed ? 'check' : 'close'} size={15} strokeWidth={2.4} className={c.passed ? 'text-success-fg' : 'text-danger-fg'} />
      <span className="truncate text-base text-fg">{label}</span>
      <span className={cn('text-base font-semibold tabular-nums', c.passed ? 'text-fg' : 'text-danger-fg')}>{c.value}</span>
      <span className="text-right text-sm text-muted">{t('blocks.qualityLimit', { value: c.threshold })}</span>
    </li>
  )
}

function Issue({ i }: { i: QualityIssue }) {
  const t = useT()
  const type = TYPE[i.type]
  const place = i.file ? (i.line ? `${i.file}:${i.line}` : i.file) : ''
  const body = (
    <>
      <span className={cn('inline-flex h-[22px] w-fit items-center rounded-[6px] px-2 text-xs font-medium', type.pill)}>{t(type.label)}</span>
      <span className="truncate text-base text-fg" title={i.message}>{i.message}</span>
      <span className="hidden truncate font-mono text-sm text-fg2 sm:block" title={place}>{place}</span>
    </>
  )
  const cls = 'grid min-h-12 grid-cols-[110px_minmax(0,1fr)] items-center gap-x-4 px-4 py-2 sm:grid-cols-[110px_minmax(0,1fr)_minmax(0,230px)]'
  return (
    <li className="[&+&]:border-t">
      {i.url ? <a href={i.url} target="_blank" rel="noreferrer" className={cn(cls, 'transition-colors hover:bg-hover/50')}>{body}</a> : <div className={cls}>{body}</div>}
    </li>
  )
}

export function QualityBlock({ block }: { block: Of<'quality'> }) {
  const t = useT()
  const { report } = block
  return (
    <div className="flex flex-col gap-5">
      <SourceCard connection={block.connection} title={`${block.project} · ${block.branch}`} when={block.analyzedAt ? t('blocks.qualityAnalyzed', { time: ago(block.analyzedAt) }) : null} url={block.url} />
      <Gate block={block} />
      <Measures block={block} />
      <section>
        <h3 className="mb-3 text-md font-medium text-fg2">{t('blocks.qualityConditions')}</h3>
        <ul className="overflow-hidden rounded-card bg-surface shadow-card">{report.conditions.map((c) => <Condition key={c.metric} c={c} />)}</ul>
      </section>
      {report.issues.length > 0 && (
        <section>
          <div className="mb-3 flex items-baseline gap-2">
            <h3 className="text-md font-medium text-fg2">{t('blocks.qualityIssues')}</h3>
            <span className="text-sm text-muted">{t('blocks.qualityIssuesCount', { n: report.issues.length, total: report.total })}</span>
            {block.url && <a href={block.url} target="_blank" rel="noreferrer" className="ml-auto inline-flex items-center gap-1 text-sm text-accent-fg hover:underline">{t('blocks.qualitySeeAll')}<Icon name="arrowUpRight" size={13} /></a>}
          </div>
          <ul className="overflow-hidden rounded-card bg-surface shadow-card">{report.issues.map((i) => <Issue key={i.key} i={i} />)}</ul>
        </section>
      )}
    </div>
  )
}
