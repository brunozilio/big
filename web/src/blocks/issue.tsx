import { useState } from 'react'
import { api } from '@/api'
import type { Block, IssueData } from '@/api/contract'
import { errorText, useCall } from '@/api/store'
import { cn } from '@/ui/cn'
import { ProgressBar } from '@/ui/display'
import { PersonAvatar, personName } from '@/ui/domain'
import { Icon } from '@/ui/icon'
import { MarkdownEditor } from '@/ui/markdown'
import { MarkdownPreview } from '@/ui/markdownProse'
import { motion, tween } from '@/ui/motion'
import { useT } from '@/i18n'
import { useBlockHost } from './context'
import { Loading, SendError, Thread } from './shared'

type Of<T extends Block['type']> = Extract<Block, { type: T }>

function SubHead({ children, right }: { children: React.ReactNode; right?: React.ReactNode }) {
  return (
    <div className="mb-3 flex items-center gap-2">
      <h3 className="text-md font-medium text-fg2">{children}</h3>
      {right}
    </div>
  )
}

function Attachments({ issue }: { issue: IssueData }) {
  const t = useT()
  if (issue.attachments.length === 0) return null
  return (
    <section>
      <SubHead>{t('blocks.issueAttachments')}</SubHead>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
        {issue.attachments.map((a) => {
          const image = /\.(png|jpe?g|gif|webp)$/i.test(a.name)
          return (
            <motion.a key={a.name} href={a.url} target="_blank" rel="noreferrer" layout transition={tween} className="group relative overflow-hidden rounded-card bg-surface text-left shadow-card transition-shadow duration-150 ease-out hover:shadow-pop">
              <div className={cn('flex h-24 items-center justify-center', image ? 'mesh-blue' : 'bg-inset')}>
                {image ? <Icon name="image" size={26} className="text-white drop-shadow" /> : <Icon name="file" size={26} className="text-faint" />}
              </div>
              <div className="px-3 py-2.5">
                <p className="truncate text-sm font-medium text-fg" title={a.name}>{a.name}</p>
                <p className="text-xs text-muted">{a.size}</p>
              </div>
            </motion.a>
          )
        })}
      </div>
    </section>
  )
}

function Check({ checked }: { checked: boolean }) {
  return (
    <motion.svg viewBox="0 0 20 20" width={20} height={20} initial={false} animate={checked ? { scale: [1, 1.18, 1] } : { scale: 1 }} transition={tween}>
      <motion.circle cx="10" cy="10" r="8.4" initial={false} animate={{ fill: checked ? 'rgb(22 163 74)' : 'rgba(0,0,0,0)', stroke: checked ? 'rgb(22 163 74)' : 'rgb(190 190 190)' }} strokeWidth="1.5" strokeDasharray={checked ? '0' : '2.6 2.2'} />
      <motion.path d="M6.4 10.4l2.4 2.4 4.8-5.2" fill="none" stroke="white" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" initial={false} animate={{ opacity: checked ? 1 : 0 }} />
    </motion.svg>
  )
}

function Subtasks({ issue }: { issue: IssueData }) {
  const t = useT()
  if (issue.subtasks.length === 0) return null
  const done = issue.subtasks.filter((s) => s.done).length
  return (
    <section>
      <SubHead right={<span className="ml-auto flex w-32 items-center gap-2 text-sm tabular-nums text-muted"><ProgressBar value={(done / issue.subtasks.length) * 100} tone="info" />{done}/{issue.subtasks.length}</span>}>{t('blocks.issueSubtasks')}</SubHead>
      <ul className="overflow-hidden rounded-card bg-surface shadow-card">
        {issue.subtasks.map((s) => (
          <li key={s.key} className="flex h-12 items-center gap-3 px-4 [&+&]:border-t">
            <Check checked={s.done} />
            <span className="w-[68px] shrink-0 text-sm text-muted">{s.key}</span>
            {s.type && <span className="shrink-0 rounded-full bg-inset px-2 py-0.5 text-xs text-fg2">{s.type}</span>}
            <span className={cn('flex-1 truncate text-base transition-colors', s.done ? 'text-muted line-through' : 'text-fg')} title={s.title}>{s.title}</span>
          </li>
        ))}
      </ul>
    </section>
  )
}

function CommentBox({ connection, issueKey, onSent }: { connection: string; issueKey: string; onSent: () => void }) {
  const t = useT()
  const [text, setText] = useState('')
  const [key, setKey] = useState(0)
  const [error, setError] = useState<string | null>(null)
  const save = () => {
    const body = text.trim()
    if (!body) return
    api()
      .call(connection, 'tracker.comment', { key: issueKey, body })
      .then(() => {
        setText('')
        setKey(key + 1)
        onSent()
      })
      .catch((e: unknown) => setError(errorText(e)))
  }
  return (
    <div className="mt-3">
      <MarkdownEditor key={key} compact minHeight={110} label={t('blocks.issueCommentLabel')} placeholder={t('blocks.issueCommentPlaceholder')} value={text} onChange={setText} saveLabel={t('blocks.comment')} saveDisabled={!text.trim()} onSave={save} />
      <SendError error={error} />
    </div>
  )
}

export function IssueBlock({ block }: { block: Of<'issue'> }) {
  const t = useT()
  const { version } = useBlockHost()
  const [bump, setBump] = useState(0)
  const { data: issue, error } = useCall(block.connection, 'tracker.issue', { key: block.key }, `${version}:${bump}`)
  if (!issue) return <Loading error={error} />
  return (
    <div className="space-y-8 px-2 py-2">
      <div className="flex items-center gap-2 text-sm text-muted">
        {issue.type && <span className="rounded-full bg-inset px-2 py-0.5 text-xs font-medium text-fg2">{issue.type}</span>}
        {issue.parent && <span className="text-faint">↑ {issue.parent}</span>}
        {issue.status && <span>{issue.status}</span>}
        {issue.assignee && (
          <>
            <span className="text-faint">·</span>
            <PersonAvatar person={issue.assignee} size={18} />
            {personName(issue.assignee)}
          </>
        )}
      </div>
      <div>
        <h2 className="text-2xl font-medium tracking-tighter2 text-fg">{issue.title}</h2>
        <div className="-mx-3 mt-3 rounded-card px-3 py-2">
          <MarkdownPreview text={issue.description} empty={t('blocks.issueNoDescription')} />
        </div>
      </div>
      <Attachments issue={issue} />
      <Subtasks issue={issue} />
      <section>
        <h3 className="mb-3 text-md font-medium text-fg2">{t('blocks.issueComments')}</h3>
        <Thread comments={issue.comments} />
        {block.comment && <CommentBox connection={block.connection} issueKey={block.key} onSent={() => setBump(bump + 1)} />}
      </section>
    </div>
  )
}
