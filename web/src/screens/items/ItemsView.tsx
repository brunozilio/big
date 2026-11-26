import { AnimatePresence, motion } from 'framer-motion'
import { memo, useCallback, useEffect, useMemo, useRef, useState, type DragEvent, type KeyboardEvent, type ReactNode } from 'react'
import { useInRouterContext, useSearchParams } from 'react-router-dom'
import { BOARD_GROUPS, BTW_WORKFLOW, type BoardGroup, type Item } from '@/api/contract'
import { useStore } from '@/api/store'
import { useT, type T } from '@/i18n'
import { titleOf } from '@/i18n/server'
import { cn } from '@/ui/cn'
import { StatusIcon, type StatusGlyph } from '@/ui/domain'
import { Input, SegmentedControl } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { Collapse, stagger, tween, tweenClose } from '@/ui/motion'
import { toast } from '@/ui/toast'
import { grouping, membersOf, movable, usePackageBoard, type PackageBoard } from './packages/board'
import { DRAG_TYPE, GroupRow } from './packages/GroupRow'
import { PackageTray } from './packages/PackageTray'
import { archiveToggle, IssueKey, LiveDot, OriginCell, RowRight, SourceChip, stop } from './parts'
import { readView, toggleClosed, writeView, type Kind, type ViewState } from './viewState'

export type { Kind } from './viewState'

export function groupLabel(t: T, group: BoardGroup): string {
  return t(`items.group_${group}`)
}
const GROUP_GLYPH: Record<BoardGroup, StatusGlyph> = { you: 'in_review', running: 'in_progress', delivered: 'done' }

export function groupItems(items: Item[], group: BoardGroup): Item[] {
  const list = items.filter((it) => it.group === group)
  if (group !== 'delivered') return list
  return list.slice().sort((a, b) => (b.doneAt ?? '').localeCompare(a.doneAt ?? ''))
}

export type Filter = { kind: Kind; archived: boolean; query?: string }

export const isBtw = (it: Item) => it.workflow === BTW_WORKFLOW

function haystack(it: Item): string {
  const sources = (it.sources ?? []).flatMap((s) => [s.key, s.repository])
  const git = (it.git ?? []).flatMap((g) => [g.head, g.name])
  return [it.title, it.issue?.key, it.change?.key, ...sources, ...git].filter(Boolean).join('\n').toLowerCase()
}

export function matchesQuery(it: Item, query: string): boolean {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean)
  if (words.length === 0) return true
  const text = haystack(it)
  return words.every((w) => text.includes(w))
}

export function filterItems(items: Item[], filter: Filter): Item[] {
  const query = (filter.query ?? '').trim()
  const found = (it: Item) => !query || matchesQuery(it, query) || membersOf(it, items).some((member) => matchesQuery(member, query))
  return items.filter((it) => {
    if (filter.archived !== !!it.archivedAt) return false
    if (filter.kind === 'btw' && !isBtw(it)) return false
    if (filter.kind === 'work' && isBtw(it)) return false
    return found(it)
  })
}

type ViewProps = { onOpen: (it: Item, step?: string) => void; onConnect: (provider: string) => void; fresh: string | null }
type BoardProps = ViewProps & { view: ViewState; onView: (patch: Partial<ViewState>) => void }

export function ItemsView(props: ViewProps) {
  return useInRouterContext() ? <UrlBoard {...props} /> : <LocalBoard {...props} />
}

function UrlBoard(props: ViewProps) {
  const [params, setParams] = useSearchParams()
  const view = useMemo(() => readView(params), [params])
  return <Board {...props} view={view} onView={(patch) => setParams((now) => writeView(now, patch), { replace: true })} />
}

function LocalBoard(props: ViewProps) {
  const [params, setParams] = useState(() => new URLSearchParams())
  const view = useMemo(() => readView(params), [params])
  return <Board {...props} view={view} onView={(patch) => setParams((now) => writeView(now, patch))} />
}

function useLatest<A extends unknown[]>(fn: (...args: A) => void): (...args: A) => void {
  const latest = useRef(fn)
  latest.current = fn
  return useCallback((...args: A) => latest.current(...args), [])
}

