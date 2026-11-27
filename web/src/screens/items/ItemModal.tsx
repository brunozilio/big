import { motion } from 'framer-motion'
import { useEffect, useMemo, useRef, useState } from 'react'
import { api } from '@/api'
import { mins } from '@/api/clock'
import type { Block, HeadsUp, Item, StepState, StepView } from '@/api/contract'
import { useStepView, useStore } from '@/api/store'
import { BlockContext, type BlockHost } from '@/blocks/context'
import { BlockView, fillsPanel } from '@/blocks/registry'
import { ChatView } from '@/chat/ChatView'
import { useT } from '@/i18n'
import { spoken, titleOf } from '@/i18n/server'
import { PlanTarget, type PlanDestination } from '@/chat/PlanCard'
import { Button, IconButton } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Card, Spinner } from '@/ui/display'
import { SegmentedControl } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { springPop } from '@/ui/motion'
import { Dialog, Menu, MenuContent, MenuItem, MenuTrigger } from '@/ui/overlay'
import { BaseFix } from './BaseFix'
import { UsageCard } from './UsageCard'
import { archiveToggle, ChangeChip, IssueKey, Node, nodeState, report, RowRight, stateLabel } from './parts'
import { useMinutesSince } from './ticker'

export function followed(before: string | null, now: string | null, viewing: string | null): string | null {
  return before && now && now !== before && viewing === before ? now : null
}

function useFollowCurrent(it: Item | null, step: string | null, onStep: (s: string) => void) {
  const was = useRef<{ id: string; current: string | null } | null>(null)
  const current = it?.current ?? null
  useEffect(() => {
    const before = was.current && was.current.id === it?.id ? was.current.current : null
    was.current = it ? { id: it.id, current } : null
    const next = followed(before, current, step)
    if (next) onStep(next)
  }, [it?.id, current])
}

type DetailProps = { it: Item | null; step: string | null; onStep: (s: string) => void; onClose: () => void; onConnect: (provider: string) => void }
type OpenProps = DetailProps & { it: Item }

const shownStep = (it: Item, step: string | null) => step ?? it.current ?? it.steps[0]?.id ?? ''

export function ItemModal({ it, step, onStep, onClose, onConnect }: DetailProps) {
  useFollowCurrent(it, step, onStep)
  const t = useT()
  return (
    <Dialog open={!!it} onOpenChange={(o) => !o && onClose()} size="xl" bare title={it ? `${it.issue?.key ?? ''} ${titleOf(t, it)}` : ''}>
      {it && (
        <div className="overflow-hidden">
          <ItemHeader it={it} step={step} onStep={onStep} onClose={onClose} onConnect={onConnect} className="h-16 px-5" />
          <div className="px-4 pb-4">
            <div className="h-[min(660px,calc(88dvh-84px))]">
              <ItemDetail it={it} step={shownStep(it, step)} onStep={onStep} />
            </div>
          </div>
        </div>
      )}
    </Dialog>
  )
}

export function ItemPage({ it, step, onStep, onClose, onConnect }: DetailProps) {
  useFollowCurrent(it, step, onStep)
  const t = useT()
  if (!it) return null
  return (
    <section aria-label={`${it.issue?.key ?? ''} ${titleOf(t, it)}`.trim()} className="flex h-[calc(100dvh-176px)] min-h-[520px] flex-col gap-3">
      <ItemHeader it={it} step={step} onStep={onStep} onClose={onClose} onConnect={onConnect} page className="min-h-10" />
      <div className="min-h-0 flex-1">
        <ItemDetail it={it} step={shownStep(it, step)} onStep={onStep} page />
      </div>
    </section>
  )
}

function ItemHeader({ it, step, onStep, onClose, onConnect, page, className }: OpenProps & { page?: boolean; className?: string }) {
  const t = useT()
  const title = titleOf(t, it)
  return (
    <div className={cn('flex shrink-0 items-center gap-3', className)}>
      {page && <IconButton tooltip={t('app.back')} onClick={onClose}><Icon name="left" size={18} /></IconButton>}
      {it.issue && <IssueKey issue={it.issue} />}
      <span className="min-w-[120px] flex-1 truncate text-lg font-medium tracking-tightish text-fg" title={title}>{title}</span>
      <span className="flex min-w-0 max-w-[60%] items-center justify-end gap-2">
        {it.impediment ? <RowRight it={it} onStep={onStep} onConnect={(p) => { onClose(); onConnect(p) }} /> : <ChangeChip change={it.change} />}
      </span>
      <ItemMenu it={it} step={step ?? it.current} onClose={onClose} />
      {!page && <IconButton tooltip={t('items.close')} onClick={onClose}><Icon name="close" size={18} /></IconButton>}
    </div>
  )
}

