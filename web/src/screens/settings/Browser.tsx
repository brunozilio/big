import { useEffect, useState } from 'react'
import { request } from '@/api/http'
import { Button } from '@/ui/button'
import { Card } from '@/ui/display'
import { Icon } from '@/ui/icon'

export interface ConnectedBrowser {
  deviceId: string
  name: string
  platform: string
  version: string
}

export interface BrowserStatus {
  server: string
  browsers: ConnectedBrowser[]
  extensionFolder: string
  tools: string[]
}

const PLATFORMS: Record<string, string> = { mac: 'macOS', win: 'Windows', linux: 'Linux', cros: 'ChromeOS' }

export function platformName(platform: string) {
  return PLATFORMS[platform] ?? platform
}

function useBrowserStatus() {
  const [status, setStatus] = useState<BrowserStatus | null>(null)
  useEffect(() => {
    let alive = true
    const load = () =>
      request<BrowserStatus>('GET', '/api/browser')
        .then((next) => alive && setStatus(next))
        .catch(() => alive && setStatus(null))
    void load()
    const timer = setInterval(load, 3000)
    return () => {
      alive = false
      clearInterval(timer)
    }
  }, [])
  return status
}

function Steps({ folder }: { folder: string }) {
  const [copied, setCopied] = useState(false)
  const copy = () => {
    void navigator.clipboard?.writeText(folder).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    })
  }
  return (
    <ol className="list-decimal space-y-1.5 pl-5 text-sm text-muted">
      <li>No Chrome, abra <code className="text-fg">chrome://extensions</code> e ative o Modo do desenvolvedor.</li>
      <li>
        Clique em Carregar sem compactação e escolha a pasta:
        <span className="mt-1 flex items-center gap-2">
          <code className="min-w-0 flex-1 truncate rounded bg-secondary px-2 py-1 text-fg" title={folder}>{folder}</code>
          <Button size="sm" variant="secondary" icon={<Icon name={copied ? 'check' : 'copy'} size={14} />} onClick={copy}>{copied ? 'Copiado' : 'Copiar'}</Button>
        </span>
      </li>
      <li>A extensão se conecta sozinha a este app enquanto ele estiver aberto.</li>
    </ol>
  )
}

export function BrowserSection() {
  const status = useBrowserStatus()
  if (!status) return null
  const connected = status.browsers.length > 0
  return (
    <section data-group="Navegador">
      <h2 className="mb-2.5 text-base font-medium text-muted">Navegador</h2>
      <Card className="space-y-3 px-4 py-3.5">
        <div className="flex items-center gap-3">
          <span className={connected ? 'flex size-8 items-center justify-center rounded-full bg-success-soft text-success-fg' : 'flex size-8 items-center justify-center rounded-full bg-secondary text-muted'}>
            <Icon name="globe" size={16} />
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-base font-medium text-fg">Chrome</p>
            <p className="text-sm text-muted">
              {connected
                ? `${status.browsers.length === 1 ? '1 navegador conectado' : `${status.browsers.length} navegadores conectados`} · ${status.tools.length} ferramentas para os agentes`
                : 'Nenhum navegador conectado. Os agentes usam o seu Chrome, com os seus logins, pela extensão.'}
            </p>
          </div>
        </div>
        {connected ? (
          <ul className="space-y-1.5">
            {status.browsers.map((b) => (
              <li key={b.deviceId} className="flex items-center gap-2 text-sm">
                <Icon name="check" size={13} className="text-success-fg" />
                <span className="font-medium text-fg">{b.name}</span>
                <span className="text-muted">{platformName(b.platform)}</span>
                <span className="ml-auto truncate font-mono text-xs text-muted" title={b.deviceId}>{b.deviceId.slice(0, 8)}</span>
              </li>
            ))}
          </ul>
        ) : (
          <Steps folder={status.extensionFolder} />
        )}
      </Card>
    </section>
  )
}
