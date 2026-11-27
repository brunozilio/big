import { motion } from 'framer-motion'
import { useState } from 'react'
import { api } from '@/api'
import { useAdmin } from '@/api/me'
import { errorText, setOnboarding, useStore } from '@/api/store'
import { Button } from '@/ui/button'
import { Icon } from '@/ui/icon'
import { tweenClose } from '@/ui/motion'
import { useT, type T } from '@/i18n'
import { groupName } from '@/i18n/server'
import { ConnectDialog } from '../settings/ConnectDialog'
import { ConnectionCard } from '../settings/ConnectionCard'
import { accountsOf } from '../settings/Settings'
import { ApplicationEditor } from '../settings/ApplicationEditor'
import { GridSkeleton } from '@/ui/display'
import { needsOnboarding, returnedTo, type Resumed } from '@/app/gate'
import { installerLabels, OnboardingShell, StageHeading, wasInstalled } from './Shell'

export { needsOnboarding, returnedTo }
export type { Resumed }

type Stage = 'connections' | 'ai' | 'application'

const ORDER: Stage[] = ['connections', 'ai', 'application']

const STAGES = (t: T): { stage: Stage; label: string; title: string; text: string }[] => [
  { stage: 'connections', label: t('app.stageConnectionsLabel'), title: t('app.stageConnectionsTitle'), text: t('app.stageConnectionsText') },
  { stage: 'ai', label: t('app.stageAiLabel'), title: t('app.stageAiTitle'), text: t('app.stageAiText') },
  { stage: 'application', label: t('app.stageApplicationLabel'), title: t('app.stageApplicationTitle'), text: t('app.stageApplicationText') },
]

function ConnectionsStage({ ai, onConnect }: { ai: boolean; onConnect: (provider: string) => void }) {
  const t = useT()
  const data = useStore((s) => s.connections)
  const admin = useAdmin()
  if (!data) return <GridSkeleton groups={1} cards={2} />
  const providers = data.providers.filter((p) => !!p.ai === ai && !p.custom && (admin || !p.ai))
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

function resumeProps(flow: string | null): Record<string, unknown> {
  return flow ? { resume: flow } : {}
}

export function stageOf(provider: string | null, providers: { key: string; ai?: boolean }[]): Stage {
  return providers.find((p) => p.key === provider)?.ai ? 'ai' : 'connections'
}

export function Onboarding({ onDone, resumed, onResumed }: { onDone: () => void; resumed?: Resumed | null; onResumed?: () => void }) {
  const providers = useStore((s) => s.connections?.providers ?? [])
  const [stage, setStage] = useState<Stage>(() => stageOf(resumed?.provider ?? null, providers))
  const [connecting, setConnecting] = useState<string | null>(resumed?.provider ?? null)
  const [flow, setFlow] = useState<string | null>(resumed?.flow ?? null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const skip = () => {
    setBusy(true)
    setError(null)
    api()
      .saveOnboarding({ skipped: true })
      .then((kept) => {
        setOnboarding(kept)
        onDone()
      })
      .catch((e: unknown) => setError(errorText(e)))
      .finally(() => setBusy(false))
  }
  const t = useT()
  const [installed] = useState(wasInstalled)
  const at = ORDER.indexOf(stage)
  const current = STAGES(t)[at]
  const steps = (
    <>
      {at > 0 && <Button variant="ghost" icon={<Icon name="left" size={15} />} onClick={() => setStage(ORDER[at - 1])}>{t('app.back')}</Button>}
      <Button variant="ghost" disabled={busy} onClick={skip}>{t('app.skipForNow')}</Button>
    </>
  )
  const connect = (
    <ConnectDialog provider={connecting} {...resumeProps(flow)} onClose={() => {
      setConnecting(null)
      setFlow(null)
      onResumed?.()
    }} />
  )
  const labels = STAGES(t).map((s) => s.label)
  return (
    <OnboardingShell labels={installed ? [...installerLabels(t), ...labels] : labels} current={installed ? at + 2 : at} outside={connect}>
      <motion.div key={stage} initial={{ opacity: 0, x: 8 }} animate={{ opacity: 1, x: 0 }} transition={tweenClose} className="flex min-h-0 flex-1 flex-col">
        <StageHeading title={current.title} text={current.text} />
        <div className="scrollbar-thin -mx-1 min-h-0 flex-1 overflow-y-auto px-1 pb-2">
          {stage !== 'application' ? (
            <ConnectionsStage ai={stage === 'ai'} onConnect={setConnecting} />
          ) : (
            <div className="rounded-card bg-surface shadow-card">
              <ApplicationEditor bare needsFolder application={null} saveLabel={t('app.createApplication')} actions={steps} onSaved={onDone} />
            </div>
          )}
        </div>
      </motion.div>
      {error && <p role="alert" className="mt-3 text-sm text-danger-fg">{error}</p>}
      {stage !== 'application' && (
        <div className="mt-6 flex items-center gap-2">
          {steps}
          <Button variant="primary" className="ml-auto" onClick={() => setStage(ORDER[at + 1])}>{t('app.continue')}</Button>
        </div>
      )}
    </OnboardingShell>
  )
}
