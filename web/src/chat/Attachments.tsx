import { AnimatePresence, motion } from 'framer-motion'
import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from '@/api'
import { errorText } from '@/api/store'
import { cn } from '@/ui/cn'
import { Icon, type IconName } from '@/ui/icon'
import { dur, tween, tweenClose } from '@/ui/motion'
import { Tooltip } from '@/ui/overlay'
import { currentLanguage, translate, useT } from '@/i18n'

export type AttachmentKind = 'image' | 'pdf' | 'sheet' | 'slides' | 'code' | 'video' | 'audio' | 'archive' | 'file'

export interface Attachment {
  id: string
  name: string
  size: number
  kind: AttachmentKind
  url: string | null
  progress: number | null
  serverId: string | null
}

const kindMeta: Record<AttachmentKind, { icon: IconName; cls: string }> = {
  image: { icon: 'image', cls: 'text-cat-6 bg-cat-6/10' },
  pdf: { icon: 'document', cls: 'text-cat-5 bg-cat-5/10' },
  sheet: { icon: 'table', cls: 'text-cat-3 bg-cat-3/10' },
  slides: { icon: 'layout', cls: 'text-cat-4 bg-cat-4/10' },
  code: { icon: 'code', cls: 'text-cat-1 bg-cat-1/10' },
  video: { icon: 'play', cls: 'text-cat-2 bg-cat-2/10' },
  audio: { icon: 'mic', cls: 'text-cat-4 bg-cat-4/10' },
  archive: { icon: 'box', cls: 'text-muted bg-inset' },
  file: { icon: 'attach', cls: 'text-muted bg-inset' },
}

const extensions: [AttachmentKind, RegExp][] = [
  ['image', /\.(png|jpe?g|gif|webp|avif|svg|heic)$/i],
  ['pdf', /\.pdf$/i],
  ['sheet', /\.(xlsx?|csv|tsv|numbers|ods)$/i],
  ['slides', /\.(pptx?|key|odp)$/i],
  ['code', /\.(tsx?|jsx?|rs|py|go|java|kt|rb|php|cs|swift|json|ya?ml|toml|sql|sh|md)$/i],
  ['video', /\.(mp4|mov|webm|mkv|avi)$/i],
  ['audio', /\.(mp3|wav|m4a|ogg|flac)$/i],
  ['archive', /\.(zip|tar|gz|tgz|rar|7z)$/i],
]

export function kindOf(name: string, mime = ''): AttachmentKind {
  if (mime.startsWith('image/')) return 'image'
  if (mime.startsWith('video/')) return 'video'
  if (mime.startsWith('audio/')) return 'audio'
  return extensions.find(([, re]) => re.test(name))?.[0] ?? 'file'
}

let seq = 0

export const ATTACHMENTS_PER_MESSAGE = 10
const MAX_BYTES = 25 * 1024 * 1024

export type Uploader = (file: File, onProgress: (percent: number) => void) => Promise<{ id: string }>

export const uploadAttachment: Uploader = (file) => api().upload(file)

export function acceptedFiles(files: File[], already: number, refuse: (message: string) => void): File[] {
  const fits = files.filter((file) => {
    const refusal = file.size > MAX_BYTES ? translate(currentLanguage(), 'chat.tooLarge', { name: file.name }) : null
    if (refusal) refuse(refusal)
    return !refusal
  })
  const room = Math.max(0, ATTACHMENTS_PER_MESSAGE - already)
  if (fits.length > room) refuse(translate(currentLanguage(), 'chat.tooMany', { n: ATTACHMENTS_PER_MESSAGE }))
  return fits.slice(0, room)
}

export function useAttachments(refuse: (message: string) => void, upload: Uploader | null = uploadAttachment) {
  const [items, setItems] = useState<Attachment[]>([])
  const urls = useRef(new Set<string>())
  const count = useRef(0)
  count.current = items.length

  const patch = useCallback((id: string, change: Partial<Attachment>) => {
    setItems((cur) => cur.map((a) => (a.id === id ? { ...a, ...change } : a)))
  }, [])

  const add = useCallback(
    (files: File[]) => {
      const next = acceptedFiles(files, count.current, refuse).map((f): [Attachment, File] => {
        const kind = kindOf(f.name, f.type)
        const url = kind === 'image' ? URL.createObjectURL(f) : null
        if (url) urls.current.add(url)
        seq += 1
        return [{ id: `att-${seq}`, name: f.name, size: f.size, kind, url, progress: upload ? 0 : null, serverId: null }, f]
      })
      if (!next.length) return
      setItems((cur) => [...cur, ...next.map(([a]) => a)])
      if (!upload) return
      for (const [item, file] of next) {
        upload(file, (progress) => patch(item.id, { progress }))
          .then((done) => patch(item.id, { progress: 100, serverId: done.id }))
          .catch((error: unknown) => {
            refuse(errorText(error))
            setItems((cur) => cur.filter((a) => a.id !== item.id))
          })
      }
    },
    [patch, refuse, upload],
  )

  const remove = useCallback((id: string) => {
    setItems((cur) => {
      const gone = cur.find((a) => a.id === id)
      if (gone?.url) {
        URL.revokeObjectURL(gone.url)
        urls.current.delete(gone.url)
      }
      return cur.filter((a) => a.id !== id)
    })
  }, [])

  const clear = useCallback(() => setItems([]), [])

  const active = items.find((a) => a.progress !== 100)
  const activeId = active?.id ?? null

  useEffect(() => {
    if (!activeId || upload) return
    const t = setInterval(() => {
      setItems((cur) =>
        cur.map((a) => {
          if (a.id !== activeId) return a
          const p = a.progress ?? 0
          return { ...a, progress: Math.min(100, p + 3 + Math.round(Math.random() * 6)) }
        }),
      )
    }, 55)
    return () => clearInterval(t)
  }, [activeId, upload])

  useEffect(() => {
    const held = urls.current
    return () => held.forEach((u) => URL.revokeObjectURL(u))
  }, [])

  const serverIds = items.flatMap((a) => (a.serverId ? [a.serverId] : []))

  return { items, add, remove, clear, uploading: !!active, serverIds }
}

