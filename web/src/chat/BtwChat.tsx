import { useState } from 'react'
import { api } from '@/api'
import { errorText } from '@/api/store'
import { Spinner } from '@/ui/display'
import { Icon } from '@/ui/icon'
import { motion, tween } from '@/ui/motion'
import { ChatView } from './ChatView'
import { useT } from '@/i18n'

export type BtwState = { kind: 'opening'; question: string } | { kind: 'open'; session: string } | { kind: 'failed'; question: string; error: string }

export function useBtw(application: string | null, model: string | null) {
  const [state, setState] = useState<BtwState | null>(null)
  const ask = (question: string) => {
    setState({ kind: 'opening', question })
    api()
      .btw({ text: question, application, model })
      .then((r) => setState({ kind: 'open', session: r.session }))
      .catch((e: unknown) => setState({ kind: 'failed', question, error: errorText(e) }))
  }
  return { state, ask, close: () => setState(null) }
}

export function BtwChat({ state, onClose }: { state: BtwState; onClose: () => void }) {
  const t = useT()
  return (
    <motion.section initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={tween} data-testid="btw-chat" className="overflow-hidden rounded-card border border-dashed border-line-strong bg-surface">
      <header className="flex items-center gap-1.5 px-4 pb-1 pt-3 text-sm text-muted">
        <Icon name="help" size={14} />
        <span className="font-medium">{t('chat.aside')}</span>
        <button aria-label={t('chat.closeAside')} onClick={onClose} className="ml-auto flex rounded-full p-0.5 text-faint transition-colors hover:text-fg">
          <Icon name="close" size={14} />
        </button>
      </header>
      {state.kind === 'open' ? (
        <div className="h-[520px]"><ChatView sessionId={state.session} composer /></div>
      ) : (
        <div className="space-y-2 px-4 pb-4">
          <p className="whitespace-pre-wrap text-md text-fg2">{state.question}</p>
          {state.kind === 'opening' ? (
            <p className="flex items-center gap-2 text-sm text-muted"><Spinner size={13} /> {t('chat.opening')}</p>
          ) : (
            <p role="alert" className="text-sm text-danger-fg">{state.error}</p>
          )}
        </div>
      )}
    </motion.section>
  )
}
