import { useState } from 'react'
import { ChatMarkdown } from '@/ui/chatMarkdown'
import { cn } from '@/ui/cn'
import { Icon, type IconName } from '@/ui/icon'
import { Tooltip } from '@/ui/overlay'
import { type Notice, type NoticeKind, protocolLabel } from './noticeText'

const LOOK: Record<NoticeKind, { icon: IconName; label: string }> = {
  teammate: { icon: 'users', label: 'Auxiliar' },
  observer: { icon: 'eye', label: 'Observer' },
  monitor: { icon: 'terminal', label: 'Monitor' },
  session: { icon: 'link', label: 'Outra sessão' },
  channel: { icon: 'chat', label: 'Canal' },
  event: { icon: 'bell', label: 'Evento' },
}

const LONG = 280

function ProtocolLine({ notice }: { notice: Notice }) {
  const look = LOOK[notice.kind]
  return (
    <p data-notice={notice.kind} className="flex min-w-0 items-center gap-2 text-sm text-muted">
      <Tooltip content={look.label}>
        <span className="flex shrink-0"><Icon name={look.icon} size={14} /></span>
      </Tooltip>
      <span className="min-w-0 truncate">
        <span className="font-medium text-fg2">{notice.from}</span> {protocolLabel(notice.protocol ?? '')}
      </span>
    </p>
  )
}

function NoticeBody({ notice }: { notice: Notice }) {
  const [open, setOpen] = useState(false)
  const look = LOOK[notice.kind]
  const long = notice.text.length > LONG || notice.text.split('\n').length > 3
  return (
    <div data-notice={notice.kind} className="flex gap-2.5">
      <Tooltip content={`${look.label}: ${notice.from}`}>
        <span className="mt-[3px] flex shrink-0 text-muted"><Icon name={look.icon} size={16} /></span>
      </Tooltip>
      <div className="min-w-0 flex-1 text-md leading-6 text-muted">
        <p className="truncate text-sm">
          <span className="font-medium text-fg2">{notice.from}</span>
          <span className="text-faint"> · {look.label}</span>
          {notice.summary && <span> · {notice.summary}</span>}
        </p>
        <div className={cn('prose-chat prose-thought break-words', !open && 'line-clamp-3')}>
          <ChatMarkdown text={notice.text} />
        </div>
        {long && (
          <button onClick={() => setOpen(!open)} className="mt-1 text-sm text-faint transition-colors hover:text-fg">
            {open ? 'Mostrar menos' : 'Mostrar tudo'}
          </button>
        )}
      </div>
    </div>
  )
}

export function NoticeLine({ notice }: { notice: Notice }) {
  return notice.protocol ? <ProtocolLine notice={notice} /> : <NoticeBody notice={notice} />
}

export function NoticeMessage({ notices }: { notices: Notice[] }) {
  return (
    <div data-testid="notices" className="flex flex-col gap-3">
      {notices.map((notice, i) => <NoticeLine key={`${notice.from}-${i}`} notice={notice} />)}
    </div>
  )
}