const TILE = 56
const RADIUS = 12

export function AttachmentStrip({ items, onRemove }: { items: Attachment[]; onRemove: (id: string) => void }) {
  return (
    <AnimatePresence initial={false}>
      {items.length > 0 && (
        <motion.div initial={{ height: 0, opacity: 0 }} animate={{ height: 'auto', opacity: 1 }} exit={{ height: 0, opacity: 0 }} transition={tween} className="overflow-hidden">
          <div className="flex gap-2 overflow-x-auto px-4 pb-1 pt-4 [scrollbar-width:none]">
            <AnimatePresence initial={false} mode="popLayout">
              {items.map((a) => (
                <AttachmentTile key={a.id} item={a} onRemove={() => onRemove(a.id)} />
              ))}
            </AnimatePresence>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  )
}

function AttachmentTile({ item, onRemove }: { item: Attachment; onRemove: () => void }) {
  const t = useT()
  const done = item.progress === 100
  const queued = item.progress === null
  const meta = kindMeta[item.kind]
  const isImage = item.kind === 'image' && item.url
  return (
    <motion.div
      layout
      initial={{ opacity: 0, scale: 0.96, filter: 'blur(2px)' }}
      animate={{ opacity: 1, scale: 1, filter: 'blur(0px)' }}
      exit={{ opacity: 0, scale: 0.96, filter: 'blur(0px)', transition: tweenClose }}
      transition={tween}
      style={{ width: TILE, height: TILE }}
      className="group relative shrink-0"
    >
      <Tooltip content={<span className="text-xs">{item.name}</span>}>
        <div
          style={{ borderRadius: RADIUS }}
          className={cn('relative size-full overflow-hidden transition-opacity', isImage ? 'bg-inset' : 'bg-surface shadow-[inset_0_0_0_1px_rgb(var(--border))]', queued && 'opacity-50')}
        >
          {isImage ? (
            <motion.img src={item.url ?? undefined} alt={item.name} animate={{ scale: done ? 1 : 1.06, filter: done ? 'saturate(1)' : 'saturate(0.6)' }} transition={tween} className="size-full object-cover" />
          ) : (
            <div className="flex size-full flex-col items-center justify-center gap-1 px-1">
              <span className={cn('flex size-6 items-center justify-center rounded-control-sm', meta.cls)}>
                <Icon name={meta.icon} size={14} strokeWidth={1.9} />
              </span>
              <span className="w-full truncate text-center text-2xs leading-[13px] text-muted">{item.name}</span>
            </div>
          )}
        </div>
      </Tooltip>

      <AnimatePresence>
        {!done && (
          <motion.svg key="ring" initial={{ opacity: 1 }} exit={{ opacity: 0, transition: tween }} className="pointer-events-none absolute inset-0 overflow-visible" width={TILE} height={TILE}>
            <rect x={0.75} y={0.75} width={TILE - 1.5} height={TILE - 1.5} rx={RADIUS - 0.5} fill="none" strokeWidth={1.5} className="stroke-[rgb(var(--border))]" />
            <motion.rect
              x={0.75}
              y={0.75}
              width={TILE - 1.5}
              height={TILE - 1.5}
              rx={RADIUS - 0.5}
              fill="none"
              strokeWidth={2}
              strokeLinecap="round"
              pathLength={100}
              strokeDasharray="100 100"
              initial={{ strokeDashoffset: 100 }}
              animate={{ strokeDashoffset: 100 - (item.progress ?? 0) }}
              transition={{ ease: 'linear', duration: dur.micro }}
              className="stroke-info"
            />
          </motion.svg>
        )}
      </AnimatePresence>

      <span className="absolute right-[3px] top-[3px] flex h-[16px] min-w-[16px] items-center justify-center">
        <AnimatePresence initial={false} mode="popLayout">
          {done ? (
            <motion.button
              key="x"
              aria-label={t('chat.remove', { name: item.name })}
              onClick={onRemove}
              initial={{ opacity: 0, filter: 'blur(2px)', scale: 0.96 }}
              animate={{ opacity: 1, filter: 'blur(0px)', scale: 1 }}
              exit={{ opacity: 0, filter: 'blur(2px)', scale: 0.96 }}
              transition={tween}
              whileTap={{ scale: 0.97 }}
              className={cn('flex size-[16px] items-center justify-center rounded-full', isImage ? 'bg-black/55 text-white backdrop-blur-sm' : 'text-faint hover:bg-hover hover:text-fg')}
            >
              <Icon name="close" size={10} strokeWidth={2.4} />
            </motion.button>
          ) : (
            <motion.span
              key="pct"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0, filter: 'blur(2px)' }}
              transition={tween}
              className={cn('rounded-[4px] px-[3px] text-2xs font-medium tabular-nums leading-[14px]', isImage ? 'bg-black/45 text-white' : 'text-info-fg')}
            >
              {queued ? '' : `${item.progress}%`}
            </motion.span>
          )}
        </AnimatePresence>
      </span>
    </motion.div>
  )
}
