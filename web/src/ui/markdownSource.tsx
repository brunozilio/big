import { useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from 'react'
import { useT, type T } from '@/i18n'
import { cn } from './cn'
import { SegmentedControl } from './form'
import { Icon, type IconName } from './icon'
import {
  continueList,
  cycleHeading,
  highlightMarkdown,
  indentLines,
  insertBlock,
  insertLink,
  insertTable,
  syncedScrollTop,
  toggleLinePrefix,
  toggleWrap,
  type TextEdit,
  type TokenKind,
} from './markdownOps'
import { MarkdownPreview } from './markdownProse'
import { Tool, ToolSep, Toolbar, useNarrow } from './markdownToolbar'

export type SourceView = 'write' | 'split' | 'preview'

type Op = (text: string, start: number, end: number) => TextEdit | null

interface SourceTool {
  key: string
  label: string
  icon: IconName
  shortcut?: string
  run: Op
}

function codeBlock(text: string, start: number, end: number): TextEdit {
  const body = text.slice(start, end)
  const r = insertBlock(text.slice(0, start) + text.slice(end), start, `\`\`\`\n${body}\n\`\`\``)
  const caret = r.text.indexOf('```\n', start) + 4
  return { text: r.text, start: caret, end: caret + body.length }
}

const sourceTools = (tr: T): (SourceTool | 'sep')[] => [
  { key: 'bold', label: tr('editor.bold'), icon: 'bold', shortcut: '⌘B', run: (t, s, e) => toggleWrap(t, s, e, '**', tr('editor.sampleBold')) },
  { key: 'italic', label: tr('editor.italic'), icon: 'italic', shortcut: '⌘I', run: (t, s, e) => toggleWrap(t, s, e, '_', tr('editor.sampleItalic')) },
  { key: 'heading', label: tr('editor.headingCycle'), icon: 'heading', run: cycleHeading },
  { key: 'strike', label: tr('editor.strike'), icon: 'strike', run: (t, s, e) => toggleWrap(t, s, e, '~~', tr('editor.sampleStrike')) },
  'sep',
  { key: 'ul', label: tr('editor.bulletList'), icon: 'list', shortcut: '⌘⇧8', run: (t, s, e) => toggleLinePrefix(t, s, e, 'ul') },
  { key: 'ol', label: tr('editor.orderedList'), icon: 'listOrdered', shortcut: '⌘⇧7', run: (t, s, e) => toggleLinePrefix(t, s, e, 'ol') },
  { key: 'task', label: tr('editor.checklist'), icon: 'checklist', run: (t, s, e) => toggleLinePrefix(t, s, e, 'task') },
  'sep',
  { key: 'quote', label: tr('editor.quote'), icon: 'quote', run: (t, s, e) => toggleLinePrefix(t, s, e, 'quote') },
  { key: 'code', label: tr('editor.code'), icon: 'code', shortcut: '⌘E', run: (t, s, e) => toggleWrap(t, s, e, '`', tr('editor.sampleCode')) },
  { key: 'codeBlock', label: tr('editor.codeBlock'), icon: 'codeBlock', run: codeBlock },
  { key: 'table', label: tr('editor.table'), icon: 'table', run: (t, s) => insertTable(t, s) },
  { key: 'hr', label: tr('editor.divider'), icon: 'rule', run: (t, s) => insertBlock(t, s, '---') },
  'sep',
  { key: 'link', label: tr('editor.link'), icon: 'link', shortcut: '⌘K', run: (t, s, e) => insertLink(t, s, e) },
  { key: 'image', label: tr('editor.image'), icon: 'image', run: (t, s, e) => insertLink(t, s, e, true) },
]

const byKey = (tools: (SourceTool | 'sep')[], key: string) => tools.find((t): t is SourceTool => t !== 'sep' && t.key === key)

const shortcutKeys: Record<string, string> = { b: 'bold', i: 'italic', k: 'link', e: 'code' }

const tokenClass: Record<TokenKind, string> = {
  plain: '',
  heading: 'md-hl-strong text-fg',
  mark: 'text-faint',
  bold: 'md-hl-strong text-fg',
  italic: 'italic',
  strike: 'text-muted line-through',
  code: 'rounded-sm bg-inset text-fg',
  link: 'text-info-fg',
  quote: 'italic text-muted',
  fence: 'text-faint',
  rule: 'text-faint',
}

export function SourceEditor({
  value,
  onChange,
  placeholder,
  autoFocus,
  label,
  compact,
  minHeight,
  maxHeight,
  fill,
  defaultView = 'split',
  trailing,
  onSubmit,
}: {
  value: string
  onChange: (v: string) => void
  placeholder?: string
  autoFocus?: boolean
  label: string
  compact?: boolean
  minHeight: number
  maxHeight?: number
  fill?: boolean
  defaultView?: SourceView
  trailing?: ReactNode
  onSubmit?: () => void
}) {
  const tr = useT()
  const tools = useMemo(() => sourceTools(tr), [tr])
  const narrow = useNarrow()
  const [view, setView] = useState<SourceView>(defaultView)
  const shown: SourceView = narrow && view === 'split' ? 'write' : view
  const area = useRef<HTMLTextAreaElement>(null)
  const editPane = useRef<HTMLDivElement>(null)
  const previewPane = useRef<HTMLDivElement>(null)
  const pending = useRef<{ start: number; end: number } | null>(null)
  const history = useRef<{ past: TextEdit[]; future: TextEdit[]; typedAt: number }>({ past: [], future: [], typedAt: 0 })
  const scrollLock = useRef<{ from: 'edit' | 'preview'; until: number } | null>(null)
  const lines = useMemo(() => highlightMarkdown(value), [value])

  useLayoutEffect(() => {
    const el = area.current
    if (!el || !pending.current) return
    el.setSelectionRange(pending.current.start, pending.current.end)
    pending.current = null
  }, [value, shown])

  const selection = () => ({ start: area.current?.selectionStart ?? value.length, end: area.current?.selectionEnd ?? value.length })

  const commit = (next: TextEdit, typing = false) => {
    const h = history.current
    const now = Date.now()
    if (!typing || now - h.typedAt > 700) h.past.push({ text: value, ...selection() })
    if (h.past.length > 200) h.past.shift()
    h.future = []
    h.typedAt = typing ? now : 0
    pending.current = { start: next.start, end: next.end }
    onChange(next.text)
  }

  const apply = (op: Op) => {
    const { start, end } = selection()
    const r = op(value, start, end)
    if (!r) return false
    commit(r)
    area.current?.focus()
    return true
  }

  const travel = (back: boolean) => {
    const h = history.current
    const target = back ? h.past.pop() : h.future.pop()
    if (!target) return
    ;(back ? h.future : h.past).push({ text: value, ...selection() })
    h.typedAt = 0
    pending.current = { start: target.start, end: target.end }
    onChange(target.text)
    area.current?.focus()
  }

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.nativeEvent.isComposing) return
    const mod = e.metaKey || e.ctrlKey
    const key = e.key.toLowerCase()
    if (mod && key === 'enter' && onSubmit) {
      e.preventDefault()
      onSubmit()
      return
    }
    if (mod && (key === 'z' || key === 'y')) {
      e.preventDefault()
      travel(key === 'z' && !e.shiftKey)
      return
    }
    if (mod && e.shiftKey && (e.code === 'Digit7' || e.code === 'Digit8')) {
      e.preventDefault()
      apply((t, s, en) => toggleLinePrefix(t, s, en, e.code === 'Digit7' ? 'ol' : 'ul'))
      return
    }
    if (mod && !e.shiftKey && shortcutKeys[key]) {
      const tool = byKey(tools, shortcutKeys[key])
      if (tool) {
        e.preventDefault()
        apply(tool.run)
      }
      return
    }
    if (e.key === 'Tab' && !mod && !e.altKey) {
      if (apply((t, s, en) => indentLines(t, s, en, e.shiftKey))) e.preventDefault()
      return
    }
    if (e.key === 'Enter' && !e.shiftKey && !mod && !e.altKey) {
      if (apply(continueList)) e.preventDefault()
    }
  }

  const onPaneScroll = (from: 'edit' | 'preview') => {
    if (shown !== 'split') return
    const lock = scrollLock.current
    if (lock && lock.from !== from && performance.now() < lock.until) return
    const a = from === 'edit' ? editPane.current : previewPane.current
    const b = from === 'edit' ? previewPane.current : editPane.current
    if (!a || !b) return
    scrollLock.current = { from, until: performance.now() + 120 }
    b.scrollTop = syncedScrollTop({ top: a.scrollTop, height: a.scrollHeight, client: a.clientHeight }, { height: b.scrollHeight, client: b.clientHeight })
  }

  const field = cn('col-start-1 row-start-1 block w-full whitespace-pre-wrap break-words font-sans', compact ? 'px-3 py-2.5 text-base leading-6' : 'px-5 py-4 text-md leading-7')
  const paneStyle = fill ? undefined : { minHeight, maxHeight }
  const pane = cn('overflow-auto', fill && 'min-h-0')

  const views = [
    { value: 'write' as const, label: tr('editor.viewWrite') },
    ...(narrow ? [] : [{ value: 'split' as const, label: tr('editor.viewSplit') }]),
    { value: 'preview' as const, label: tr('editor.viewPreview') },
  ]

  return (
    <div className={cn('flex flex-col', fill && 'min-h-0 flex-1')}>
      <Toolbar
        label={tr('editor.markdownFormatting')}
        end={
          <>
            <SegmentedControl size="sm" value={shown} onChange={setView} options={views} />
            {trailing}
          </>
        }
      >
        {tools.map((t, i) =>
          t === 'sep' ? (
            <ToolSep key={`sep-${i}`} />
          ) : (
            <Tool key={t.key} label={t.label} shortcut={t.shortcut} disabled={shown === 'preview'} onClick={() => apply(t.run)}>
              <Icon name={t.icon} size={17} />
            </Tool>
          ),
        )}
        <ToolSep />
        <Tool label={tr('editor.undo')} shortcut="⌘Z" disabled={shown === 'preview'} onClick={() => travel(true)}>
          <Icon name="undo" size={17} />
        </Tool>
        <Tool label={tr('editor.redo')} shortcut="⌘⇧Z" disabled={shown === 'preview'} onClick={() => travel(false)}>
          <Icon name="redo" size={17} />
        </Tool>
      </Toolbar>
      <div className={cn('grid', shown === 'split' && 'grid-cols-2', fill && 'min-h-0 flex-1')}>
        {shown !== 'preview' && (
          <div ref={editPane} onScroll={() => onPaneScroll('edit')} className={pane} style={paneStyle}>
            <div className="grid min-h-full" style={fill ? undefined : { minHeight }} onClick={() => area.current?.focus()}>
              <div aria-hidden className={cn(field, 'pointer-events-none text-fg2')}>
                {lines.map((line, i) => (
                  <span key={i}>
                    {line.map((tok, j) => (
                      <span key={j} className={tokenClass[tok.kind]}>
                        {tok.text}
                      </span>
                    ))}
                    {i < lines.length - 1 ? '\n' : '\n '}
                  </span>
                ))}
              </div>
              <textarea
                ref={area}
                rows={1}
                autoFocus={autoFocus}
                value={value}
                aria-label={label}
                spellCheck
                placeholder={placeholder}
                onChange={(e) => commit({ text: e.target.value, start: e.target.selectionStart, end: e.target.selectionEnd }, true)}
                onKeyDown={onKeyDown}
                className={cn(field, 'h-full resize-none overflow-hidden bg-transparent text-transparent caret-fg outline-none placeholder:text-muted')}
              />
            </div>
          </div>
        )}
        {shown !== 'write' && (
          <div ref={previewPane} onScroll={() => onPaneScroll('preview')} className={cn(pane, shown === 'split' && 'border-l', compact ? 'px-3 py-2.5' : 'px-5 py-4')} style={paneStyle}>
            <MarkdownPreview text={value} />
          </div>
        )}
      </div>
    </div>
  )
}
