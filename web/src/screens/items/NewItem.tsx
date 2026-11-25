import { AnimatePresence, motion } from 'framer-motion'
import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { api } from '@/api'
import type { Application, CodeBranch, Connection, IssueSummary, Item, NewGit, NewSource, OpenPull, Provider } from '@/api/contract'
import { errorText, setItems, useStore } from '@/api/store'
import { Composer, type ComposerImport } from '@/chat/Composer'
import { useRememberedModel, useRememberedTuning } from '@/chat/remembered'
import { currentLanguage, translate, useT } from '@/i18n'
import { Button, IconButton } from '@/ui/button'
import { ApplicationDialog } from '../settings/Applications'
import { StatusIcon, ProviderLogo } from '@/ui/domain'
import { Input, SearchInput } from '@/ui/form'
import { CHIP, ChipPicker } from '@/ui/chipPicker'
import { cn } from '@/ui/cn'
import { Icon } from '@/ui/icon'
import { tween } from '@/ui/motion'
import { Popover } from '@/ui/overlay'
import { configuredSpaces, rememberApplication, rememberedApplication, searchTargets, type Space } from './projects'

interface Source {
  connection: Connection
  provider: Provider
}

interface Picked extends IssueSummary {
  source: Source
}

export function useTrackers(): Source[] {
  const connections = useStore((s) => s.connections)
  return useMemo(
    () =>
      (connections?.connections ?? []).flatMap((c) => {
        const provider = connections?.providers.find((p) => p.key === c.provider)
        return c.capabilities.includes('tracker') && provider ? [{ connection: c, provider }] : []
      }),
    [connections],
  )
}

function ApplicationMenu({ applications, chosen, onChoose, onAdd }: { applications: Application[]; chosen: Application | null; onChoose: (application: Application) => void; onAdd: () => void }) {
  const t = useT()
  const options = applications.map((a) => ({ value: a.id, label: a.name, icon: <Icon name="apps" size={14} className="text-muted" /> }))
  const add = { key: 'add', label: t('items.addApplication'), icon: <Icon name="plus" size={15} />, shortcut: 'N', onSelect: onAdd }
  return (
    <ChipPicker
      label={chosen ? t('items.applicationNamed', { name: chosen.name }) : t('items.application')}
      placeholder={`${t('items.application')}…`}
      empty={t('items.application')}
      leading={<Icon name="apps" size={14} className="text-muted" />}
      className="max-w-[200px]"
      value={chosen?.id ?? ''}
      options={options}
      actions={[add]}
      onChange={(id) => {
        const picked = applications.find((a) => a.id === id)
        if (picked) onChoose(picked)
      }}
    />
  )
}
function PickerCard({ label, children }: { label: string; children: ReactNode }) {
  return (
    <motion.div role="dialog" aria-label={label} initial={{ opacity: 0, y: 4 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 4 }} transition={tween} className="rounded-card bg-surface p-1.5 shadow-pop">
      {children}
    </motion.div>
  )
}

function PickerSearch({ value, onChange, placeholder, onClose }: { value: string; onChange: (q: string) => void; placeholder: string; onClose: () => void }) {
  const t = useT()
  return (
    <div className="flex items-center gap-1 p-1">
      <div className="min-w-0 flex-1">
        <SearchInput autoFocus value={value} onChange={(e) => onChange(e.target.value)} onKeyDown={(e) => e.key === 'Escape' && onClose()} placeholder={placeholder} />
      </div>
      <IconButton tooltip={t('items.close')} size="sm" onClick={onClose}><Icon name="close" size={15} /></IconButton>
    </div>
  )
}

