import { type FormEvent, useCallback, useEffect, useState } from 'react'
import { createPortal } from 'react-dom'
import { useSearchParams } from 'react-router-dom'
import { errorText } from '@/api/store'
import { AgentAvatar, knownLook } from '@/ui/agentAvatar'
import { Button } from '@/ui/button'
import { Badge, Card, EmptyState, ListSkeleton } from '@/ui/display'
import { Input, SearchInput, UnderlineTabs } from '@/ui/form'
import { Icon, NamedIcon } from '@/ui/icon'
import { Popover } from '@/ui/overlay'
import { useT } from '@/i18n'
import { LibraryEditor } from './LibraryEditor'
import { type Library, type LibraryCard, type LibrarySection, libraryApi, matching } from './libraryData'

type Loaded = { state: 'loading' } | { state: 'failed' } | { state: 'ready'; library: Library }

function useLibrary() {
  const [loaded, setLoaded] = useState<Loaded>({ state: 'loading' })
  const reload = useCallback(() => {
    libraryApi
      .list()
      .then((library) => setLoaded({ state: 'ready', library }))
      .catch(() => setLoaded((current) => (current.state === 'ready' ? current : { state: 'failed' })))
  }, [])
  useEffect(reload, [reload])
  return { loaded, reload }
}

function Mark({ section, item }: { section: LibrarySection; item: LibraryCard }) {
  const look = knownLook(item.avatar)
  if (look) return <AgentAvatar look={look} name={item.title} size={28} />
  return (
    <span aria-hidden className="flex size-7 shrink-0 items-center justify-center rounded-control-sm bg-inset text-muted">
      <NamedIcon name={section.icon} fallback="file" size={15} />
    </span>
  )
}

function Row({ section, item, onOpen }: { section: LibrarySection; item: LibraryCard; onOpen: () => void }) {
  const t = useT()
  return (
    <li>
      <button type="button" onClick={onOpen} className="group flex min-h-14 w-full items-center gap-3 px-4 py-2.5 text-left transition-colors hover:bg-hover">
        <Mark section={section} item={item} />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-base font-medium text-fg">{item.title}</span>
          {item.description && <span className="block truncate text-sm text-muted">{item.description}</span>}
        </span>
        {item.origin !== 'shipped' && <Badge tone={item.origin === 'edited' ? 'info' : 'neutral'}>{t(`settings.lib_origin_${item.origin}`)}</Badge>}
        <Icon name="right" size={15} className="shrink-0 text-faint opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100" />
      </button>
    </li>
  )
}

function NewItem({ section, onCreated }: { section: LibrarySection; onCreated: (path: string) => void }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (!name.trim() || busy) return
    setBusy(true)
    setError(null)
    libraryApi
      .create(section.key, name.trim())
      .then((made) => {
        setOpen(false)
        setName('')
        onCreated(made.path)
      })
      .catch((failure: unknown) => setError(errorText(failure) || t('settings.lib_saveFailed')))
      .finally(() => setBusy(false))
  }
  return (
    <Popover open={open} onOpenChange={setOpen} align="end" className="w-72 p-3" trigger={<Button icon={<Icon name="plus" size={15} />}>{t('settings.lib_new')}</Button>}>
      <form onSubmit={submit} className="space-y-2.5">
        <Input autoFocus aria-label={t('settings.lib_name')} placeholder={t('settings.lib_name')} value={name} onChange={(e) => setName(e.target.value)} />
        {error && <p role="alert" className="text-sm text-danger-fg">{error}</p>}
        <Button type="submit" variant="primary" className="w-full" loading={busy} disabled={!name.trim()}>{t('settings.lib_create')}</Button>
      </form>
    </Popover>
  )
}

function SectionList({ section, editable, onOpen }: { section: LibrarySection; editable: boolean; onOpen: (path: string) => void }) {
  const t = useT()
  const [query, setQuery] = useState('')
  const shown = matching(section.items, query)
  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2.5">
        <SearchInput className="flex-1" aria-label={t('settings.lib_search')} placeholder={section.hint} value={query} onChange={(e) => setQuery(e.target.value)} />
        {editable && section.creatable && <NewItem section={section} onCreated={onOpen} />}
      </div>
      {shown.length === 0 ? (
        <EmptyState size="inline" title={t('settings.lib_none')} />
      ) : (
        <Card className="overflow-hidden">
          <ul aria-label={section.label} className="divide-y">
            {shown.map((item) => <Row key={item.path} section={section} item={item} onOpen={() => onOpen(item.path)} />)}
          </ul>
        </Card>
      )}
    </div>
  )
}

export function LibraryView() {
  const t = useT()
  const { loaded, reload } = useLibrary()
  const [search, setSearch] = useSearchParams()
  const patch = (change: (next: URLSearchParams) => void, replace = false) => {
    const next = new URLSearchParams(search)
    change(next)
    setSearch(next, { replace })
  }
  if (loaded.state === 'loading') return <ListSkeleton />
  if (loaded.state === 'failed') return <p role="alert" className="text-sm text-danger-fg">{t('settings.lib_loadFailed')}</p>
  const { library } = loaded
  const section = library.sections.find((s) => s.key === search.get('section')) ?? library.sections[0]
  const item = search.get('item')
  if (!section) return <EmptyState size="inline" title={t('settings.lib_none')} />
  return (
    <div className="space-y-5">
      <UnderlineTabs value={section.key} onChange={(key) => patch((next) => next.set('section', key), true)} options={library.sections.map((s) => ({ value: s.key, label: s.label, count: s.items.length }))} className="no-scrollbar overflow-x-auto" />
      <SectionList key={section.key} section={section} editable={library.editable} onOpen={(path) => patch((next) => next.set('item', path))} />
      {item && createPortal(<LibraryEditor path={item} onPath={(path) => patch((next) => next.set('item', path), true)} onClose={() => patch((next) => next.delete('item'))} onChanged={reload} />, document.body)}
    </div>
  )
}
