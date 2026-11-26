import { motion } from 'framer-motion'
import { memo, useState, type DragEvent, type KeyboardEvent } from 'react'
import type { Item, StageView } from '@/api/contract'
import { useT } from '@/i18n'
import { titleOf } from '@/i18n/server'
import { cn } from '@/ui/cn'
import { Icon } from '@/ui/icon'
import { Collapse, tween } from '@/ui/motion'
import { Tooltip } from '@/ui/overlay'
import { stageStep } from '@/blocks/stageStep'
import { IssueKey, LiveDot, Node, OriginCell, stop } from '../parts'

export const DRAG_TYPE = 'application/x-item-id'
export const FROM_TYPE = 'application/x-item-from'

function openOnKey(e: KeyboardEvent, open: () => void) {
  if (e.target !== e.currentTarget || (e.key !== 'Enter' && e.key !== ' ')) return
  e.preventDefault()
  open()
}

function Trail({ stages }: { stages: StageView[] }) {
  return (
    <span className="hidden items-center md:flex">
      {stages.map((stage, i) => (
        <span key={stage.id} className="flex items-center">
          {i > 0 && (
            <span className={cn('flex items-center', stage.state === 'passed' ? 'text-success/70' : 'text-faint')}>
              <span className={cn('block h-px w-1.5', stage.state === 'passed' ? 'bg-success/50' : 'border-t border-dashed border-line-strong')} />
              {stage.gate === 'change' && <Icon name="approval" size={11} strokeWidth={1.8} />}
              <span className={cn('block h-px w-1.5', stage.state === 'passed' ? 'bg-success/50' : 'border-t border-dashed border-line-strong')} />
            </span>
          )}
          <Tooltip content={stage.change ? `${stage.label} · ${stage.change.key} ${stage.change.label}` : stage.label}>
            <span className="flex rounded-full" title={stage.label}><Node s={stageStep(stage)} state={stage.state} size={20} /></span>
          </Tooltip>
        </span>
      ))}
    </span>
  )
}

function Member({ it, from, movable, onOpen }: { it: Item; from: string; movable: boolean; onOpen: () => void }) {
  const t = useT()
  const drag = movable ? { draggable: true, onDragStart: (e: DragEvent) => { e.dataTransfer.setData(DRAG_TYPE, it.id); e.dataTransfer.setData(FROM_TYPE, from) } } : {}
  return (
    <div role="button" tabIndex={0} onClick={onOpen} onKeyDown={(e) => openOnKey(e, onOpen)} {...drag} className={cn('flex min-h-[44px] cursor-pointer items-center gap-3 rounded-control pl-9 pr-2.5 transition-colors hover:bg-hover', movable && 'cursor-grab active:cursor-grabbing')}>
      <OriginCell>{it.issue && <IssueKey issue={it.issue} className="min-w-0 shrink" />}</OriginCell>
      <span className="truncate text-base text-fg2">{titleOf(t, it)}</span>
    </div>
  )
}

type GroupRowProps = { it: Item; members: Item[]; onOpen: (it: Item) => void; onDropItem: (target: Item, id: string, from: string | null) => void }

export const GroupRow = memo(function GroupRow({ it, members, onOpen, onDropItem }: GroupRowProps) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const [over, setOver] = useState(false)
  const accepts = !!it.joinable
  const drop = (e: DragEvent) => {
    setOver(false)
    const id = e.dataTransfer.getData(DRAG_TYPE)
    const from = e.dataTransfer.getData(FROM_TYPE) || null
    if (id && accepts && from !== it.id) onDropItem(it, id, from)
  }
  return (
    <div onDragOver={(e) => { if (accepts && e.dataTransfer.types.includes(DRAG_TYPE)) { e.preventDefault(); setOver(true) } }} onDragLeave={() => setOver(false)} onDrop={drop} className={cn('rounded-card transition-colors', over && 'bg-accent-soft/60 shadow-[0_0_0_1.5px_rgb(var(--accent)/0.5)]')}>
      <div role="button" tabIndex={0} onClick={() => onOpen(it)} onKeyDown={(e) => openOnKey(e, () => onOpen(it))} className="group flex min-h-[52px] w-full cursor-pointer items-center gap-3 rounded-control px-2.5 text-left transition-colors hover:bg-hover">
        <OriginCell><Icon name="package" size={15} className="shrink-0 text-muted" /></OriginCell>
        <span className="flex min-w-0 items-center gap-2">
          <span className="truncate text-md text-fg">{titleOf(t, it)}</span>
          <button aria-expanded={open} onPointerDown={(e) => e.stopPropagation()} onClick={(e) => { e.stopPropagation(); setOpen(!open) }} className="inline-flex h-6 items-center gap-1 rounded-control-sm bg-inset px-2 text-sm text-fg2 transition-colors hover:bg-active hover:text-fg">
            <Icon name="sTicket" size={13} />
            <span className="tabular-nums">{members.length}</span>
            <motion.span initial={false} animate={{ rotate: open ? 0 : -90 }} transition={tween} className="flex text-faint"><Icon name="down" size={12} /></motion.span>
          </button>
          {it.live && <LiveDot />}
        </span>
        <span className="ml-auto flex shrink-0 items-center justify-end gap-3" {...stop}>
          {it.trail && <Trail stages={it.trail} />}
        </span>
      </div>
      <Collapse open={open || over}>
        {members.map((member) => <Member key={member.id} it={member} from={it.id} movable={accepts} onOpen={() => onOpen(member)} />)}
      </Collapse>
    </div>
  )
})
