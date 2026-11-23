import { useEffect, useRef, useState, type ReactNode } from 'react'
import { cn } from '@/ui/cn'
import { Icon } from '@/ui/icon'
import { AnimatePresence, dur, easeOut, IconSwap, motion, tween } from '@/ui/motion'
import { Tooltip } from '@/ui/overlay'
import { AttachmentStrip, useAttachments } from './Attachments'
import { ModelPicker } from './ModelPicker'

export interface ComposerSubmit {
  text: string
  attachments: string[]
  model: string | null
}

export function Composer({
  onSubmit,
  running,
  onStop,
  placeholder,
  disabled,
  autoFocus,
  leading,
  allowEmpty,
  model,
  onModel,
}: {
  onSubmit: (input: ComposerSubmit) => void
  running?: boolean
  onStop?: () => void
  placeholder?: string
  disabled?: boolean
  autoFocus?: boolean
  leading?: ReactNode
  allowEmpty?: boolean
  model?: string | null
  onModel?: (id: string) => void
}) {
  const [text, setText] = useState('')
  const [refusal, setRefusal] = useState<string | null>(null)
  const attachments = useAttachments(setRefusal)
  const [dragging, setDragging] = useState(false)
  const [focused, setFocused] = useState(false)
  const [ownModel, setOwnModel] = useState<string | null>(null)
  const ref = useRef<HTMLTextAreaElement>(null)
  const fileInput = useRef<HTMLInputElement>(null)
  const chosen = model === undefined ? ownModel : model
  const chooseModel = onModel ?? setOwnModel

  useEffect(() => {
    const el = ref.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, 260)}px`
  }, [text])

  const hasText = text.trim().length > 0
  const ready = (hasText || !!allowEmpty) && !disabled && !attachments.uploading

  const submit = () => {
    if (!ready) return
    onSubmit({ text: text.trim(), attachments: attachments.serverIds, model: chosen })
    setText('')
    setRefusal(null)
    attachments.clear()
  }

  return (
    <div className="relative">
      <motion.div
        className={cn('relative z-10 rounded-overlay border border-line bg-surface transition-colors', focused && 'border-line-strong', dragging && 'border-info shadow-[0_0_0_4px_rgb(var(--info)/0.12)]')}
        onDragOver={(e) => {
          if (!e.dataTransfer.types.includes('Files')) return
          e.preventDefault()
          setDragging(true)
        }}
        onDragLeave={(e) => {
          if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setDragging(false)
        }}
        onDrop={(e) => {
          if (!e.dataTransfer.files.length) return
          e.preventDefault()
          setDragging(false)
          attachments.add(Array.from(e.dataTransfer.files))
        }}
      >
        <AttachmentStrip items={attachments.items} onRemove={attachments.remove} />
        <textarea
          ref={ref}
          autoFocus={autoFocus}
          rows={1}
          value={text}
          disabled={disabled}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault()
              submit()
            }
          }}
          onPaste={(e) => {
            const pasted = Array.from(e.clipboardData.files)
            if (pasted.length) {
              e.preventDefault()
              attachments.add(pasted)
            }
          }}
          placeholder={placeholder ?? 'No que vamos trabalhar?'}
          aria-label="Mensagem"
          className="relative block max-h-[260px] min-h-[76px] w-full resize-none bg-transparent px-5 pb-3 pt-[18px] text-lg leading-[26px] text-fg caret-fg outline-none [scrollbar-width:none] placeholder:text-muted disabled:cursor-not-allowed"
        />
        <input
          ref={fileInput}
          type="file"
          multiple
          hidden
          onChange={(e) => {
            attachments.add(Array.from(e.target.files ?? []))
            e.target.value = ''
          }}
        />
        <div className="flex items-center gap-1 px-2.5 pb-3">
          <Tooltip content="Anexar arquivo">
            <button aria-label="Anexar arquivo" onClick={() => fileInput.current?.click()} className="flex size-8 items-center justify-center rounded-full text-fg2 transition-colors hover:bg-inset hover:text-fg">
              <Icon name="attach" size={18} />
            </button>
          </Tooltip>
          {leading}
          <span className="ml-auto flex items-center">
            <ModelPicker value={chosen} onChange={chooseModel} />
            <SendButton running={!!running} ready={ready} onSend={submit} onStop={onStop} />
          </span>
        </div>
      </motion.div>
      <AnimatePresence>
        {refusal && (
          <motion.p initial={{ opacity: 0, y: -4 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} transition={tween} role="alert" className="mt-2 px-2 text-sm text-danger-fg">
            {refusal}
          </motion.p>
        )}
      </AnimatePresence>
    </div>
  )
}

function SendButton({ running, ready, onSend, onStop }: { running: boolean; ready: boolean; onSend: () => void; onStop?: () => void }) {
  return (
    <Tooltip content={running ? 'Parar' : 'Enviar  ↵'}>
      <motion.button
        aria-label={running ? 'Parar' : 'Enviar'}
        disabled={!running && !ready}
        onClick={running ? onStop : onSend}
        whileTap={{ scale: 0.97 }}
        animate={{ backgroundColor: running || ready ? 'rgb(var(--primary))' : 'rgb(var(--border-strong))', color: running || ready ? 'rgb(var(--primary-fg))' : 'rgb(255 255 255)' }}
        transition={{ duration: dur.quick, ease: easeOut }}
        className="relative ml-1 flex size-9 items-center justify-center rounded-full"
      >
        <IconSwap k={running ? 'stop' : 'send'}>{running ? <span className="size-3 rounded-[3px] bg-current" /> : <Icon name="send" size={18} strokeWidth={2} />}</IconSwap>
      </motion.button>
    </Tooltip>
  )
}
