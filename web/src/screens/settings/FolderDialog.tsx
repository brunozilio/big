import { useEffect, useState, type ReactNode } from 'react'
import { api } from '@/api'
import type { DirEntry, DirListing } from '@/api/contract'
import { errorText } from '@/api/store'
import { Button, IconButton } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Spinner } from '@/ui/display'
import { Checkbox } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { Dialog, Popover } from '@/ui/overlay'
import { useT } from '@/i18n'

function EntryRow({ entry, picked, onOpen, onPick }: { entry: DirEntry; picked: boolean | null; onOpen: () => void; onPick: (next: boolean) => void }) {
  return (
    <li className="flex items-center gap-1 rounded-control-sm pl-2.5 transition-colors hover:bg-hover">
      {picked !== null && <Checkbox checked={picked} label={entry.name} onCheckedChange={onPick} />}
      <button onClick={onOpen} className="flex h-9 min-w-0 flex-1 items-center gap-2.5 px-1.5 text-left text-base text-fg2 hover:text-fg">
        <Icon name={entry.repo ? 'branch' : 'folder'} size={16} className={entry.repo ? 'text-info-fg' : 'text-muted'} />
        <span className="min-w-0 flex-1 truncate">{entry.name}</span>
        <Icon name="right" size={14} className="mr-1 text-faint" />
      </button>
    </li>
  )
}

function toggled(picked: string[], path: string, on: boolean): string[] {
  return on ? [...picked.filter((p) => p !== path), path] : picked.filter((p) => p !== path)
}

interface Choosing {
  onChoose: (listing: DirListing) => void
  onChooseMany?: (paths: string[]) => void
}

function useFolders(open: boolean, start: string | null, { onChoose, onChooseMany }: Choosing) {
  const [path, setPath] = useState<string | null>(start)
  const [listing, setListing] = useState<DirListing | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [picked, setPicked] = useState<string[]>([])
  useEffect(() => {
    if (open) {
      setPath(start)
      setPicked([])
    }
  }, [open, start])
  useEffect(() => {
    if (!open) return
    let alive = true
    setError(null)
    api()
      .dirs(path)
      .then((found) => alive && setListing(found))
      .catch((e: unknown) => {
        if (!alive) return
        setError(errorText(e))
        if (path !== null && path === start) setPath(null)
      })
    return () => {
      alive = false
    }
  }, [open, path, start])
  const shown = listing && (path === null || listing.path === path) ? listing : null
  const many = onChooseMany !== undefined
  const choose = () => {
    if (many && picked.length > 0) onChooseMany(picked)
    else if (shown) onChoose(shown)
  }
  return { path, setPath, listing, shown, error, picked, setPicked, many, choose }
}

type Folders = ReturnType<typeof useFolders>

function FolderList({ folders, compact }: { folders: Folders; compact?: boolean }) {
  const t = useT()
  const { path, setPath, listing, shown, error, picked, setPicked, many } = folders
  return (
    <>
      <div className="flex items-center gap-1">
        <IconButton tooltip={t('settings.folder_up')} size="sm" disabled={!shown?.parent} onClick={() => shown?.parent && setPath(shown.parent)}>
          <Icon name="up" size={16} />
        </IconButton>
        <IconButton tooltip={t('settings.folder_home')} size="sm" onClick={() => setPath(listing?.home ?? null)}>
          <Icon name="home" size={16} />
        </IconButton>
        <div className="ml-1 flex h-8 min-w-0 flex-1 items-center gap-2 rounded-control-sm bg-inset px-2.5">
          <Icon name={shown?.repo ? 'branch' : 'folder'} size={14} className="shrink-0 text-muted" />
          <span dir="rtl" className="truncate text-left font-mono text-sm text-fg2" data-testid="folder-path">
            <bdi>{shown?.path ?? path ?? ''}</bdi>
          </span>
        </div>
      </div>
      <ul aria-label={t('settings.folder_list')} className={cn('mt-3 overflow-y-auto rounded-card bg-surface p-1 shadow-card', compact ? 'h-[240px]' : 'h-[300px]')}>
        {!shown && !error && (
          <li className="flex h-full items-center justify-center text-muted"><Spinner /></li>
        )}
        {shown?.entries.map((entry) => (
          <EntryRow key={entry.path} entry={entry} picked={many ? picked.includes(entry.path) : null} onOpen={() => setPath(entry.path)} onPick={(on) => setPicked((p) => toggled(p, entry.path, on))} />
        ))}
        {shown && shown.entries.length === 0 && <li className="px-2.5 py-3 text-base text-faint">{t('settings.folder_empty')}</li>}
      </ul>
      {many && <p className="mt-2 text-sm text-muted">{picked.length > 0 ? t('settings.folder_picked', { n: picked.length }) : t('settings.folder_pickHint')}</p>}
      {error && <p role="alert" className="mt-2 text-sm text-danger-fg">{error}</p>}
    </>
  )
}

function FolderActions({ folders, onClose }: { folders: Folders; onClose: () => void }) {
  const t = useT()
  const { shown, picked, many, choose } = folders
  return (
    <>
      <Button variant="ghost" onClick={onClose}>{t('settings.cancel')}</Button>
      <Button variant="primary" disabled={!shown && picked.length === 0} onClick={choose}>{many && picked.length > 0 ? t('settings.folder_addPicked', { n: picked.length }) : t('settings.choose')}</Button>
    </>
  )
}

export function FolderDialog({ open, start, onClose, onChoose, onChooseMany }: Choosing & { open: boolean; start: string | null; onClose: () => void }) {
  const t = useT()
  const folders = useFolders(open, start, { onChoose, onChooseMany })
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()} title={t('settings.folder_title')} footer={<FolderActions folders={folders} onClose={onClose} />}>
      <FolderList folders={folders} />
    </Dialog>
  )
}

export function FolderPopover({ open, start, trigger, onOpenChange, onChoose, onChooseMany }: Choosing & { open: boolean; start: string | null; trigger: ReactNode; onOpenChange: (open: boolean) => void }) {
  const folders = useFolders(open, start, { onChoose, onChooseMany })
  return (
    <Popover open={open} onOpenChange={onOpenChange} trigger={trigger} className="w-[min(26rem,calc(100vw-2rem))] p-3">
      <FolderList folders={folders} compact />
      <div className="mt-3 flex justify-end gap-2">
        <FolderActions folders={folders} onClose={() => onOpenChange(false)} />
      </div>
    </Popover>
  )
}
