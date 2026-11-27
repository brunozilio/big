import { BOARD_GROUPS, type BoardGroup } from '@/api/contract'

export type Kind = 'all' | 'work' | 'btw'

export interface ViewState {
  kind: Kind
  archived: boolean
  query: string
  application: string
  closed: BoardGroup[]
}

export const VIEW_PARAMS = { kind: 'kind', archived: 'archived', query: 'q', application: 'app', closed: 'closed' } as const

const KINDS: Kind[] = ['all', 'work', 'btw']

export function readView(params: URLSearchParams): ViewState {
  const kind = KINDS.find((k) => k === params.get(VIEW_PARAMS.kind)) ?? 'all'
  const named = (params.get(VIEW_PARAMS.closed) ?? '').split(',')
  return {
    kind,
    archived: params.get(VIEW_PARAMS.archived) === '1',
    query: params.get(VIEW_PARAMS.query) ?? '',
    application: params.get(VIEW_PARAMS.application) ?? '',
    closed: BOARD_GROUPS.filter((g) => named.includes(g)),
  }
}

export function writeView(params: URLSearchParams, patch: Partial<ViewState>): URLSearchParams {
  const view = { ...readView(params), ...patch }
  const next = new URLSearchParams(params)
  const put = (name: string, value: string) => {
    if (value) next.set(name, value)
    else next.delete(name)
  }
  put(VIEW_PARAMS.kind, view.kind === 'all' ? '' : view.kind)
  put(VIEW_PARAMS.archived, view.archived ? '1' : '')
  put(VIEW_PARAMS.query, view.query)
  put(VIEW_PARAMS.application, view.application)
  put(VIEW_PARAMS.closed, BOARD_GROUPS.filter((g) => view.closed.includes(g)).join(','))
  return next
}

export function toggleClosed(closed: BoardGroup[], group: BoardGroup): BoardGroup[] {
  return closed.includes(group) ? closed.filter((g) => g !== group) : [...closed, group]
}
