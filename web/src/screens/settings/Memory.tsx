import { type ReactNode, useEffect, useState } from 'react'
import { request } from '@/api/http'
import { IconButton } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Badge, Card } from '@/ui/display'
import { SearchInput } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { Menu, MenuContent, MenuItem, MenuLabel, MenuTrigger } from '@/ui/overlay'
import { useT } from '@/i18n'
import { type ApplicationMemory, type Memory, type MemoryTree, type Place, type RepositoryMemory, type Shelf, countOf, folderName, forgottenOf, keyOf, matches, placesOf } from './memoryTree'

type Loaded = { state: 'loading' } | { state: 'failed' } | { state: 'ready'; tree: MemoryTree }

interface Actions {
  forget: (shelf: Shelf, id: number) => void
  restore: (shelf: Shelf, id: number) => void
  move: (from: Shelf, id: number, to: Shelf) => void
}

function useMemoryTree() {
  const t = useT()
  const [loaded, setLoaded] = useState<Loaded>({ state: 'loading' })
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let alive = true
    request<MemoryTree>('GET', '/api/memories')
      .then((tree) => alive && setLoaded({ state: 'ready', tree }))
      .catch(() => alive && setLoaded({ state: 'failed' }))
    return () => {
      alive = false
    }
  }, [])
  const send = (path: string, body: unknown) => {
    setError(null)
    request<MemoryTree>('POST', path, body)
      .then((tree) => setLoaded({ state: 'ready', tree }))
      .catch(() => setError(t('settings.mem_saveFailed')))
  }
  const actions: Actions = {
    forget: (shelf, id) => send('/api/memories', { shelf, edit: { op: 'forget', id } }),
    restore: (shelf, id) => send('/api/memories', { shelf, edit: { op: 'restore', id } }),
    move: (from, id, to) => send('/api/memories/move', { from, id, to }),
  }
  return { loaded, error, actions }
}

function MoveMenu({ memory, from, places, onMove }: { memory: Memory; from: Shelf; places: Place[]; onMove: (to: Shelf) => void }) {
  const t = useT()
  const here = keyOf(from)
  return (
    <Menu>
      <MenuTrigger asChild>
        <IconButton size="sm" tooltip={t('settings.mem_move')} aria-label={`${t('settings.mem_move')}: ${memory.text}`} className="opacity-60 group-hover:opacity-100">
          <Icon name="layers" size={14} />
        </IconButton>
      </MenuTrigger>
      <MenuContent align="end">
        <MenuLabel>{t('settings.mem_moveTo')}</MenuLabel>
        {places.filter((p) => p.key !== here).map((p) => <MenuItem key={p.key} onSelect={() => onMove(p.shelf)}>{p.label}</MenuItem>)}
      </MenuContent>
    </Menu>
  )
}

function Row({ memory, shelf, places, actions }: { memory: Memory; shelf: Shelf; places: Place[]; actions: Actions }) {
  const t = useT()
  return (
    <li className="group flex items-start gap-3 px-4 py-2.5">
      <Badge className="mt-0.5 shrink-0">{t(`settings.mem_kind_${memory.kind}`)}</Badge>
      <p className="min-w-0 flex-1 whitespace-pre-wrap pt-0.5 text-sm text-fg">{memory.text}</p>
      <MoveMenu memory={memory} from={shelf} places={places} onMove={(to) => actions.move(shelf, memory.id, to)} />
      <IconButton size="sm" tooltip={t('settings.mem_forget')} aria-label={`${t('settings.mem_forget')}: ${memory.text}`} onClick={() => actions.forget(shelf, memory.id)} className="opacity-60 group-hover:opacity-100">
        <Icon name="trash" size={14} />
      </IconButton>
    </li>
  )
}

function TeamRow({ memory }: { memory: Memory }) {
  const t = useT()
  return (
    <li className="flex items-start gap-3 px-4 py-2.5">
      <Badge tone="outline" className="mt-0.5 shrink-0">{t('settings.mem_teamBadge')}</Badge>
      <p className="min-w-0 flex-1 whitespace-pre-wrap pt-0.5 text-sm text-fg">{memory.text}</p>
    </li>
  )
}

function Group({ title, count, open: start, forced, nested, children }: { title: string; count: number; open: boolean; forced: boolean; nested?: boolean; children: ReactNode }) {
  const [open, setOpen] = useState(start)
  const shown = forced || open
  return (
    <section className={cn(nested && 'border-t')}>
      <button type="button" aria-expanded={shown} onClick={() => setOpen(!shown)} className={cn('flex w-full items-center gap-2 py-2.5 text-left', nested ? 'px-4' : 'px-1')}>
        <Icon name={shown ? 'down' : 'right'} size={14} className="text-muted" />
        <span className={cn('min-w-0 flex-1 truncate', nested ? 'text-sm text-fg2' : 'text-base font-medium text-fg')}>{title}</span>
        <span className="text-sm tabular-nums text-muted">{count}</span>
      </button>
      {shown && children}
    </section>
  )
}

function Empty({ text }: { text: string }) {
  return <li className="px-4 py-3 text-sm text-muted">{text}</li>
}

interface Shared {
  query: string
  places: Place[]
  actions: Actions
}