function sameList(a: Item[], b: Item[]): boolean {
  return a.length === b.length && a.every((x, i) => x === b[i])
}

function useSteady(list: Item[]): Item[] {
  const kept = useRef(list)
  if (!sameList(kept.current, list)) kept.current = list
  return kept.current
}

type Lists = Record<BoardGroup, Item[]>

function useLists(items: Item[], view: ViewState, hidden: Set<string>): Lists {
  const { kind, archived, query } = view
  const lists = useMemo(() => {
    const live = filterItems(items, { kind, query, archived: false }).filter((it) => !hidden.has(it.id))
    const done = archived ? filterItems(items, { kind, query, archived: true }) : live
    return { you: groupItems(live, 'you'), running: groupItems(live, 'running'), delivered: groupItems(done, 'delivered') }
  }, [items, hidden, kind, archived, query])
  return { you: useSteady(lists.you), running: useSteady(lists.running), delivered: useSteady(lists.delivered) }
}

function Board({ onOpen, onConnect, fresh, view, onView }: BoardProps) {
  const items = useStore((s) => s.items)
  const t = useT()
  const board = usePackageBoard(items)
  const lists = useLists(items, view, board.hidden)
  const archivedCount = useMemo(() => items.filter((it) => it.archivedAt).length, [items])
  const open = useLatest(onOpen)
  const connect = useLatest(onConnect)
  const move = useLatest(board.move)
  const query = view.query.trim()
  const empty = (g: BoardGroup) => (query ? t('items.noMatches', { query }) : g === 'delivered' && view.archived ? t('items.noArchived') : t('items.emptyGroup'))
  const toggle = <ArchivedToggle on={view.archived} count={archivedCount} onChange={(archived) => onView({ archived })} />
  return (
    <>
      <Toolbar view={view} onView={onView} />
      <div className="space-y-7">
        {BOARD_GROUPS.map((g, i) => (
          <Group
            key={g}
            index={i}
            group={g}
            count={lists[g].length}
            open={!view.closed.includes(g)}
            onToggle={() => onView({ closed: toggleClosed(view.closed, g) })}
            action={g === 'delivered' ? <>{toggle}<BuildPackage board={board} /></> : null}
            top={g === 'delivered' && !view.archived ? <Tray board={board} items={items} /> : null}
          >
            <Rows items={lists[g]} empty={empty(g)} fresh={fresh} accepting={board.accepting} onOpen={open} onConnect={connect} onMove={move} />
          </Group>
        ))}
      </div>
    </>
  )
}

function typing(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))
}

function Search({ query, onQuery }: { query: string; onQuery: (query: string) => void }) {
  const t = useT()
  const field = useRef<HTMLInputElement>(null)
  const [draft, setDraft] = useState(query)
  const sent = useRef(query)
  useEffect(() => {
    if (query === sent.current) return
    sent.current = query
    setDraft(query)
  }, [query])
  const change = (next: string) => {
    sent.current = next
    setDraft(next)
    onQuery(next)
  }
  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key !== '/' || e.metaKey || e.ctrlKey || e.altKey || typing(e.target)) return
      e.preventDefault()
      field.current?.focus()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== 'Escape') return
    e.preventDefault()
    if (draft) change('')
    else field.current?.blur()
  }
  const clear = (
    <button type="button" aria-label={t('items.clearSearch')} onClick={() => { change(''); field.current?.focus() }} className="hit-44 flex size-6 items-center justify-center rounded-control-sm text-muted transition-colors hover:bg-hover hover:text-fg">
      <Icon name="close" size={14} />
    </button>
  )
  return (
    <Input
      ref={field}
      type="search"
      value={draft}
      onChange={(e) => change(e.target.value)}
      onKeyDown={onKeyDown}
      placeholder={t('items.search')}
      aria-label={t('items.search')}
      aria-keyshortcuts="/"
      leading={<Icon name="search" size={15} />}
      trailing={draft ? clear : undefined}
      className="h-8 min-w-[180px] flex-1 px-2.5 sm:max-w-[320px] [&_input]:text-sm [&_input::-webkit-search-cancel-button]:hidden"
    />
  )
}

