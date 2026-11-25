import { Reorder, useDragControls } from 'framer-motion'
import { useEffect, useRef, useState } from 'react'
import type { Block, ChangeTaskView, TaskKindView } from '@/api/contract'
import { useT } from '@/i18n'
import { IconButton } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Badge } from '@/ui/display'
import { Icon, isIconName } from '@/ui/icon'
import { Menu, MenuContent, MenuItem, MenuTrigger } from '@/ui/overlay'
import { SendError, useSend } from './shared'
import { TaskFiles } from './taskFiles'

type Of<T extends Block['type']> = Extract<Block, { type: T }>
type Edit = (change: Partial<ChangeTaskView>) => void

const TONES: Record<string, string> = { info: 'text-info', warning: 'text-warning', success: 'text-success', accent: 'text-cat-2', teal: 'text-cat-6', neutral: 'text-muted' }
const plain = 'w-full min-w-0 rounded-sm bg-transparent outline-none transition-colors placeholder:text-faint hover:bg-hover focus:bg-inset'

function TimeField({ value, onChange, label, disabled }: { value: string; onChange: (v: string) => void; label: string; disabled: boolean }) {
  return (
    <input
      aria-label={label}
      value={value}
      disabled={disabled}
      inputMode="numeric"
      maxLength={5}
      placeholder="--:--"
      onChange={(e) => onChange(e.target.value.replace(/[^\d:]/g, ''))}
      onBlur={(e) => { const d = e.target.value.replace(/\D/g, ''); if (d.length >= 3) onChange(`${d.slice(0, -2).padStart(2, '0')}:${d.slice(-2)}`) }}
      className="h-7 w-[52px] rounded-control-sm bg-inset text-center text-sm tabular-nums text-fg outline-none transition-colors placeholder:text-faint focus:bg-active disabled:bg-transparent"
    />
  )
}

function Link({ task, label }: { task: ChangeTaskView; label: string }) {
  if (!task.link) return null
  return (
    <a href={task.link} target="_blank" rel="noreferrer" aria-label={label} title={label} className="flex size-7 shrink-0 items-center justify-center rounded-control-sm text-fg2 transition-colors hover:bg-hover hover:text-fg">
      <Icon name="external" size={14} />
    </a>
  )
}

function Body({ task, edit, editable, link }: { task: ChangeTaskView; edit: Edit; editable: boolean; link: string }) {
  const t = useT()
  if (!editable) {
    return (
      <span className="flex min-w-0 flex-col">
        <span className="flex min-w-0 items-center gap-1"><span className="truncate py-1 text-base text-fg">{task.title}</span><Link task={task} label={link} /></span>
        {task.detail && <span className="text-xs text-muted">{task.detail}</span>}
      </span>
    )
  }
  return (
    <span className="flex min-w-0 flex-col">
      <span className="flex min-w-0 items-center gap-1">
        <input aria-label={t('blocks.taskTitle')} value={task.title} onChange={(e) => edit({ title: e.target.value })} placeholder={t('blocks.taskTitle')} className={cn(plain, '-mx-1 h-7 w-auto min-w-[6ch] max-w-full px-1 text-base text-fg [field-sizing:content]')} />
        <Link task={task} label={link} />
      </span>
      <textarea aria-label={t('blocks.taskDetail')} rows={1} value={task.detail} onChange={(e) => edit({ detail: e.target.value })} placeholder={t('blocks.taskDetail')} className={cn(plain, '-mx-1 min-h-6 resize-none px-1 py-0.5 text-xs leading-5 text-muted [field-sizing:content]')} />
    </span>
  )
}