function ItemMenu({ it, step, onClose }: { it: Item; step: string | null; onClose: () => void }) {
  const editor = useStore((s) => s.branding?.editor)
  const t = useT()
  const label = it.steps.find((s) => s.id === step)?.label ?? ''
  return (
    <Menu>
      <MenuTrigger asChild>
        <button aria-label={t('items.moreActions')} className="flex size-9 shrink-0 items-center justify-center rounded-control text-fg2 transition-colors hover:bg-hover hover:text-fg">
          <Icon name="more" size={18} />
        </button>
      </MenuTrigger>
      <MenuContent align="end">
        {editor && <MenuItem icon={<Icon name="code" size={16} />} onSelect={() => void api().openFolders(it.id).catch(report)}>{t('items.openIn', { name: editor })}</MenuItem>}
        {step && <MenuItem icon={<Icon name="refresh" size={16} />} onSelect={() => void api().rerun(it.id, step).catch(report)}>{t('items.rerunFrom', { step: label })}</MenuItem>}
        {it.group === 'delivered' && <MenuItem icon={<Icon name={it.archivedAt ? 'refresh' : 'archive'} size={16} />} onSelect={() => void archiveToggle(it)}>{t(it.archivedAt ? 'items.unarchive' : 'items.archive')}</MenuItem>}
        {!it.doneAt && <MenuItem danger icon={<Icon name="stop" size={16} />} onSelect={() => void api().cancel(it.id).then(onClose).catch(report)}>{t('items.cancelItem')}</MenuItem>}
      </MenuContent>
    </Menu>
  )
}

function StepList({ it, step, onStep, page }: { it: Item; step: string; onStep: (s: string) => void; page?: boolean }) {
  const tr = useT()
  return (
    <ul aria-label={tr('items.steps')} className={cn(page ? 'scrollbar-thin flex gap-1 overflow-x-auto pb-1 md:block md:space-y-0.5 md:overflow-visible md:pb-0' : 'space-y-0.5')}>
      {it.steps.map((x) => {
        const st = nodeState(it, x)
        const t = Math.round(x.seconds / 60)
        const active = x.id === step
        const locked = st === 'pending' && !active
        return (
          <li key={x.id} className={cn(page && 'shrink-0 md:shrink')}>
            <button
              disabled={locked}
              aria-current={active ? 'step' : undefined}
              onClick={() => onStep(x.id)}
              style={{ paddingLeft: 8 + (x.depth ?? 0) * 14 }}
              className={cn('relative flex w-full items-center gap-2.5 rounded-control-sm pr-2 text-left transition-colors', x.depth ? 'h-8 text-sm' : 'h-9 text-base', page && 'whitespace-nowrap', active ? 'text-fg' : locked ? 'cursor-default text-fg2' : 'text-fg2 hover:bg-hover')}
            >
              {active && <motion.span layoutId={`step-sel-${it.id}`} transition={springPop} className={cn('absolute inset-0 rounded-control-sm', page ? 'bg-inset' : 'bg-surface shadow-card')} />}
              <span className="relative flex"><Node s={x} state={st} size={x.depth ? 16 : 20} /></span>
              <span className={cn('relative flex-1 truncate leading-none', st === 'pending' && 'text-muted')}>{x.label}</span>
              <span className="relative text-xs tabular-nums leading-none text-faint">{st === 'skipped' ? tr('items.skipped') : x.attempts > 1 ? `${x.attempts}× · ${mins(t)}` : t ? mins(t) : ''}</span>
            </button>
          </li>
        )
      })}
    </ul>
  )
}

function ItemDetail({ it, step, onStep, page }: { it: Item; step: string; onStep: (s: string) => void; page?: boolean }) {
  const list = <StepList it={it} step={step} onStep={onStep} page={page} />
  return (
    <div className={cn('grid h-full gap-3', page ? 'grid-cols-1 grid-rows-[auto_minmax(0,1fr)] md:grid-cols-[220px_minmax(0,1fr)] md:grid-rows-[minmax(0,1fr)] md:gap-5' : 'grid-cols-[190px_minmax(0,1fr)] grid-rows-[minmax(0,1fr)] md:grid-cols-[210px_minmax(0,1fr)]')}>
      <div className={cn('flex min-h-0 min-w-0 flex-col gap-3', page ? 'md:overflow-y-auto' : 'self-start')}>
        {page ? list : <Card variant="inset" className="p-1.5">{list}</Card>}
        <div className={cn(page && 'hidden md:block')}><UsageCard it={it} /></div>
      </div>
      <div key={step} className="h-full min-h-0 min-w-0">
        <StepPanel it={it} step={step} />
      </div>
    </div>
  )
}

