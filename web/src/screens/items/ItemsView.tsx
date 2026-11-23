import { AnimatePresence, LayoutGroup, motion } from 'framer-motion'
import { useState, type ReactNode } from 'react'
import { BOARD_GROUPS, type BoardGroup, type Item } from '@/api/contract'
import { useStore } from '@/api/store'
import { cn } from '@/ui/cn'
import { StatusIcon, type StatusGlyph } from '@/ui/domain'
import { Icon } from '@/ui/icon'
import { Collapse, stagger, tween } from '@/ui/motion'
import { IssueKey, LiveDot, RowRight, stop } from './parts'

export const GROUP_LABEL: Record<BoardGroup, string> = { you: 'Com você', running: 'Em andamento', delivered: 'Entregue' }
const GROUP_GLYPH: Record<BoardGroup, StatusGlyph> = { you: 'in_review', running: 'in_progress', delivered: 'done' }

export function groupItems(items: Item[], group: BoardGroup): Item[] {
  const list = items.filter((it) => it.group === group)
  if (group !== 'delivered') return list
  return list.slice().sort((a, b) => (b.doneAt ?? '').localeCompare(a.doneAt ?? ''))
}

export function ItemsView({ onOpen, onConnect, fresh }: { onOpen: (it: Item, step?: string) => void; onConnect: (provider: string) => void; fresh: string | null }) {
  const items = useStore((s) => s.items)
  return (
    <LayoutGroup>
      <div className="space-y-7">
        {BOARD_GROUPS.map((g, i) => (
          <Group key={g} index={i} group={g} items={groupItems(items, g)}>
            {(it) => <Row it={it} fresh={fresh === it.id} onOpen={(s) => onOpen(it, s)} onConnect={onConnect} />}
          </Group>
        ))}
      </div>
    </LayoutGroup>
  )
}

function Group({ group, items, index, children }: { group: BoardGroup; items: Item[]; index: number; children: (it: Item) => ReactNode }) {
  const [open, setOpen] = useState(true)
  return (
    <motion.section layout="position" initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ ...tween, delay: stagger(index) }}>
      <button onClick={() => setOpen(!open)} className="mb-2 inline-flex h-8 items-center gap-2 rounded-control-sm bg-inset px-2.5 text-base font-medium text-fg transition-colors hover:bg-active">
        <StatusIcon status={GROUP_GLYPH[group]} size={15} />
        {GROUP_LABEL[group]}
        <span className="font-normal tabular-nums text-muted">{items.length}</span>
        <motion.span initial={false} animate={{ rotate: open ? 0 : -90 }} transition={tween} className="flex text-faint">
          <Icon name="down" size={14} />
        </motion.span>
      </button>
      <Collapse open={open}>
        <ul>
          <AnimatePresence initial={false}>
            {items.map((it) => (
              <motion.li key={it.id} layout layoutId={`item-${it.id}`} initial={{ opacity: 0, y: -6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} transition={tween}>
                {children(it)}
              </motion.li>
            ))}
          </AnimatePresence>
          {items.length === 0 && <li className="px-2.5 py-3 text-base text-faint">Nada aqui.</li>}
        </ul>
      </Collapse>
    </motion.section>
  )
}

function Row({ it, fresh, onOpen, onConnect }: { it: Item; fresh: boolean; onOpen: (s?: string) => void; onConnect: (provider: string) => void }) {
  return (
    <div className={cn('rounded-card transition-colors duration-200', fresh && 'flash-new')}>
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
        className="group flex h-[52px] w-full cursor-pointer items-center gap-3 rounded-control px-2.5 text-left transition-colors hover:bg-hover"
      >
        {it.issue && <IssueKey issue={it.issue} />}
        <span className="flex min-w-0 items-center gap-2">
          <span className="truncate text-md text-fg" title={it.title}>{it.title}</span>
          {it.live && <LiveDot />}
        </span>
        <span className="ml-auto flex max-w-[60%] shrink-0 items-center justify-end gap-3" {...stop}>
          <RowRight it={it} onStep={(s) => onOpen(s)} onConnect={onConnect} />
        </span>
      </div>
    </div>
  )
}
