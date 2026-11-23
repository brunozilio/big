import { AnimatePresence, motion } from 'framer-motion'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { api } from '@/api'
import type { Connection, IssueSummary, Item, Provider, TrackerProject } from '@/api/contract'
import { errorText, setItems, useStore } from '@/api/store'
import { Composer, type Tuning } from '@/chat/Composer'
import { Button, IconButton } from '@/ui/button'
import { StatusIcon, ProviderLogo } from '@/ui/domain'
import { SearchInput } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { tween } from '@/ui/motion'
import { Menu, MenuContent, MenuItem, MenuLabel, MenuTrigger, Popover } from '@/ui/overlay'
import { initialProject, rememberProject, sameProject, searchTargets, type ProjectChoice } from './projects'

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

function ProjectMenu({ sources, projects, choice, onChoose }: { sources: Source[]; projects: Record<string, TrackerProject[]>; choice: ProjectChoice | null; onChoose: (choice: ProjectChoice | null) => void }) {
  const listed = sources.filter((s) => (projects[s.connection.id] ?? []).length > 0)
  const chosen = choice ? (projects[choice.connection] ?? []).find((p) => p.key === choice.key) : undefined
  const label = chosen?.name ?? choice?.key ?? 'Todos os projetos'
  return (
    <Menu>
      <MenuTrigger asChild>
        <button aria-label={`Projeto: ${label}`} className="flex h-9 max-w-[180px] shrink-0 items-center gap-1.5 rounded-control px-2.5 text-base text-fg2 transition-colors hover:bg-hover data-[state=open]:bg-inset">
          <Icon name="folder" size={15} className="shrink-0 text-muted" />
          <span className="truncate">{label}</span>
          <Icon name="down" size={14} className="shrink-0 text-muted" />
        </button>
      </MenuTrigger>
      <MenuContent align="start" className="max-h-[320px] overflow-y-auto">
        <MenuItem icon={<Icon name="folder" size={15} />} onSelect={() => onChoose(null)} right={choice === null ? <Icon name="check" size={15} /> : undefined}>Todos os projetos</MenuItem>
        {listed.map((s) => (
          <div key={s.connection.id}>
            {listed.length > 1 && <MenuLabel>{s.connection.account || s.connection.label}</MenuLabel>}
            {(projects[s.connection.id] ?? []).map((p) => {
              const next = { connection: s.connection.id, key: p.key }
              return (
                <MenuItem key={p.id || p.key} icon={p.avatar ? <img src={p.avatar} alt="" className="size-[15px] rounded-[4px]" /> : <Icon name="folder" size={15} />} onSelect={() => onChoose(next)} right={sameProject(choice, next) ? <Icon name="check" size={15} /> : undefined}>
                  <span className="text-fg2">{p.name}</span> <span className="text-muted">{p.key}</span>
                </MenuItem>
              )
            })}
          </div>
        ))}
      </MenuContent>
    </Menu>
  )
}

function useProjects(open: boolean, sources: Source[]): Record<string, TrackerProject[]> {
  const [projects, setProjects] = useState<Record<string, TrackerProject[]>>({})
  const asked = useRef(new Set<string>())
  useEffect(() => {
    if (!open) return
    for (const s of sources) {
      const id = s.connection.id
      if (asked.current.has(id)) continue
      asked.current.add(id)
      api()
        .call(id, 'tracker.projects', {})
        .then((list) => setProjects((known) => ({ ...known, [id]: list })))
        .catch(() => {
          asked.current.delete(id)
          setProjects((known) => ({ ...known, [id]: [] }))
        })
    }
  }, [open, sources])
  return projects
}

function IssuePicker({ sources, project, onProject, onPick }: { sources: Source[]; project: ProjectChoice | null; onProject: (choice: ProjectChoice | null) => void; onPick: (issue: Picked) => void }) {
  const [open, setOpen] = useState(false)
  const [q, setQ] = useState('')
  const [found, setFound] = useState<Picked[]>([])
  const [error, setError] = useState<string | null>(null)
  const projects = useProjects(open, sources)
  useEffect(() => {
    const known = project ? projects[project.connection] : undefined
    if (project && known && known.length > 0 && !known.some((p) => p.key === project.key)) onProject(null)
  }, [project, projects, onProject])
  useEffect(() => {
    if (!open) return
    let alive = true
    const t = setTimeout(() => {
      const targets = searchTargets(sources.map((s) => s.connection.id), project, q)
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
  }, [open, q, sources, project])
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
          {project && <span className="max-w-[96px] truncate text-base text-fg2">{project.key}</span>}
        </button>
      }
    >
      <div className="flex items-center gap-1 p-1">
        <ProjectMenu sources={sources} projects={projects} choice={project} onChoose={onProject} />
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

export function NewItem({ onCreated }: { onCreated: (it: Item) => void }) {
  const [busy, setBusy] = useState(false)
  const [issue, setIssue] = useState<Picked | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [model, setModel] = useState<string | null>(null)
  const sources = useTrackers()
  const ids = useMemo(() => sources.map((s) => s.connection.id), [sources])
  const [chosenProject, setChosenProject] = useState<ProjectChoice | null | undefined>(undefined)
  const project = useMemo(() => (chosenProject === undefined ? initialProject(ids) : chosenProject), [chosenProject, ids])
  const chooseProject = useCallback(
    (choice: ProjectChoice | null) => {
      setChosenProject(choice)
      rememberProject(ids, choice)
    },
    [ids],
  )
  const defaultModel = useStore((s) => s.models?.default ?? null)
  const [tuning, setTuning] = useState<Tuning>({ effort: 'medium', fast: false })
  const start = (text: string, attachments: string[]) => {
    if (!issue && !text) return
    setBusy(true)
    setError(null)
    api()
      .createItem({ text, issue: issue ? { connection: issue.source.connection.id, key: issue.key } : null, project: issue ? null : project, model: model ?? defaultModel ?? 'auto', ...tuning, attachments })
      .then((it) => {
        setItems((items) => (items.some((x) => x.id === it.id) ? items : [it, ...items]))
        setIssue(null)
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
      </AnimatePresence>
      <Composer
        autoFocus
        placeholder={issue ? `Instruções para planejar ${issue.key} (opcional)` : 'O que precisa ser feito?'}
        disabled={busy}
        allowEmpty={!!issue}
        model={model}
        onModel={setModel}
        tuning={tuning}
        onTuning={setTuning}
        leading={<IssuePicker sources={sources} project={project} onProject={chooseProject} onPick={setIssue} />}
        onSubmit={(input) => start(input.text, input.attachments)}
      />
      {error && <p role="alert" className="px-2 text-sm text-danger-fg">{error}</p>}
    </div>
  )
}
