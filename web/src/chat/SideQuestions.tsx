import { useEffect, useRef, useState } from 'react'
import type { SideAnswer } from '@/api/contract'
import { errorText } from '@/api/store'
import { ChatMarkdown } from '@/ui/chatMarkdown'
import { Spinner } from '@/ui/display'
import { Icon } from '@/ui/icon'
import { motion, tween } from '@/ui/motion'

export interface SideQuestion {
  id: number
  question: string
  answer: string | null
  error: string | null
}

export function useSideQuestions(scope: string, send: (question: string) => Promise<SideAnswer>) {
  const [asked, setAsked] = useState<SideQuestion[]>([])
  const next = useRef(0)
  const current = useRef(scope)
  useEffect(() => {
    current.current = scope
    setAsked([])
  }, [scope])
  const settle = (id: number, patch: Partial<SideQuestion>) => setAsked((list) => list.map((q) => (q.id === id ? { ...q, ...patch } : q)))
  const ask = (question: string) => {
    const id = ++next.current
    const asking = scope
    setAsked((list) => [...list, { id, question, answer: null, error: null }])
    send(question)
      .then((r) => current.current === asking && settle(id, { answer: r.answer }))
      .catch((e: unknown) => current.current === asking && settle(id, { error: errorText(e) }))
  }
  const dismiss = (id: number) => setAsked((list) => list.filter((q) => q.id !== id))
  return { asked, ask, dismiss }
}

export function SideAnswers({ asked, onDismiss }: { asked: SideQuestion[]; onDismiss: (id: number) => void }) {
  return (
    <>
      {asked.map((q) => (
        <motion.section key={q.id} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={tween} data-testid="side-question" className="mb-10 rounded-card border border-dashed border-line-strong bg-surface px-4 py-3">
          <header className="mb-2 flex items-center gap-1.5 text-sm text-muted">
            <Icon name="help" size={14} />
            <span className="font-medium">Pergunta à parte</span>
            <span className="text-faint">· não entra na conversa</span>
            <button aria-label="Fechar pergunta à parte" onClick={() => onDismiss(q.id)} className="ml-auto flex rounded-full p-0.5 text-faint transition-colors hover:text-fg">
              <Icon name="close" size={14} />
            </button>
          </header>
          <p className="mb-2 whitespace-pre-wrap text-md text-fg2">{q.question}</p>
          {q.error ? (
            <p role="alert" className="text-sm text-danger-fg">{q.error}</p>
          ) : q.answer === null ? (
            <p className="flex items-center gap-2 text-sm text-muted"><Spinner size={13} /> Respondendo…</p>
          ) : (
            <div className="prose-chat"><ChatMarkdown text={q.answer} /></div>
          )}
        </motion.section>
      ))}
    </>
  )
}
