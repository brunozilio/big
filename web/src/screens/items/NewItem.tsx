import { AnimatePresence, motion } from 'framer-motion'
import { useEffect, useMemo, useState } from 'react'
import { api } from '@/api'
import type { Application, Connection, IssueSummary, Item, NewGit, NewSource, Provider } from '@/api/contract'
import { errorText, setItems, useStore } from '@/api/store'
import { Composer } from '@/chat/Composer'
import { useRememberedModel, useRememberedTuning } from '@/chat/remembered'
import { Button, IconButton } from '@/ui/button'
import { StatusIcon, ProviderLogo } from '@/ui/domain'
import { Input, SearchInput } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { tween } from '@/ui/motion'
import { Menu, MenuContent, MenuItem, MenuLabel, MenuTrigger, Popover } from '@/ui/overlay'
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

function ApplicationMenu({ applications, chosen, onChoose }: { applications: Application[]; chosen: Application | null; onChoose: (application: Application) => void }) {
  if (applications.length === 0 || !chosen) return null
  return (
    <Menu>
      <MenuTrigger asChild>
        <button aria-label={`Aplicação: ${chosen.name}`} className="flex h-8 max-w-[160px] items-center gap-1.5 rounded-full px-2.5 text-base text-fg2 transition-colors hover:bg-inset data-[state=open]:bg-inset">
          <Icon name="apps" size={15} className="shrink-0 text-muted" />
          <span className="truncate">{chosen.name}</span>
        </button>
      </MenuTrigger>
      <MenuContent align="start" className="max-h-[320px] overflow-y-auto">
        <MenuLabel>Aplicação</MenuLabel>
        {applications.map((a) => (
          <MenuItem key={a.id} icon={<Icon name="apps" size={15} />} onSelect={() => onChoose(a)} right={a.id === chosen.id ? <Icon name="check" size={15} /> : undefined}>
            {a.name}
          </MenuItem>
        ))}
      </MenuContent>
    </Menu>
  )
}

function IssuePicker({ sources, spaces, onPick }: { sources: Source[]; spaces: Space[]; onPick: (issue: Picked) => void }) {
  const [open, setOpen] = useState(false)
  const [q, setQ] = useState('')
  const [found, setFound] = useState<Picked[]>([])
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    if (!open) return
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
  }, [open, q, sources, spaces])
  const first = sources[0]
  if (!first) return null
  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      side="bottom"
      align="start"
      className="w-[460px] max-w-[calc(100vw-24px)] p-1.5"
      trigger={
        <button aria-label={`Começar de uma issue do ${first.provider.name}`} className="flex h-8 min-w-8 items-center justify-center gap-1.5 rounded-full px-2 transition-colors hover:bg-inset data-[state=open]:bg-inset">
          <ProviderLogo logo={first.provider.logo} name={first.provider.name} size={16} />
        </button>
      }
    >
      <div className="flex items-center gap-1 p-1">
        <div className="min-w-0 flex-1"><SearchInput autoFocus value={q} onChange={(e) => setQ(e.target.value)} placeholder={`Buscar issue do ${first.provider.name}`} /></div>
      </div>
      <ul className="mt-1 max-h-[280px] overflow-y-auto">
        {found.map((i) => (
          <li key={`${i.source.connection.id}:${i.key}`}>
            <button onClick={() => { setOpen(false); onPick(i) }} className="flex h-10 w-full items-center gap-3 rounded-control px-2.5 text-left transition-colors hover:bg-hover">
              <StatusIcon status="todo" size={15} />
              <span className="w-[64px] shrink-0 text-base text-fg2">{i.key}</span>
              <span className="min-w-0 flex-1 truncate text-base text-fg">{i.title}</span>
            </button>
          </li>
        ))}
        {found.length === 0 && <li className="px-2.5 py-3 text-base text-faint">{error ?? 'Nenhuma issue encontrada.'}</li>}
      </ul>
    </Popover>
  )
}

