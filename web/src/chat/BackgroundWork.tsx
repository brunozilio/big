import type { BackgroundJob, SessionTask, SessionWork } from '@/api/contract'
import { cn } from '@/ui/cn'
import { Spinner } from '@/ui/display'
import { Icon } from '@/ui/icon'
import { plural, useT } from '@/i18n'
import { LiveDot, StopJobButton } from './jobs'

function TaskRow({ task }: { task: SessionTask }) {
  const t = useT()
  const done = task.status === 'completed'
  const status = task.status === 'pending' || task.status === 'in_progress' || task.status === 'completed' ? t(`plan.task_${task.status}`) : task.status
  return (
    <li className="flex items-center gap-2.5 text-sm">
      <span className="flex size-[18px] shrink-0 items-center justify-center text-muted">
        {task.status === 'in_progress' ? <Spinner size={14} className="text-fg2" /> : <Icon name={done ? 'checkCircle' : 'dashedCircle'} size={16} />}
      </span>
      <span className={cn('min-w-0 flex-1 truncate text-fg2', done && 'text-muted line-through')}>{task.subject}</span>
      <span className="text-xs text-muted">{status}</span>
    </li>
  )
}

function JobRow({ job, stoppable }: { job: BackgroundJob; stoppable: boolean }) {
  return (
    <li className="flex min-w-0 items-center gap-2.5 text-sm">
      <span className="flex size-[18px] shrink-0 items-center justify-center">
        <LiveDot />
      </span>
      <code className="min-w-0 flex-1 truncate font-mono text-xs text-fg2" title={job.command || job.id}>{job.command || job.id}</code>
      {stoppable && <StopJobButton job={job.id} compact />}
    </li>
  )
}

export function hasWork(work: SessionWork | undefined): work is SessionWork {
  return !!work && (work.background.length > 0 || work.tasks.length > 0)
}

export function BackgroundWork({ work }: { work: SessionWork }) {
  const t = useT()
  const running = work.background.length
  return (
    <section aria-label={t('plan.workInProgress')} className="mb-8 rounded-card border bg-subtle px-4 py-3">
      {running > 0 && (
        <>
          <p className="text-sm text-muted">{t(`plan.jobs_${plural(running)}`, { n: running })}</p>
          <ul className="mt-2 space-y-1.5">
            {work.background.map((job) => (
              <JobRow key={job.id} job={job} stoppable={!!work.stoppable} />
            ))}
          </ul>
        </>
      )}
      {work.tasks.length > 0 && (
        <ol aria-label={t('plan.agentTasks')} className={cn('space-y-1.5', running > 0 && 'mt-3')}>
          {work.tasks.map((task) => (
            <TaskRow key={task.id} task={task} />
          ))}
        </ol>
      )}
    </section>
  )
}
