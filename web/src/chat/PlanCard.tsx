import { createContext, useContext, useMemo, useState, type ReactNode } from 'react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { Button } from '@/ui/button'
import { cn } from '@/ui/cn'
import { ProviderLogo } from '@/ui/domain'
import { Textarea } from '@/ui/form'
import { Icon, type IconName } from '@/ui/icon'
import { Collapse, motion, tween } from '@/ui/motion'
import { Failure, ResolvedCheck, useSend, type Respond } from './actionBlocks'
import { AgentMark, type Ev } from './events'
import { fileEntry, parsePlan, structured, type PlanSection } from './planDoc'

export interface PlanDestination {
  name: string
  logo?: string
}

export const PlanTarget = createContext<PlanDestination | null>(null)

function Md({ text, className }: { text: string; className?: string }) {
  return (
    <div className={cn('prose-chat min-w-0 [&>*:last-child]:mb-0', className)}>
      <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown>
    </div>
  )
}

const sectionIcon: Record<PlanSection['kind'], IconName> = { context: 'info', steps: 'list', files: 'file', risks: 'alert', checks: 'checkCircle', other: 'document' }

function Heading({ section }: { section: PlanSection }) {
  return (
    <p className="mb-2.5 flex items-center gap-2 text-sm font-medium text-muted">
      <Icon name={sectionIcon[section.kind]} size={15} />
      {section.heading}
    </p>
  )
}

function Steps({ items }: { items: string[] }) {
  return (
    <ol className="space-y-3">
      {items.map((item, i) => (
        <li key={i} className="flex gap-3">
          <span className="mt-0.5 flex size-6 shrink-0 items-center justify-center rounded-full bg-subtle text-sm font-medium tabular-nums text-fg2 shadow-[0_0_0_1px_rgb(var(--border))]">{i + 1}</span>
          <Md text={item} className="flex-1" />
        </li>
      ))}
    </ol>
  )
}

function Files({ items }: { items: string[] }) {
  return (
    <ul className="divide-y overflow-hidden rounded-control bg-subtle shadow-[0_0_0_1px_rgb(var(--border))]">
      {items.map((item, i) => {
        const { path, note } = fileEntry(item)
        return (
          <li key={i} className="flex min-w-0 items-baseline gap-2.5 px-3 py-2">
            <Icon name="file" size={14} className="shrink-0 translate-y-0.5 text-muted" />
            <code className="truncate font-mono text-sm text-fg" title={path}>{path}</code>
            {note && <span className="min-w-0 truncate text-sm text-muted" title={note}>{note}</span>}
          </li>
        )
      })}
    </ul>
  )
}

function Marked({ items, icon, tone }: { items: string[]; icon: IconName; tone: string }) {
  return (
    <ul className="space-y-2">
      {items.map((item, i) => (
        <li key={i} className="flex gap-2.5">
          <Icon name={icon} size={15} className={cn('mt-[5px] shrink-0', tone)} />
          <Md text={item} className="flex-1" />
        </li>
      ))}
    </ul>
  )
}

function SectionBody({ section }: { section: PlanSection }) {
  const { kind, items, body } = section
  if (items.length === 0 || kind === 'context' || kind === 'other') return <Md text={body} />
  if (kind === 'steps') return <Steps items={items} />
  if (kind === 'files') return <Files items={items} />
  if (kind === 'risks') return <Marked items={items} icon="alert" tone="text-warning-fg" />
  return <Marked items={items} icon="check" tone="text-success-fg" />
}

function PlanBody({ text }: { text: string }) {
  const doc = useMemo(() => parsePlan(text), [text])
  if (!structured(doc)) return <div className="px-5 py-4"><Md text={text} /></div>
  return (
    <div className="space-y-6 px-5 py-5">
      {doc.intro && <Md text={doc.intro} />}
      {doc.sections.filter((s) => s.body).map((s, i) => (
        <section key={`${i}-${s.heading}`}>
          <Heading section={s} />
          <SectionBody section={s} />
        </section>
      ))}
    </div>
  )
}