export interface PickedPull {
  subject: string
  connection: string | null
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

function PullPicker({ application, onPick }: { application: Application | null; onPick: (pull: PickedPull) => void }) {
  const [open, setOpen] = useState(false)
  const [text, setText] = useState('')
  const code = useCodeConnections()
  if (code.length === 0) return null
  const linked = application?.repositories.find((r) => r.code)?.code?.connection ?? null
  const use = () => {
    const subject = pullSubject(text, application)
    if (!subject) return
    setOpen(false)
    setText('')
    onPick({ subject, connection: linked ?? (code.length === 1 ? code[0].id : null) })
  }
  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      side="bottom"
      align="start"
      className="w-[420px] max-w-[calc(100vw-24px)] p-2"
      trigger={
        <button aria-label="Começar de um pull request" className="flex size-8 items-center justify-center rounded-full text-fg2 transition-colors hover:bg-inset data-[state=open]:bg-inset">
          <Icon name="hGitPullRequest" size={16} />
        </button>
      }
    >
      <form className="flex items-center gap-2" onSubmit={(e) => { e.preventDefault(); use() }}>
        <div className="min-w-0 flex-1"><Input autoFocus aria-label="Pull request" value={text} onChange={(e) => setText(e.target.value)} placeholder="Link ou referência do pull request (dono/repo#12)" /></div>
        <Button size="sm" variant="primary" type="submit" disabled={!pullSubject(text, application)}>Usar</Button>
      </form>
    </Popover>
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

function BasePicker({ places, bases, onChange }: { places: Place[]; bases: Record<string, string>; onChange: (next: Record<string, string>) => void }) {
  const chosen = places.some((p) => bases[p.id]?.trim())
  return (
    <Popover
      side="bottom"
      align="start"
      className="w-[360px] max-w-[calc(100vw-24px)] space-y-2 p-3"
      trigger={
        <button aria-label="Branch base" className={`flex size-8 items-center justify-center rounded-full transition-colors hover:bg-inset data-[state=open]:bg-inset ${chosen ? 'text-accent' : 'text-fg2'}`}>
          <Icon name="branch" size={16} />
        </button>
      }
    >
      <p className="text-sm font-medium text-fg">Branch base</p>
      {places.map((p) => (
        <label key={p.id} className="block space-y-1">
          <span className="block truncate text-sm text-muted">{p.name}</span>
          <Input aria-label={`Branch base de ${p.name}`} value={bases[p.id] ?? ''} onChange={(e) => onChange({ ...bases, [p.id]: e.target.value })} placeholder="padrão do repositório" />
        </label>
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

export function NewItem({ onCreated }: { onCreated: (it: Item) => void }) {
  const [busy, setBusy] = useState(false)
  const [issue, setIssue] = useState<Picked | null>(null)
  const [pull, setPull] = useState<PickedPull | null>(null)
  const [bases, setBases] = useState<Record<string, string>>({})
  const [error, setError] = useState<string | null>(null)
  const [remembered, setModel] = useRememberedModel()
  const sources = useTrackers()
  const applications = useStore((s) => s.applications)
  const [chosenId, setChosenId] = useState<string | null>(() => rememberedApplication())
  const chosen = useMemo(() => {
    const listed = applications ?? []
    return listed.find((a) => a.id === chosenId) ?? listed[0] ?? null
  }, [applications, chosenId])
  const choose = (next: Application) => {
    setChosenId(next.id)
    setBases({})
    rememberApplication(next.id)
  }
  const places = useMemo(() => placesOf(chosen), [chosen])
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
    const origins = sourcesOf(picked, pull)
    api()
      .createItem({ application: chosen?.id ?? null, text, issue: picked, sources: origins, git: gitOf(bases, places), model: chosenModel, ...tuning, attachments })
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
      <AnimatePresence initial={false}>
        {issue && (
          <motion.div initial={{ opacity: 0, y: -4 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -4 }} transition={tween} className="flex items-center gap-2.5 rounded-card bg-surface px-3 py-2 shadow-card">
            <ProviderLogo logo={issue.source.provider.logo} name={issue.source.provider.name} size={15} />
            <span className="text-base text-fg2">{issue.key}</span>
            <span className="min-w-0 flex-1 truncate text-base text-fg">{issue.title}</span>
            <Button size="sm" variant="primary" loading={busy} icon={<Icon name="sPlan" size={15} />} onClick={() => start('', [])}>Planejar</Button>
            <IconButton tooltip="Remover issue" size="sm" onClick={() => setIssue(null)}><Icon name="close" size={15} /></IconButton>
          </motion.div>
        )}
        {pull && (
          <motion.div initial={{ opacity: 0, y: -4 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -4 }} transition={tween} className="flex items-center gap-2.5 rounded-card bg-surface px-3 py-2 shadow-card">
            <Icon name="hGitPullRequest" size={15} className="text-fg2" />
            <span className="min-w-0 flex-1 truncate text-base text-fg">{pull.subject}</span>
            <Button size="sm" variant="primary" loading={busy} icon={<Icon name="sCode" size={15} />} onClick={() => start('', [])}>Começar</Button>
            <IconButton tooltip="Remover pull request" size="sm" onClick={() => setPull(null)}><Icon name="close" size={15} /></IconButton>
          </motion.div>
        )}
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
        leading={
          <>
            <ApplicationMenu applications={applications ?? []} chosen={chosen} onChoose={choose} />
            <IssuePicker sources={sources} spaces={spaces} onPick={setIssue} />
            <PullPicker application={chosen} onPick={setPull} />
            <BasePicker places={places} bases={bases} onChange={setBases} />
          </>
        }
        onSubmit={(input) => start(input.text, input.attachments)}
      />
      {error && <p role="alert" className="px-2 text-sm text-danger-fg">{error}</p>}
    </div>
  )
}
