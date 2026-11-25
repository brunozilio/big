import { type ReactNode, useEffect, useState } from 'react'
import { request } from '@/api/http'
import { Button } from '@/ui/button'
import { Card } from '@/ui/display'
import { Switch } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { useT } from '@/i18n'

export interface ConnectedBrowser {
  deviceId: string
  name: string
  platform: string
  version: string
}

export interface BrowserStatus {
  enabled: boolean
  server: string
  browsers: ConnectedBrowser[]
  extensionFolder: string
  tools: string[]
}

type Loaded = { state: 'loading' } | { state: 'failed' } | { state: 'ready'; status: BrowserStatus }

function useBrowserStatus() {
  const [loaded, setLoaded] = useState<Loaded>({ state: 'loading' })
  useEffect(() => {
    let alive = true
    const load = () =>
      request<BrowserStatus>('GET', '/api/browser')
        .then((status) => alive && setLoaded({ state: 'ready', status }))
        .catch(() => alive && setLoaded({ state: 'failed' }))
    void load()
    const timer = setInterval(load, 3000)
    return () => {
      alive = false
      clearInterval(timer)
    }
  }, [])
  const setEnabled = (enabled: boolean) => {
    setLoaded((now) => (now.state === 'ready' ? { state: 'ready', status: { ...now.status, enabled } } : now))
    void request<BrowserStatus>('PUT', '/api/browser', { enabled })
      .then((status) => setLoaded({ state: 'ready', status }))
      .catch(() => setLoaded({ state: 'failed' }))
  }
  return { loaded, setEnabled }
}

function CopyPath({ path }: { path: string }) {
  const t = useT()
  const [copied, setCopied] = useState(false)
  const copy = () => {
    void navigator.clipboard?.writeText(path).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    })
  }
  return (
    <span className="mt-2 flex items-center gap-2">
      <code className="min-w-0 flex-1 truncate rounded-md bg-secondary px-2.5 py-1.5 text-sm text-fg" title={path}>{path}</code>
      <Button size="sm" variant="secondary" icon={<Icon name={copied ? 'check' : 'copy'} size={14} />} onClick={copy}>{copied ? t('settings.copied') : t('settings.copy')}</Button>
    </span>
  )
}

function Step({ n, children }: { n: number; children: ReactNode }) {
  return (
    <li className="flex gap-3">
      <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-secondary text-xs font-medium text-fg">{n}</span>
      <div className="min-w-0 flex-1 pt-0.5 text-sm text-fg">{children}</div>
    </li>
  )
}

function Setup({ folder }: { folder: string }) {
  const t = useT()
  return (
    <Card className="px-4 py-4">
      <h3 className="mb-4 text-sm font-medium text-fg">{t('settings.browser_setup')}</h3>
      <ol className="space-y-4">
        <Step n={1}>
          {t('settings.browser_step1_open')}{' '}
          <a href="chrome://extensions" target="_blank" rel="noreferrer" className="font-mono text-info underline-offset-2 hover:underline">chrome://extensions</a>{' '}
          {t('settings.browser_step1_rest')}
        </Step>
        <Step n={2}>
          {t('settings.browser_step2')}
          <CopyPath path={folder} />
        </Step>
        <Step n={3}>{t('settings.browser_step3')}</Step>
      </ol>
    </Card>
  )
}

function StatusBadge({ connected }: { connected: boolean }) {
  const t = useT()
  return connected ? (
    <span className="inline-flex items-center gap-1.5 rounded-full bg-success-soft px-2 py-0.5 text-xs font-medium text-success-fg">
      <Icon name="check" size={12} />
      {t('settings.connected')}
    </span>
  ) : (
    <span className="rounded-full bg-secondary px-2 py-0.5 text-xs font-medium text-muted">{t('settings.browser_notConnected')}</span>
  )
}

export function BrowserView() {
  const t = useT()
  const { loaded, setEnabled } = useBrowserStatus()
  if (loaded.state === 'loading') return null
  if (loaded.state === 'failed') return <p role="alert" className="text-sm text-danger-fg">{t('settings.browser_failed')}</p>
  const { enabled, browsers, extensionFolder } = loaded.status
  const connected = browsers.length > 0
  return (
    <section className="space-y-3">
      <Card className="flex items-center gap-4 px-4 py-3.5">
        <div className="flex min-w-0 flex-1 items-center gap-2.5">
          <p className="text-base font-medium text-fg">Chrome</p>
          <StatusBadge connected={connected} />
        </div>
        <span className="text-sm text-muted">{enabled ? t('settings.browser_on') : t('settings.browser_off')}</span>
        <Switch label={t('settings.browser_use')} checked={enabled} onCheckedChange={setEnabled} />
      </Card>
      {!connected && <Setup folder={extensionFolder} />}
    </section>
  )
}
