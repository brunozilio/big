import { useState, type DragEvent } from 'react'
import { api } from '@/api'
import type { Item } from '@/api/contract'
import { errorText, getState, setItems } from '@/api/store'
import { DRAG_TYPE } from './GroupRow'
import { TRAY } from './PackageTray'

export const PACKAGE_WORKFLOW = 'package'

export interface PackageBoard {
  hidden: Set<string>
  draft: string[] | null
  members: (it: Item) => Item[]
  draggable: (it: Item) => { draggable?: boolean; onDragStart?: (e: DragEvent) => void }
  move: (target: Item, id: string, from: string | null) => void
  open: () => void
  close: () => void
  add: (id: string) => void
  remove: (id: string) => void
  build: (title: string, model: string) => Promise<string | null>
}

const grouping = (it: Item) => it.workflow === PACKAGE_WORKFLOW || (it.links?.length ?? 0) > 0

function send(item: Item, action: string, payload: unknown) {
  return api().input(item.id, item.current ?? '', { action, payload })
}

function ready(id: string, left = 200): Promise<Item> {
  const found = getState().items.find((it) => it.id === id)
  if (found?.joinable && found.current) return Promise.resolve(found)
  if (left <= 0) return Promise.reject(new Error('timeout'))
  return new Promise((resolve) => setTimeout(resolve, 300)).then(() => ready(id, left - 1))
}

async function create(title: string, model: string, ids: string[]): Promise<void> {
  const it = await api().createItem({ workflow: PACKAGE_WORKFLOW, text: title, issue: null, model, effort: 'medium', fast: false, attachments: [] })
  setItems((all) => (all.some((x) => x.id === it.id) ? all : [it, ...all]))
  const live = await ready(it.id)
  await send(live, 'link', { items: ids })
}

export function usePackageBoard(items: Item[]): PackageBoard {
  const [draft, setDraft] = useState<string[] | null>(null)
  const groups = items.filter((it) => grouping(it) && !it.archivedAt)
  const linked = new Set(groups.flatMap((it) => it.links ?? []))
  const hidden = new Set([...linked, ...(draft ?? [])])
  const accepting = !!draft || groups.some((it) => it.joinable)
  const loose = (it: Item) => !grouping(it) && !linked.has(it.id) && it.group === 'delivered' && !it.archivedAt
  return {
    hidden,
    draft,
    members: (it) => (it.links ?? []).map((id) => items.find((one) => one.id === id)).filter((one): one is Item => !!one),
    draggable: (it) => (accepting && loose(it) && !hidden.has(it.id) ? { draggable: true, onDragStart: (e) => e.dataTransfer.setData(DRAG_TYPE, it.id) } : {}),
    move: (target, id, from) => {
      if (from === TRAY) setDraft((d) => d && d.filter((x) => x !== id))
      const source = from && from !== TRAY ? items.find((it) => it.id === from) : undefined
      const leave = source && source.id !== target.id ? send(source, 'unlink', { item: id }) : Promise.resolve()
      void leave.then(() => send(target, 'link', { item: id })).catch(() => undefined)
    },
    open: () => setDraft((d) => d ?? []),
    close: () => setDraft(null),
    add: (id) => {
      const it = items.find((one) => one.id === id)
      if (it && loose(it)) setDraft((d) => (d && !d.includes(id) ? [...d, id] : d))
    },
    remove: (id) => setDraft((d) => d && d.filter((x) => x !== id)),
    build: (title, model) => {
      const ids = draft ?? []
      return create(title, model, ids)
        .then(() => {
          setDraft(null)
          return null
        })
        .catch((e: unknown) => errorText(e))
    },
  }
}

export { grouping }