function Toolbar({ view, onView }: { view: ViewState; onView: (patch: Partial<ViewState>) => void }) {
  const t = useT()
  const kinds: { value: Kind; label: string }[] = [
    { value: 'all', label: t('items.filterAll') },
    { value: 'work', label: t('items.filterWork') },
    { value: 'btw', label: t('items.filterBtw') },
  ]
  return (
    <div className="mb-5 flex flex-wrap items-center gap-2">
      <SegmentedControl size="sm" value={view.kind} onChange={(kind) => onView({ kind })} options={kinds} />
      <Search query={view.query} onQuery={(query) => onView({ query })} />
    </div>
  )
}

const toggleClass = (on: boolean) => cn('hit-44 inline-flex h-8 items-center gap-1.5 rounded-control px-2.5 text-sm transition-colors', on ? 'bg-active text-fg' : 'text-muted hover:bg-hover hover:text-fg2')

function BuildPackage({ board }: { board: PackageBoard }) {
  const t = useT()
  const on = board.draft !== null
  return (
    <button aria-pressed={on} onClick={on ? board.close : board.open} className={toggleClass(on)}>
      <Icon name="package" size={14} />
      {t('items.buildPackage')}
    </button>
  )
}

function Tray({ board, items }: { board: PackageBoard; items: Item[] }) {
  const t = useT()
  const [busy, setBusy] = useState(false)
  const chosen = (board.draft ?? []).map((id) => items.find((it) => it.id === id)).filter((it): it is Item => !!it)
  const create = () => {
    setBusy(true)
    void board.build(t('items.newPackage'), 'auto').then((failed) => {
      if (failed) toast.error(failed)
      setBusy(false)
    })
  }
  return (
    <AnimatePresence initial={false}>
      {board.draft !== null && <PackageTray chosen={chosen} busy={busy} onAdd={board.add} onReturn={board.remove} onCancel={board.close} onCreate={create} />}
    </AnimatePresence>
  )
}

function ArchivedToggle({ on, count, onChange }: { on: boolean; count: number; onChange: (on: boolean) => void }) {
  const t = useT()
  return (
    <button aria-pressed={on} onClick={() => onChange(!on)} className={toggleClass(on)}>
      <Icon name="archive" size={14} />
      {t('items.filterArchived')}
      <span className="tabular-nums text-faint">{count}</span>
    </button>
  )
}

type GroupProps = { group: BoardGroup; count: number; index: number; open: boolean; onToggle: () => void; action: ReactNode; top?: ReactNode; children: ReactNode }

function Group({ group, count, index, open, onToggle, action, top, children }: GroupProps) {
  const t = useT()
  return (
    <motion.section initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ ...tween, delay: stagger(index) }}>
      <div className="mb-2 flex flex-wrap items-center gap-2">
        <button aria-expanded={open} onClick={onToggle} className="inline-flex h-8 items-center gap-2 rounded-control-sm bg-inset px-2.5 text-base font-medium text-fg transition-colors hover:bg-active">
          <StatusIcon status={GROUP_GLYPH[group]} size={15} />
          {groupLabel(t, group)}
          <span className="font-normal tabular-nums text-muted">{count}</span>
          <motion.span initial={false} animate={{ rotate: open ? 0 : -90 }} transition={tween} className="flex text-faint">
            <Icon name="down" size={14} />
          </motion.span>
        </button>
        {action}
      </div>
      <Collapse open={open}>
        {top}
        {children}
      </Collapse>
    </motion.section>
  )
}

type RowsProps = {
  items: Item[]
  empty: string
  fresh: string | null
  accepting: boolean
  onOpen: (it: Item, step?: string) => void
  onConnect: (provider: string) => void
  onMove: (target: Item, id: string, from: string | null) => void
}

