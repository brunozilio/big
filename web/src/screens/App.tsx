import { motion } from 'framer-motion'
import { lazy, Suspense, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { useLocation, useNavigate, useParams, useSearchParams } from 'react-router-dom'
import { retry, useStore } from '@/api/store'
import { CommandPalette } from '@/app/CommandPalette'
import { ConnectionIndicator } from '@/app/connection'
import { hotkeyLabel, useEscapeBack, useHotkey } from '@/app/escape'
import { deciding, needsOnboarding, returnedTo } from '@/app/gate'
import { itemPath } from '@/app/palette'
import { settingsTab } from '@/app/settingsNav'
import { useBrandTitle, useThemeSync } from '@/app/theme'
import { useLanguageSync } from '@/i18n/sync'
import { useT } from '@/i18n'
import { Button, IconButton } from '@/ui/button'
import { cn } from '@/ui/cn'
import { GridSkeleton, ListSkeleton, Skeleton } from '@/ui/display'
import { ProductMark } from '@/ui/domain'
import { Icon } from '@/ui/icon'
import { FadeIn, tween } from '@/ui/motion'
import { Dialog, Tooltip } from '@/ui/overlay'
import { Toaster } from '@/ui/toast'
import { ItemsView } from './items/ItemsView'
import { NewItem } from './items/NewItem'

const loadConnectDialog = () => import('./settings/ConnectDialog')
const ConnectDialog = lazy(() => loadConnectDialog().then((m) => ({ default: m.ConnectDialog })))
const ItemPage = lazy(() => import('./items/ItemModal').then((m) => ({ default: m.ItemPage })))
const Onboarding = lazy(() => import('./onboarding/Onboarding').then((m) => ({ default: m.Onboarding })))
const SecurityView = lazy(() => import('./security/SecurityView').then((m) => ({ default: m.SecurityView })))
const SettingsView = lazy(() => import('./settings/Settings').then((m) => ({ default: m.SettingsView })))

type Page = 'settings' | 'security' | 'item' | 'items'

export function App() {
  useThemeSync()
  useLanguageSync()
  useBrandTitle()
  const t = useT()
  const location = useLocation()
  const navigate = useNavigate()
  const ready = useStore((s) => s.ready)
  const applications = useStore((s) => s.applications)
  const onboarding = useStore((s) => s.onboarding)
  const [, setSettled] = useState(0)
  const resumed = returnedTo(location.pathname, location.search)
  const gatedPath = resumed || !location.pathname.startsWith('/settings')
  const gated = gatedPath && needsOnboarding(ready, applications, onboarding)
  const waiting = gatedPath && deciding(ready, applications)
  return (
    <>
      {waiting ? (
        <div className="h-full bg-subtle" aria-busy="true" />
      ) : gated ? (
        <div className="scrollbar-thin h-full overflow-y-auto">
          <Suspense fallback={<div className="mx-auto w-full max-w-xl px-5 py-16"><ListSkeleton /></div>}>
            <Onboarding resumed={resumed} onResumed={() => resumed && navigate('/', { replace: true })} onDone={() => {
              setSettled((n) => n + 1)
              navigate('/')
            }} />
          </Suspense>
        </div>
      ) : (
        <div className="flex h-full bg-subtle">
          <div className="flex min-w-0 flex-1 flex-col md:p-2">
            <main className="scrollbar-thin relative min-h-0 flex-1 overflow-y-auto bg-bg md:rounded-card md:shadow-card">
              <DeliveryPage />
            </main>
          </div>
        </div>
      )}
      <Toaster closeLabel={t('editor.close')} />
    </>
  )
}

function LoadState() {
  const ready = useStore((s) => s.ready)
  const error = useStore((s) => s.error)
  const t = useT()
  if (!ready) return null
  if (!error) return null
  return (
    <div className="mb-8 flex items-center gap-3 rounded-card bg-danger-soft px-4 py-3 text-base text-danger-fg">
      <Icon name="alert" size={16} />
      <span className="min-w-0 flex-1">{error}</span>
      <Button size="sm" variant="secondary" onClick={() => void retry()}>{t('app.retry')}</Button>
    </div>
  )
}

function headerButton(active: boolean) {
  return cn('relative flex size-9 items-center justify-center rounded-control text-fg2 transition-colors hover:bg-hover hover:text-fg', active && 'bg-active text-fg')
}

function Header({ page, onHome, onPalette }: { page: Page; onHome: () => void; onPalette: () => void }) {
  const t = useT()
  const navigate = useNavigate()
  const settings = page === 'settings'
  const security = page === 'security'
  const search = t('app.paletteOpen', { keys: hotkeyLabel('k') })
  return (
    <div className="mb-8 flex h-10 items-center gap-3">
      {settings || security ? (
        <FadeIn key="back" className="flex items-center gap-2">
          <IconButton tooltip={t('app.back')} onClick={onHome}><Icon name="left" size={18} /></IconButton>
          <h1 className="text-lg font-medium text-fg">{security ? t('app.security') : t('app.settings')}</h1>
        </FadeIn>
      ) : (
        <FadeIn key="mark" className="flex">
          <ProductMark />
        </FadeIn>
      )}
      <div className="ml-auto flex items-center gap-2">
        <ConnectionIndicator />
        <Tooltip content={search}>
          <button aria-label={search} onClick={onPalette} className={headerButton(false)}>
            <Icon name="search" size={19} />
          </button>
        </Tooltip>
        <Tooltip content={t('app.security')}>
          <button aria-label={t('app.security')} onClick={() => navigate(security ? '/' : '/security')} className={headerButton(security)}>
            <Icon name="shield" size={19} />
          </button>
        </Tooltip>
        <Tooltip content={t('app.settings')}>
          <button aria-label={t('app.settings')} onClick={() => navigate(settings ? '/' : '/settings/connections')} className={headerButton(settings)}>
            <motion.span animate={{ rotate: settings ? 90 : 0 }} transition={tween} className="flex"><Icon name="settings" size={19} /></motion.span>
          </button>
        </Tooltip>
      </div>
    </div>
  )
}

function ConnectDialogSkeleton({ onClose }: { onClose: () => void }) {
  return (
    <Dialog open static onOpenChange={(o) => !o && onClose()} title={<Skeleton className="mt-1.5 h-4 w-48" />}>
      <div aria-busy="true" className="flex min-h-[242px] flex-col gap-4">
        <div className="flex items-center gap-3">
          <div className="skeleton size-11 shrink-0 !rounded-[28%]" />
          <div className="min-w-0 flex-1 space-y-2">
            <Skeleton className="w-1/3" />
            <Skeleton className="h-2.5 w-2/3" />
          </div>
        </div>
        <Skeleton className="h-10 w-full !rounded-control" />
        <Skeleton className="h-10 w-full !rounded-control" />
      </div>
    </Dialog>
  )
}

function useScrollMemory(page: Page) {
  const root = useRef<HTMLDivElement>(null)
  const saved = useRef(0)
  const scroller = () => root.current?.closest('main') ?? null
  useLayoutEffect(() => {
    const el = scroller()
    if (!el) return
    el.scrollTop = page === 'items' ? saved.current : 0
    if (page === 'items') saved.current = 0
  }, [page])
  return { root, remember: () => { saved.current = scroller()?.scrollTop ?? 0 } }
}

function DeliveryPage() {
  const location = useLocation()
  const navigate = useNavigate()
  const params = useParams()
  const [search, setSearch] = useSearchParams()
  const [fresh, setFresh] = useState<string | null>(null)
  const [palette, setPalette] = useState(false)
  const ready = useStore((s) => s.ready)
  const items = useStore((s) => s.items)
  const settings = location.pathname.startsWith('/settings')
  const security = location.pathname.startsWith('/security')
  const aside = settings || security
  const connecting = settings ? search.get('connect') : null
  const openId = aside ? null : params.id ?? null
  const open = openId ? items.find((x) => x.id === openId) ?? null : null
  const pending = !!openId && !ready
  const page: Page = settings ? 'settings' : security ? 'security' : open || pending ? 'item' : 'items'
  const { root, remember } = useScrollMemory(page)
  const wanted = useRef(false)
  if (connecting) wanted.current = true
  const home = () => navigate({ pathname: '/', search: aside ? '' : location.search })
  const openItem = (id: string, step?: string) => {
    if (page === 'items') remember()
    navigate({ pathname: itemPath(id, step), search: location.search })
  }
  const goConnect = (provider: string) => navigate(`/settings/connections?connect=${encodeURIComponent(provider)}`)
  const patchSearch = (patch: (next: URLSearchParams) => void) => setSearch((before) => {
    const next = new URLSearchParams(before)
    patch(next)
    return next
  })
  useEffect(() => {
    if (settings) void loadConnectDialog()
  }, [settings])
  useEscapeBack(page !== 'items', home)
  useHotkey('k', () => setPalette((v) => !v))
  return (
    <div ref={root} className="mx-auto w-full max-w-[1120px] px-5 pb-16 pt-6 md:px-10">
      <Header page={page} onHome={home} onPalette={() => setPalette(true)} />
      <LoadState />
      {settings && (
        <FadeIn key="settings">
          <Suspense fallback={<GridSkeleton />}>
            <SettingsView tab={settingsTab(params.tab)} onTab={(next) => navigate(`/settings/${next}`)} onConnect={(p) => patchSearch((s) => s.set('connect', p))} />
          </Suspense>
        </FadeIn>
      )}
      {security && (
        <FadeIn key="security">
          <Suspense fallback={<ListSkeleton />}><SecurityView /></Suspense>
        </FadeIn>
      )}
      {!aside && (
        <div aria-hidden={page === 'item' || undefined} {...(page === 'item' ? { inert: '' } : {})} className={page === 'item' ? 'invisible h-0 overflow-hidden' : undefined}>
          <NewItem onCreated={(it) => {
            setFresh(it.id)
            setTimeout(() => setFresh(null), 1600)
          }} onAsked={(it) => openItem(it.id, it.current ?? undefined)} />
          <ItemsView fresh={fresh} onOpen={(it, step) => openItem(it.id, step)} onConnect={goConnect} />
        </div>
      )}
      {pending && <ListSkeleton />}
      {open && (
        <FadeIn key={open.id}>
          <Suspense fallback={<ListSkeleton />}>
            <ItemPage it={open} step={params.step ?? null} onStep={(s) => openItem(open.id, s)} onClose={home} onConnect={goConnect} />
          </Suspense>
        </FadeIn>
      )}
      {wanted.current && (
        <Suspense fallback={connecting ? <ConnectDialogSkeleton onClose={() => patchSearch((s) => s.delete('connect'))} /> : null}>
          <ConnectDialog provider={connecting} resume={search.get('flow')} onClose={() => patchSearch((s) => {
            s.delete('connect')
            s.delete('flow')
          })} />
        </Suspense>
      )}
      <CommandPalette open={palette} onClose={() => setPalette(false)} />
    </div>
  )
}
