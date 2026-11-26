import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { useStore } from '@/api/store'
import { useT } from '@/i18n'
import { titleOf } from '@/i18n/server'
import { cn } from '@/ui/cn'
import { Kbd } from '@/ui/display'
import { Icon } from '@/ui/icon'
import { Dialog } from '@/ui/overlay'
import { filterEntries, paletteEntries, type PaletteEntry } from './palette'

export function CommandPalette({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useT()
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()} bare placement="top" title={t('app.palette')}>
      <PaletteBody onClose={onClose} />
    </Dialog>
  )
}

function usePaletteEntries(): PaletteEntry[] {
  const t = useT()
  const items = useStore((s) => s.items)
  return useMemo(
    () => paletteEntries(items, { deliveries: t('app.deliveries'), security: t('app.security'), settings: t('app.settings'), tab: (tab) => t(`settings.tab_${tab}`), title: (it) => titleOf(t, it) }),
    [items, t],
  )
}

function PaletteBody({ onClose }: { onClose: () => void }) {
  const t = useT()
  const navigate = useNavigate()
  const listId = useId()
  const entries = usePaletteEntries()
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const list = useRef<HTMLDivElement>(null)
  const found = useMemo(() => filterEntries(entries, query), [entries, query])
  const index = Math.min(active, Math.max(0, found.length - 1))
  const go = (entry: PaletteEntry | undefined) => {
    if (!entry) return
    onClose()
    navigate(entry.to)
  }
  useEffect(() => {
    list.current?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView?.({ block: 'nearest' })
  }, [index, found])
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      if (found.length === 0) return
      setActive((index + (e.key === 'ArrowDown' ? 1 : found.length - 1)) % found.length)
      return
    }
    if (e.key === 'Home' || e.key === 'End') {
      if (query) return
      e.preventDefault()
      setActive(e.key === 'Home' ? 0 : found.length - 1)
      return
    }
    if (e.key === 'Enter' && !e.nativeEvent.isComposing) {
      e.preventDefault()
      go(found[index])
    }
  }
  return (
    <div className="flex flex-col">
      <div className="flex h-12 shrink-0 items-center gap-2.5 border-b px-4">
        <Icon name="search" size={17} className="text-muted" />
        <input
          role="combobox"
          aria-expanded
          aria-controls={listId}
          aria-activedescendant={found[index] ? `${listId}-${index}` : undefined}
          aria-autocomplete="list"
          aria-label={t('app.palette')}
          autoComplete="off"
          spellCheck={false}
          value={query}
          placeholder={t('app.paletteHint')}
          onChange={(e) => {
            setQuery(e.target.value)
            setActive(0)
          }}
          onKeyDown={onKeyDown}
          className="h-full min-w-0 flex-1 bg-transparent text-md text-fg outline-none placeholder:text-muted"
        />
        <Kbd>esc</Kbd>
      </div>
      <div ref={list} id={listId} role="listbox" aria-label={t('app.palette')} className="scrollbar-thin max-h-[min(52dvh,420px)] overflow-y-auto p-1.5">
        {found.length === 0 && <p className="px-3 py-6 text-center text-base text-muted">{t('app.paletteEmpty')}</p>}
        {found.map((entry, i) => (
          <PaletteRow key={entry.id} id={`${listId}-${i}`} entry={entry} active={i === index} heading={found[i - 1]?.kind !== entry.kind} onHover={() => setActive(i)} onPick={() => go(entry)} />
        ))}
      </div>
    </div>
  )
}

function PaletteRow({ id, entry, active, heading, onHover, onPick }: { id: string; entry: PaletteEntry; active: boolean; heading: boolean; onHover: () => void; onPick: () => void }) {
  const t = useT()
  return (
    <>
      {heading && <p role="presentation" className="px-2.5 pb-1 pt-2 text-xs text-muted">{entry.kind === 'page' ? t('app.palettePages') : t('app.paletteItems')}</p>}
      <div
        id={id}
        role="option"
        aria-selected={active}
        onMouseMove={() => !active && onHover()}
        onMouseDown={(e) => e.preventDefault()}
        onClick={onPick}
        className={cn('flex h-10 cursor-pointer items-center gap-2.5 rounded-control-sm px-2.5 text-base text-fg2', active && 'bg-hover text-fg')}
      >
        <Icon name={entry.kind === 'page' ? 'right' : 'task'} size={15} className="text-muted" />
        <span className="min-w-0 flex-1 truncate">{entry.label}</span>
        {entry.hint && <span className="shrink-0 text-sm tabular-nums text-muted">{entry.hint}</span>}
      </div>
    </>
  )
}
