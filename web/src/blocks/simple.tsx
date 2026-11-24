import { useState } from 'react'
import type { Block, ButtonVariant } from '@/api/contract'
import { Button } from '@/ui/button'
import { Field, Input, Select, Textarea } from '@/ui/form'
import { Icon, isIconName } from '@/ui/icon'
import { ToolCallItem } from '@/chat/events'
import { MarkdownPreview } from '@/ui/markdownProse'
import { Commands, Muted, SendError, useSend } from './shared'

type Of<T extends Block['type']> = Extract<Block, { type: T }>

export function MarkdownBlock({ block }: { block: Of<'markdown'> }) {
  if (block.muted) return <Muted>{block.text}</Muted>
  return <MarkdownPreview text={block.text} className="text-base" />
}

export function CallsBlock({ block }: { block: Of<'calls'> }) {
  const last = block.calls.length - 1
  return (
    <ol>
      {block.calls.map((call, i) => (
        <ToolCallItem key={call.id} call={call} last={i === last} single={i === last} />
      ))}
    </ol>
  )
}

export function LogsBlock({ block }: { block: Of<'logs'> }) {
  return <Commands commands={block.commands} />
}

export function FormBlock({ block }: { block: Of<'form'> }) {
  const [values, setValues] = useState<Record<string, string>>(() => Object.fromEntries(block.fields.map((f) => [f.name, f.value ?? ''])))
  const { busy, error, send } = useSend()
  const set = (name: string, value: string) => setValues((v) => ({ ...v, [name]: value }))
  return (
    <form
      className="space-y-3"
      onSubmit={(e) => {
        e.preventDefault()
        void send(block.submit.action, values)
      }}
    >
      {block.title && <p className="text-base font-medium text-fg">{block.title}</p>}
      {block.fields.map((f) => (
        <Field key={f.name} label={f.label}>
          {f.kind === 'textarea' ? (
            <Textarea rows={3} value={values[f.name]} placeholder={f.placeholder} onChange={(e) => set(f.name, e.target.value)} />
          ) : f.kind === 'select' ? (
            <Select value={values[f.name]} onValueChange={(v) => set(f.name, v)} options={(f.options ?? []).map((o) => ({ value: o.value, label: o.label }))} label={f.label} />
          ) : (
            <Input value={values[f.name]} placeholder={f.placeholder} className={f.mono ? 'font-mono' : undefined} onChange={(e) => set(f.name, e.target.value)} />
          )}
        </Field>
      ))}
      <div className="flex justify-end">
        <Button type="submit" size="sm" variant="primary" loading={busy}>
          {block.submit.label}
        </Button>
      </div>
      <SendError error={error} />
    </form>
  )
}

const buttonVariant = (v: ButtonVariant | undefined) => (v === 'danger' ? 'ghost' : v ?? 'secondary')

export function ActionsBlock({ block }: { block: Of<'actions'> }) {
  const { busy, error, send } = useSend()
  return (
    <div>
      <div className="flex flex-wrap justify-end gap-2">
        {block.buttons.map((b) => (
          <Button key={`${b.action}-${b.label}`} size="sm" variant={buttonVariant(b.variant)} className={b.variant === 'danger' ? 'text-danger-fg hover:text-danger-fg' : undefined} disabled={busy} icon={b.icon && isIconName(b.icon) ? <Icon name={b.icon} size={15} /> : undefined} onClick={() => void send(b.action, b.payload ?? null)}>
            {b.label}
          </Button>
        ))}
      </div>
      <SendError error={error} />
    </div>
  )
}
