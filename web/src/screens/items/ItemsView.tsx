import { AnimatePresence, LayoutGroup, motion } from 'framer-motion'
import { useState, type ReactNode } from 'react'
import { BOARD_GROUPS, BTW_WORKFLOW, type BoardGroup, type Item } from '@/api/contract'
import { useStore } from '@/api/store'
import { useT, type T } from '@/i18n'
import { titleOf } from '@/i18n/server'
import { cn } from '@/ui/cn'
import { StatusIcon, type StatusGlyph } from '@/ui/domain'
import { SegmentedControl } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { Collapse, stagger, tween } from '@/ui/motion'
import { grouping, usePackageBoard, type PackageBoard } from './packages/board'
import { GroupRow } from './packages/GroupRow'
import { PackageTray } from './packages/PackageTray'
import { archiveToggle, IssueKey, LiveDot, RowRight, SourceChip, stop } from './parts'

export function groupLabel(t: T, group: BoardGroup): string {
  return t(`items.group_${group}`)
}
const GROUP_GLYPH: Record<BoardGroup, StatusGlyph> = { you: 'in_review', running: 'in_progress', delivered: 'done' }

export function groupItems(items: Item[], group: BoardGroup): Item[] {
  const list = items.filter((it) => it.group === group)
  if (group !== 'delivered') return list
  return list.slice().sort((a, b) => (b.doneAt ?? '').localeCompare(a.doneAt ?? ''))
}

export type Kind = 'all' | 'work' | 'btw'
export type Filter = { kind: Kind; archived: boolean }

export const isBtw = (it: Item) => it.workflow === BTW_WORKFLOW

export function filterItems(items: Item[], filter: Filter): Item[] {
  return items.filter((it) => {
    if (filter.archived !== !!it.archivedAt) return false
    if (filter.kind === 'btw') return isBtw(it)
    if (filter.kind === 'work') return !isBtw(it)
    return true
  })
}

export function ItemsView({ onOpen, onConnect, fresh }: { onOpen: (it: Item, step?: string) => void; onConnect: (provider: string) => void; fresh: string | null }) {
  const items = useStore((s) => s.items)
  const t = useT()
  const [filter, setFilter] = useState<Filter>({ kind: 'all', archived: false })
  const board = usePackageBoard(items)
  const live = filterItems(items, { ...filter, archived: false }).filter((it) => !board.hidden.has(it.id))
  const archived = filterItems(items, { ...filter, archived: true })
  const showArchived = (g: BoardGroup) => g === 'delivered' && filter.archived
  const toggle = <ArchivedToggle on={filter.archived} count={items.filter((it) => it.archivedAt).length} onChange={(on) => setFilter({ ...filter, archived: on })} />
  return (
    <LayoutGroup>
      <Kinds filter={filter} onChange={setFilter} />
      <div className="space-y-7">
        {BOARD_GROUPS.map((g, i) => (
          <Group
            key={g}
            index={i}
            group={g}
            items={groupItems(showArchived(g) ? archived : live, g)}
            action={g === 'delivered' ? <>{toggle}<BuildPackage board={board} /></> : null}
            top={g === 'delivered' && !filter.archived ? <Tray board={board} items={items} /> : null}
            empty={showArchived(g) ? t('items.noArchived') : t('items.emptyGroup')}
          >
            {(it) => grouping(it) ? <GroupRow it={it} members={board.members(it)} onOpen={() => onOpen(it)} onOpenMember={(member) => onOpen(member)} onDropItem={(id, from) => board.move(it, id, from)} /> : <Row it={it} board={board} fresh={fresh === it.id} onOpen={(s) => onOpen(it, s)} onConnect={onConnect} />}
          </Group>
        ))}
      </div>
    </LayoutGroup>
  )
}

function Kinds({ filter, onChange }: { filter: Filter; onChange: (f: Filter) => void }) {
  const t = useT()
  const kinds: { value: Kind; label: string }[] = [
    { value: 'all', label: t('items.filterAll') },
    { value: 'work', label: t('items.filterWork') },
    { value: 'btw', label: t('items.filterBtw') },
  ]
  return (
    <div className="mb-5 flex flex-wrap items-center gap-2">
      <SegmentedControl size="sm" value={filter.kind} onChange={(kind) => onChange({ ...filter, kind })} options={kinds} />
    </div>
  )
}

function BuildPackage({ board }: { board: PackageBoard }) {
  const t = useT()
  const on = board.draft !== null
  return (
    <button aria-pressed={on} onClick={on ? board.close : board.open} className={cn('hit-44 inline-flex h-8 items-center gap-1.5 rounded-control px-2.5 text-sm transition-colors', on ? 'bg-active text-fg' : 'text-muted hover:bg-hover hover:text-fg2')}>
      <Icon name="package" size={14} />
      {t('items.buildPackage')}
    </button>
  )
}

