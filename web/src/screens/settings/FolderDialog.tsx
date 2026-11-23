import { useEffect, useState } from 'react'
import { api } from '@/api'
import type { DirListing } from '@/api/contract'
import { errorText } from '@/api/store'
import { Button, IconButton } from '@/ui/button'
import { Spinner } from '@/ui/display'
import { Icon } from '@/ui/icon'
import { Dialog } from '@/ui/overlay'

export function FolderDialog({ open, start, onClose, onChoose }: { open: boolean; start: string | null; onClose: () => void; onChoose: (listing: DirListing) => void }) {
  const [path, setPath] = useState<string | null>(start)
  const [listing, setListing] = useState<DirListing | null>(null)
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    if (open) setPath(start)
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
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !o && onClose()}
      title="Diretório local"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>Cancelar</Button>
          <Button variant="primary" disabled={!shown} onClick={() => shown && onChoose(shown)}>Escolher</Button>
        </>
      }
    >
      <div className="flex items-center gap-1">
        <IconButton tooltip="Pasta de cima" size="sm" disabled={!shown?.parent} onClick={() => shown?.parent && setPath(shown.parent)}>
          <Icon name="up" size={16} />
        </IconButton>
        <IconButton tooltip="Início" size="sm" onClick={() => setPath(listing?.home ?? null)}>
          <Icon name="home" size={16} />
        </IconButton>
        <div className="ml-1 flex h-8 min-w-0 flex-1 items-center gap-2 rounded-control-sm bg-inset px-2.5">
          <Icon name={shown?.repo ? 'branch' : 'folder'} size={14} className="shrink-0 text-muted" />
          <span dir="rtl" className="truncate text-left font-mono text-sm text-fg2" data-testid="folder-path">
            <bdi>{shown?.path ?? path ?? ''}</bdi>
          </span>
        </div>
      </div>
      <ul aria-label="Pastas" className="mt-3 h-[300px] overflow-y-auto rounded-card bg-surface p-1 shadow-card">
        {!shown && !error && (
          <li className="flex h-full items-center justify-center text-muted"><Spinner /></li>
        )}
        {shown?.entries.map((entry) => (
          <li key={entry.path}>
            <button onClick={() => setPath(entry.path)} className="flex h-9 w-full items-center gap-2.5 rounded-control-sm px-2.5 text-left text-base text-fg2 transition-colors hover:bg-hover hover:text-fg">
              <Icon name={entry.repo ? 'branch' : 'folder'} size={16} className={entry.repo ? 'text-info-fg' : 'text-muted'} />
              <span className="min-w-0 flex-1 truncate">{entry.name}</span>
              <Icon name="right" size={14} className="text-faint" />
            </button>
          </li>
        ))}
        {shown && shown.entries.length === 0 && <li className="px-2.5 py-3 text-base text-faint">Pasta vazia</li>}
      </ul>
      {error && <p role="alert" className="mt-2 text-sm text-danger-fg">{error}</p>}
    </Dialog>
  )
}
