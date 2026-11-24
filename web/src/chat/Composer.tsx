import { useEffect, useRef, useState, type ReactNode } from 'react'
import { cn } from '@/ui/cn'
import { Icon } from '@/ui/icon'
import { Button } from '@/ui/button'
import { AnimatePresence, dur, easeOut, IconSwap, motion, tween } from '@/ui/motion'
import { Menu, MenuContent, MenuItem, MenuTrigger, Tooltip } from '@/ui/overlay'
import { AttachmentStrip, useAttachments } from './Attachments'
import type { Effort } from '@/api/contract'
import { DEFAULT_EFFORT, ModelPicker, useModels } from './ModelPicker'
import { useRememberedModel, useRememberedTuning } from './remembered'

export interface Tuning {
  effort: Effort
  fast: boolean
}

export interface ComposerImport {
  key: string
  label: string
  icon: ReactNode
  onSelect: () => void
}

export interface ComposerSubmit {
  text: string
  attachments: string[]
  model: string | null
  effort: Effort
  fast: boolean
}

const BTW = /^\/btw(?:\s+([\s\S]*))?$/

export function btwOf(text: string): string | null {
  const found = BTW.exec(text.trim())
  return found ? (found[1] ?? '').trim() : null
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
  tuning,
  onTuning,
  suggestion,
  onAside,
  onBtw,
  imports,
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
  onModel?: (id: string | null) => void
  tuning?: Tuning
  onTuning?: (tuning: Tuning) => void
  suggestion?: string | null
  onAside?: (text: string) => void
  onBtw?: (question: string) => void
  imports?: ComposerImport[]
}) {
  const [text, setText] = useState('')
  const [refusal, setRefusal] = useState<string | null>(null)
  const attachments = useAttachments(setRefusal)
  const [dragging, setDragging] = useState(false)
  const [focused, setFocused] = useState(false)
  const [ownModel, setOwnModel] = useRememberedModel()
  const [ownTuning, setOwnTuning] = useRememberedTuning({ effort: DEFAULT_EFFORT, fast: false })
  const { effort, fast } = tuning ?? ownTuning
  const tune = (patch: Partial<Tuning>) => (onTuning ?? setOwnTuning)({ effort, fast, ...patch })
  const models = useModels()
  const noModel = models.loaded && models.list.length === 0
  const ref = useRef<HTMLTextAreaElement>(null)
  const fileInput = useRef<HTMLInputElement>(null)
  const wanted = model === undefined ? ownModel : model
  const chosen = wanted && models.loaded && !models.list.some((m) => m.id === wanted) ? null : wanted
  const chooseModel = onModel ?? setOwnModel

  useEffect(() => {
    const el = ref.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${Math.min(el.scrollHeight, 260)}px`
  }, [text])

  const hasText = text.trim().length > 0
  const ready = (hasText || !!allowEmpty) && !disabled && !attachments.uploading && !noModel
  const aside = !!running && !!onAside
  const btw = onBtw ? btwOf(text) : null
  const suggested = !running && !hasText && !disabled && suggestion ? suggestion : null

  const askAside = () => {
    if (btw !== null && onBtw) {
      if (!btw || disabled) {
        setRefusal('Escreva a pergunta: /btw <pergunta>')
        return
      }
      onBtw(btw)
      setText('')
      setRefusal(null)
      return
    }
    if (!aside || !hasText || disabled) return
    onAside(text.trim())
    setText('')
  }

  const submit = () => {
    if (btw !== null && onBtw) {
      askAside()
      return
    }
    if (!ready) return
    onSubmit({ text: text.trim(), attachments: attachments.serverIds, model: chosen, effort, fast })
    setText('')
    setRefusal(null)
    attachments.clear()
  }

  return (
    <div className="relative">
      {suggested && (
        <SuggestionChip text={suggested} onPick={() => {
          setText(suggested)
          ref.current?.focus()
        }} />
      )}
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
            if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'u') {
              e.preventDefault()
              fileInput.current?.click()
              return
            }
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
          <AddMenu onFiles={() => fileInput.current?.click()} imports={imports ?? []} />
          {leading}
          <span className="ml-auto flex items-center">
            {(btw !== null || (aside && hasText)) && <AsideButton onAsk={askAside} disabled={!!disabled || btw === ''} />}
            <ModelPicker value={chosen} onChange={chooseModel} effort={effort} onEffort={(next) => tune({ effort: next })} fast={fast} onFast={(next) => tune({ fast: next })} />
            <SendButton running={!!running && !hasText} ready={ready && btw === null} onSend={submit} onStop={onStop} />
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

const MOD = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform) ? '⌘' : 'Ctrl+'

function AddMenu({ onFiles, imports }: { onFiles: () => void; imports: ComposerImport[] }) {
  const imported = useRef(false)
  return (
    <Menu>
      <MenuTrigger asChild>
        <button aria-label="Adicionar" className="flex size-8 items-center justify-center rounded-full text-fg2 transition-colors hover:bg-inset hover:text-fg data-[state=open]:bg-inset data-[state=open]:text-fg">
          <Icon name="plus" size={18} />
        </button>
      </MenuTrigger>
      <MenuContent align="start" side="top" className="min-w-64" onCloseAutoFocus={(e) => {
        if (imported.current) e.preventDefault()
        imported.current = false
      }}>
        <MenuItem icon={<Icon name="attach" size={16} />} onSelect={onFiles} right={<span className="text-sm text-faint">{MOD}U</span>}>Adicionar arquivos ou fotos</MenuItem>
        {imports.map((i) => (
          <MenuItem key={i.key} icon={i.icon} onSelect={() => { imported.current = true; i.onSelect() }}>{i.label}</MenuItem>
        ))}
      </MenuContent>
    </Menu>
  )
}

function SuggestionChip({ text, onPick }: { text: string; onPick: () => void }) {
  return (
    <motion.div initial={{ opacity: 0, y: 4 }} animate={{ opacity: 1, y: 0 }} transition={tween} className="mb-2 flex">
      <button onClick={onPick} aria-label={`Usar sugestão: ${text}`} className="inline-flex h-8 min-w-0 max-w-full items-center gap-1.5 rounded-full bg-surface px-3 text-sm text-fg2 shadow-card transition-colors hover:bg-hover hover:text-fg">
        <Icon name="sparkles" size={14} className="shrink-0 text-muted" />
        <span className="truncate">{text}</span>
      </button>
    </motion.div>
  )
}

function AsideButton({ onAsk, disabled }: { onAsk: () => void; disabled: boolean }) {
  return (
    <Tooltip content="Pergunta sem interromper nem criar atividade  ↵">
      <Button size="sm" variant="soft" disabled={disabled} onClick={onAsk} icon={<Icon name="help" size={15} />} className="mr-1">
        Perguntar à parte
      </Button>
    </Tooltip>
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
