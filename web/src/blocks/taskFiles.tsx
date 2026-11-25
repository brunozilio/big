import { useRef } from 'react'
import type { TaskFile } from '@/api/contract'
import { useT } from '@/i18n'
import { cn } from '@/ui/cn'
import { Icon } from '@/ui/icon'

function size(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

export function TaskFiles({ files, locked, onChange }: { files: TaskFile[]; locked: boolean; onChange: (files: TaskFile[]) => void }) {
  const t = useT()
  const input = useRef<HTMLInputElement>(null)
  if (files.length === 0 && locked) return null
  return (
    <span className="mt-1 flex flex-wrap items-center gap-1.5">
      {files.map((file, i) => (
        <span key={`${file.name}-${i}`} className="inline-flex h-6 max-w-[260px] items-center gap-1.5 rounded-control-sm bg-inset pl-2 pr-1 text-xs text-fg2">
          <Icon name="attach" size={12} className="shrink-0 text-muted" />
          <span className="truncate">{file.name}</span>
          <span className="shrink-0 tabular-nums text-faint">{size(file.size)}</span>
          {!locked && (
            <button aria-label={t('blocks.removeFile', { name: file.name })} onClick={() => onChange(files.filter((_, j) => j !== i))} className="flex size-4 shrink-0 items-center justify-center rounded-sm text-faint transition-colors hover:bg-active hover:text-fg">
              <Icon name="close" size={11} />
            </button>
          )}
        </span>
      ))}
      {!locked && (
        <>
          <input ref={input} type="file" multiple hidden onChange={(e) => {
            const picked = [...(e.target.files ?? [])].map((f) => ({ name: f.name, size: f.size }))
            if (picked.length) onChange([...files, ...picked])
            e.target.value = ''
          }} />
          <button onClick={() => input.current?.click()} className={cn('inline-flex h-6 items-center gap-1 rounded-control-sm px-1.5 text-xs text-muted transition-colors hover:bg-hover hover:text-fg', files.length === 0 && '-ml-1.5')}>
            <Icon name="attach" size={12} />
            {t('blocks.attach')}
          </button>
        </>
      )}
    </span>
  )
}
