import { currentLanguage, translate, type MessageKey, type T } from '@/i18n'

export type NoticeKind = 'teammate' | 'observer' | 'monitor' | 'session' | 'channel' | 'event'

export type Notice = {
  kind: NoticeKind
  from: string
  summary: string | null
  text: string
  protocol: string | null
}

const PREAMBLE = /^Events arrived while you worked[^\n]*\n/
const TAG = /<(teammate-message|event|channel)((?:\s+[\w-]+="[^"]*")*)\s*>\n?([\s\S]*?)\n?<\/\1>/g
const ATTRIBUTE = /([\w-]+)="([^"]*)"/g
const PREFIXED: NoticeKind[] = ['observer', 'monitor', 'session']

const PROTOCOL: Record<string, MessageKey> = {
  idle_notification: 'plan.protocol_idle_notification',
  shutdown_request: 'plan.protocol_shutdown_request',
  shutdown_approved: 'plan.protocol_shutdown_approved',
  shutdown_rejected: 'plan.protocol_shutdown_rejected',
  task_assignment: 'plan.protocol_task_assignment',
  permission_request: 'plan.protocol_permission_request',
  permission_response: 'plan.protocol_permission_response',
  plan_approval_request: 'plan.protocol_plan_approval_request',
  plan_approval_response: 'plan.protocol_plan_approval_response',
}

function unescape(text: string): string {
  return text.replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&')
}

function attributes(raw: string): Record<string, string> {
  const found: Record<string, string> = {}
  for (const [, key, value] of raw.matchAll(ATTRIBUTE)) found[key] = unescape(value)
  return found
}

function protocolOf(text: string): string | null {
  if (!text.trimStart().startsWith('{')) return null
  try {
    const value: unknown = JSON.parse(text)
    if (value && typeof value === 'object' && 'type' in value && typeof value.type === 'string') return value.type
  } catch {
    return null
  }
  return null
}

function sourced(source: string): { kind: NoticeKind; from: string } {
  const [head, ...rest] = source.split(' ')
  const kind = PREFIXED.find((k) => k === head)
  if (kind && rest.length) return { kind, from: rest.join(' ') }
  if (head === 'teammate' && rest.length) return { kind: 'teammate', from: rest.join(' ') }
  return { kind: 'event', from: source }
}

function noticeOf(tag: string, raw: string, body: string): Notice {
  const attrs = attributes(raw)
  const text = unescape(body).trim()
  const protocol = protocolOf(text)
  const fallback = (key: MessageKey) => translate(currentLanguage(), key)
  if (tag === 'teammate-message') return { kind: 'teammate', from: attrs.teammate_id ?? fallback('plan.fallback_teammate'), summary: attrs.summary ?? null, text, protocol }
  if (tag === 'channel') return { kind: 'channel', from: attrs.source ?? fallback('plan.fallback_channel'), summary: attrs.user ?? null, text, protocol }
  return { ...sourced(attrs.source ?? ''), summary: null, text, protocol }
}

export function parseNotices(message: string): Notice[] | null {
  const body = message.replace(PREAMBLE, '')
  const found: Notice[] = []
  let rest = body
  for (const [whole, tag, raw, inner] of body.matchAll(TAG)) {
    found.push(noticeOf(tag, raw, inner))
    rest = rest.replace(whole, '')
  }
  return found.length > 0 && rest.trim() === '' ? found : null
}

export function protocolLabel(protocol: string, t: T): string {
  const key = PROTOCOL[protocol]
  return key ? t(key) : protocol.replace(/_/g, ' ')
}

const STATUS = /^\[(teammate|observer|monitor|session|channel) ([^\]]+)\] ([\s\S]+)$/

export function statusNotice(line: string): Notice | null {
  const found = STATUS.exec(line.trim())
  if (!found) return null
  const kind = found[1] as NoticeKind
  const text = found[3].trim()
  return { kind, from: found[2], summary: null, text, protocol: protocolOf(text) }
}
