import { ChatMarkdown } from '@/ui/chatMarkdown'
import { Icon, type IconName } from '@/ui/icon'
import { Tooltip } from '@/ui/overlay'
import { useT } from '@/i18n'
import { type Notice, type NoticeKind, protocolLabel } from './noticeText'

const rail = 'flex h-6 w-[22px] shrink-0 items-center justify-center'

const ICON: Record<NoticeKind, IconName> = {
  teammate: 'users',
  observer: 'eye',
  monitor: 'terminal',
  session: 'link',
  channel: 'chat',
  event: 'bell',
}

function useLook(kind: NoticeKind): { icon: IconName; label: string } {
  const t = useT()
  return { icon: ICON[kind], label: t(`plan.notice_${kind}`) }
}

function ProtocolLine({ notice }: { notice: Notice }) {
  const t = useT()
  const look = useLook(notice.kind)
  return (
    <p data-notice={notice.kind} className="flex min-w-0 items-center gap-3.5 text-sm text-muted">
      <Tooltip content={look.label}>
        <span className={rail}><Icon name={look.icon} size={16} /></span>
      </Tooltip>
      <span className="min-w-0 truncate">
        <span className="font-medium text-fg2">{notice.from}</span> {protocolLabel(notice.protocol ?? '', t)}
      </span>
    </p>
  )
}

function NoticeBody({ notice }: { notice: Notice }) {
  const look = useLook(notice.kind)
  return (
    <div data-notice={notice.kind} className="flex gap-3.5">
      <Tooltip content={`${look.label}: ${notice.from}`}>
        <span className={`${rail} text-muted`}><Icon name={look.icon} size={16} /></span>
      </Tooltip>
      <div className="min-w-0 flex-1 text-md leading-6 text-muted">
        <p className="truncate text-sm">
          <span className="font-medium text-fg2">{notice.from}</span>
          <span className="text-faint"> · {look.label}</span>
          {notice.summary && <span> · {notice.summary}</span>}
        </p>
        <div className="prose-chat prose-thought break-words">
          <ChatMarkdown text={notice.text} />
        </div>
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
