import { lazy, Suspense, useEffect, useRef, useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import { api } from '@/api'
import type { BrandingPatch, Connection, Language, OutputStyle, Provider, Theme } from '@/api/contract'
import { errorText, setBranding, useStore } from '@/api/store'
import { SETTINGS_GROUPS, type SettingsTab } from '@/app/settingsNav'
import { cn } from '@/ui/cn'
import { Card, GridSkeleton, ListSkeleton } from '@/ui/display'
import { SegmentedControl, Select } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { FadeIn } from '@/ui/motion'
import { LANGUAGES, useLanguage, useT } from '@/i18n'
import { groupName, styleDescription, styleName } from '@/i18n/server'
import { ConnectionCard } from './ConnectionCard'

export type { SettingsTab } from '@/app/settingsNav'

const BrowserView = lazy(() => import('./Browser').then((m) => ({ default: m.BrowserView })))
const ApplicationsView = lazy(() => import('./Applications').then((m) => ({ default: m.ApplicationsView })))
const MemoryView = lazy(() => import('./Memory').then((m) => ({ default: m.MemoryView })))
const PermissionsView = lazy(() => import('./Permissions').then((m) => ({ default: m.PermissionsView })))
const UsageView = lazy(() => import('./Usage').then((m) => ({ default: m.UsageView })))

export function SettingsView({ tab, onTab, onConnect }: { tab: SettingsTab; onTab: (t: SettingsTab) => void; onConnect: (provider: string) => void }) {
  return (
    <div className="md:grid md:grid-cols-[184px_minmax(0,1fr)] md:gap-10">
      <SettingsNav tab={tab} onTab={onTab} />
      <FadeIn key={tab} className="min-w-0">
        <Suspense fallback={<ListSkeleton />}>
          {tab === 'connections' ? <ConnectionsView onConnect={onConnect} /> : tab === 'browser' ? <BrowserView /> : tab === 'permissions' ? <PermissionsView /> : tab === 'memory' ? <MemoryView /> : tab === 'applications' ? <ApplicationsView /> : tab === 'usage' ? <UsageView /> : <CustomView />}
        </Suspense>
      </FadeIn>
    </div>
  )
}

function SettingsNav({ tab, onTab }: { tab: SettingsTab; onTab: (t: SettingsTab) => void }) {
  const t = useT()
  const nav = useRef<HTMLElement>(null)
  useEffect(() => {
    nav.current?.querySelector<HTMLElement>('[aria-current="page"]')?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' })
  }, [tab])
  return (
    <nav ref={nav} aria-label={t('app.settings')} className="no-scrollbar -mx-5 mb-6 flex gap-1 overflow-x-auto border-b px-5 pb-2 md:sticky md:top-6 md:mx-0 md:mb-0 md:flex-col md:gap-5 md:self-start md:overflow-visible md:border-b-0 md:px-0 md:pb-0">
      {SETTINGS_GROUPS.map((group) => (
        <div key={group.key} role="group" aria-label={t(`app.settingsGroup_${group.key}`)} className="contents md:block">
          <p aria-hidden className="mb-1 hidden px-2.5 text-xs font-medium text-faint md:block">{t(`app.settingsGroup_${group.key}`)}</p>
          <div className="contents md:flex md:flex-col md:gap-0.5">
            {group.tabs.map((value) => (
              <button
                key={value}
                type="button"
                aria-current={value === tab ? 'page' : undefined}
                onClick={() => onTab(value)}
                className={cn('flex h-9 shrink-0 items-center whitespace-nowrap rounded-control-sm px-2.5 text-base transition-colors', value === tab ? 'bg-active font-medium text-fg' : 'text-muted hover:bg-hover hover:text-fg')}
              >
                {t(`settings.tab_${value}`)}
              </button>
            ))}
          </div>
        </div>
      ))}
    </nav>
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
      <OutputStyleSection brand={branding?.name ?? ''} styles={branding?.output_styles ?? []} value={branding?.output_style ?? null} onChange={(output_style) => save({ output_style })} />
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

function OutputStyleSection({ brand, styles, value, onChange }: { brand: string; styles: OutputStyle[]; value: string | null; onChange: (key: string | null) => void }) {
  const t = useT()
  const chosen = styles.find((s) => s.key === value)
  const options = [{ value: DEFAULT_STYLE, label: t('settings.style_default') }, ...styles.map((s) => ({ value: s.key, label: styleName(t, s) }))]
  return (
    <section>
      <h2 className="mb-2.5 text-base font-medium text-muted">{t('settings.replies')}</h2>
      <Card className="flex items-center gap-4 px-4 py-3.5">
        <div className="min-w-0 flex-1">
          <p className="text-base font-medium text-fg">{t('settings.style')}</p>
          <p className="text-sm text-muted">{chosen ? styleDescription(t, chosen, brand) : t('settings.styleHint')}</p>
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
  if (!data) return <GridSkeleton />
  const groups = [...new Set(data.providers.map((p) => p.group))]
  return (
    <div className="space-y-8">
      {groups.map((g) => (
        <section key={g} data-group={g}>
          <h2 className={cn('mb-2.5 text-base font-medium transition-colors', focus === g ? 'text-fg' : 'text-muted')}>{groupName(t, g)}</h2>
          <div className="grid gap-2.5 sm:grid-cols-2 md:grid-cols-1 lg:grid-cols-2">
            {data.providers.filter((p) => p.group === g).map((p) => <ConnectionCard key={p.key} provider={p} accounts={accountsOf(p, data.connections)} onConnect={onConnect} />)}
          </div>
        </section>
      ))}
    </div>
  )
}
