import { useEffect, useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { api } from '@/api'
import type { Application, ApplicationRepository, Binding, Block, Finding, FindingKind, QualityReport } from '@/api/contract'
import { errorText, useStore } from '@/api/store'
import { TaskTargetProvider } from '@/blocks/findingTask'
import { BlockView } from '@/blocks/registry'
import { Button } from '@/ui/button'
import { Card, EmptyState, ListSkeleton } from '@/ui/display'
import { Input, Select } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { useT } from '@/i18n'
import { folderName } from '../settings/Applications'
import { useBoundLook } from '../settings/bindings'

export const REPORTS = ['quality', 'security'] as const

export type ReportKind = (typeof REPORTS)[number]

export interface Target {
  key: string
  application: Application
  repository: ApplicationRepository
  capability: ReportKind
  binding: Binding
}

export type Loaded =
  | { state: 'loading' }
  | { state: 'failed'; error: string }
  | { state: 'pending' }
  | { state: 'done'; block: Block }

const ALL = 'all'
const KINDS: FindingKind[] = ['static', 'dependency', 'dynamic', 'manual']

export function targetsOf(applications: Application[]): Target[] {
  return applications.flatMap((application) =>
    application.repositories.flatMap((repository) =>
      REPORTS.flatMap((capability) => {
        const binding = repository.bindings?.[capability]
        return binding ? [{ key: `${application.id}/${repository.id}/${capability}`, application, repository, capability, binding }] : []
      }),
    ),
  )
}

const text = (value: unknown) => (typeof value === 'string' ? value : value === null || value === undefined ? '' : String(value))
const maybe = (value: unknown) => (typeof value === 'string' && value !== '' ? value : null)

export function findingOf(raw: Record<string, unknown>): Finding {
  const severity = Number(raw.severity) || 0
  const kind = KINDS.includes(raw.kind as FindingKind) ? (raw.kind as FindingKind) : 'static'
  const policy = raw.policy === true
  return {
    id: text(raw.id),
    title: text(raw.title),
    severity,
    kind,
    new: raw.new === true,
    policy,
    cwe: (raw.cwe as Finding['cwe']) ?? null,
    cve: (raw.cve as Finding['cve']) ?? null,
    file: maybe(raw.file),
    line: typeof raw.line === 'number' ? raw.line : null,
    component: maybe(raw.component),
    version: maybe(raw.version),
    fixedIn: maybe(raw.fixedIn),
    description: text(raw.description),
    held: null,
    suggested: policy || severity >= 4 ? 'fix' : 'task',
    decision: null,
    url: maybe(raw.url),
  }
}

const count = (value: unknown) => (typeof value === 'number' && Number.isFinite(value) ? value : 0)
const percent = (value: unknown) => (typeof value === 'number' && Number.isFinite(value) ? value : null)
const grade = (value: unknown) => (['A', 'B', 'C', 'D', 'E'].includes(value as string) ? (value as QualityReport['ratings']['security']) : 'A')
const listOf = <T,>(value: unknown) => (Array.isArray(value) ? (value as T[]) : [])

export function qualityOf(raw: Record<string, unknown>): QualityReport {
  const ratings = (raw.ratings ?? {}) as Record<string, unknown>
  const counts = (raw.counts ?? {}) as Record<string, unknown>
  return {
    gate: raw.gate === 'failed' ? 'failed' : 'passed',
    ratings: { reliability: grade(ratings.reliability), security: grade(ratings.security), maintainability: grade(ratings.maintainability) },
    counts: { bugs: count(counts.bugs), vulnerabilities: count(counts.vulnerabilities), smells: count(counts.smells), hotspots: count(counts.hotspots), hotspotsReviewed: count(counts.hotspotsReviewed) },
    coverage: percent(raw.coverage),
    duplication: percent(raw.duplication),
    conditions: listOf(raw.conditions),
    issues: listOf(raw.issues),
    total: count(raw.total),
  }
}

async function load(target: Target): Promise<Loaded> {
  const { connection, value } = target.binding
  if (target.capability === 'quality') {
    const found = await api().call(connection, 'quality.report', { project: value })
    if (found.status !== 'done') return { state: 'pending' }
    const block: Block = { type: 'quality', connection, project: value, branch: '', analyzedAt: maybe(found.analyzedAt), url: maybe(found.url), report: qualityOf(found) }
    return { state: 'done', block }
  }
  const [listed, status] = await Promise.all([
    api().call(connection, 'security.findings', { project: value, open: true, limit: 500 }),
    api().call(connection, 'security.status', { project: value }).catch(() => null),
  ])
  if (listed.length === 0 && status && status.scannedAt === null) return { state: 'pending' }
  const findings = listed.map(findingOf).sort((a, b) => b.severity - a.severity)
  const block: Block = { type: 'findings', connection, project: value, scannedAt: status?.scannedAt ?? null, url: null, triage: null, findings }
  return { state: 'done', block }
}

export function useReports(targets: Target[]): Record<string, Loaded> {
  const [loaded, setLoaded] = useState<Record<string, Loaded>>({})
  useEffect(() => {
    let alive = true
    setLoaded(Object.fromEntries(targets.map((t) => [t.key, { state: 'loading' } as Loaded])))
    for (const target of targets) {
      load(target)
        .then((done) => alive && setLoaded((known) => ({ ...known, [target.key]: done })))
        .catch((e: unknown) => alive && setLoaded((known) => ({ ...known, [target.key]: { state: 'failed', error: errorText(e) } })))
    }
    return () => {
      alive = false
    }
  }, [targets])
  return loaded
}

export interface Filters {
  application: string
  source: string
  severity: number
  search: string
}

const matches = (search: string, ...texts: (string | null)[]) => {
  const wanted = search.trim().toLowerCase()
  return wanted === '' || texts.some((t) => (t ?? '').toLowerCase().includes(wanted))
}

export function filtered(block: Block, filters: Filters): Block | null {
  if (block.type === 'findings') {
    const findings = block.findings.filter((f) => f.severity >= filters.severity && matches(filters.search, f.title, f.file, f.component, f.cwe?.name ?? null, f.cve?.id ?? null))
    return findings.length === 0 ? null : { ...block, findings }
  }
  if (block.type === 'quality') {
    if (filters.search.trim() === '') return block
    const issues = block.report.issues.filter((i) => matches(filters.search, i.message, i.file))
    return issues.length === 0 ? null : { ...block, report: { ...block.report, issues } }
  }
  return block
}

function Report({ target, loaded, filters }: { target: Target; loaded: Loaded | undefined; filters: Filters }) {
  const t = useT()
  if (!loaded || loaded.state === 'loading') return <p className="px-1 text-sm text-muted">{t('app.securityLoading')}</p>
  if (loaded.state === 'failed') return <p role="alert" className="px-1 text-sm text-danger-fg">{t('app.securityFailed', { error: loaded.error })}</p>
  if (loaded.state === 'pending') return <p className="px-1 text-sm text-muted">{t('app.securityPending')}</p>
  const shown = filtered(loaded.block, filters)
  if (!shown) return <p className="px-1 text-sm text-muted">{t('app.securityNothing')}</p>
  return <div data-testid={`report-${target.key}`}><BlockView block={shown} /></div>
}

function Repository({ targets, loaded, filters }: { targets: Target[]; loaded: Record<string, Loaded>; filters: Filters }) {
  const look = useBoundLook()
  const { application, repository } = targets[0]
  const tracker = application.bindings?.tracker ?? repository.bindings?.tracker
  const task = tracker ? { connection: tracker.connection, project: tracker.value } : null
  return (
    <TaskTargetProvider value={task}>
    <Card className="space-y-4 px-4 py-4">
      <div className="flex min-w-0 items-baseline gap-2">
        <span className="truncate text-md font-medium text-fg">{application.name}</span>
        <span className="truncate font-mono text-sm text-muted">{folderName(repository.directory)}</span>
      </div>
      {targets.map((target) => (
        <section key={target.key} className="space-y-2">
          <h3 className="text-sm font-medium text-muted">{look(target.capability, target.binding).label} · {target.binding.label || target.binding.value}</h3>
          <Report target={target} loaded={loaded[target.key]} filters={filters} />
        </section>
      ))}
    </Card>
    </TaskTargetProvider>
  )
}

function NoProjects() {
  const t = useT()
  const navigate = useNavigate()
  const icon = <Icon name="shield" size={28} className="mb-4 text-faint" />
  const action = <Button variant="primary" onClick={() => navigate('/settings/applications')}>{t('app.securityEmptyAction')}</Button>
  return <EmptyState icon={icon} title={t('app.securityEmpty')} text={t('app.securityEmptyText')} action={action} />
}

export function SecurityView() {
  const t = useT()
  const applications = useStore((s) => s.applications)
  const look = useBoundLook()
  const all = useMemo(() => targetsOf(applications ?? []), [applications])
  const loaded = useReports(all)
  const [filters, setFilters] = useState<Filters>({ application: ALL, source: ALL, severity: 0, search: '' })
  const sources = [...new Map(all.map((target) => [target.capability, look(target.capability, target.binding).label])).entries()]
  const chosen = all.filter((target) => (filters.application === ALL || target.application.id === filters.application) && (filters.source === ALL || target.capability === filters.source))
  const groups = [...new Map(chosen.map((target) => [`${target.application.id}/${target.repository.id}`, chosen.filter((other) => other.application.id === target.application.id && other.repository.id === target.repository.id)])).values()]
  if (!applications) return <ListSkeleton />
  if (all.length === 0) return <NoProjects />
  const set = (next: Partial<Filters>) => setFilters((known) => ({ ...known, ...next }))
  const appOptions = [{ value: ALL, label: t('app.securityAll') }, ...[...new Map(all.map((target) => [target.application.id, target.application.name])).entries()].map(([value, label]) => ({ value, label }))]
  const sourceOptions = [{ value: ALL, label: t('app.securityAll') }, ...sources.map(([value, label]) => ({ value, label }))]
  const severities = [{ value: '0', label: t('app.securityAnySeverity') }, ...[1, 2, 3, 4, 5].map((n) => ({ value: String(n), label: `≥ ${n}` }))]
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-2">
        <Select size="sm" label={t('app.securityApplication')} value={filters.application} options={appOptions} onValueChange={(application) => set({ application })} />
        <Select size="sm" label={t('app.securitySource')} value={filters.source} options={sourceOptions} onValueChange={(source) => set({ source })} />
        <Select size="sm" label={t('app.securitySeverity')} value={String(filters.severity)} options={severities} onValueChange={(v) => set({ severity: Number(v) })} />
        <Input aria-label={t('app.securitySearch')} className="h-8 w-full max-w-[260px] text-sm" value={filters.search} placeholder={t('app.securitySearchHint')} onChange={(e) => set({ search: e.target.value })} />
      </div>
      {groups.map((targets) => <Repository key={`${targets[0].application.id}/${targets[0].repository.id}`} targets={targets} loaded={loaded} filters={filters} />)}
    </div>
  )
}