function Row({ task, kind, locked, env, edit, remove }: { task: ChangeTaskView; kind: TaskKindView | undefined; locked: boolean; env: string; edit: Edit; remove: () => void }) {
  const t = useT()
  const controls = useDragControls()
  const editable = !locked && !!kind?.editable
  return (
    <Reorder.Item value={task} dragListener={false} dragControls={controls} className="group relative flex items-center gap-2 border-b border-line bg-surface py-2 last:border-b-0">
      <button aria-label={t('blocks.dragTask')} disabled={locked} onPointerDown={(e) => !locked && controls.start(e)} className={cn('flex h-7 w-4 shrink-0 touch-none items-center justify-center text-faint', locked ? 'invisible' : 'cursor-grab active:cursor-grabbing')}><Icon name="drag" size={14} /></button>
      <span className={cn('flex size-6 shrink-0 items-center justify-center rounded-control-sm bg-inset', TONES[kind?.tone ?? 'neutral'])}><Icon name={kind && isIconName(kind.icon) ? kind.icon : 'task'} size={14} /></span>
      <span className="min-w-0 flex-1">
        <Body task={task} edit={edit} editable={editable} link={t('blocks.taskLink', { env })} />
        {kind?.attachable && <TaskFiles files={task.files} locked={locked} onChange={(files) => edit({ files })} />}
      </span>
      <span className="flex w-[180px] shrink-0 justify-end">{task.owner && <Badge>{task.owner}</Badge>}</span>
      <span className="flex shrink-0 items-center gap-1 text-faint">
        <TimeField label={t('blocks.taskStart')} value={task.start} disabled={locked} onChange={(start) => edit({ start })} />
        <span>–</span>
        <TimeField label={t('blocks.taskEnd')} value={task.end} disabled={locked} onChange={(end) => edit({ end })} />
      </span>
      <span className="flex size-8 shrink-0 items-center justify-center">{!locked && <IconButton tooltip={t('blocks.removeTask')} size="sm" className="opacity-0 group-hover:opacity-100" onClick={remove}><Icon name="trash" size={14} /></IconButton>}</span>
    </Reorder.Item>
  )
}

function AddTask({ kinds, onAdd }: { kinds: TaskKindView[]; onAdd: (template: string) => void }) {
  const t = useT()
  const templates = kinds.filter((kind) => kind.id && kind.label)
  if (templates.length === 0) return null
  return (
    <Menu>
      <MenuTrigger asChild>
        <button className="inline-flex h-7 items-center gap-1 rounded-control-sm px-2 text-sm text-muted transition-colors hover:bg-hover hover:text-fg"><Icon name="plus" size={14} />{t('blocks.addTask')}</button>
      </MenuTrigger>
      <MenuContent align="end">
        {templates.map((kind) => <MenuItem key={kind.id} icon={<Icon name={isIconName(kind.icon) ? kind.icon : 'task'} size={15} />} onSelect={() => onAdd(kind.id ?? '')}>{kind.label}</MenuItem>)}
      </MenuContent>
    </Menu>
  )
}

function kindOf(kinds: TaskKindView[], task: ChangeTaskView): TaskKindView | undefined {
  return kinds.find((kind) => kind.id !== undefined && kind.id === task.template) ?? kinds.find((kind) => kind.kind === task.kind)
}

export function TasksBlock({ block }: { block: Of<'tasks'> }) {
  const { error, send } = useSend()
  const [list, setList] = useState(block.tasks)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  useEffect(() => setList(block.tasks), [block.tasks])
  useEffect(() => () => { if (timer.current) clearTimeout(timer.current) }, [])
  const save = (next: ChangeTaskView[]) => {
    setList(next)
    if (timer.current) clearTimeout(timer.current)
    timer.current = setTimeout(() => void send('tasks', { tasks: next }), 600)
  }
  const env = block.environment ?? ''
  return (
    <div>
      {!block.locked && <div className="mb-1 flex justify-end"><AddTask kinds={block.kinds} onAdd={(template) => void send('add_task', { template })} /></div>}
      <Reorder.Group axis="y" values={list} onReorder={save}>
        {list.map((task) => (
          <Row key={task.id} task={task} kind={kindOf(block.kinds, task)} locked={block.locked} env={env} edit={(change) => save(list.map((one) => (one.id === task.id ? { ...one, ...change } : one)))} remove={() => save(list.filter((one) => one.id !== task.id))} />
        ))}
      </Reorder.Group>
      <SendError error={error} />
    </div>
  )
}
