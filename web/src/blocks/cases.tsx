import { useState } from 'react'
import type { Block, CaseStep, TestCaseView } from '@/api/contract'
import { cn } from '@/ui/cn'
import { Badge, Card } from '@/ui/display'
import { Icon } from '@/ui/icon'

type Of<T extends Block['type']> = Extract<Block, { type: T }>

const KINDS = { api: { label: 'API', icon: 'server' }, ui: { label: 'Tela', icon: 'computer' } } as const

function KindBadge({ kind }: { kind: TestCaseView['kind'] }) {
  const meta = KINDS[kind]
  return <Badge tone={kind === 'ui' ? 'accent' : 'info'} icon={<Icon name={meta.icon} size={12} />}>{meta.label}</Badge>
}

function RefBadge({ caseRef, url }: { caseRef: string | null; url: string | null }) {
  if (!caseRef) return null
  if (!url) return <Badge tone="outline">{caseRef}</Badge>
  return (
    <a href={url} target="_blank" rel="noreferrer" onClick={(e) => e.stopPropagation()} className="rounded-sm">
      <Badge tone="outline" icon={<Icon name="external" size={11} />}>{caseRef}</Badge>
    </a>
  )
}

function Step({ step, index }: { step: CaseStep; index: number }) {
  return (
    <li className="grid grid-cols-[24px_1fr] gap-x-3 gap-y-1.5 px-4 py-3 sm:grid-cols-[24px_1fr_1fr] [&+&]:border-t">
      <span className="flex h-6 w-6 items-center justify-center rounded-full bg-inset text-xs font-medium tabular-nums text-muted">{index + 1}</span>
      <div className="min-w-0 space-y-1.5">
        <p className="text-base text-fg">{step.action}</p>
        {step.data && <p className="rounded-control-sm bg-inset px-2 py-1 text-sm text-fg2"><span className="text-muted">Dados: </span>{step.data}</p>}
      </div>
      <div className="col-start-2 flex min-w-0 gap-2 sm:col-start-3">
        <Icon name="checkCircle" size={16} className="mt-0.5 shrink-0 text-success-fg" />
        <p className="text-base text-fg2">{step.expected}</p>
      </div>
    </li>
  )
}

function Steps({ steps }: { steps: CaseStep[] }) {
  if (steps.length === 0) return null
  return (
    <div className="overflow-hidden rounded-control border">
      <div className="hidden grid-cols-[24px_1fr_1fr] gap-x-3 border-b bg-inset px-4 py-1.5 text-xs font-medium text-muted sm:grid">
        <span />
        <span>Ação</span>
        <span>Resultado esperado</span>
      </div>
      <ol>{steps.map((s, i) => <Step key={i} step={s} index={i} />)}</ol>
    </div>
  )
}

function CaseCard({ entry, index }: { entry: TestCaseView; index: number }) {
  const [open, setOpen] = useState(true)
  return (
    <Card>
      <button type="button" aria-expanded={open} onClick={() => setOpen((v) => !v)} className="flex w-full items-center gap-3 px-4 py-3 text-left">
        <span className="w-5 shrink-0 text-sm tabular-nums text-faint">{index + 1}</span>
        <span className="min-w-0 flex-1 text-md font-medium text-fg">{entry.title}</span>
        <span className="flex shrink-0 items-center gap-1.5">
          <RefBadge caseRef={entry.ref} url={entry.url} />
          <KindBadge kind={entry.kind} />
          <Icon name="down" size={14} className={cn('text-faint transition-transform duration-150', !open && '-rotate-90')} />
        </span>
      </button>
      {open && (
        <div className="space-y-3 px-4 pb-4 sm:pl-12">
          {entry.objective && <p className="text-base text-fg2">{entry.objective}</p>}
          {entry.precondition && (
            <div className="flex gap-2 rounded-control bg-inset px-3 py-2 text-sm">
              <Icon name="info" size={15} className="mt-0.5 shrink-0 text-muted" />
              <p className="text-fg2"><span className="font-medium text-fg">Pré-condição: </span>{entry.precondition}</p>
            </div>
          )}
          <Steps steps={entry.steps} />
        </div>
      )}
    </Card>
  )
}

function Summary({ cases }: { cases: TestCaseView[] }) {
  const api = cases.filter((c) => c.kind === 'api').length
  const ui = cases.length - api
  const steps = cases.reduce((total, c) => total + c.steps.length, 0)
  return (
    <div className="flex flex-wrap items-center gap-2 text-sm text-muted">
      <span className="font-medium text-fg2">{cases.length} {cases.length === 1 ? 'caso' : 'casos'}</span>
      <span>·</span>
      <span className="tabular-nums">{steps} passos</span>
      <span className="ml-auto flex gap-1.5">
        {api > 0 && <Badge tone="info" icon={<Icon name="server" size={12} />}>API {api}</Badge>}
        {ui > 0 && <Badge tone="accent" icon={<Icon name="computer" size={12} />}>Tela {ui}</Badge>}
      </span>
    </div>
  )
}

export function CasesBlock({ block }: { block: Of<'cases'> }) {
  if (block.cases.length === 0) return <p className="text-base text-muted">Nenhum caso de teste.</p>
  return (
    <div className="space-y-3">
      <Summary cases={block.cases} />
      {block.cases.map((c, i) => <CaseCard key={`${i}-${c.title}`} entry={c} index={i} />)}
    </div>
  )
}