const Rows = memo(function Rows({ items, empty, fresh, accepting, onOpen, onConnect, onMove }: RowsProps) {
  return (
    <ul>
      <AnimatePresence initial={false}>
        {items.map((it) => (
          <motion.li key={it.id} initial={{ opacity: 0, y: -4 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, transition: tweenClose }} transition={tween}>
            {grouping(it) ? <PackageRow it={it} onOpen={onOpen} onMove={onMove} /> : <Row it={it} fresh={fresh === it.id} draggable={movable(it, accepting)} onOpen={onOpen} onConnect={onConnect} />}
          </motion.li>
        ))}
      </AnimatePresence>
      {items.length === 0 && <li className="px-2.5 py-3 text-base text-faint">{empty}</li>}
    </ul>
  )
})

const PackageRow = memo(function PackageRow({ it, onOpen, onMove }: { it: Item; onOpen: (it: Item) => void; onMove: (target: Item, id: string, from: string | null) => void }) {
  const items = useStore((s) => s.items)
  const members = useSteady(useMemo(() => membersOf(it, items), [it, items]))
  return <GroupRow it={it} members={members} onOpen={onOpen} onDropItem={onMove} />
})

type RowProps = { it: Item; fresh: boolean; draggable: boolean; onOpen: (it: Item, step?: string) => void; onConnect: (provider: string) => void }

const Row = memo(function Row({ it, fresh, draggable, onOpen, onConnect }: RowProps) {
  const t = useT()
  const title = titleOf(t, it)
  const drag = draggable ? { draggable: true, onDragStart: (e: DragEvent) => e.dataTransfer.setData(DRAG_TYPE, it.id) } : {}
  return (
    <div {...drag} className={cn('rounded-card transition-colors duration-200', fresh && 'flash-new', draggable && 'cursor-grab active:cursor-grabbing')}>
      <div
        role="button"
        tabIndex={0}
        onClick={() => onOpen(it)}
        onKeyDown={(e) => {
          if (e.target !== e.currentTarget) return
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault()
            onOpen(it)
          }
        }}
        className="group flex min-h-[52px] w-full cursor-pointer items-center gap-3 rounded-control px-2.5 text-left transition-colors hover:bg-hover"
      >
        <OriginCell><Origin it={it} /></OriginCell>
        <span className="flex min-w-0 flex-1 items-center gap-2">
          <span className="truncate text-md text-fg" title={title}>{title}</span>
          {it.live && <LiveDot className="shrink-0" />}
        </span>
        <span className="flex max-w-[64%] shrink-0 items-center justify-end gap-3" {...stop}>
          <RowRight it={it} onStep={(s) => onOpen(it, s)} onConnect={onConnect} />
          {it.group === 'delivered' && <ArchiveButton it={it} />}
        </span>
      </div>
    </div>
  )
})

function Origin({ it }: { it: Item }) {
  if (isBtw(it)) return <BtwChip />
  if (it.issue) return <IssueKey issue={it.issue} className="min-w-0 shrink" />
  const source = (it.sources ?? []).find((s) => s.kind === 'branch' || (s.kind === 'pull_request' && !it.change))
  return source ? <SourceChip source={source} className="max-w-full shrink" /> : null
}

function BtwChip() {
  const t = useT()
  return (
    <span className="inline-flex h-6 min-w-0 items-center gap-1.5 rounded-control-sm bg-inset px-2 text-sm leading-none text-fg2">
      <Icon name="help" size={13} className="shrink-0" />
      <span className="truncate">{t('items.btwMark')}</span>
    </span>
  )
}

function ArchiveButton({ it }: { it: Item }) {
  const t = useT()
  const label = it.archivedAt ? t('items.unarchive') : t('items.archive')
  return (
    <button
      aria-label={label}
      title={label}
      onClick={() => void archiveToggle(it)}
      className="hit-44 flex size-7 shrink-0 items-center justify-center rounded-control-sm text-muted opacity-100 transition-[opacity,color] hover:text-fg focus-visible:opacity-100 sm:opacity-60 sm:group-hover:opacity-100 [@media(hover:none)]:opacity-100"
    >
      <Icon name={it.archivedAt ? 'refresh' : 'archive'} size={15} />
    </button>
  )
}
