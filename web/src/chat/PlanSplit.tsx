import { useEffect, useState } from 'react'
import { api } from '@/api'
import type { SessionEvent } from '@/api/contract'
import { useSession } from '@/api/store'
import { SegmentedControl } from '@/ui/form'
import { useT } from '@/i18n'
import type { Respond } from './actionBlocks'
import { ChatView } from './ChatView'
import { AgentsContext } from './events'
import { PlanBlock } from './PlanCard'

type Tab = 'plan' | 'chat'

type PlanEvent = Extract<SessionEvent, { type: 'plan' }>

const isPlan = (e: SessionEvent): e is PlanEvent => e.type === 'plan'

export function PlanSplit({ sessionId, composer }: { sessionId: string; composer: boolean }) {
  const t = useT()
  const { session } = useSession(sessionId)
  const plans = session?.events.filter(isPlan) ?? []
  const latest = plans[plans.length - 1]
  const [tab, setTab] = useState<Tab>('plan')
  useEffect(() => {
    if (latest?.resolution === 'pending') setTab('plan')
  }, [latest?.id, latest?.resolution])
  if (!session || !latest) return <ChatView sessionId={sessionId} composer={composer} />
  const respond: Respond = (event, response) => api().respond(session.id, event, response)
  const options = [
    { value: 'plan' as const, label: t('plan.tab_plan') },
    { value: 'chat' as const, label: t('plan.tab_chat') },
  ]
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-center border-b bg-surface px-4 py-3">
        <SegmentedControl<Tab> size="sm" value={tab} onChange={setTab} options={options} />
      </div>
      {tab === 'plan' ? (
        <AgentsContext.Provider value={session.agents}>
          <div className="scrollbar-thin min-h-0 flex-1 overflow-y-auto">
            <div className="mx-auto w-full max-w-[760px] px-5 py-6 sm:px-8">
              <PlanBlock key={latest.id} event={latest} respond={respond} />
            </div>
          </div>
        </AgentsContext.Provider>
      ) : (
        <div className="min-h-0 flex-1"><ChatView sessionId={sessionId} composer={composer} hidePlans /></div>
      )}
    </div>
  )
}
