import type { SessionTask, SessionWork } from '@/api/contract'
import { cn } from '@/ui/cn'
import { Spinner } from '@/ui/display'
import { Icon } from '@/ui/icon'

const taskLabel: Record<string, string> = { pending: 'A fazer', in_progress: 'Fazendo', completed: 'Feito' }

function TaskRow({ task }: { task: SessionTask }) {
  const done = task.status === 'completed'
  return (
    <li className="flex items-center gap-2.5 text-sm">
      <span className="flex size-[18px] shrink-0 items-center justify-center text-muted">
        {task.status === 'in_progress' ? <Spinner size={14} className="text-fg2" /> : <Icon name={done ? 'checkCircle' : 'dashedCircle'} size={16} />}
      </span>
      <span className={cn('min-w-0 flex-1 truncate text-fg2', done && 'text-muted line-through')}>{task.subject}</span>
      <span className="text-xs text-muted">{taskLabel[task.status] ?? task.status}</span>
    </li>
  )
}

export function hasWork(work: SessionWork | undefined): work is SessionWork {
  return !!work && (work.background.length > 0 || work.tasks.length > 0)
}

export function BackgroundWork({ work }: { work: SessionWork }) {
  const running = work.background.length
  return (
    <section aria-label="Trabalho em andamento" className="mb-8 rounded-card border bg-subtle px-4 py-3">
      {running > 0 && (
        <p className="flex items-center gap-2 text-sm text-fg2">
          <Spinner size={14} className="text-fg2" />
          {running === 1 ? '1 tarefa em segundo plano' : `${running} tarefas em segundo plano`}
        </p>
      )}
      {work.tasks.length > 0 && (
        <ol aria-label="Tarefas do agente" className={cn('space-y-1.5', running > 0 && 'mt-3')}>
          {work.tasks.map((task) => (
            <TaskRow key={task.id} task={task} />
          ))}
        </ol>
      )}
    </section>
  )
}
