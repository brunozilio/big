import type { Block } from '@/api/contract'
import { useStore } from '@/api/store'
import { useT } from '@/i18n'
import { titleOf } from '@/i18n/server'
import { IconButton } from '@/ui/button'
import { Icon } from '@/ui/icon'
import { IssueKey } from '@/screens/items/parts'
import { SendError, useSend } from './shared'

type Of<T extends Block['type']> = Extract<Block, { type: T }>

export function MembersBlock({ block }: { block: Of<'members'> }) {
  const t = useT()
  const items = useStore((s) => s.items)
  const { busy, error, send } = useSend()
  return (
    <div>
      <ul className="divide-y divide-line">
        {block.items.map((member) => {
          const known = items.find((it) => it.id === member.id)
          return (
            <li key={member.id} className="group flex min-h-[44px] items-center gap-3">
              {known?.issue ? <IssueKey issue={known.issue} /> : member.issue && <span className="min-w-[88px] text-base text-fg2">{member.issue}</span>}
              <span className="truncate text-base text-fg">{known ? titleOf(t, known) : member.title}</span>
              {block.removable && (
                <IconButton tooltip={t('blocks.unlink')} size="sm" className="ml-auto opacity-50 hover:opacity-100 focus-visible:opacity-100 group-hover:opacity-100" disabled={busy} onClick={() => void send('unlink', { item: member.id })}>
                  <Icon name="minus" size={14} />
                </IconButton>
              )}
            </li>
          )
        })}
      </ul>
      <SendError error={error} />
    </div>
  )
}