const titleOf = (text: string) => parsePlan(text).title ?? 'Plano de entrega'

const statusOf = { pending: ['Aguardando você', 'bg-info-soft text-info-fg'], approved: ['Aprovado', 'bg-success-soft text-success-fg'], revised: ['Refeito', 'bg-subtle text-muted'] } as const

function Header({ event }: { event: Ev<'plan'> }) {
  const [label, tone] = statusOf[event.resolution]
  return (
    <div className="flex items-center gap-3 border-b px-5 py-4">
      <AgentMark size={20} agent={event.agent} />
      <div className="min-w-0 flex-1">
        <p className="text-sm text-muted">Plano</p>
        <p className="truncate text-lg font-medium tracking-tightish text-fg">{titleOf(event.text)}</p>
      </div>
      <span className={cn('shrink-0 rounded-full px-2.5 py-1 text-sm font-medium', tone)}>{label}</span>
    </div>
  )
}

function Decide({ event, readOnly, respond }: { event: Ev<'plan'>; readOnly?: boolean; respond: Respond }) {
  const target = useContext(PlanTarget)
  const { busy, error, send } = useSend(respond)
  const [revising, setRevising] = useState(false)
  const [feedback, setFeedback] = useState('')
  const approve: ReactNode = target ? <>Criar no {target.name}</> : 'Aprovar plano'
  const icon = target?.logo ? <ProviderLogo logo={target.logo} name={target.name} size={15} /> : <Icon name="check" size={15} />
  return (
    <div className="border-t bg-subtle px-5 py-4">
      <Collapse open={revising}>
        <Textarea rows={3} value={feedback} onChange={(e) => setFeedback(e.target.value)} placeholder="O que precisa mudar no plano?" className="mb-3 bg-surface" />
      </Collapse>
      <div className="flex flex-wrap items-center gap-x-4 gap-y-3">
        <p className="min-w-0 flex-1 text-md text-muted">
          {revising ? 'O planejador refaz o plano com o que você pedir.' : target ? null : 'O desenvolvimento só começa depois que você aprovar.'}
        </p>
        <div className="flex shrink-0 gap-2">
          {revising ? (
            <>
              <Button size="sm" variant="ghost" onClick={() => setRevising(false)}>Cancelar</Button>
              <Button size="sm" variant="primary" disabled={readOnly || busy || !feedback.trim()} onClick={() => send(event.id, { kind: 'plan', decision: 'revise', feedback: feedback.trim() })}>Enviar ajustes</Button>
            </>
          ) : (
            <>
              <Button size="sm" disabled={readOnly || busy} onClick={() => setRevising(true)}>Pedir mudanças</Button>
              <Button size="sm" variant="primary" icon={icon} loading={busy} disabled={readOnly} onClick={() => send(event.id, { kind: 'plan', decision: 'approve' })}>{approve}</Button>
            </>
          )}
        </div>
      </div>
      <Failure text={error} />
    </div>
  )
}

export function PlanBlock({ event, readOnly, respond }: { event: Ev<'plan'>; readOnly?: boolean; respond: Respond }) {
  return (
    <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={tween} className={cn('overflow-hidden rounded-card bg-surface shadow-card', event.resolution === 'revised' && 'opacity-60')}>
      <Header event={event} />
      <PlanBody text={event.text} />
      {event.resolution === 'pending' ? (
        <Decide event={event} readOnly={readOnly} respond={respond} />
      ) : (
        <p className="flex items-center gap-2 border-t px-5 py-3 text-md text-muted">
          <ResolvedCheck tone={event.resolution === 'approved' ? 'success' : 'danger'} />
          {event.resolution === 'approved' ? 'Plano aprovado' : `Mudanças pedidas${event.feedback ? `: ${event.feedback}` : ''}`}
        </p>
      )}
    </motion.div>
  )
}
