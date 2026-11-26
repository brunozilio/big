import { useEffect, useState } from 'react'
import { useStore } from '@/api/store'
import { useT } from '@/i18n'
import { Spinner } from '@/ui/display'

export const RECONNECT_DELAY_MS = 2000

export function useHeld(value: boolean, ms: number): boolean {
  const [held, setHeld] = useState(false)
  useEffect(() => {
    if (!value) {
      setHeld(false)
      return
    }
    const id = setTimeout(() => setHeld(true), ms)
    return () => clearTimeout(id)
  }, [value, ms])
  return value && held
}

export function Reconnecting({ down, delay = RECONNECT_DELAY_MS }: { down: boolean; delay?: number }) {
  const t = useT()
  const shown = useHeld(down, delay)
  return (
    <span role="status" className="flex items-center">
      {shown && (
        <span className="flex items-center gap-1.5 whitespace-nowrap text-sm text-muted animate-in fade-in-0 duration-250">
          <Spinner size={13} />
          <span className="max-sm:sr-only">{t('app.reconnecting')}</span>
        </span>
      )}
    </span>
  )
}

export function ConnectionIndicator() {
  const ready = useStore((s) => s.ready)
  const connected = useStore((s) => s.connected)
  return <Reconnecting down={ready && !connected} />
}
