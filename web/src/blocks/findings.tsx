import { useMemo, useState, type ReactNode } from 'react'
import { ago } from '@/api/clock'
import type { Block, Finding, FindingChoice, FindingKind, IgnoreReason, LinkRef } from '@/api/contract'
import { useProviderOf } from '@/api/store'
import { Button } from '@/ui/button'
import { cn } from '@/ui/cn'
import { LogoTile } from '@/ui/domain'
import { Select } from '@/ui/form'
import { Icon, type IconName } from '@/ui/icon'
import { AnimatePresence, motion, tween } from '@/ui/motion'
import { useT, type MessageKey } from '@/i18n'
import { TaskButton, useTaskTarget } from './findingTask'
import { SendError, useSend } from './shared'

type Of<T extends Block['type']> = Extract<Block, { type: T }>

type Pick = 'fix' | 'task' | 'ignore:false_positive' | 'ignore:accepted_risk'

const KINDS: FindingKind[] = ['static', 'dependency', 'dynamic', 'manual']

const KIND_LABEL: Record<FindingKind, MessageKey> = {
  static: 'blocks.findingsKindStatic',
  dependency: 'blocks.findingsKindDependency',
  dynamic: 'blocks.findingsKindDynamic',
  manual: 'blocks.findingsKindManual',
}

const SEVERITY: { label: MessageKey; pill: string; tile: string }[] = [
  { label: 'blocks.findingsSev0', pill: 'bg-inset text-muted', tile: 'bg-inset text-fg2' },
  { label: 'blocks.findingsSev1', pill: 'bg-inset text-fg2', tile: 'bg-inset text-fg2' },
  { label: 'blocks.findingsSev2', pill: 'bg-info-soft text-info-fg', tile: 'bg-info-soft text-info-fg' },
  { label: 'blocks.findingsSev3', pill: 'bg-warning-soft text-warning-fg', tile: 'bg-warning-soft text-warning-fg' },
  { label: 'blocks.findingsSev4', pill: 'bg-danger-soft text-danger-fg', tile: 'bg-danger-soft text-danger-fg' },
  { label: 'blocks.findingsSev5', pill: 'bg-danger text-white', tile: 'bg-danger text-white' },
]

const level = (severity: number) => SEVERITY[Math.max(0, Math.min(5, Math.round(severity)))]

const pickOf = (choice: FindingChoice, reason: IgnoreReason | null): Pick => (choice === 'ignore' ? `ignore:${reason ?? 'accepted_risk'}` : choice)

const decisionOf = (pick: Pick) => {
  const [choice, reason] = pick.split(':') as [FindingChoice, IgnoreReason | undefined]
  return { choice, reason: reason ?? null }
}

function where(f: Finding): string {
  if (f.file) return f.line ? `${f.file}:${f.line}` : f.file
  if (f.component) return f.version ? `${f.component}@${f.version}` : f.component
  return ''
}

function usePicks() {
  const t = useT()
  return useMemo(
    () => [
      { value: 'fix', label: t('blocks.findingsChoiceFix'), icon: <Icon name="sCode" size={14} /> },
      { value: 'task', label: t('blocks.findingsChoiceTask'), icon: <Icon name="sTicket" size={14} /> },
      { value: 'ignore:false_positive', label: t('blocks.findingsChoiceFalsePositive'), icon: <Icon name="skip" size={14} /> },
      { value: 'ignore:accepted_risk', label: t('blocks.findingsChoiceAcceptedRisk'), icon: <Icon name="skip" size={14} /> },
    ],
    [t],
  )
}

function SeverityPill({ severity }: { severity: number }) {
  const t = useT()
  const s = level(severity)
  return <span className={cn('inline-flex h-[22px] w-fit shrink-0 items-center rounded-[6px] px-2 text-xs font-medium', s.pill)}>{t(s.label)}</span>
}

