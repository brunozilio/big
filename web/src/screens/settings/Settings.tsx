import { AnimatePresence, motion } from 'framer-motion'
import { useEffect, useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import { api } from '@/api'
import type { BrandingPatch, Connection, Language, OutputStyle, Provider, Theme } from '@/api/contract'
import { errorText, setBranding, useStore } from '@/api/store'
import { cn } from '@/ui/cn'
import { Card } from '@/ui/display'
import { SegmentedControl, Select, UnderlineTabs } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { stagger, tween, tweenClose } from '@/ui/motion'
import { LANGUAGES, useLanguage, useT } from '@/i18n'
import { groupName } from '@/i18n/server'
import { BrowserView } from './Browser'
import { ConnectionCard } from './ConnectionCard'
import { ApplicationsView } from './Applications'
import { PermissionsView } from './Permissions'
import { UsageView } from './Usage'

export type SettingsTab = 'connections' | 'browser' | 'permissions' | 'applications' | 'usage' | 'appearance'

export function SettingsView({ tab, onTab, onConnect }: { tab: SettingsTab; onTab: (t: SettingsTab) => void; onConnect: (provider: string) => void }) {
  const t = useT()
  return (
    <>
      <UnderlineTabs<SettingsTab> value={tab} onChange={onTab} className="mb-7" options={[{ value: 'connections', label: t('settings.tab_connections') }, { value: 'browser', label: t('settings.tab_browser') }, { value: 'permissions', label: t('settings.tab_permissions') }, { value: 'applications', label: t('settings.tab_applications') }, { value: 'usage', label: t('settings.tab_usage') }, { value: 'appearance', label: t('settings.tab_appearance') }]} />
      <AnimatePresence mode="wait" initial={false}>
        <motion.div key={tab} initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -4 }} transition={tweenClose}>
          {tab === 'connections' ? <ConnectionsView onConnect={onConnect} /> : tab === 'browser' ? <BrowserView /> : tab === 'permissions' ? <PermissionsView /> : tab === 'applications' ? <ApplicationsView /> : tab === 'usage' ? <UsageView /> : <CustomView />}
        </motion.div>
      </AnimatePresence>
    </>
  )
}

function CustomView() {
  const t = useT()
  const branding = useStore((s) => s.branding)
  const [error, setError] = useState<string | null>(null)
  const save = (patch: BrandingPatch) => {
    setError(null)
    api()
      .saveBranding(patch)
      .then(setBranding)
      .catch((e: unknown) => setError(errorText(e)))
  }
  return (
    <div className="space-y-8">
      <section>
        <h2 className="mb-2.5 text-base font-medium text-muted">{t('settings.appearance')}</h2>
        <Card className="flex items-center gap-4 px-4 py-3.5">
          <div className="min-w-0 flex-1">
            <p className="text-base font-medium text-fg">{t('settings.theme')}</p>
            <p className="text-sm text-muted">{t('settings.themeHint')}</p>
          </div>
          <SegmentedControl<Theme>
            size="sm"
            value={branding?.theme ?? 'system'}
            onChange={(theme) => save({ theme })}
            options={[
              { value: 'system', label: t('settings.theme_system'), icon: <Icon name="computer" size={14} /> },
              { value: 'light', label: t('settings.theme_light'), icon: <Icon name="sun" size={14} /> },
              { value: 'dark', label: t('settings.theme_dark'), icon: <Icon name="moon" size={14} /> },
            ]}
          />
        </Card>
        <LanguageCard onChange={(language) => save({ language })} />
      </section>
      <OutputStyleSection styles={branding?.output_styles ?? []} value={branding?.output_style ?? null} onChange={(output_style) => save({ output_style })} />
      {error && <p role="alert" className="text-sm text-danger-fg">{error}</p>}
    </div>
  )
}

function LanguageCard({ onChange }: { onChange: (language: Language) => void }) {
  const t = useT()
  const language = useLanguage()
  return (
    <Card className="mt-2.5 flex items-center gap-4 px-4 py-3.5">
      <div className="min-w-0 flex-1">
        <p className="text-base font-medium text-fg">{t('common.language')}</p>
        <p className="text-sm text-muted">{t('common.languageHint')}</p>
      </div>
      <SegmentedControl<Language> size="sm" value={language} onChange={onChange} options={LANGUAGES.map((l) => ({ value: l.value, label: l.label }))} />
    </Card>
  )
}

const DEFAULT_STYLE = 'default'

function OutputStyleSection({ styles, value, onChange }: { styles: OutputStyle[]; value: string | null; onChange: (key: string | null) => void }) {
  const t = useT()
  const chosen = styles.find((s) => s.key === value)
  const options = [{ value: DEFAULT_STYLE, label: t('settings.style_default') }, ...styles.map((s) => ({ value: s.key, label: s.name }))]
  return (
    <section>
      <h2 className="mb-2.5 text-base font-medium text-muted">{t('settings.replies')}</h2>
      <Card className="flex items-center gap-4 px-4 py-3.5">
        <div className="min-w-0 flex-1">
          <p className="text-base font-medium text-fg">{t('settings.style')}</p>
          <p className="text-sm text-muted">{chosen?.description ?? t('settings.styleHint')}</p>
        </div>
        <Select size="sm" label={t('settings.style')} className="w-44" value={chosen?.key ?? DEFAULT_STYLE} options={options} onValueChange={(v) => onChange(v === DEFAULT_STYLE ? null : v)} />
      </Card>
    </section>
  )
}

export function accountsOf(provider: Provider, connections: Connection[]) {
  return connections.filter((c) => c.provider === provider.key)
}

function ConnectionsView({ onConnect }: { onConnect: (provider: string) => void }) {
  const t = useT()
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
          <h2 className={cn('mb-2.5 text-base font-medium transition-colors', focus === g ? 'text-fg' : 'text-muted')}>{groupName(t, g)}</h2>
          <div className="grid gap-2.5 sm:grid-cols-2 lg:grid-cols-3">
            {data.providers.filter((p) => p.group === g).map((p) => <ConnectionCard key={p.key} provider={p} accounts={accountsOf(p, data.connections)} onConnect={onConnect} />)}
          </div>
        </motion.section>
      ))}
    </div>
  )
}
