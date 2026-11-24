import { AnimatePresence, motion } from 'framer-motion'
import { useState } from 'react'
import { useLocation, useNavigate, useParams, useSearchParams } from 'react-router-dom'
import { retry, useStore } from '@/api/store'
import { useBrandTitle, useThemeSync } from '@/app/theme'
import { useLanguageSync } from '@/i18n/sync'
import { Button, IconButton } from '@/ui/button'
import { cn } from '@/ui/cn'
import { ProductMark } from '@/ui/domain'
import { Icon } from '@/ui/icon'
import { tween, tweenClose } from '@/ui/motion'
import { Tooltip } from '@/ui/overlay'
import { ItemModal } from './items/ItemModal'
import { ItemsView } from './items/ItemsView'
import { NewItem } from './items/NewItem'
import { needsOnboarding, Onboarding, returnedTo } from './onboarding/Onboarding'
import { ConnectDialog } from './settings/ConnectDialog'
import { SettingsView, type SettingsTab } from './settings/Settings'

export function App() {
  useThemeSync()
  useLanguageSync()
  useBrandTitle()
  const location = useLocation()
  const navigate = useNavigate()
  const ready = useStore((s) => s.ready)
  const applications = useStore((s) => s.applications)
  const [, setSettled] = useState(0)
  const resumed = returnedTo(location.pathname, location.search)
  if ((resumed || !location.pathname.startsWith('/settings')) && needsOnboarding(ready, applications)) {
    return (
      <div className="scrollbar-thin h-full overflow-y-auto">
        <Onboarding resumed={resumed} onResumed={() => resumed && navigate('/', { replace: true })} onDone={() => {
          setSettled((n) => n + 1)
          navigate('/')
        }} />
      </div>
    )
  }
  return (
    <div className="flex h-full bg-subtle">
      <div className="flex min-w-0 flex-1 flex-col md:p-2">
        <main className="scrollbar-thin relative min-h-0 flex-1 overflow-y-auto bg-bg md:rounded-card md:shadow-card">
          <DeliveryPage />
        </main>
      </div>
    </div>
  )
}

function LoadState() {
  const ready = useStore((s) => s.ready)
  const error = useStore((s) => s.error)
  if (!ready) return null
  if (!error) return null
  return (
    <div className="mb-8 flex items-center gap-3 rounded-card bg-danger-soft px-4 py-3 text-base text-danger-fg">
      <Icon name="alert" size={16} />
      <span className="min-w-0 flex-1">{error}</span>
      <Button size="sm" variant="secondary" onClick={() => void retry()}>Tentar de novo</Button>
    </div>
  )
}

function DeliveryPage() {
  const location = useLocation()
  const navigate = useNavigate()
  const params = useParams()
  const [search, setSearch] = useSearchParams()
  const [fresh, setFresh] = useState<string | null>(null)
  const settings = location.pathname.startsWith('/settings')
  const tab: SettingsTab = params.tab === 'appearance' || params.tab === 'browser' || params.tab === 'applications' || params.tab === 'usage' ? params.tab : 'connections'
  const connecting = search.get('connect')
  const items = useStore((s) => s.items)
  const openId = params.id ?? null
  const open = openId ? items.find((x) => x.id === openId) ?? null : null
  const openItem = (id: string, step?: string) => navigate(step ? `/items/${encodeURIComponent(id)}/${encodeURIComponent(step)}` : `/items/${encodeURIComponent(id)}`)
  const goConnect = (provider: string) => navigate(`/settings/connections?connect=${encodeURIComponent(provider)}`)
  return (
    <div className="mx-auto w-full max-w-[1120px] px-5 pb-16 pt-6 md:px-10">
      <div className="mb-8 flex h-10 items-center gap-3">
        <AnimatePresence mode="wait" initial={false}>
          {settings ? (
            <motion.div key="back" initial={{ opacity: 0, x: -6 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0 }} transition={tweenClose} className="flex items-center gap-2">
              <IconButton tooltip="Voltar" onClick={() => navigate('/')}><Icon name="left" size={18} /></IconButton>
              <span className="text-lg font-medium text-fg">Configurações</span>
            </motion.div>
          ) : (
            <motion.span key="mark" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={tweenClose} className="flex">
              <ProductMark size={30} />
            </motion.span>
          )}
        </AnimatePresence>
        <div className="ml-auto flex items-center gap-2">
          <Tooltip content="Configurações">
            <button aria-label="Configurações" onClick={() => navigate(settings ? '/' : '/settings/connections')} className={cn('relative flex size-9 items-center justify-center rounded-control text-fg2 transition-colors hover:bg-hover hover:text-fg', settings && 'bg-active text-fg')}>
              <motion.span animate={{ rotate: settings ? 90 : 0 }} transition={tween} className="flex"><Icon name="settings" size={19} /></motion.span>
            </button>
          </Tooltip>
        </div>
      </div>
      <LoadState />
      <AnimatePresence mode="wait" initial={false}>
        <motion.div key={settings ? 'settings' : 'items'} initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -4 }} transition={tweenClose}>
          {settings ? (
            <SettingsView tab={tab} onTab={(t) => navigate(`/settings/${t}`)} onConnect={(p) => setSearch({ connect: p })} />
          ) : (
            <>
              <NewItem onCreated={(it) => {
                setFresh(it.id)
                setTimeout(() => setFresh(null), 1600)
              }} />
              <ItemsView fresh={fresh} onOpen={(it, step) => openItem(it.id, step)} onConnect={goConnect} />
            </>
          )}
        </motion.div>
      </AnimatePresence>
      <ItemModal it={settings ? null : open} step={params.step ?? null} onStep={(s) => open && openItem(open.id, s)} onClose={() => navigate('/')} onConnect={goConnect} />
      <ConnectDialog provider={settings ? connecting : null} resume={search.get('flow')} onClose={() => setSearch({})} />
    </div>
  )
}