function IssuePanel({ sources, spaces, onPick, onClose }: { sources: Source[]; spaces: Space[]; onPick: (issue: Picked) => void; onClose: () => void }) {
  const [q, setQ] = useState('')
  const [found, setFound] = useState<Picked[]>([])
  const [error, setError] = useState<string | null>(null)
  const tr = useT()
  useEffect(() => {
    let alive = true
    const t = setTimeout(() => {
      const targets = searchTargets(sources.map((s) => s.connection.id), spaces, q)
      Promise.all(
        targets.map((target) => {
          const source = sources.find((s) => s.connection.id === target.connection)
          return api().call(target.connection, 'tracker.search', target.args).then((list) => (source ? list.map((i) => ({ ...i, source })) : []))
        }),
      )
        .then((lists) => alive && (setFound(lists.flat()), setError(null)))
        .catch((e: unknown) => alive && setError(errorText(e)))
    }, 150)
    return () => {
      alive = false
      clearTimeout(t)
    }
  }, [q, sources, spaces])
  const first = sources[0]
  if (!first) return null
  return (
    <PickerCard label={tr('items.importIssue', { provider: first.provider.name })}>
      <PickerSearch value={q} onChange={setQ} onClose={onClose} placeholder={tr('items.searchIssue', { provider: first.provider.name })} />
      <ul className="mt-1 max-h-[280px] overflow-y-auto">
        {found.map((i) => (
          <li key={`${i.source.connection.id}:${i.key}`}>
            <button onClick={() => onPick(i)} className="flex h-10 w-full items-center gap-3 rounded-control px-2.5 text-left transition-colors hover:bg-hover">
              <StatusIcon status="todo" size={15} />
              <span className="min-w-[72px] shrink-0 whitespace-nowrap text-base tabular-nums text-fg2">{i.key}</span>
              <span className="min-w-0 flex-1 truncate text-base text-fg">{i.title}</span>
            </button>
          </li>
        ))}
        {found.length === 0 && <li className="px-2.5 py-3 text-base text-faint">{error ?? tr('items.noIssues')}</li>}
      </ul>
    </PickerCard>
  )
}

export interface PickedPull {
  subject: string
  connection: string | null
  title?: string
}

const PULL_URL = /^https?:\/\/[^/]+\/(.+?)\/(?:pull|pulls|pull-requests|merge_requests)\/(\d+)/

