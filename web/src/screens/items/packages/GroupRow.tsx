import { motion } from 'framer-motion'
import { useState, type DragEvent } from 'react'
import type { Item, StageView } from '@/api/contract'
import { useT } from '@/i18n'
import { titleOf } from '@/i18n/server'
import { cn } from '@/ui/cn'
import { Icon } from '@/ui/icon'
import { Collapse, tween } from '@/ui/motion'
import { Tooltip } from '@/ui/overlay'
import { stageStep } from '@/blocks/stages'
import { IssueKey, LiveDot, Node, stop } from '../parts'

export const DRAG_TYPE = 'application/x-item-id'
export const FROM_TYPE = 'application/x-item-from'

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
            <span className="flex rounded-full"><Node s={stageStep(stage)} state={stage.state} /></span>
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
    <div role="button" tabIndex={0} onClick={onOpen} {...drag} className={cn('flex min-h-[44px] cursor-pointer items-center gap-3 rounded-control pl-9 pr-2.5 transition-colors hover:bg-hover', movable && 'cursor-grab active:cursor-grabbing')}>
      {it.issue && <IssueKey issue={it.issue} />}
      <span className="truncate text-base text-fg2">{titleOf(t, it)}</span>
    </div>
  )
}

export function GroupRow({ it, members, onOpen, onOpenMember, onDropItem }: { it: Item; members: Item[]; onOpen: () => void; onOpenMember: (member: Item) => void; onDropItem: (id: string, from: string | null) => void }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const [over, setOver] = useState(false)
  const accepts = !!it.joinable
  const drop = (e: DragEvent) => {
    setOver(false)
    const id = e.dataTransfer.getData(DRAG_TYPE)
    const from = e.dataTransfer.getData(FROM_TYPE) || null
    if (id && accepts && from !== it.id) onDropItem(id, from)
  }
  return (
    <div onDragOver={(e) => { if (accepts && e.dataTransfer.types.includes(DRAG_TYPE)) { e.preventDefault(); setOver(true) } }} onDragLeave={() => setOver(false)} onDrop={drop} className={cn('rounded-card transition-colors', over && 'bg-accent-soft/60 shadow-[0_0_0_1.5px_rgb(var(--accent)/0.5)]')}>
      <div role="button" tabIndex={0} onClick={onOpen} className="group flex min-h-[52px] w-full cursor-pointer items-center gap-3 rounded-control px-2.5 text-left transition-colors hover:bg-hover">
        <Icon name="package" size={15} className="shrink-0 text-muted" />
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
        {members.map((member) => <Member key={member.id} it={member} from={it.id} movable={accepts} onOpen={() => onOpenMember(member)} />)}
      </Collapse>
    </div>
  )
}
