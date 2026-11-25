import { AnimatePresence, motion } from 'framer-motion'
import { useState } from 'react'
import { api } from '@/api'
import type { Application, ApplicationRepository, CodeRepo, DirListing, TrackerProject } from '@/api/contract'
import { errorText, setApplications, useStore } from '@/api/store'
import { Button } from '@/ui/button'
import { cn } from '@/ui/cn'
import { ProductMark, ProviderLogo } from '@/ui/domain'
import { Field, Input, Select } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { tweenClose } from '@/ui/motion'
import { LanguagePicker } from '@/i18n/LanguagePicker'
import { useT, type T } from '@/i18n'
import { groupName } from '@/i18n/server'
import { ConnectDialog } from '../settings/ConnectDialog'
import { ConnectionCard } from '../settings/ConnectionCard'
import { FolderDialog } from '../settings/FolderDialog'
import { accountsOf } from '../settings/Settings'
import { choiceKey, folderName, fromKey, loadProjects, loadRepos, suggestedCode, useListed, useSources } from '../settings/Applications'

const SKIPPED = 'onboarding:skipped'
const NONE = 'none'

type Stage = 'connections' | 'ai' | 'application'

const ORDER: Stage[] = ['connections', 'ai', 'application']

const STAGES = (t: T): { stage: Stage; label: string; title: string; text: string }[] => [
  { stage: 'connections', label: t('app.stageConnectionsLabel'), title: t('app.stageConnectionsTitle'), text: t('app.stageConnectionsText') },
  { stage: 'ai', label: t('app.stageAiLabel'), title: t('app.stageAiTitle'), text: t('app.stageAiText') },
  { stage: 'application', label: t('app.stageApplicationLabel'), title: t('app.stageApplicationTitle'), text: t('app.stageApplicationText') },
]

export function skippedOnboarding(): boolean {
  try {
    return globalThis.localStorage?.getItem(SKIPPED) === 'true'
  } catch {
    return false
  }
}

function skipOnboarding() {
  try {
    globalThis.localStorage?.setItem(SKIPPED, 'true')
  } catch {
    return
  }
}

export function needsOnboarding(ready: boolean, applications: Application[] | null): boolean {
  return ready && applications !== null && applications.length === 0 && !skippedOnboarding()
}

interface Draft {
  name: string
  tracker: { connection: string; project: string } | null
  repository: ApplicationRepository
}

const blank: Draft = { name: '', tracker: null, repository: { id: '', code: null, directory: '', run: [], verify: [] } }

function Steps({ stage }: { stage: Stage }) {
  const t = useT()
  const at = ORDER.indexOf(stage)
  return (
    <div className="mb-6 flex items-center gap-2 text-sm text-muted">
      {STAGES(t).map(({ label }, i) => (
        <span key={label} className="flex items-center gap-2">
          {i > 0 && <span className="h-px w-6 bg-border" />}
          <span className={cn('flex size-5 items-center justify-center rounded-full text-xs tabular-nums', i < at ? 'bg-success text-white' : i === at ? 'bg-fg text-bg' : 'bg-inset text-faint')}>
            {i < at ? <Icon name="check" size={11} strokeWidth={2.6} /> : i + 1}
          </span>
          <span className={i === at ? 'text-fg' : undefined}>{label}</span>
        </span>
      ))}
    </div>
  )
}

function ConnectionsStage({ ai, onConnect }: { ai: boolean; onConnect: (provider: string) => void }) {
  const t = useT()
  const data = useStore((s) => s.connections)
  if (!data) return null
  const providers = data.providers.filter((p) => !!p.ai === ai)
  const groups = [...new Set(providers.map((p) => p.group))]
  return (
    <div className="space-y-5">
      {groups.map((g) => (
        <section key={g}>
          <h3 className="mb-2 text-sm font-medium text-muted">{groupName(t, g)}</h3>
          <div className="grid gap-2">
            {providers.filter((p) => p.group === g).map((p) => <ConnectionCard key={p.key} provider={p} accounts={accountsOf(p, data.connections)} onConnect={onConnect} />)}
          </div>
        </section>
      ))}
    </div>
  )
}

