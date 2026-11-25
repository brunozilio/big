import { useEffect, useState } from 'react'
import { ROUTES, type ArtifactKind, type SessionEvent } from '@/api/contract'
import { useT } from '@/i18n'
import { Button, IconButton } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Spinner } from '@/ui/display'
import { Icon, type IconName } from '@/ui/icon'
import { MarkdownPreview } from '@/ui/markdownProse'
import { Dialog, Tooltip } from '@/ui/overlay'

type ArtifactEvent = Extract<SessionEvent, { type: 'artifact' }>

const kindIcons: Record<ArtifactKind, IconName> = { page: 'document', markdown: 'document', pdf: 'file', image: 'image', text: 'table' }

export function ArtifactCard({ event }: { event: ArtifactEvent }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const kind = event.kind ?? 'text'
  return (
    <>
      <div className="flex max-w-xl items-center gap-3 rounded-card bg-surface p-3 shadow-card">
        <button onClick={() => setOpen(true)} className="flex min-w-0 flex-1 items-center gap-3 text-left">
          <span className="grid size-10 shrink-0 place-items-center rounded-control bg-inset text-fg2"><Icon name={kindIcons[kind]} size={20} /></span>
          <span className="min-w-0">
            <span className="block truncate text-base font-medium text-fg">{event.title}</span>
            <span className="block truncate text-sm text-muted">{t(`chat.artifact_${kind}`)} · {event.name}</span>
          </span>
        </button>
        <IconLink tooltip={t('chat.artifactDownload')} href={ROUTES.artifactContent(event.artifactId, true)} download={event.name} icon="download" />
        <Button size="sm" onClick={() => setOpen(true)}>{t('chat.artifactOpen')}</Button>
      </div>
      <ArtifactViewer event={event} open={open} onOpenChange={setOpen} />
    </>
  )
}

function IconLink({ tooltip, href, icon, download, newTab }: { tooltip: string; href: string; icon: IconName; download?: string; newTab?: boolean }) {
  return (
    <Tooltip content={tooltip}>
      <a
        href={href}
        download={download}
        aria-label={tooltip}
        {...(newTab ? { target: '_blank', rel: 'noopener noreferrer' } : {})}
        className="inline-flex size-9 items-center justify-center rounded-control-sm text-muted transition-[background,color,transform] duration-150 hover:bg-hover hover:text-fg active:scale-[0.94]"
      >
        <Icon name={icon} size={18} />
      </a>
    </Tooltip>
  )
}

function ArtifactViewer({ event, open, onOpenChange }: { event: ArtifactEvent; open: boolean; onOpenChange: (open: boolean) => void }) {
  const t = useT()
  const source = ROUTES.artifactContent(event.artifactId)
  return (
    <Dialog open={open} onOpenChange={onOpenChange} size="xl" bare title={event.title}>
      <div className="flex h-[84dvh] flex-col">
        <div className="flex items-center gap-2 border-b px-4 py-3">
          <span className="min-w-0 flex-1 truncate text-md font-medium text-fg">{event.title}</span>
          <IconLink tooltip={t('chat.artifactNewTab')} href={source} icon="external" newTab />
          <IconLink tooltip={t('chat.artifactDownload')} href={ROUTES.artifactContent(event.artifactId, true)} download={event.name} icon="download" />
          <IconButton tooltip={t('editor.close')} onClick={() => onOpenChange(false)}><Icon name="close" size={18} /></IconButton>
        </div>
        <div className="min-h-0 flex-1 overflow-auto bg-bg">{open && <ArtifactBody kind={event.kind ?? 'text'} source={source} title={event.title} />}</div>
      </div>
    </Dialog>
  )
}

function ArtifactBody({ kind, source, title }: { kind: ArtifactKind; source: string; title: string }) {
  if (kind === 'page') return <iframe src={source} title={title} sandbox="allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox" className="size-full border-0 bg-white" />
  if (kind === 'pdf') return <iframe src={source} title={title} className="size-full border-0" />
  if (kind === 'image') return <div className="grid min-h-full place-items-center p-6"><img src={source} alt={title} className="max-w-full" /></div>
  return <ArtifactText source={source} markdown={kind === 'markdown'} />
}

function ArtifactText({ source, markdown }: { source: string; markdown: boolean }) {
  const t = useT()
  const [text, setText] = useState<string | null>(null)
  const [failed, setFailed] = useState(false)
  useEffect(() => {
    let live = true
    fetch(source)
      .then((res) => (res.ok ? res.text() : Promise.reject(new Error(res.statusText))))
      .then((body) => live && setText(body))
      .catch(() => live && setFailed(true))
    return () => {
      live = false
    }
  }, [source])
  if (failed) return <p className="p-6 text-base text-muted">{t('chat.artifactFailed')}</p>
  if (text === null) return <div className="grid h-full place-items-center"><Spinner size={20} /></div>
  if (markdown) return <MarkdownPreview text={text} className="mx-auto max-w-3xl px-6 py-8" />
  return <pre className={cn('whitespace-pre-wrap break-words p-6 font-mono text-sm text-fg')}>{text}</pre>
}
