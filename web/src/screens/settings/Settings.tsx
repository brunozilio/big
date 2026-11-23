import { AnimatePresence, motion } from 'framer-motion'
import { useEffect, useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import { api } from '@/api'
import type { Connection, Provider, Theme } from '@/api/contract'
import { errorText, setBranding, useStore } from '@/api/store'
import { cn } from '@/ui/cn'
import { Card } from '@/ui/display'
import { ProductMark } from '@/ui/domain'
import { SegmentedControl, UnderlineTabs } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { LogoUpload } from '@/ui/logoUpload'
import { stagger, tween, tweenClose } from '@/ui/motion'
import { ConnectionCard } from './ConnectionCard'

export type SettingsTab = 'connections' | 'appearance'

export function SettingsView({ tab, onTab, onConnect }: { tab: SettingsTab; onTab: (t: SettingsTab) => void; onConnect: (provider: string) => void }) {
  return (
    <>
      <UnderlineTabs<SettingsTab> value={tab} onChange={onTab} className="mb-7" options={[{ value: 'connections', label: 'Conexões' }, { value: 'appearance', label: 'Personalização' }]} />
      <AnimatePresence mode="wait" initial={false}>
        <motion.div key={tab} initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -4 }} transition={tweenClose}>
          {tab === 'connections' ? <ConnectionsView onConnect={onConnect} /> : <CustomView />}
        </motion.div>
      </AnimatePresence>
    </>
  )
}

function CustomView() {
  const branding = useStore((s) => s.branding)
  const [error, setError] = useState<string | null>(null)
  const save = (patch: { logo?: string | null; theme?: Theme }) => {
    setError(null)
    api()
      .saveBranding(patch)
      .then(setBranding)
      .catch((e: unknown) => setError(errorText(e)))
  }
  return (
    <div className="space-y-8">
      <section>
        <h2 className="mb-2.5 text-base font-medium text-muted">Marca</h2>
        <Card className="px-4 py-4">
          <LogoUpload value={branding?.logo ?? null} onChange={(logo) => save({ logo })} preview={<ProductMark size={64} />} />
        </Card>
      </section>
      <section>
        <h2 className="mb-2.5 text-base font-medium text-muted">Aparência</h2>
        <Card className="flex items-center gap-4 px-4 py-3.5">
          <div className="min-w-0 flex-1">
            <p className="text-base font-medium text-fg">Tema</p>
            <p className="text-sm text-muted">Claro, escuro ou igual ao sistema</p>
          </div>
          <SegmentedControl<Theme>
            size="sm"
            value={branding?.theme ?? 'system'}
            onChange={(theme) => save({ theme })}
            options={[
              { value: 'system', label: 'Sistema', icon: <Icon name="computer" size={14} /> },
              { value: 'light', label: 'Claro', icon: <Icon name="sun" size={14} /> },
              { value: 'dark', label: 'Escuro', icon: <Icon name="moon" size={14} /> },
            ]}
          />
        </Card>
      </section>
      {error && <p role="alert" className="text-sm text-danger-fg">{error}</p>}
    </div>
  )
}

export function accountsOf(provider: Provider, connections: Connection[]) {
  return connections.filter((c) => c.provider === provider.key)
}

function ConnectionsView({ onConnect }: { onConnect: (provider: string) => void }) {
  const data = useStore((s) => s.connections)
  const [search] = useSearchParams()
  const focus = search.get('group')
  const loaded = !!data
  useEffect(() => {
    if (!focus || !loaded) return
    const el = Array.from(document.querySelectorAll<HTMLElement>('[data-group]')).find((x) => x.dataset.group === focus)
    el?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }, [focus, loaded])
  if (!data) return null
  const groups = [...new Set(data.providers.map((p) => p.group))]
  return (
    <div className="space-y-8">
      {groups.map((g, gi) => (
        <motion.section key={g} data-group={g} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ ...tween, delay: stagger(gi) }}>
          <h2 className={cn('mb-2.5 text-base font-medium transition-colors', focus === g ? 'text-fg' : 'text-muted')}>{g}</h2>
          <div className="grid gap-2.5 sm:grid-cols-2 lg:grid-cols-3">
            {data.providers.filter((p) => p.group === g).map((p) => <ConnectionCard key={p.key} provider={p} accounts={accountsOf(p, data.connections)} onConnect={onConnect} />)}
          </div>
        </motion.section>
      ))}
    </div>
  )
}