export function dockedChat(blocks: Block[]) {
  const [first, ...rest] = blocks
  if (!first || first.type !== 'chat' || first.title || rest.length === 0) return null
  if (!rest.every((b) => b.type === 'markdown' || b.type === 'actions')) return null
  return { chat: first, rest }
}

const ATTEMPT_GLYPH: Partial<Record<StepState, 'close' | 'check' | 'skip' | 'clock'>> = { failed: 'close', passed: 'check', skipped: 'skip', waiting_human: 'clock' }
const ATTEMPT_TONE: Partial<Record<StepState, string>> = { failed: 'text-danger', passed: 'text-success', skipped: 'text-faint', waiting_human: 'text-warning-fg' }

export function AttemptMark({ state }: { state: StepState }) {
  const t = useT()
  const glyph = ATTEMPT_GLYPH[state]
  return (
    <span className={cn('flex', ATTEMPT_TONE[state] ?? 'text-info')} aria-label={stateLabel(t, state)}>
      {glyph ? <Icon name={glyph} size={12} strokeWidth={2.4} /> : <Spinner size={11} className="text-info" />}
    </span>
  )
}

function RunPicker({ view, onPick, className = 'mb-4' }: { view: StepView; onPick: (n: number | null) => void; className?: string }) {
  const t = useT()
  if (view.attempts.length < 2) return null
  const last = view.attempts[view.attempts.length - 1].n
  return (
    <div className={cn('flex items-center gap-2', className)}>
      <span className="text-sm text-muted">{t('items.run')}</span>
      <SegmentedControl<string>
        size="sm"
        value={String(view.attempt)}
        onChange={(v) => onPick(Number(v) === last ? null : Number(v))}
        options={view.attempts.map((a) => ({ value: String(a.n), label: `#${a.n}`, icon: <AttemptMark state={a.state} /> }))}
      />
    </div>
  )
}

export function planDestination(it: Item, step: string): PlanDestination | null {
  if (it.issue) return null
  const at = it.steps.findIndex((s) => s.id === step)
  const next = it.steps.slice(at + 1).find((s) => s.requires === 'tracker')
  if (at < 0 || !next?.provider || next.state !== 'pending') return null
  return { name: next.provider, logo: next.logo }
}

export function StepPanel(props: { it: Item; step: string }) {
  return (
    <PlanTarget.Provider value={planDestination(props.it, props.step)}>
      <StepContent {...props} />
    </PlanTarget.Provider>
  )
}

function StepContent({ it, step }: { it: Item; step: string }) {
  const [attempt, setAttempt] = useState<number | null>(null)
  useEffect(() => setAttempt(null), [it.id, step])
  const [nonce, setNonce] = useState(0)
  const { data: view, error } = useStepView(it, step, attempt, nonce)
  const empty = !view || view.step !== step || view.blocks.length === 0
  const running = it.steps.find((s) => s.id === step)?.state === 'running'
  useEffect(() => {
    if (!empty || !running) return
    const timer = setInterval(() => setNonce((n) => n + 1), 1500)
    return () => clearInterval(timer)
  }, [empty, running])
  const host = useMemo(() => ({ item: it.id, step, version: it.updatedAt, send: (action: string, payload: unknown) => api().input(it.id, step, { action, payload }).finally(() => setNonce((n) => n + 1)) }), [it.id, step, it.updatedAt])
  const current = view && view.step === step && view.item === it.id ? view : null
  if (!current) {
    return (
      <Card className="flex h-full items-center justify-center p-4">
        {error ? <p role="alert" className="text-base text-danger-fg">{error}</p> : <Spinner />}
      </Card>
    )
  }
  return (
    <div className="flex h-full flex-col gap-3">
      {current.headsUp && <HeadsUpNote headsUp={current.headsUp} />}
      <div className="min-h-0 flex-1">
        <StepBody it={it} step={step} view={current} host={host} onPick={setAttempt} />
      </div>
    </div>
  )
}

