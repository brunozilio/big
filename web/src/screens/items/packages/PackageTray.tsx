import { motion } from 'framer-motion'
import { useState } from 'react'
import type { Item } from '@/api/contract'
import { useT } from '@/i18n'
import { titleOf } from '@/i18n/server'
import { Button, IconButton } from '@/ui/button'
import { cn } from '@/ui/cn'
import { Icon } from '@/ui/icon'
import { tween } from '@/ui/motion'
import { IssueKey } from '../parts'
import { DRAG_TYPE, FROM_TYPE } from './GroupRow'

export const TRAY = 'tray'

function Entry({ it, onReturn }: { it: Item; onReturn: () => void }) {
  const t = useT()
  return (
    <li
      draggable
      onDragStart={(e) => { e.dataTransfer.setData(DRAG_TYPE, it.id); e.dataTransfer.setData(FROM_TYPE, TRAY) }}
      className="group flex min-h-[44px] cursor-grab items-center gap-3 rounded-control px-2.5 transition-colors hover:bg-hover active:cursor-grabbing"
    >
      <Icon name="drag" size={14} className="text-faint" />
      {it.issue && <IssueKey issue={it.issue} />}
      <span className="truncate text-base text-fg">{titleOf(t, it)}</span>
      <IconButton tooltip={t('items.returnToDone')} size="sm" className="ml-auto opacity-0 group-hover:opacity-100 focus-visible:opacity-100" onClick={onReturn}><Icon name="close" size={14} /></IconButton>
    </li>
  )
}

type TrayProps = { chosen: Item[]; busy: boolean; error: string | null; onAdd: (id: string) => void; onReturn: (id: string) => void; onCancel: () => void; onCreate: () => void }

export function PackageTray({ chosen, busy, error, onAdd, onReturn, onCancel, onCreate }: TrayProps) {
  const t = useT()
  const [over, setOver] = useState(false)
  return (
    <motion.div initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: 'auto' }} exit={{ opacity: 0, height: 0 }} transition={tween} className="overflow-hidden">
      <div
        onDragOver={(e) => { if (e.dataTransfer.types.includes(DRAG_TYPE)) { e.preventDefault(); e.stopPropagation(); setOver(true) } }}
        onDragLeave={() => setOver(false)}
        onDrop={(e) => { e.stopPropagation(); setOver(false); const id = e.dataTransfer.getData(DRAG_TYPE); if (id) onAdd(id) }}
        className={cn('mb-3 rounded-card border border-dashed p-2 transition-colors', over ? 'border-accent bg-accent-soft/50' : 'border-line-strong bg-subtle')}
      >
        {chosen.length === 0 ? (
          <div className="flex min-h-[72px] items-center justify-center gap-2 text-sm text-muted">
            <Icon name="up" size={15} />
            {t('items.packageHint')}
          </div>
        ) : (
          <ul>{chosen.map((it) => <Entry key={it.id} it={it} onReturn={() => onReturn(it.id)} />)}</ul>
        )}
        <div className="flex items-center justify-end gap-2 px-1 pt-2">
          {error && <span className="mr-auto truncate text-sm text-danger-fg">{error}</span>}
          <Button size="sm" variant="ghost" onClick={onCancel} disabled={busy}>{t('items.cancelPackage')}</Button>
          <Button size="sm" variant="primary" disabled={busy || chosen.length === 0} onClick={onCreate}>{t('items.createPackage')}</Button>
        </div>
      </div>
    </motion.div>
  )
}