function RepositoryGroup({ repository, shared }: { repository: RepositoryMemory; shared: Shared }) {
  const t = useT()
  const fits = matches(shared.query)
  const memories = repository.memories.filter(fits)
  const team = repository.team.filter(fits)
  const shelf: Shelf = { scope: 'repository', directory: repository.directory }
  if (shared.query && memories.length + team.length === 0) return null
  return (
    <Group nested title={folderName(repository.directory)} count={memories.length + team.length} open={memories.length + team.length > 0} forced={!!shared.query}>
      <ul aria-label={folderName(repository.directory)} className="divide-y border-t">
        {memories.length + team.length === 0 && <Empty text={t('settings.mem_empty')} />}
        {memories.map((memory) => <Row key={memory.id} memory={memory} shelf={shelf} places={shared.places} actions={shared.actions} />)}
        {team.map((memory) => <TeamRow key={memory.id} memory={memory} />)}
      </ul>
    </Group>
  )
}

function ApplicationGroup({ application, shared }: { application: ApplicationMemory; shared: Shared }) {
  const t = useT()
  const memories = application.memories.filter(matches(shared.query))
  const shelf: Shelf = { scope: 'application', id: application.id }
  const inside = application.repositories.map((r) => r.memories.filter(matches(shared.query)).length + r.team.filter(matches(shared.query)).length).reduce((a, b) => a + b, 0)
  if (shared.query && memories.length + inside === 0) return null
  return (
    <Group title={application.name} count={shared.query ? memories.length + inside : countOf(application)} open forced={!!shared.query}>
      <Card>
        <ul aria-label={application.name} className="divide-y">
          {memories.length === 0 && !shared.query && <Empty text={t('settings.mem_emptyApplication')} />}
          {memories.map((memory) => <Row key={memory.id} memory={memory} shelf={shelf} places={shared.places} actions={shared.actions} />)}
        </ul>
        {application.repositories.map((repository) => <RepositoryGroup key={repository.directory} repository={repository} shared={shared} />)}
      </Card>
    </Group>
  )
}

function GlobalGroup({ tree, shared }: { tree: MemoryTree; shared: Shared }) {
  const t = useT()
  const memories = tree.global.memories.filter(matches(shared.query))
  const shelf: Shelf = { scope: 'global' }
  if (shared.query && memories.length === 0) return null
  return (
    <Group title={t('settings.mem_global')} count={memories.length} open forced={!!shared.query}>
      <Card>
        <ul aria-label={t('settings.mem_global')} className="divide-y">
          {memories.length === 0 && <Empty text={t('settings.mem_emptyGlobal')} />}
          {memories.map((memory) => <Row key={memory.id} memory={memory} shelf={shelf} places={shared.places} actions={shared.actions} />)}
        </ul>
      </Card>
    </Group>
  )
}

function ForgottenGroup({ tree, actions }: { tree: MemoryTree; actions: Actions }) {
  const t = useT()
  const forgotten = forgottenOf(tree, t('settings.mem_global'))
  if (forgotten.length === 0) return null
  return (
    <Group title={t('settings.mem_forgotten')} count={forgotten.length} open={false} forced={false}>
      <Card>
        <ul aria-label={t('settings.mem_forgotten')} className="divide-y">
          {forgotten.map(({ shelf, label, forgotten: { memory } }) => (
            <li key={`${keyOf(shelf)}:${memory.id}`} className="group flex items-start gap-3 px-4 py-2.5">
              <Badge tone="outline" className="mt-0.5 max-w-40 shrink-0 truncate">{label}</Badge>
              <p className="min-w-0 flex-1 whitespace-pre-wrap pt-0.5 text-sm text-muted">{memory.text}</p>
              <IconButton size="sm" tooltip={t('settings.mem_restore')} aria-label={`${t('settings.mem_restore')}: ${memory.text}`} onClick={() => actions.restore(shelf, memory.id)} className="opacity-60 group-hover:opacity-100">
                <Icon name="undo" size={14} />
              </IconButton>
            </li>
          ))}
        </ul>
      </Card>
    </Group>
  )
}

export function MemoryView() {
  const t = useT()
  const { loaded, error, actions } = useMemoryTree()
  const [query, setQuery] = useState('')
  if (loaded.state === 'loading') return null
  if (loaded.state === 'failed') return <p role="alert" className="text-sm text-danger-fg">{t('settings.mem_loadFailed')}</p>
  const { tree } = loaded
  const shared: Shared = { query, places: placesOf(tree, t('settings.mem_global')), actions }
  const nothing = query && tree.global.memories.filter(matches(query)).length === 0 && tree.applications.every((a) => !a.memories.some(matches(query)) && a.repositories.every((r) => !r.memories.some(matches(query)) && !r.team.some(matches(query))))
  return (
    <div className="space-y-5">
      <SearchInput aria-label={t('settings.mem_search')} placeholder={t('settings.mem_search')} value={query} onChange={(e) => setQuery(e.target.value)} />
      <GlobalGroup tree={tree} shared={shared} />
      {tree.applications.map((application) => <ApplicationGroup key={application.id} application={application} shared={shared} />)}
      {tree.applications.length === 0 && !query && <p className="px-1 text-sm text-muted">{t('settings.mem_noApplications')}</p>}
      {nothing && <p className="px-1 text-sm text-muted">{t('settings.mem_noMatch')}</p>}
      {!query && <ForgottenGroup tree={tree} actions={actions} />}
      {error && <p role="alert" className="text-sm text-danger-fg">{error}</p>}
    </div>
  )
}