export function HeadsUpNote({ headsUp }: { headsUp: HeadsUp }) {
  const t = useT()
  return (
    <div role="note" data-urgent={headsUp.urgent} className={cn('flex shrink-0 gap-2.5 rounded-card px-4 py-3 text-base', headsUp.urgent ? 'bg-danger-soft text-danger-fg shadow-[0_0_0_1px_rgb(var(--danger)/0.35)]' : 'bg-warning-soft text-warning-fg')}>
      <Icon name={headsUp.urgent ? 'alert' : 'info'} size={17} className="mt-px shrink-0" />
      <div className="min-w-0">
        <p className="text-sm opacity-80">{t('items.headsUp')}</p>
        <p className="mt-0.5 break-words"><span className="font-medium">{headsUp.title}</span> <span>{headsUp.explain}</span></p>
      </div>
    </div>
  )
}

function StepBody({ it, step, view: current, host, onPick: setAttempt }: { it: Item; step: string; view: StepView; host: BlockHost; onPick: (n: number | null) => void }) {
  const docked = dockedChat(current.blocks)
  if (docked) {
    return (
      <BlockContext.Provider value={host}>
        <div className="flex h-full flex-col overflow-hidden rounded-card bg-bg shadow-card">
          <RunPicker view={current} onPick={setAttempt} className="shrink-0 px-4 pt-3" />
          <div className="min-h-0 flex-1">
            <ChatView sessionId={docked.chat.session} composer={docked.chat.composer} dock={docked.rest.map((b, i) => <BlockView key={`${current.attempt}-${i}-${b.type}`} block={b} />)} />
          </div>
        </div>
      </BlockContext.Provider>
    )
  }
  if (fillsPanel(current.blocks)) {
    return (
      <BlockContext.Provider value={host}>
        <div className="flex h-full flex-col overflow-hidden rounded-card bg-bg shadow-card">
          <RunPicker view={current} onPick={setAttempt} className="shrink-0 px-4 pt-3" />
          <div className="min-h-0 flex-1">
            <BlockView key={current.attempt} block={current.blocks[0]} />
          </div>
        </div>
      </BlockContext.Provider>
    )
  }
  if (current.blocks.length === 0) {
    return (
      <Card className="scrollbar-thin h-full overflow-y-auto p-4">
        <RunPicker view={current} onPick={setAttempt} />
        <EmptyStep it={it} step={step} />
      </Card>
    )
  }
  return (
    <BlockContext.Provider value={host}>
      <Card className="scrollbar-thin h-full overflow-y-auto p-4">
        <RunPicker view={current} onPick={setAttempt} />
        <div className={cn('space-y-4', current.blocks.some((b) => b.type === 'review' || b.type === 'cases') && 'h-full')}>
          {current.blocks.map((b, i) => <BlockView key={`${current.attempt}-${i}-${b.type}`} block={b} />)}
        </div>
      </Card>
    </BlockContext.Provider>
  )
}

export function EmptyStep({ it, step }: { it: Item; step: string }) {
  const t = useT()
  const label = it.steps.find((s) => s.id === step)?.label ?? step
  const stuck = it.impediment && it.current === step ? it.impediment : null
  const running = it.steps.find((s) => s.id === step)?.state === 'running'
  const doing = !stuck && running && it.current === step && it.activity ? it.activity : null
  const waited = useMinutesSince(doing?.since)
  if (!stuck && running) {
    return (
      <p className="flex items-center gap-2 text-base text-muted">
        <Spinner size={14} />
        <span>{doing ? spoken(t, doing.said, doing.text) : t('items.inProgress')}…</span>
        {doing && <span className="tabular-nums text-faint">{mins(waited)}</span>}
      </p>
    )
  }
  if (!stuck) return <p className="text-base text-muted">{t('items.nothingYet')}</p>
  return (
    <div className="space-y-3">
      <p className="text-base font-medium text-fg">{stuck.reason}</p>
      {stuck.detail && stuck.detail !== stuck.reason && <p className="whitespace-pre-wrap break-words text-sm text-muted">{stuck.detail}</p>}
      {stuck.base && <BaseFix it={it} gap={stuck.base} />}
      <div className="flex flex-wrap gap-2">
        {!stuck.base && <Button size="sm" variant="primary" icon={<Icon name="refresh" size={15} />} onClick={() => api().rerun(it.id, step).catch(report)}>{t('items.rerunFrom', { step: label })}</Button>}
        {!it.doneAt && <Button size="sm" variant="secondary" icon={<Icon name="stop" size={15} />} onClick={() => api().cancel(it.id).catch(report)}>{t('items.cancelItem')}</Button>}
      </div>
    </div>
  )
}