export function pullSubject(text: string, application: Application | null): string | null {
  const trimmed = text.trim().replace(/^#/, '')
  if (!trimmed) return null
  const url = PULL_URL.exec(trimmed)
  if (url) return `${url[1].replace(/\/-$/, '')}#${url[2]}`
  if (/^\d+$/.test(trimmed)) {
    const repos = linkedRepos(application).map((r) => r.repo)
    return repos.length === 1 ? `${repos[0]}#${trimmed}` : trimmed
  }
  return trimmed
}

export function useCodeConnections(): Connection[] {
  const connections = useStore((s) => s.connections)
  return useMemo(() => (connections?.connections ?? []).filter((c) => c.capabilities.includes('code')), [connections])
}

export interface Linked {
  place: string
  connection: string
  repo: string
}

export function linkedRepos(application: Application | null): Linked[] {
  return (application?.repositories ?? []).flatMap((r) => {
    const code = r.bindings?.code
    return code ? [{ place: r.id, connection: code.connection, repo: code.value }] : []
  })
}

interface Listed {
  pull: OpenPull
  connection: string
}

export function matchesPull(pull: OpenPull, q: string): boolean {
  const wanted = q.trim().toLowerCase()
  return !wanted || [pull.key, pull.title, pull.head, pull.author].some((field) => field.toLowerCase().includes(wanted))
}

function useOpenPulls(linked: Linked[]): { listed: Listed[]; error: string | null; loading: boolean } {
  const [state, setState] = useState<{ listed: Listed[]; error: string | null; loading: boolean }>({ listed: [], error: null, loading: true })
  useEffect(() => {
    let alive = true
    setState({ listed: [], error: null, loading: true })
    Promise.all(linked.map((l) => api().call(l.connection, 'code.pulls', { repo: l.repo }).then((pulls) => pulls.map((pull) => ({ pull, connection: l.connection })))))
      .then((lists) => alive && setState({ listed: lists.flat(), error: null, loading: false }))
      .catch((e: unknown) => alive && setState({ listed: [], error: errorText(e), loading: false }))
    return () => {
      alive = false
    }
  }, [linked])
  return state
}

function PullPanel({ application, onPick, onClose }: { application: Application | null; onPick: (pull: PickedPull) => void; onClose: () => void }) {
  const [q, setQ] = useState('')
  const t = useT()
  const code = useCodeConnections()
  const linked = useMemo(() => linkedRepos(application), [application])
  const { listed, error, loading } = useOpenPulls(linked)
  const shown = listed.filter((l) => matchesPull(l.pull, q))
  const typed = pullSubject(q, application)
  const manual = typed && typed.includes('#') && !shown.some((l) => l.pull.key === typed) ? typed : null
  const empty = error ?? (loading ? t('items.loadingPulls') : linked.length === 0 ? t('items.noLinkedRepository') : t('items.noPulls'))
  return (
    <PickerCard label={t('items.importPull')}>
      <PickerSearch value={q} onChange={setQ} onClose={onClose} placeholder={t('items.searchPull')} />
      <ul className="mt-1 max-h-[280px] overflow-y-auto">
        {shown.map((l) => (
          <li key={l.pull.key}>
            <button onClick={() => onPick({ subject: l.pull.key, connection: l.connection, title: l.pull.title })} className="flex h-10 w-full items-center gap-3 rounded-control px-2.5 text-left transition-colors hover:bg-hover">
              <Icon name="hGitPullRequest" size={15} className={l.pull.draft ? 'text-faint' : 'text-success'} />
              <span className="shrink-0 text-base tabular-nums text-fg2">#{l.pull.number}</span>
              <span className="min-w-0 flex-1 truncate text-base text-fg">{l.pull.title}</span>
              <span className="hidden max-w-[140px] shrink-0 truncate text-sm text-muted sm:block">{l.pull.head}</span>
            </button>
          </li>
        ))}
        {manual && (
          <li>
            <button onClick={() => onPick({ subject: manual, connection: linked[0]?.connection ?? (code.length === 1 ? code[0].id : null) })} className="flex h-10 w-full items-center gap-3 rounded-control px-2.5 text-left transition-colors hover:bg-hover">
              <Icon name="link" size={15} className="text-muted" />
              <span className="min-w-0 flex-1 truncate text-base text-fg">{t('items.useSubject', { subject: manual })}</span>
            </button>
          </li>
        )}
        {shown.length === 0 && !manual && <li className="px-2.5 py-3 text-base text-faint">{empty}</li>}
      </ul>
    </PickerCard>
  )
}

interface Place {
  id: string
  name: string
}

export function placesOf(application: Application | null): Place[] {
  if (!application) return [{ id: '', name: translate(currentLanguage(), 'items.repository') }]
  return application.repositories.map((r) => ({ id: r.id, name: r.bindings?.code?.value ?? r.directory.split('/').filter(Boolean).pop() ?? r.directory }))
}

export function gitOf(bases: Record<string, string>, places: Place[]): NewGit | undefined {
  const entries = places.flatMap((p) => {
    const base = bases[p.id]?.trim()
    return base ? [[p.id, { base }] as const] : []
  })
  return entries.length ? Object.fromEntries(entries) : undefined
}

export function defaultBranch(list: CodeBranch[] | undefined): string | null {
  if (!list?.length) return null
  return list.find((b) => b.default)?.name ?? list[0].name
}

export function basesOf(chosen: Record<string, string>, branches: Record<string, CodeBranch[]>, places: Place[]): Record<string, string> {
  return Object.fromEntries(places.map((p) => [p.id, chosen[p.id]?.trim() || defaultBranch(branches[p.id]) || '']))
}

function useBranches(linked: Linked[]): Record<string, CodeBranch[]> {
  const [branches, setBranches] = useState<Record<string, CodeBranch[]>>({})
  useEffect(() => {
    let alive = true
    setBranches({})
    for (const l of linked) {
      api()
        .call(l.connection, 'code.branches', { repo: l.repo })
        .then((list) => alive && setBranches((had) => ({ ...had, [l.place]: list })))
        .catch(() => undefined)
    }
    return () => {
      alive = false
    }
  }, [linked])
  return branches
}

function BranchList({ place, list, q, value, onChoose }: { place: Place; list: CodeBranch[]; q: string; value: string; onChoose: (name: string) => void }) {
  const t = useT()
  const shown = list.filter((b) => b.name.toLowerCase().includes(q.trim().toLowerCase()))
  return (
    <ul className="max-h-[240px] overflow-y-auto" aria-label={t('items.branchesOf', { name: place.name })}>
      {shown.map((b) => (
        <li key={b.name}>
          <button onClick={() => onChoose(b.name)} className="flex h-9 w-full items-center gap-2.5 rounded-control-sm px-2.5 text-left text-base text-fg2 transition-colors hover:bg-hover hover:text-fg">
            <Icon name="branch" size={14} className="shrink-0 text-muted" />
            <span className="min-w-0 flex-1 truncate">{b.name}</span>
            {b.name === value && <Icon name="check" size={15} className="shrink-0 text-fg" />}
          </button>
        </li>
      ))}
      {shown.length === 0 && <li className="px-2.5 py-2 text-sm text-faint">{t('items.noBranches')}</li>}
    </ul>
  )
}

function BasePicker({ places, branches, bases, effective, onChange }: { places: Place[]; branches: Record<string, CodeBranch[]>; bases: Record<string, string>; effective: Record<string, string>; onChange: (next: Record<string, string>) => void }) {
  const [open, setOpen] = useState(false)
  const [q, setQ] = useState('')
  const t = useT()
  const names = [...new Set(places.map((p) => effective[p.id]).filter(Boolean))]
  const label = names.length > 1 ? t('items.source_branch') : names[0] ?? t('items.defaultBase')
  const described = places.length > 1 ? places.map((p) => `${p.name} ${effective[p.id] || t('items.defaultBase')}`).join(', ') : label
  const listed = places.some((p) => branches[p.id])
  const choose = (place: string, name: string) => {
    onChange({ ...bases, [place]: name })
    if (places.length === 1) setOpen(false)
  }
  const only = places.length === 1 ? places[0] : null
  const onlyList = only ? branches[only.id] : undefined
  if (only && onlyList) {
    const options = onlyList.map((b) => ({ value: b.name, label: b.name, icon: <Icon name="branch" size={14} className="text-muted" /> }))
    return (
      <ChipPicker
        label={t('items.baseBranch', { name: label })}
        placeholder={t('items.searchBranch')}
        empty={t('items.defaultBase')}
        leading={<Icon name="branch" size={14} className="text-muted" />}
        className="max-w-[200px]"
        side="top"
        value={effective[only.id] ?? ''}
        options={options}
        onChange={(name) => onChange({ ...bases, [only.id]: name })}
      />
    )
  }
  return (
    <Popover
      open={open}
      onOpenChange={(o) => (setOpen(o), setQ(''))}
      side="top"
      align="start"
      className="w-[320px] max-w-[calc(100vw-24px)] space-y-1 p-1.5"
      trigger={
        <button aria-label={t('items.baseBranch', { name: described })} title={described} className={cn(CHIP, 'max-w-[200px]')}>
          <Icon name="branch" size={14} className="shrink-0 text-muted" />
          <span className="truncate">{label}</span>
        </button>
      }
    >
      {listed && <div className="p-1"><SearchInput autoFocus value={q} onChange={(e) => setQ(e.target.value)} placeholder={t('items.searchBranch')} /></div>}
      {places.map((p) => (
        <div key={p.id} className="space-y-1">
          {places.length > 1 && <p className="truncate px-2.5 pt-1.5 text-xs text-muted">{p.name}</p>}
          {branches[p.id] ? (
            <BranchList place={p} list={branches[p.id]} q={q} value={effective[p.id] ?? ''} onChoose={(name) => choose(p.id, name)} />
          ) : (
            <div className="p-1"><Input aria-label={t('items.baseBranchOf', { name: p.name })} value={bases[p.id] ?? ''} onChange={(e) => onChange({ ...bases, [p.id]: e.target.value })} placeholder={t('items.repositoryDefault')} /></div>
          )}
        </div>
      ))}
    </Popover>
  )
}

export function sourcesOf(issue: { connection: string; key: string } | null, pull: PickedPull | null): NewSource[] {
  const sources: NewSource[] = []
  if (pull) sources.push({ kind: 'pull_request', connection: pull.connection, subject: pull.subject })
  if (issue) sources.push({ kind: 'issue', connection: issue.connection, subject: issue.key })
  return sources
}

function PickedCards({ issue, pull, busy, onStart, onIssue, onPull }: { issue: Picked | null; pull: PickedPull | null; busy: boolean; onStart: () => void; onIssue: () => void; onPull: () => void }) {
  const t = useT()
  return (
    <AnimatePresence initial={false}>
      {issue && (
        <motion.div initial={{ opacity: 0, y: -4 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -4 }} transition={tween} className="flex items-center gap-2.5 rounded-card bg-surface px-3 py-2 shadow-card">
          <ProviderLogo logo={issue.source.provider.logo} name={issue.source.provider.name} size={15} />
          <span className="shrink-0 whitespace-nowrap text-base text-fg2">{issue.key}</span>
          <span className="min-w-0 flex-1 truncate text-base text-fg">{issue.title}</span>
          <Button size="sm" variant="primary" loading={busy} icon={<Icon name="sPlan" size={15} />} onClick={onStart}>{t('items.plan')}</Button>
          <IconButton tooltip={t('items.removeIssue')} size="sm" onClick={onIssue}><Icon name="close" size={15} /></IconButton>
        </motion.div>
      )}
      {pull && (
        <motion.div initial={{ opacity: 0, y: -4 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -4 }} transition={tween} className="flex items-center gap-2.5 rounded-card bg-surface px-3 py-2 shadow-card">
          <Icon name="hGitPullRequest" size={15} className="text-fg2" />
          <span className="shrink-0 text-base text-fg2">{pull.subject}</span>
          {pull.title && <span className="min-w-0 flex-1 truncate text-base text-fg">{pull.title}</span>}
          {!pull.title && <span className="flex-1" />}
          <Button size="sm" variant="primary" loading={busy} icon={<Icon name="sCode" size={15} />} onClick={onStart}>{t('items.start')}</Button>
          <IconButton tooltip={t('items.removePull')} size="sm" onClick={onPull}><Icon name="close" size={15} /></IconButton>
        </motion.div>
      )}
    </AnimatePresence>
  )
}

function useImports(sources: Source[], onPick: (what: 'issue' | 'pull') => void): ComposerImport[] {
  const code = useCodeConnections()
  const t = useT()
  const first = sources[0]
  const found: ComposerImport[] = []
  if (first) found.push({ key: 'issue', label: t('items.importIssue', { provider: first.provider.name }), icon: <ProviderLogo logo={first.provider.logo} name={first.provider.name} size={16} />, onSelect: () => onPick('issue') })
  if (code.length > 0) found.push({ key: 'pull', label: t('items.importPull'), icon: <Icon name="hGitPullRequest" size={16} />, onSelect: () => onPick('pull') })
  return found
}

export function NewItem({ onCreated, onAsked }: { onCreated: (it: Item) => void; onAsked?: (it: Item) => void }) {
  const [busy, setBusy] = useState(false)
  const [issue, setIssue] = useState<Picked | null>(null)
  const [pull, setPull] = useState<PickedPull | null>(null)
  const [picking, setPicking] = useState<'issue' | 'pull' | null>(null)
  const [adding, setAdding] = useState(false)
  const [bases, setBases] = useState<Record<string, string>>({})
  const [error, setError] = useState<string | null>(null)
  const t = useT()
  const [remembered, setModel] = useRememberedModel()
  const sources = useTrackers()
  const imports = useImports(sources, setPicking)
  const applications = useStore((s) => s.applications)
  const [chosenId, setChosenId] = useState<string | null>(() => rememberedApplication())
  const chosen = useMemo(() => {
    const listed = applications ?? []
    return listed.find((a) => a.id === chosenId) ?? listed[0] ?? null
  }, [applications, chosenId])
  const choose = (next: Application) => {
    setChosenId(next.id)
    setBases({})
    setPull(null)
    rememberApplication(next.id)
  }
  const places = useMemo(() => placesOf(chosen), [chosen])
  const linked = useMemo(() => linkedRepos(chosen), [chosen])
  const branches = useBranches(linked)
  const effective = basesOf(bases, branches, places)
  const spaces = useMemo(() => configuredSpaces(chosen ? [chosen] : [], sources.map((s) => s.connection.id)), [chosen, sources])
  const models = useStore((s) => s.models)
  const defaultModel = models?.default ?? null
  const model = remembered && models?.models.some((m) => m.id === remembered) ? remembered : null
  const [tuning, setTuning] = useRememberedTuning({ effort: 'medium', fast: false })
  const start = (text: string, attachments: string[]) => {
    if (!issue && !pull && !text) return
    const chosenModel = model ?? defaultModel
    if (!chosenModel) return
    setBusy(true)
    setError(null)
    const picked = issue ? { connection: issue.source.connection.id, key: issue.key } : null
    const git = pull ? undefined : gitOf(bases, places)
    api()
      .createItem({ application: chosen?.id ?? null, text, issue: picked, sources: sourcesOf(picked, pull), git, model: chosenModel, ...tuning, attachments })
      .then((it) => {
        setItems((items) => (items.some((x) => x.id === it.id) ? items : [it, ...items]))
        setIssue(null)
        setPull(null)
        setBases({})
        onCreated(it)
      })
      .catch((e: unknown) => setError(errorText(e)))
      .finally(() => setBusy(false))
  }
  const ask = (text: string) => {
    setBusy(true)
    setError(null)
    api()
      .btw({ text, application: chosen?.id ?? null, model: model ?? defaultModel })
      .then((it) => {
        setItems((items) => (items.some((x) => x.id === it.id) ? items : [it, ...items]))
        onCreated(it)
        onAsked?.(it)
      })
      .catch((e: unknown) => setError(errorText(e)))
      .finally(() => setBusy(false))
  }
  return (
    <div className="mb-8 space-y-2">
      <PickedCards issue={issue} pull={pull} busy={busy} onStart={() => start('', [])} onIssue={() => setIssue(null)} onPull={() => setPull(null)} />
      <AnimatePresence initial={false}>
        {picking === 'issue' && <IssuePanel key="issue" sources={sources} spaces={spaces} onPick={(i) => (setIssue(i), setPicking(null))} onClose={() => setPicking(null)} />}
        {picking === 'pull' && <PullPanel key="pull" application={chosen} onPick={(p) => (setPull(p), setPicking(null))} onClose={() => setPicking(null)} />}
      </AnimatePresence>
      <Composer
        autoFocus
        placeholder={issue ? t('items.instructionsIssue', { key: issue.key }) : pull ? t('items.instructionsPull', { subject: pull.subject }) : t('items.whatToDo')}
        disabled={busy}
        allowEmpty={!!issue || !!pull}
        model={model}
        onModel={setModel}
        tuning={tuning}
        onTuning={setTuning}
        imports={imports}
        leading={
          <>
            <ApplicationMenu applications={applications ?? []} chosen={chosen} onChoose={choose} onAdd={() => setAdding(true)} />
            {!pull && <BasePicker places={places} branches={branches} bases={bases} effective={effective} onChange={setBases} />}
          </>
        }
        onSubmit={(input) => start(input.text, input.attachments)}
        onBtw={ask}
      />
      {error && <p role="alert" className="px-2 text-sm text-danger-fg">{error}</p>}
      {adding && <ApplicationDialog open application={null} onClose={() => setAdding(false)} />}
    </div>
  )
}
