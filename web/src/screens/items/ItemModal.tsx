import { AnimatePresence, motion } from 'framer-motion'
import { useEffect, useState } from 'react'
import { api } from '@/api'
import { mins } from '@/api/clock'
import type { Item, StepState, StepView } from '@/api/contract'
import { errorText, useStepView } from '@/api/store'
import { BlockContext } from '@/blocks/context'
import { BlockView, fillsPanel } from '@/blocks/registry'
import { IconButton } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Card, Spinner } from '@/ui/display'
import { SegmentedControl } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { springPop, tweenClose } from '@/ui/motion'
import { Dialog, Menu, MenuContent, MenuItem, MenuTrigger } from '@/ui/overlay'
import { ChangeChip, IssueKey, Node, nodeState, RowRight } from './parts'

export function ItemModal({ it, step, onStep, onClose, onConnect }: { it: Item | null; step: string | null; onStep: (s: string) => void; onClose: () => void; onConnect: (provider: string) => void }) {
  return (
    <Dialog open={!!it} onOpenChange={(o) => !o && onClose()} size="xl" bare title={it ? `${it.issue?.key ?? ''} ${it.title}` : ''}>
      {it && (
        <div className="overflow-hidden">
          <div className="flex h-16 items-center gap-3 px-5">
            {it.issue && <IssueKey issue={it.issue} />}
            <span className="min-w-0 flex-1 truncate text-lg font-medium tracking-tightish text-fg">{it.title}</span>
            <span className="flex shrink-0 items-center gap-2">
              {it.impediment ? <RowRight it={it} onStep={onStep} onConnect={(p) => { onClose(); onConnect(p) }} /> : <ChangeChip change={it.change} />}
            </span>
            <ItemMenu it={it} step={step ?? it.current} onClose={onClose} />
            <IconButton tooltip="Fechar" onClick={onClose}><Icon name="close" size={18} /></IconButton>
          </div>
          <ItemDetail it={it} step={step ?? it.current ?? it.steps[0]?.id ?? ''} onStep={onStep} />
        </div>
      )}
    </Dialog>
  )
}

function ItemMenu({ it, step, onClose }: { it: Item; step: string | null; onClose: () => void }) {
  const [error, setError] = useState<string | null>(null)
  const label = it.steps.find((s) => s.id === step)?.label
  return (
    <Menu>
      <MenuTrigger asChild>
        <button aria-label="Mais ações" className="flex size-9 items-center justify-center rounded-control text-fg2 transition-colors hover:bg-hover hover:text-fg">
          <Icon name={error ? 'alert' : 'more'} size={18} className={error ? 'text-danger-fg' : undefined} />
        </button>
      </MenuTrigger>
      <MenuContent align="end">
        {step && <MenuItem icon={<Icon name="refresh" size={16} />} onSelect={() => api().rerun(it.id, step).catch((e: unknown) => setError(errorText(e)))}>Rodar de novo a partir de {label}</MenuItem>}
        {!it.doneAt && <MenuItem danger icon={<Icon name="stop" size={16} />} onSelect={() => api().cancel(it.id).then(onClose).catch((e: unknown) => setError(errorText(e)))}>Cancelar item</MenuItem>}
      </MenuContent>
    </Menu>
  )
}

function ItemDetail({ it, step, onStep }: { it: Item; step: string; onStep: (s: string) => void }) {
  return (
    <div className="px-4 pb-4">
      <div className="grid h-[min(660px,calc(88dvh-84px))] grid-cols-[190px_minmax(0,1fr)] grid-rows-[minmax(0,1fr)] gap-3 md:grid-cols-[210px_minmax(0,1fr)]">
        <Card variant="inset" className="self-start p-1.5">
          <ul className="space-y-0.5">
            {it.steps.map((x) => {
              const st = nodeState(it, x)
              const t = Math.round(x.seconds / 60)
              const active = x.id === step
              return (
                <li key={x.id}>
                  <button onClick={() => onStep(x.id)} className={cn('relative flex h-9 w-full items-center gap-2.5 rounded-control-sm px-2 text-left text-base transition-colors', active ? 'text-fg' : 'text-fg2 hover:bg-hover')}>
                    {active && <motion.span layoutId={`step-sel-${it.id}`} transition={springPop} className="absolute inset-0 rounded-control-sm bg-surface shadow-card" />}
                    <span className="relative flex"><Node s={x} state={st} size={20} /></span>
                    <span className={cn('relative flex-1 truncate leading-none', st === 'pending' && 'text-muted')}>{x.label}</span>
                    <span className="relative text-xs tabular-nums leading-none text-faint">{st === 'skipped' ? 'pulado' : x.attempts > 1 ? `${x.attempts}× · ${mins(t)}` : t ? mins(t) : ''}</span>
                  </button>
                </li>
              )
            })}
          </ul>
        </Card>
        <AnimatePresence mode="wait" initial={false}>
          <motion.div key={step} initial={{ opacity: 0, x: 6 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -4 }} transition={tweenClose} className="h-full min-h-0 min-w-0">
            <StepPanel it={it} step={step} />
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  )
}

const attemptTone = (st: StepState) => (st === 'failed' ? 'text-danger' : st === 'passed' ? 'text-success' : 'text-info')

function RunPicker({ view, onPick }: { view: StepView; onPick: (n: number | null) => void }) {
  if (view.attempts.length < 2) return null
  const last = view.attempts[view.attempts.length - 1].n
  return (
    <div className="mb-4 flex items-center gap-2">
      <span className="text-sm text-muted">Execução</span>
      <SegmentedControl<string>
        size="sm"
        value={String(view.attempt)}
        onChange={(v) => onPick(Number(v) === last ? null : Number(v))}
        options={view.attempts.map((a) => ({ value: String(a.n), label: `#${a.n}`, icon: <span className={cn('flex', attemptTone(a.state))}>{a.state === 'failed' ? <Icon name="close" size={12} strokeWidth={2.4} /> : a.state === 'passed' ? <Icon name="check" size={12} strokeWidth={2.4} /> : <Spinner size={11} className="text-info" />}</span> }))}
      />
    </div>
  )
}

export function StepPanel({ it, step }: { it: Item; step: string }) {
  const [attempt, setAttempt] = useState<number | null>(null)
  useEffect(() => setAttempt(null), [it.id, step])
  const { data: view, error } = useStepView(it, step, attempt)
  const host = { item: it.id, step, version: it.updatedAt, send: (action: string, payload: unknown) => api().input(it.id, step, { action, payload }) }
  const current = view && view.step === step && view.item === it.id ? view : null
  if (!current) {
    return (
      <Card className="flex h-full items-center justify-center p-4">
        {error ? <p role="alert" className="text-base text-danger-fg">{error}</p> : <Spinner />}
      </Card>
    )
  }
  if (fillsPanel(current.blocks)) {
    return (
      <BlockContext.Provider value={host}>
        <div className="flex h-full flex-col overflow-hidden rounded-card bg-bg shadow-card">
          <div className="min-h-0 flex-1">
            <BlockView block={current.blocks[0]} />
          </div>
        </div>
      </BlockContext.Provider>
    )
  }
  return (
    <BlockContext.Provider value={host}>
      <Card className="scrollbar-thin h-full overflow-y-auto p-4">
        <RunPicker view={current} onPick={setAttempt} />
        <div className={cn('space-y-4', current.blocks.some((b) => b.type === 'review') && 'h-full')}>
          {current.blocks.map((b, i) => <BlockView key={`${current.attempt}-${i}-${b.type}`} block={b} />)}
        </div>
      </Card>
    </BlockContext.Provider>
  )
}
