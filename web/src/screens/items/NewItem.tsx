import { AnimatePresence, motion } from 'framer-motion'
import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { api } from '@/api'
import type { Application, CodeBranch, Connection, IssueSummary, Item, NewGit, NewSource, OpenPull, Provider } from '@/api/contract'
import { errorText, setItems, useStore } from '@/api/store'
import { Composer, type ComposerImport } from '@/chat/Composer'
import { useRememberedModel, useRememberedTuning } from '@/chat/remembered'
import { BtwChat, useBtw } from '@/chat/BtwChat'
import { Button, IconButton } from '@/ui/button'
import { ApplicationDialog } from '../settings/Applications'
import { StatusIcon, ProviderLogo } from '@/ui/domain'
import { Input, SearchInput } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { tween } from '@/ui/motion'
import { Menu, MenuContent, MenuItem, MenuLabel, MenuSeparator, MenuTrigger, Popover } from '@/ui/overlay'
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

const CHIP = 'flex h-8 min-w-0 max-w-[200px] items-center gap-1.5 rounded-full px-2.5 text-base text-fg2 transition-colors hover:bg-inset hover:text-fg data-[state=open]:bg-inset data-[state=open]:text-fg'

function ApplicationMenu({ applications, chosen, onChoose, onAdd }: { applications: Application[]; chosen: Application | null; onChoose: (application: Application) => void; onAdd: () => void }) {
  return (
    <Menu>
      <MenuTrigger asChild>
        <button aria-label={chosen ? `Aplicação: ${chosen.name}` : 'Aplicação'} className={CHIP}>
          <Icon name="apps" size={15} className="shrink-0 text-muted" />
          <span className="truncate">{chosen?.name ?? 'Aplicação'}</span>
          <Icon name="down" size={13} className="shrink-0 text-muted" />
        </button>
      </MenuTrigger>
      <MenuContent align="start" className="max-h-[320px] overflow-y-auto">
        {applications.length > 0 && <MenuLabel>Aplicação</MenuLabel>}
        {applications.map((a) => (
          <MenuItem key={a.id} icon={<Icon name="apps" size={15} />} onSelect={() => onChoose(a)} right={a.id === chosen?.id ? <Icon name="check" size={15} /> : undefined}>
            {a.name}
          </MenuItem>
        ))}
        {applications.length > 0 && <MenuSeparator />}
        <MenuItem icon={<Icon name="plus" size={15} />} onSelect={onAdd}>Adicionar aplicação</MenuItem>
      </MenuContent>
    </Menu>
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
  return (
    <div className="flex items-center gap-1 p-1">
      <div className="min-w-0 flex-1">
        <SearchInput autoFocus value={value} onChange={(e) => onChange(e.target.value)} onKeyDown={(e) => e.key === 'Escape' && onClose()} placeholder={placeholder} />
      </div>
      <IconButton tooltip="Fechar" size="sm" onClick={onClose}><Icon name="close" size={15} /></IconButton>
    </div>
  )
}

function IssuePanel({ sources, spaces, onPick, onClose }: { sources: Source[]; spaces: Space[]; onPick: (issue: Picked) => void; onClose: () => void }) {
  const [q, setQ] = useState('')
  const [found, setFound] = useState<Picked[]>([])
  const [error, setError] = useState<string | null>(null)
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
    <PickerCard label={`Importar issue do ${first.provider.name}`}>
      <PickerSearch value={q} onChange={setQ} onClose={onClose} placeholder={`Buscar issue do ${first.provider.name}`} />
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
        {found.length === 0 && <li className="px-2.5 py-3 text-base text-faint">{error ?? 'Nenhuma issue encontrada.'}</li>}
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
    const repos = (application?.repositories ?? []).flatMap((r) => (r.code ? [r.code.repo] : []))
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
  return (application?.repositories ?? []).flatMap((r) => (r.code ? [{ place: r.id, connection: r.code.connection, repo: r.code.repo }] : []))
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
  const code = useCodeConnections()
  const linked = useMemo(() => linkedRepos(application), [application])
  const { listed, error, loading } = useOpenPulls(linked)
  const shown = listed.filter((l) => matchesPull(l.pull, q))
  const typed = pullSubject(q, application)
  const manual = typed && typed.includes('#') && !shown.some((l) => l.pull.key === typed) ? typed : null
  const empty = error ?? (loading ? 'Carregando pull requests…' : linked.length === 0 ? 'A aplicação não tem repositório conectado. Cole o link do pull request.' : 'Nenhum pull request aberto.')
  return (
    <PickerCard label="Importar pull request">
      <PickerSearch value={q} onChange={setQ} onClose={onClose} placeholder="Buscar pull request aberto ou colar o link" />
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
              <span className="min-w-0 flex-1 truncate text-base text-fg">Usar {manual}</span>
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
  if (!application) return [{ id: '', name: 'Repositório' }]
  return application.repositories.map((r) => ({ id: r.id, name: r.code?.repo ?? r.directory.split('/').filter(Boolean).pop() ?? r.directory }))
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
  return list.find((b) => b.name === 'main')?.name ?? list.find((b) => b.default)?.name ?? list[0].name
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
  const shown = list.filter((b) => b.name.toLowerCase().includes(q.trim().toLowerCase()))
  return (
    <ul className="max-h-[240px] overflow-y-auto" aria-label={`Branches de ${place.name}`}>
      {shown.map((b) => (
        <li key={b.name}>
          <button onClick={() => onChoose(b.name)} className="flex h-9 w-full items-center gap-2.5 rounded-control-sm px-2.5 text-left text-base text-fg2 transition-colors hover:bg-hover hover:text-fg">
            <Icon name="branch" size={14} className="shrink-0 text-muted" />
            <span className="min-w-0 flex-1 truncate">{b.name}</span>
            {b.name === value && <Icon name="check" size={15} className="shrink-0 text-fg" />}
          </button>
        </li>
      ))}
      {shown.length === 0 && <li className="px-2.5 py-2 text-sm text-faint">Nenhum branch encontrado.</li>}
    </ul>
  )
}

function BasePicker({ places, branches, bases, effective, onChange }: { places: Place[]; branches: Record<string, CodeBranch[]>; bases: Record<string, string>; effective: Record<string, string>; onChange: (next: Record<string, string>) => void }) {
  const [open, setOpen] = useState(false)
  const [q, setQ] = useState('')
  const label = places.map((p) => effective[p.id]).find(Boolean) ?? 'padrão'
  const listed = places.some((p) => branches[p.id])
  const choose = (place: string, name: string) => {
    onChange({ ...bases, [place]: name })
    if (places.length === 1) setOpen(false)
  }
  return (
    <Popover
      open={open}
      onOpenChange={(o) => (setOpen(o), setQ(''))}
      side="top"
      align="start"
      className="w-[320px] max-w-[calc(100vw-24px)] space-y-1 p-1.5"
      trigger={
        <button aria-label={`Branch base: ${label}`} className={CHIP}>
          <Icon name="branch" size={15} className="shrink-0 text-muted" />
          <span className="truncate">{label}</span>
        </button>
      }
    >
      {listed && <div className="p-1"><SearchInput autoFocus value={q} onChange={(e) => setQ(e.target.value)} placeholder="Buscar branch" /></div>}
      {places.map((p) => (
        <div key={p.id} className="space-y-1">
          {places.length > 1 && <p className="truncate px-2.5 pt-1.5 text-xs text-muted">{p.name}</p>}
          {branches[p.id] ? (
            <BranchList place={p} list={branches[p.id]} q={q} value={effective[p.id] ?? ''} onChoose={(name) => choose(p.id, name)} />
          ) : (
            <div className="p-1"><Input aria-label={`Branch base de ${p.name}`} value={bases[p.id] ?? ''} onChange={(e) => onChange({ ...bases, [p.id]: e.target.value })} placeholder="padrão do repositório" /></div>
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
  return (
    <AnimatePresence initial={false}>
      {issue && (
        <motion.div initial={{ opacity: 0, y: -4 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -4 }} transition={tween} className="flex items-center gap-2.5 rounded-card bg-surface px-3 py-2 shadow-card">
          <ProviderLogo logo={issue.source.provider.logo} name={issue.source.provider.name} size={15} />
          <span className="shrink-0 whitespace-nowrap text-base text-fg2">{issue.key}</span>
          <span className="min-w-0 flex-1 truncate text-base text-fg">{issue.title}</span>
          <Button size="sm" variant="primary" loading={busy} icon={<Icon name="sPlan" size={15} />} onClick={onStart}>Planejar</Button>
          <IconButton tooltip="Remover issue" size="sm" onClick={onIssue}><Icon name="close" size={15} /></IconButton>
        </motion.div>
      )}
      {pull && (
        <motion.div initial={{ opacity: 0, y: -4 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -4 }} transition={tween} className="flex items-center gap-2.5 rounded-card bg-surface px-3 py-2 shadow-card">
          <Icon name="hGitPullRequest" size={15} className="text-fg2" />
          <span className="shrink-0 text-base text-fg2">{pull.subject}</span>
          {pull.title && <span className="min-w-0 flex-1 truncate text-base text-fg">{pull.title}</span>}
          {!pull.title && <span className="flex-1" />}
          <Button size="sm" variant="primary" loading={busy} icon={<Icon name="sCode" size={15} />} onClick={onStart}>Começar</Button>
          <IconButton tooltip="Remover pull request" size="sm" onClick={onPull}><Icon name="close" size={15} /></IconButton>
        </motion.div>
      )}
    </AnimatePresence>
  )
}

function useImports(sources: Source[], onPick: (what: 'issue' | 'pull') => void): ComposerImport[] {
  const code = useCodeConnections()
  const first = sources[0]
  const found: ComposerImport[] = []
  if (first) found.push({ key: 'issue', label: `Importar issue do ${first.provider.name}`, icon: <ProviderLogo logo={first.provider.logo} name={first.provider.name} size={16} />, onSelect: () => onPick('issue') })
  if (code.length > 0) found.push({ key: 'pull', label: 'Importar pull request', icon: <Icon name="hGitPullRequest" size={16} />, onSelect: () => onPick('pull') })
  return found
}

export function NewItem({ onCreated }: { onCreated: (it: Item) => void }) {
  const [busy, setBusy] = useState(false)
  const [issue, setIssue] = useState<Picked | null>(null)
  const [pull, setPull] = useState<PickedPull | null>(null)
  const [picking, setPicking] = useState<'issue' | 'pull' | null>(null)
  const [adding, setAdding] = useState(false)
  const [bases, setBases] = useState<Record<string, string>>({})
  const [error, setError] = useState<string | null>(null)
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
  const btw = useBtw(chosen?.id ?? null, model ?? defaultModel)
  const start = (text: string, attachments: string[]) => {
    if (!issue && !pull && !text) return
    const chosenModel = model ?? defaultModel
    if (!chosenModel) return
    setBusy(true)
    setError(null)
    const picked = issue ? { connection: issue.source.connection.id, key: issue.key } : null
    const git = pull ? undefined : gitOf(effective, places)
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
  return (
    <div className="mb-8 space-y-2">
      <PickedCards issue={issue} pull={pull} busy={busy} onStart={() => start('', [])} onIssue={() => setIssue(null)} onPull={() => setPull(null)} />
      <AnimatePresence initial={false}>
        {picking === 'issue' && <IssuePanel key="issue" sources={sources} spaces={spaces} onPick={(i) => (setIssue(i), setPicking(null))} onClose={() => setPicking(null)} />}
        {picking === 'pull' && <PullPanel key="pull" application={chosen} onPick={(p) => (setPull(p), setPicking(null))} onClose={() => setPicking(null)} />}
      </AnimatePresence>
      <Composer
        autoFocus
        placeholder={issue ? `Instruções para planejar ${issue.key} (opcional)` : pull ? `Instruções para ${pull.subject} (opcional)` : 'O que precisa ser feito?'}
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
        onBtw={btw.ask}
      />
      {error && <p role="alert" className="px-2 text-sm text-danger-fg">{error}</p>}
      {btw.state && <div className="pt-4"><BtwChat state={btw.state} onClose={btw.close} /></div>}
      {adding && <ApplicationDialog open application={null} onClose={() => setAdding(false)} />}
    </div>
  )
}
