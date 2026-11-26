import * as RDialog from '@radix-ui/react-dialog'
import { AnimatePresence, motion } from 'framer-motion'
import { useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { Button } from './button'
import { plural, useT } from '@/i18n'
import { cn } from './cn'
import { Icon } from './icon'
import { markdownStats } from './markdownOps'
import { SourceEditor, type SourceView } from './markdownSource'
import { dur, easeOut, tween, tweenClose } from './motion'
import { Tooltip } from './overlay'
import { RichEditor } from './richEditor'

export { MarkdownPreview, proseClass } from './markdownProse'

export type MarkdownEditorMode = 'rich' | 'source'

export interface MarkdownEditorProps {
  value: string
  onChange: (v: string) => void
  placeholder?: string
  autoFocus?: boolean
  label?: string
  compact?: boolean
  minHeight?: number
  maxHeight?: number
  defaultMode?: MarkdownEditorMode
  defaultSourceView?: SourceView
  readOnly?: boolean
  assist?: string
  onSave?: () => void
  onCancel?: () => void
  saveLabel?: string
  saveDisabled?: boolean
  className?: string
}

function StatusBar({ value }: { value: string }) {
  const t = useT()
  const s = markdownStats(value)
  return (
    <span className="flex min-w-0 items-center gap-3 truncate text-xs tabular-nums text-muted">
      <span>
        {t(`editor.words_${plural(s.words)}`, { n: s.words })}
      </span>
      <span>
        {t(`editor.chars_${plural(s.chars)}`, { n: s.chars })}
      </span>
      <span>
        {t(`editor.lines_${plural(s.lines)}`, { n: s.lines })}
      </span>
      <span className="font-medium text-faint">Markdown</span>
    </span>
  )
}

export function MarkdownEditor({
  value,
  onChange,
  placeholder,
  autoFocus,
  label,
  compact,
  minHeight,
  maxHeight,
  defaultMode = 'rich',
  defaultSourceView,
  readOnly: readOnlyProp,
  assist,
  onSave,
  onCancel,
  saveLabel,
  saveDisabled,
  className,
}: MarkdownEditorProps) {
  const t = useT()
  const [mode] = useState<MarkdownEditorMode>(defaultMode)
  const [full, setFull] = useState(false)
  const root = useRef<HTMLDivElement>(null)
  const [inDisabledFieldset, setInDisabledFieldset] = useState(false)
  useLayoutEffect(() => {
    if (root.current) setInDisabledFieldset(!!root.current.closest('fieldset:disabled'))
  })
  const readOnly = readOnlyProp || inDisabledFieldset
  const min = minHeight ?? (compact ? 120 : 360)
  const max = maxHeight ?? (compact ? 360 : 720)

  const more = null

  const actions = (onSave || onCancel) && (
    <div className="ml-auto flex shrink-0 gap-2">
      {onCancel && (
        <Button size="sm" variant="ghost" onClick={onCancel}>
          {t('editor.cancel')}
        </Button>
      )}
      {onSave && (
        <Tooltip content="⌘↵">
          <Button size="sm" variant="primary" disabled={saveDisabled} onClick={onSave}>
            {saveLabel ?? t('editor.save')}
          </Button>
        </Tooltip>
      )}
    </div>
  )

  const frame = (fill: boolean): ReactNode => {
    const shared = { value, onChange, placeholder, label: label ?? t('editor.contentLabel'), compact: compact && !fill, minHeight: min, maxHeight: fill ? undefined : max, fill, trailing: readOnly ? undefined : more, onSubmit: onSave, autoFocus: fill || autoFocus }
    return (
      <div ref={fill ? undefined : root} className={cn('ring-field-within flex flex-col overflow-hidden border bg-surface transition-shadow', fill ? 'h-full rounded-overlay' : compact ? 'rounded-control' : 'rounded-card', !fill && className)}>
        {mode === 'rich' || readOnly ? <RichEditor {...shared} readOnly={readOnly} assist={assist} /> : <SourceEditor {...shared} defaultView={defaultSourceView ?? (compact ? 'write' : 'split')} />}
        {!readOnly && (mode === 'source' || actions) && (
          <div className="flex min-h-11 items-center gap-3 border-t px-3 py-1.5">
            {mode === 'source' && <StatusBar value={value} />}
            {actions}
          </div>
        )}
      </div>
    )
  }

  return (
    <>
      {full ? (
        <div className={cn('flex items-center justify-center gap-2 rounded-card border border-dashed text-sm text-muted', className)} style={{ minHeight: Math.min(min, 160) }}>
          <Icon name="fullscreen" size={15} /> {t('editor.fullscreen')}
        </div>
      ) : (
        frame(false)
      )}
      <RDialog.Root open={full} onOpenChange={setFull}>
        <AnimatePresence>
          {full && (
            <RDialog.Portal forceMount>
              <RDialog.Overlay asChild forceMount>
                <motion.div className="fixed inset-0 z-40 bg-scrim" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: dur.quick, ease: easeOut }} />
              </RDialog.Overlay>
              <RDialog.Content asChild forceMount aria-describedby={undefined}>
                <motion.div
                  initial={{ opacity: 0, scale: 0.98 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={{ opacity: 0, scale: 0.98, transition: tweenClose }}
                  transition={tween}
                  className="fixed inset-2 z-50 flex flex-col outline-none md:inset-6"
                >
                  <RDialog.Title className="sr-only">{label}</RDialog.Title>
                  {frame(true)}
                </motion.div>
              </RDialog.Content>
            </RDialog.Portal>
          )}
        </AnimatePresence>
      </RDialog.Root>
    </>
  )
}