function ApplicationStage({ draft, onChange, onFolder, projects, repos }: { draft: Draft; onChange: (next: Draft) => void; onFolder: () => void; projects: Record<string, TrackerProject[]>; repos: Record<string, CodeRepo[]> }) {
  const trackers = useSources('tracker')
  const coders = useSources('code')
  const t = useT()
  const spaces = [
    { value: NONE, label: t('app.none') },
    ...trackers.flatMap((s) => (projects[s.connection.id] ?? []).map((p) => ({ value: choiceKey(s.connection.id, p.key), label: `${p.name} · ${p.key}`, icon: <ProviderLogo logo={s.provider.logo} name={s.provider.name} size={15} /> }))),
  ]
  const codes = [
    { value: NONE, label: t('app.none') },
    ...coders.flatMap((s) => (repos[s.connection.id] ?? []).map((r) => ({ value: choiceKey(s.connection.id, r.ref), label: r.ref, icon: <ProviderLogo logo={s.provider.logo} name={s.provider.name} size={15} /> }))),
  ]
  const space = draft.tracker ? choiceKey(draft.tracker.connection, draft.tracker.project) : NONE
  const code = draft.repository.code ? choiceKey(draft.repository.code.connection, draft.repository.code.repo) : NONE
  if (draft.repository.code && !codes.some((o) => o.value === code)) codes.push({ value: code, label: draft.repository.code.repo })
  return (
    <div className="space-y-5">
      <Field label={t('app.name')}>
        <Input aria-label={t('app.name')} value={draft.name} placeholder="Checkout" onChange={(e) => onChange({ ...draft, name: e.target.value })} />
      </Field>
      <div className="space-y-2">
        <span className="text-base font-medium text-fg">{t('app.repositoryFolder')}</span>
        <button type="button" aria-label={t('app.repositoryFolder')} onClick={onFolder} className="ring-field flex h-10 w-full items-center gap-2 rounded-control bg-surface px-3 text-left shadow-card transition-colors hover:bg-hover">
          <Icon name="folder" size={16} className="text-faint" />
          <span className={draft.repository.directory ? 'min-w-0 flex-1 truncate font-mono text-sm text-fg' : 'min-w-0 flex-1 truncate text-base text-muted'}>{draft.repository.directory || t('app.chooseFolder')}</span>
        </button>
      </div>
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label={t('app.space')}>
          <Select label={t('app.space')} className="w-full" value={space} options={spaces} onValueChange={(v) => {
            const picked = fromKey(v)
            onChange({ ...draft, tracker: picked ? { connection: picked.connection, project: picked.key } : null })
          }} />
        </Field>
        <Field label={t('app.remoteRepository')}>
          <Select label={t('app.remoteRepository')} className="w-full" value={code} options={codes} onValueChange={(v) => {
            const picked = fromKey(v)
            onChange({ ...draft, repository: { ...draft.repository, code: picked ? { connection: picked.connection, repo: picked.key } : null } })
          }} />
        </Field>
      </div>
    </div>
  )
}

function resumeProps(flow: string | null): Record<string, unknown> {
  return flow ? { resume: flow } : {}
}

export function stageOf(provider: string | null, providers: { key: string; ai?: boolean }[]): Stage {
  return providers.find((p) => p.key === provider)?.ai ? 'ai' : 'connections'
}

export interface Resumed {
  provider: string
  flow: string | null
}

export function returnedTo(path: string, search: string): Resumed | null {
  if (!path.startsWith('/settings/connections')) return null
  const params = new URLSearchParams(search)
  const provider = params.get('connect')
  return provider ? { provider, flow: params.get('flow') } : null
}