function Linked({ to, icon, children }: { to: LinkRef | null; icon: IconName; children: ReactNode }) {
  const body = <><Icon name={icon} size={13} />{children}</>
  if (!to?.url) return <span className="inline-flex items-center gap-1.5 text-sm text-fg2">{body}</span>
  return <a href={to.url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1.5 text-sm text-accent-fg hover:underline">{body}</a>
}

function Outcome({ f }: { f: Finding }) {
  const t = useT()
  if (f.held) return <Linked to={f.held} icon="pr">{t('blocks.findingsHeld', { pr: f.held.key })}</Linked>
  const d = f.decision
  if (!d) return null
  if (d.choice === 'fix') return d.pr ? <Linked to={d.pr} icon="pr">{t('blocks.findingsFixing', { pr: d.pr.key })}</Linked> : <span className="text-sm font-medium text-fg">{t('blocks.findingsChoiceFix')}</span>
  if (d.choice === 'task') return <Linked to={d.task} icon="sTicket">{d.task ? t('blocks.findingsTask', { key: d.task.key }) : t('blocks.findingsChoiceTask')}</Linked>
  return <span className="text-sm text-muted">{t('blocks.findingsIgnored')} · {t(d.reason === 'false_positive' ? 'blocks.findingsReasonFalse' : 'blocks.findingsReasonRisk')}</span>
}

function Meta({ f }: { f: Finding }) {
  const parts: ReactNode[] = []
  if (f.kind === 'dependency' && f.cve) parts.push(f.cve.cvss != null ? `${f.cve.id} · CVSS ${f.cve.cvss.toFixed(1)}` : f.cve.id)
  else if (f.cwe) parts.push(`CWE-${f.cwe.id}`)
  const place = f.kind === 'dependency' && f.fixedIn ? `${f.version} → ${f.fixedIn}` : where(f)
  if (place) parts.push(<span key="where" className="font-mono">{place}</span>)
  return <span className="truncate text-sm text-muted" title={where(f)}>{parts.map((p, i) => <span key={i}>{i > 0 && ' · '}{p}</span>)}</span>
}

function Detail({ f }: { f: Finding }) {
  const t = useT()
  const facts: { label: string; value: ReactNode }[] = []
  if (f.cwe) facts.push({ label: 'CWE', value: `CWE-${f.cwe.id} · ${f.cwe.name}` })
  if (f.cve) facts.push({ label: 'CVE', value: f.cve.cvss != null ? `${f.cve.id} · CVSS ${f.cve.cvss.toFixed(1)}` : f.cve.id })
  if (where(f)) facts.push({ label: t('blocks.findingsWhere'), value: <span className="font-mono text-sm">{where(f)}</span> })
  if (f.fixedIn) facts.push({ label: t('blocks.findingsFixedIn'), value: <span className="font-mono text-sm">{f.fixedIn}</span> })
  return (
    <motion.div initial={{ height: 0, opacity: 0 }} animate={{ height: 'auto', opacity: 1 }} exit={{ height: 0, opacity: 0 }} transition={tween} className="overflow-hidden">
      <div className="flex flex-col gap-3 border-t bg-subtle px-4 py-3.5 sm:pl-[124px]">
        {f.description && <p className="max-w-[68ch] text-base text-fg2">{f.description}</p>}
        <dl className="grid grid-cols-[110px_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-sm">
          {facts.map((x) => <div key={x.label} className="contents"><dt className="text-muted">{x.label}</dt><dd className="min-w-0 text-fg2">{x.value}</dd></div>)}
        </dl>
        {f.url && <a href={f.url} target="_blank" rel="noreferrer" className="inline-flex w-fit items-center gap-1 text-sm text-accent-fg hover:underline">{t('blocks.findingsSeeFinding')}<Icon name="arrowUpRight" size={13} /></a>}
      </div>
    </motion.div>
  )
}

function Row({ f, open, onToggle, pick, onPick }: { f: Finding; open: boolean; onToggle: () => void; pick: Pick | null; onPick: ((p: Pick) => void) | null }) {
  const t = useT()
  const picks = usePicks()
  const target = useTaskTarget()
  const outcome = !f.held && !f.decision && target ? <TaskButton f={f} target={target} /> : <Outcome f={f} />
  return (
    <li className="[&+&]:border-t">
      <div className="flex min-h-14 items-center gap-3 px-4 py-2 transition-colors hover:bg-hover/50">
        <button type="button" aria-expanded={open} onClick={onToggle} className="grid min-w-0 flex-1 grid-cols-[92px_minmax(0,1fr)] items-center gap-x-4 py-0.5 text-left">
          <SeverityPill severity={f.severity} />
          <span className="flex min-w-0 flex-col gap-0.5">
            <span className="flex min-w-0 items-center gap-2">
              <span className="truncate text-base font-medium text-fg" title={f.title}>{f.title}</span>
              {f.new && <span className="shrink-0 rounded-[5px] bg-success-soft px-1.5 text-xs font-medium text-success-fg">{t('blocks.findingsNew')}</span>}
              {f.policy && <span className="hidden shrink-0 rounded-[5px] bg-inset px-1.5 text-xs text-fg2 md:inline">{t('blocks.findingsPolicy')}</span>}
            </span>
            <Meta f={f} />
          </span>
        </button>
        <span className="flex w-[188px] shrink-0 justify-end">
          {onPick && pick ? <Select size="sm" label={t('blocks.findingsChoose', { title: f.title })} value={pick} options={picks} onValueChange={(v) => onPick(v as Pick)} /> : outcome}
        </span>
        <button type="button" aria-label={t(open ? 'blocks.findingsCollapse' : 'blocks.findingsExpand')} onClick={onToggle} className="flex size-7 shrink-0 items-center justify-center rounded-control-sm text-faint hover:bg-hover hover:text-fg2">
          <Icon name="down" size={14} className={cn('transition-transform duration-150', open && 'rotate-180')} />
        </button>
      </div>
      <AnimatePresence initial={false}>{open && <Detail f={f} />}</AnimatePresence>
    </li>
  )
}

function Totals({ findings }: { findings: Finding[] }) {
  const t = useT()
  const levels = [5, 4, 3, 2, 0]
  const count = (n: number) => findings.filter((f) => (n === 0 ? Math.round(f.severity) <= 1 : Math.round(f.severity) === n)).length
  return (
    <div className="grid grid-cols-3 gap-2 sm:grid-cols-5">
      {levels.map((n) => {
        const c = count(n)
        const s = level(n)
        return (
          <div key={n} className={cn('flex flex-col gap-0.5 rounded-card px-3.5 py-3', c > 0 ? s.tile : 'bg-inset text-faint')}>
            <span className="text-xl font-semibold tabular-nums">{c}</span>
            <span className="text-sm font-medium opacity-90">{t(n === 0 ? 'blocks.findingsSevLow' : s.label)}</span>
          </div>
        )
      })}
    </div>
  )
}

export function SourceCard({ connection, title, when, url }: { connection: string; title: string; when: string | null; url: string | null }) {
  const t = useT()
  const provider = useProviderOf(connection)
  const name = provider?.name ?? t('blocks.findingsScanner')
  return (
    <div className="flex items-center gap-3 rounded-card bg-surface px-4 py-3 shadow-card">
      {provider ? <LogoTile logo={provider.logo} name={provider.name} size={34} /> : <Icon name="shield" size={20} className="text-muted" />}
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-base font-medium text-fg">{name} · {title}</span>
        {when && <span className="text-sm text-muted">{when}</span>}
      </div>
      {url && <a href={url} target="_blank" rel="noreferrer" className="inline-flex shrink-0 items-center gap-1 text-sm text-accent-fg hover:underline">{t('blocks.findingsOpenIn', { name })}<Icon name="arrowUpRight" size={13} /></a>}
    </div>
  )
}

function Tally({ picks }: { picks: Pick[] }) {
  const t = useT()
  const fix = picks.filter((p) => p === 'fix').length
  const task = picks.filter((p) => p === 'task').length
  const ignore = picks.filter((p) => p.startsWith('ignore')).length
  const parts = [
    fix > 0 && <span key="f" className="font-medium text-fg">{t('blocks.findingsCountFix', { n: fix })}</span>,
    task > 0 && <span key="t">{t('blocks.findingsCountTask', { n: task })}</span>,
    ignore > 0 && <span key="i">{t('blocks.findingsCountIgnore', { n: ignore })}</span>,
  ].filter(Boolean)
  return <p className="min-w-0 flex-1 text-base text-fg2">{parts.map((p, i) => <span key={i}>{i > 0 && ' · '}{p}</span>)}</p>
}

function Triage({ block, picks, onDone }: { block: Of<'findings'>; picks: Record<string, Pick>; onDone: () => void }) {
  const t = useT()
  const { busy, error, send } = useSend()
  const confirm = () => {
    const decisions = Object.fromEntries(Object.entries(picks).map(([id, p]) => [id, decisionOf(p)]))
    void send(block.triage ?? 'findings.triage', { decisions }).then(onDone)
  }
  return (
    <div className="sticky bottom-0 z-10 -mx-1 rounded-card bg-surface px-4 py-3 shadow-pop">
      <div className="flex flex-wrap items-center gap-3">
        <Tally picks={Object.values(picks)} />
        <Button variant="primary" loading={busy} icon={<Icon name="check" size={15} />} onClick={confirm}>{t('blocks.findingsConfirm')}</Button>
      </div>
      <SendError error={error} />
    </div>
  )
}

function Clean({ block }: { block: Of<'findings'> }) {
  const t = useT()
  return (
    <div className="flex flex-col gap-4">
      <SourceCard connection={block.connection} title={block.project} when={block.scannedAt ? t('blocks.findingsScanned', { time: ago(block.scannedAt) }) : null} url={block.url} />
      <div className="flex flex-col items-center gap-3 rounded-card bg-surface px-6 py-10 text-center shadow-card">
        <span className="flex size-11 items-center justify-center rounded-[12px] bg-success-soft text-success-fg"><Icon name="hSecurityCheck" size={22} /></span>
        <h3 className="text-md font-medium text-fg">{t('blocks.findingsNoneTitle')}</h3>
        <p className="max-w-[48ch] text-base text-muted">{t('blocks.findingsNoneText')}</p>
      </div>
    </div>
  )
}

function initial(findings: Finding[]): Record<string, Pick> {
  return Object.fromEntries(findings.filter((f) => !f.held).map((f) => [f.id, f.decision ? pickOf(f.decision.choice, f.decision.reason) : pickOf(f.suggested, null)]))
}

export function FindingsBlock({ block }: { block: Of<'findings'> }) {
  const t = useT()
  const picksList = usePicks()
  const [open, setOpen] = useState<string | null>(null)
  const [picks, setPicks] = useState<Record<string, Pick>>(() => initial(block.findings))
  const [sent, setSent] = useState(false)
  if (block.findings.length === 0) return <Clean block={block} />
  const editable = !!block.triage && !sent
  const sorted = [...block.findings].sort((a, b) => b.severity - a.severity)
  const setAll = (ids: string[], p: Pick) => setPicks((now) => ({ ...now, ...Object.fromEntries(ids.map((id) => [id, p])) }))
  const decided = block.findings.filter((f) => f.decision).map((f) => pickOf(f.decision!.choice, f.decision!.reason))
  const held = block.findings.filter((f) => f.held).length
  return (
    <div className="flex flex-col gap-5">
      <SourceCard connection={block.connection} title={block.project} when={block.scannedAt ? t('blocks.findingsScanned', { time: ago(block.scannedAt) }) : null} url={block.url} />
      <Totals findings={block.findings} />
      {editable && (
        <div className="flex items-start gap-2.5 rounded-card bg-info-soft px-4 py-3 text-base text-info-fg">
          <Icon name="info" size={16} className="mt-0.5 shrink-0" />
          <p>{t('blocks.findingsTriageHint')}</p>
        </div>
      )}
      {KINDS.map((kind) => {
        const listed = sorted.filter((f) => f.kind === kind)
        if (listed.length === 0) return null
        const open_ = listed.filter((f) => !f.held).map((f) => f.id)
        return (
          <section key={kind}>
            <div className="mb-3 flex items-center gap-2">
              <h3 className="text-md font-medium text-fg2">{t(KIND_LABEL[kind])}</h3>
              <span className="text-sm text-muted">{t('blocks.findingsOpenCount', { n: listed.length })}</span>
              {editable && open_.length > 1 && (
                <span className="ml-auto"><Select size="sm" label={t('blocks.findingsSetAll')} placeholder={t('blocks.findingsSetAll')} value="" options={picksList} onValueChange={(v) => setAll(open_, v as Pick)} /></span>
              )}
            </div>
            <ul className="rounded-card bg-surface shadow-card">
              {listed.map((f) => (
                <Row key={f.id} f={f} open={open === f.id} onToggle={() => setOpen(open === f.id ? null : f.id)} pick={picks[f.id] ?? null} onPick={editable && !f.held ? (p) => setPicks((now) => ({ ...now, [f.id]: p })) : null} />
              ))}
            </ul>
          </section>
        )
      })}
      {editable ? (
        <Triage block={block} picks={picks} onDone={() => setSent(true)} />
      ) : decided.length === 0 && held === 0 ? null : (
        <div className="flex items-start gap-2.5 rounded-card bg-inset px-4 py-3 text-base text-fg2">
          <Icon name="info" size={16} className="mt-0.5 shrink-0 text-muted" />
          <Tally picks={decided} />
          {held > 0 && <span className="text-muted">{t('blocks.findingsHeldCount', { n: held })}</span>}
        </div>
      )}
    </div>
  )
}
