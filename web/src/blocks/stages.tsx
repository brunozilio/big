import type { Block, StageView, StepState } from '@/api/contract'
import { Button } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Badge } from '@/ui/display'
import { Icon, isIconName } from '@/ui/icon'
import { Node } from '@/screens/items/parts'
import { SendError, useSend } from './shared'
import { windowLabel, WindowPicker } from './windowPicker'

type Of<T extends Block['type']> = Extract<Block, { type: T }>

const variantOf = (variant?: string) => (variant === 'primary' ? 'primary' : 'secondary')

export function stageStep(stage: StageView) {
  return { id: stage.id, label: stage.label, icon: stage.icon, state: stage.state, attempts: 0, seconds: 0 }
}

function Change({ stage }: { stage: StageView }) {
  const change = stage.change
  if (!change) return null
  return (
    <div className="space-y-1">
      <div className="flex flex-wrap items-center gap-1.5 text-sm text-fg">
        <Icon name="approval" size={13} className="shrink-0 text-muted" />
        <span className="whitespace-nowrap">{change.key}</span>
        <Badge tone={change.status === 'done' ? 'success' : change.status === 'cancelled' ? 'danger' : 'warning'}>{change.label}</Badge>
      </div>
      {stage.window && <div className="text-xs text-muted">{windowLabel(stage.window)}</div>}
    </div>
  )
}

function Stage({ stage, last, live, send, busy }: { stage: StageView; last: boolean; live: boolean; send: (action: string, payload: unknown) => void; busy: boolean }) {
  const state: StepState = stage.state
  return (
    <div className="min-w-[180px] flex-1">
      <div className="flex items-center">
        <Node s={stageStep(stage)} state={state} size={26} />
        {!last && <span className={cn('mx-2 block h-px flex-1', state === 'passed' ? 'bg-success/50' : 'border-t border-dashed border-line-strong')} />}
      </div>
      <div className="mt-2 flex flex-col items-start gap-2 pr-4">
        <div className="text-sm font-medium text-fg">{stage.label}</div>
        <Change stage={stage} />
        {stage.picker && live && <WindowPicker value={stage.window ?? undefined} onChange={(at) => send('window', { env: stage.id, at })} />}
        {stage.busy && <span className="text-xs text-muted">{stage.busy}</span>}
        {live && stage.actions.map((action) => (
          <Button key={action.id} size="sm" variant={variantOf(action.variant)} disabled={busy || action.disabled} icon={action.icon && isIconName(action.icon) ? <Icon name={action.icon} size={14} /> : undefined} onClick={() => send(action.id, action.payload ?? null)}>
            {action.label}
          </Button>
        ))}
      </div>
    </div>
  )
}

export function StagesBlock({ block }: { block: Of<'stages'> }) {
  const { busy, error, send } = useSend()
  return (
    <div>
      <div className="flex overflow-x-auto pb-1">
        {block.stages.map((stage, i) => (
          <Stage key={stage.id} stage={stage} last={i === block.stages.length - 1} live={block.live} busy={busy} send={(action, payload) => void send(action, payload)} />
        ))}
      </div>
      <SendError error={error} />
    </div>
  )
}