export function Onboarding({ onDone, resumed, onResumed }: { onDone: () => void; resumed?: Resumed | null; onResumed?: () => void }) {
  const providers = useStore((s) => s.connections?.providers ?? [])
  const [stage, setStage] = useState<Stage>(() => stageOf(resumed?.provider ?? null, providers))
  const [draft, setDraft] = useState<Draft>(blank)
  const [connecting, setConnecting] = useState<string | null>(resumed?.provider ?? null)
  const [flow, setFlow] = useState<string | null>(resumed?.flow ?? null)
  const [choosing, setChoosing] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const trackers = useSources('tracker')
  const coders = useSources('code')
  const projects = useListed<TrackerProject>(trackers, loadProjects, stage === 'application')
  const repos = useListed<CodeRepo>(coders, loadRepos, stage === 'application')
  const chosen = (listing: DirListing) => {
    setDraft((d) => ({
      ...d,
      name: d.name.trim() ? d.name : folderName(listing.path),
      repository: { ...d.repository, directory: listing.path, code: d.repository.code ?? suggestedCode(listing.remote.ref, repos) },
    }))
    setChoosing(false)
  }
  const create = () => {
    setBusy(true)
    setError(null)
    api()
      .createApplication({ name: draft.name.trim(), tracker: draft.tracker, repositories: [draft.repository] })
      .then((saved) => {
        setApplications((list) => [...list, saved])
        onDone()
      })
      .catch((e: unknown) => setError(errorText(e)))
      .finally(() => setBusy(false))
  }
  const skip = () => {
    skipOnboarding()
    onDone()
  }
  const t = useT()
  const ready = draft.name.trim() !== '' && draft.repository.directory !== ''
  const at = ORDER.indexOf(stage)
  const current = STAGES(t)[at]
  return (
    <div className="flex min-h-full flex-col items-center bg-subtle px-4 py-10">
      <div className="mb-8 flex w-full max-w-[560px] items-center justify-between">
        <ProductMark size={34} />
        <LanguagePicker className="w-36" />
      </div>
      <div className="w-full max-w-[560px] rounded-card bg-bg shadow-card">
        <div className="flex max-h-[calc(100dvh-160px)] min-h-[520px] flex-col p-6 md:p-8">
          <Steps stage={stage} />
          <AnimatePresence mode="wait" initial={false}>
            <motion.div key={stage} initial={{ opacity: 0, x: 8 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -6 }} transition={tweenClose} className="flex min-h-0 flex-1 flex-col">
              <h1 className="text-2xl font-medium tracking-tighter2 text-fg">{current.title}</h1>
              <p className="mb-6 mt-1.5 text-base text-muted">{current.text}</p>
              <div className="scrollbar-thin -mx-1 min-h-0 flex-1 overflow-y-auto px-1 pb-2">
                {stage !== 'application' ? (
                  <ConnectionsStage ai={stage === 'ai'} onConnect={setConnecting} />
                ) : (
                  <ApplicationStage draft={draft} onChange={setDraft} onFolder={() => setChoosing(true)} projects={projects} repos={repos} />
                )}
              </div>
            </motion.div>
          </AnimatePresence>
          {error && <p role="alert" className="mt-3 text-sm text-danger-fg">{error}</p>}
          <div className="mt-6 flex items-center gap-2">
            {at > 0 && <Button variant="ghost" icon={<Icon name="left" size={15} />} onClick={() => setStage(ORDER[at - 1])}>{t('app.back')}</Button>}
            <Button variant="ghost" onClick={skip}>{t('app.skipForNow')}</Button>
            {stage !== 'application' ? (
              <Button variant="primary" className="ml-auto" onClick={() => setStage(ORDER[at + 1])}>{t('app.continue')}</Button>
            ) : (
              <Button variant="primary" className="ml-auto" loading={busy} disabled={!ready} onClick={create}>{t('app.createApplication')}</Button>
            )}
          </div>
        </div>
      </div>
      <ConnectDialog provider={connecting} {...resumeProps(flow)} onClose={() => {
        setConnecting(null)
        setFlow(null)
        onResumed?.()
      }} />
      <FolderDialog open={choosing} start={draft.repository.directory || null} onClose={() => setChoosing(false)} onChoose={chosen} />
    </div>
  )
}
