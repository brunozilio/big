import { useState, type ReactNode } from 'react'
import type { Block, MergeMethod } from '@/api/contract'
import { useCall } from '@/api/store'
import { Button } from '@/ui/button'
import { Badge } from '@/ui/display'
import { PersonAvatar, personName } from '@/ui/domain'
import { SegmentedControl } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { MarkdownPreview } from '@/ui/markdownProse'
import { useBlockHost } from './context'
import { Loading, Muted, SendError, useSend } from './shared'

type Of<T extends Block['type']> = Extract<Block, { type: T }>

export function ChangeBlock({ block }: { block: Of<'change'> }) {
  const { version } = useBlockHost()
  const { data: change, error } = useCall(block.connection, 'code.change', { change: block.change }, version)
  if (!change) return <Loading error={error} />
  const merged = change.state === 'merged'
  return (
    <div className="space-y-5 px-2 py-2">
      <div>
        <h2 className="text-2xl font-medium tracking-tighter2 text-fg">{change.title} <span className="font-normal text-faint">#{change.number}</span></h2>
        <div className="mt-3 flex flex-wrap items-center gap-2 text-sm text-muted">
          <Badge tone={merged ? 'accent' : change.state === 'closed' ? 'danger' : 'success'} icon={<Icon name={merged ? 'sMerge' : 'sPr'} size={13} />}>{merged ? 'Merged' : change.state === 'closed' ? 'Closed' : 'Open'}</Badge>
          <PersonAvatar person={change.author} size={18} />
          <span className="text-fg2">{personName(change.author)}</span>
          <span>quer fazer merge de</span>
          <span className="rounded-[5px] bg-info-soft px-1.5 py-0.5 font-mono text-xs text-info-fg">{change.head}</span>
          <span>em</span>
          <span className="rounded-[5px] bg-info-soft px-1.5 py-0.5 font-mono text-xs text-info-fg">{change.base}</span>
        </div>
      </div>
      <div className="rounded-card border">
        <div className="flex h-10 items-center gap-2 border-b bg-subtle px-4 text-sm">
          <PersonAvatar person={change.author} size={16} />
          <span className="font-medium text-fg">{personName(change.author)}</span>
          <span className="text-muted">abriu esta mudança</span>
        </div>
        <div className="px-4 py-3">
          <MarkdownPreview text={change.body} className="text-base" />
        </div>
      </div>
    </div>
  )
}

const methodLabel: Record<MergeMethod, { short: string; action: string }> = {
  squash: { short: 'Squash', action: 'Fazer merge com squash' },
  merge: { short: 'Merge commit', action: 'Fazer merge' },
  rebase: { short: 'Rebase', action: 'Fazer merge com rebase' },
}

function DetailValue({ row }: { row: Of<'merge'>['checks'][number] }) {
  const text = row.url ? <a href={row.url} target="_blank" rel="noreferrer" className="font-mono text-sm text-info-fg hover:underline">{row.text}</a> : <span className={row.mono ? 'font-mono text-sm' : undefined}>{row.text}</span>
  return (
    <>
      {row.person && <PersonAvatar person={row.person} size={20} />}
      {row.person ? personName(row.person) : text}
      {row.note && <span className="text-sm text-faint">{row.note}</span>}
    </>
  )
}

function MergeSummary({ block }: { block: Of<'merge'> }) {
  const { version } = useBlockHost()
  const { data: change, error } = useCall(block.connection, 'code.change', { change: block.change }, version)
  if (!change) return <Loading error={error} />
  const additions = change.files.reduce((n, f) => n + f.additions, 0)
  const deletions = change.files.reduce((n, f) => n + f.deletions, 0)
  return (
    <div className="space-y-2">
      <p className="text-lg font-medium text-fg">{change.title} <span className="font-normal text-faint">#{change.number}</span></p>
      <div className="flex flex-wrap items-center gap-2 text-sm text-muted">
        <PersonAvatar person={change.author} size={16} />
        <span className="text-fg2">{personName(change.author)}</span>
        <span>quer fazer merge de</span>
        <span className="rounded-[5px] bg-info-soft px-1.5 py-0.5 font-mono text-xs text-info-fg">{change.head}</span>
        <span>em</span>
        <span className="rounded-[5px] bg-info-soft px-1.5 py-0.5 font-mono text-xs text-info-fg">{change.base}</span>
        <span className="tabular-nums">· {change.files.length} {change.files.length === 1 ? 'arquivo' : 'arquivos'}</span>
        <span className="tabular-nums text-success-fg">+{additions}</span>
        <span className="tabular-nums text-danger-fg">-{deletions}</span>
      </div>
    </div>
  )
}

export function MergeBlock({ block }: { block: Of<'merge'> }) {
  const [method, setMethod] = useState<MergeMethod>(block.methods[0] ?? 'squash')
  const { busy, error, send } = useSend()
  if (block.result) {
    const row = (label: string, value: ReactNode) => (
      <div key={label} className="flex h-9 items-center gap-3 text-base">
        <span className="w-[120px] shrink-0 text-muted">{label}</span>
        <span className="flex min-w-0 items-center gap-2 text-fg">{value}</span>
      </div>
    )
    return <div>{block.result.map((r) => row(r.label, <DetailValue row={r} />))}</div>
  }
  if (!block.ready) return <Muted>Ainda não começou.</Muted>
  return (
    <div className="space-y-4">
      <MergeSummary block={block} />
      <div className={block.checks.length > 0 ? 'overflow-hidden rounded-card bg-surface shadow-card' : 'hidden'}>
        {block.checks.map((r) => (
          <div key={r.label} className="flex h-11 items-center gap-3 px-4 [&+&]:border-t">
            <span className="flex size-[18px] items-center justify-center rounded-full bg-success text-white"><Icon name="check" size={11} strokeWidth={2.6} /></span>
            <span className="w-24 text-sm text-muted">{r.label}</span>
            <span className="flex min-w-0 items-center gap-2 text-base text-fg">
              {r.person && <PersonAvatar person={r.person} size={18} />}
              {r.person ? personName(r.person) : r.text}
            </span>
          </div>
        ))}
      </div>
      <div className="flex flex-wrap items-center gap-3">
        {block.methods.length > 1 && <SegmentedControl<MergeMethod> size="sm" value={method} onChange={setMethod} options={block.methods.map((m) => ({ value: m, label: methodLabel[m].short }))} />}
        <Button variant="primary" className="ml-auto" loading={busy} icon={<Icon name="sMerge" size={16} />} onClick={() => void send(block.action, { method })}>
          {methodLabel[method].action}
        </Button>
      </div>
      <SendError error={error} />
    </div>
  )
}