function Tray({ board, items }: { board: PackageBoard; items: Item[] }) {
  const t = useT()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const chosen = (board.draft ?? []).map((id) => items.find((it) => it.id === id)).filter((it): it is Item => !!it)
  const create = () => {
    setBusy(true)
    setError(null)
    void board.build(t('items.newPackage'), 'auto').then((failed) => {
      setError(failed)
      setBusy(false)
    })
  }
  return (
    <AnimatePresence initial={false}>
      {board.draft !== null && <PackageTray chosen={chosen} busy={busy} error={error} onAdd={board.add} onReturn={board.remove} onCancel={board.close} onCreate={create} />}
    </AnimatePresence>
  )
}

function ArchivedToggle({ on, count, onChange }: { on: boolean; count: number; onChange: (on: boolean) => void }) {
  const t = useT()
  return (
    <button
      aria-pressed={on}
      onClick={() => onChange(!on)}
      className={cn('hit-44 inline-flex h-8 items-center gap-1.5 rounded-control px-2.5 text-sm transition-colors', on ? 'bg-active text-fg' : 'text-muted hover:bg-hover hover:text-fg2')}
    >
      <Icon name="archive" size={14} />
      {t('items.filterArchived')}
      <span className="tabular-nums text-faint">{count}</span>
    </button>
  )
}

type GroupProps = { group: BoardGroup; items: Item[]; index: number; action: ReactNode; top?: ReactNode; empty: string; children: (it: Item) => ReactNode }

function Group({ group, items, index, action, top, empty, children }: GroupProps) {
  const [open, setOpen] = useState(true)
  const t = useT()
  return (
    <motion.section layout="position" initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ ...tween, delay: stagger(index) }}>
      <div className="mb-2 flex items-center gap-2">
        <button onClick={() => setOpen(!open)} className="inline-flex h-8 items-center gap-2 rounded-control-sm bg-inset px-2.5 text-base font-medium text-fg transition-colors hover:bg-active">
          <StatusIcon status={GROUP_GLYPH[group]} size={15} />
          {groupLabel(t, group)}
          <span className="font-normal tabular-nums text-muted">{items.length}</span>
          <motion.span initial={false} animate={{ rotate: open ? 0 : -90 }} transition={tween} className="flex text-faint">
            <Icon name="down" size={14} />
          </motion.span>
        </button>
        {action}
      </div>
      <Collapse open={open}>
        {top}
        <ul>
          <AnimatePresence initial={false}>
            {items.map((it) => (
              <motion.li key={it.id} layout layoutId={`item-${it.id}`} initial={{ opacity: 0, y: -6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} transition={tween}>
                {children(it)}
              </motion.li>
            ))}
          </AnimatePresence>
          {items.length === 0 && <li className="px-2.5 py-3 text-base text-faint">{empty}</li>}
        </ul>
      </Collapse>
    </motion.section>
  )
}

function Row({ it, board, fresh, onOpen, onConnect }: { it: Item; board: PackageBoard; fresh: boolean; onOpen: (s?: string) => void; onConnect: (provider: string) => void }) {
  const t = useT()
  return (
    <div {...board.draggable(it)} className={cn('rounded-card transition-colors duration-200', fresh && 'flash-new', board.draggable(it).draggable && 'cursor-grab active:cursor-grabbing')}>
      <div
        role="button"
        tabIndex={0}
        onClick={() => onOpen()}
        onKeyDown={(e) => {
          if (e.target !== e.currentTarget) return
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault()
            onOpen()
          }
        }}
        className="group flex min-h-[52px] w-full cursor-pointer items-center gap-3 rounded-control px-2.5 text-left transition-colors hover:bg-hover"
      >
        {isBtw(it) ? <BtwChip /> : it.issue ? <IssueKey issue={it.issue} /> : <OriginChip it={it} />}
        <span className="flex min-w-0 items-center gap-2">
          <span className="truncate text-md text-fg" title={titleOf(t, it)}>{titleOf(t, it)}</span>
          {it.live && <LiveDot />}
        </span>
        <span className="ml-auto flex max-w-[60%] shrink-0 items-center justify-end gap-3" {...stop}>
          <RowRight it={it} onStep={(s) => onOpen(s)} onConnect={onConnect} />
          {it.group === 'delivered' && <ArchiveButton it={it} />}
        </span>
      </div>
    </div>
  )
}

function BtwChip() {
  const t = useT()
  return (
    <span className="inline-flex h-6 shrink-0 items-center gap-1.5 rounded-control-sm bg-inset px-2 text-sm leading-none text-fg2">
      <Icon name="help" size={13} />
      {t('items.btwMark')}
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
      onClick={() => void archiveToggle(it).catch(() => undefined)}
      className="flex size-7 items-center justify-center rounded-control-sm text-faint opacity-0 transition-[opacity,color] hover:text-fg focus-visible:opacity-100 group-hover:opacity-100"
    >
      <Icon name={it.archivedAt ? 'refresh' : 'archive'} size={15} />
    </button>
  )
}

function OriginChip({ it }: { it: Item }) {
  const source = (it.sources ?? []).find((s) => s.kind === 'branch' || (s.kind === 'pull_request' && !it.change))
  return source ? <SourceChip source={source} /> : null
}
