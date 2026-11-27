import { useEffect, useRef } from 'react'
import { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { javascript } from '@codemirror/lang-javascript'
import { markdown } from '@codemirror/lang-markdown'
import { bracketMatching, HighlightStyle, indentOnInput, syntaxHighlighting } from '@codemirror/language'
import { highlightSelectionMatches, search, searchKeymap } from '@codemirror/search'
import { Compartment, EditorState, type Extension, Prec } from '@codemirror/state'
import { drawSelection, EditorView, highlightActiveLine, highlightActiveLineGutter, keymap, lineNumbers } from '@codemirror/view'
import { tags } from '@lezer/highlight'
import { cn } from './cn'

export type CodeLanguage = 'markdown' | 'typescript' | 'javascript' | 'json'

const color = (token: string) => `rgb(var(--${token}))`

const look = EditorView.theme({
  '&': { height: '100%', color: color('text'), backgroundColor: 'transparent', fontSize: '13px' },
  '&.cm-focused': { outline: 'none' },
  '.cm-scroller': { fontFamily: '"Geist Mono", "JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace', lineHeight: '1.65', overflow: 'auto' },
  '.cm-content': { padding: '20px 0 96px', caretColor: color('text') },
  '.cm-line': { padding: '0 24px' },
  '.cm-gutters': { backgroundColor: 'transparent', border: 'none', color: color('text-faint'), paddingLeft: '8px' },
  '.cm-lineNumbers .cm-gutterElement': { padding: '0 4px 0 8px', minWidth: '32px' },
  '.cm-activeLine': { backgroundColor: 'rgb(var(--inset) / 0.6)' },
  '.cm-activeLineGutter': { backgroundColor: 'transparent', color: color('text') },
  '.cm-cursor': { borderLeftColor: color('text'), borderLeftWidth: '1.5px' },
  '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground': { backgroundColor: 'rgb(var(--info) / 0.18)' },
  '.cm-selectionMatch': { backgroundColor: 'rgb(var(--info) / 0.12)' },
  '.cm-matchingBracket': { backgroundColor: 'rgb(var(--info) / 0.16)', outline: 'none' },
  '.cm-panels': { backgroundColor: color('bg-subtle'), color: color('text'), borderColor: color('border') },
  '.cm-panels.cm-panels-top': { borderBottom: `1px solid ${color('border')}` },
  '.cm-search': { padding: '8px 12px', fontFamily: 'Inter, ui-sans-serif, system-ui, sans-serif', fontSize: '13px' },
  '.cm-textfield': { backgroundColor: color('surface'), border: `1px solid ${color('border-strong')}`, borderRadius: '6px', padding: '3px 8px', fontSize: '13px' },
  '.cm-button': { backgroundImage: 'none', backgroundColor: color('inset'), border: 'none', borderRadius: '6px', padding: '3px 10px', color: color('text') },
  '.cm-searchMatch': { backgroundColor: 'rgb(var(--warning) / 0.25)' },
  '.cm-searchMatch-selected': { backgroundColor: 'rgb(var(--warning) / 0.5)' },
})

const prose = EditorView.theme({
  '.cm-scroller': { fontFamily: 'Inter, ui-sans-serif, system-ui, -apple-system, sans-serif', fontSize: '15px', lineHeight: '1.7' },
  '.cm-content': { maxWidth: '780px', margin: '0 auto', padding: '32px 0 120px' },
  '.cm-line': { padding: '0 32px' },
})

const MONO = '"Geist Mono", "JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace'

const palette = HighlightStyle.define([
  { tag: tags.heading1, fontWeight: '600', fontSize: '1.35em', color: color('text') },
  { tag: tags.heading2, fontWeight: '600', fontSize: '1.15em', color: color('text') },
  { tag: [tags.heading3, tags.heading4, tags.heading], fontWeight: '600', color: color('text') },
  { tag: tags.strong, fontWeight: '600' },
  { tag: tags.emphasis, fontStyle: 'italic' },
  { tag: tags.strikethrough, textDecoration: 'line-through', color: color('text-muted') },
  { tag: [tags.link, tags.url], color: color('info-fg') },
  { tag: tags.monospace, color: color('teal-fg'), fontFamily: MONO, fontSize: '0.9em' },
  { tag: [tags.processingInstruction, tags.meta, tags.contentSeparator], color: color('text-faint') },
  { tag: tags.quote, color: color('text-muted'), fontStyle: 'italic' },
  { tag: [tags.keyword, tags.modifier, tags.operatorKeyword], color: color('cat-2') },
  { tag: [tags.string, tags.special(tags.string), tags.regexp], color: color('teal-fg') },
  { tag: [tags.number, tags.bool, tags.null, tags.atom], color: color('warning-fg') },
  { tag: [tags.comment, tags.lineComment, tags.blockComment], color: color('text-faint'), fontStyle: 'italic' },
  { tag: [tags.function(tags.variableName), tags.function(tags.propertyName)], color: color('cat-1') },
  { tag: [tags.typeName, tags.className, tags.namespace], color: color('accent-fg') },
  { tag: [tags.propertyName, tags.definition(tags.propertyName)], color: color('text-2') },
  { tag: [tags.punctuation, tags.bracket, tags.operator], color: color('text-muted') },
])

function grammar(language: CodeLanguage): Extension {
  if (language === 'markdown') return [markdown(), EditorView.lineWrapping, Prec.highest(prose)]
  return [javascript({ typescript: language === 'typescript' }), lineNumbers(), highlightActiveLine(), highlightActiveLineGutter()]
}

interface Props {
  value: string
  language: CodeLanguage
  label: string
  onChange: (value: string) => void
  onSave?: () => void
  readOnly?: boolean
  jump?: { line: number; at: number } | null
  className?: string
}

export function CodeEditor({ value, language, label, onChange, onSave, readOnly = false, jump = null, className }: Props) {
  const host = useRef<HTMLDivElement>(null)
  const view = useRef<EditorView | null>(null)
  const handlers = useRef({ onChange, onSave })
  handlers.current = { onChange, onSave }
  const parts = useRef({ grammar: new Compartment(), locked: new Compartment() })
  const first = useRef({ value, language, readOnly, label })

  useEffect(() => {
    if (!host.current) return
    const start = first.current
    const made = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: start.value,
        extensions: [
          history(),
          drawSelection(),
          indentOnInput(),
          bracketMatching(),
          closeBrackets(),
          highlightSelectionMatches(),
          search({ top: true }),
          syntaxHighlighting(palette),
          look,
          EditorState.tabSize.of(2),
          EditorView.contentAttributes.of({ 'aria-label': start.label }),
          parts.current.grammar.of(grammar(start.language)),
          parts.current.locked.of([EditorState.readOnly.of(start.readOnly), EditorView.editable.of(!start.readOnly)]),
          keymap.of([
            { key: 'Mod-s', preventDefault: true, run: () => (handlers.current.onSave?.(), true) },
            ...closeBracketsKeymap,
            ...defaultKeymap,
            ...historyKeymap,
            ...searchKeymap,
            indentWithTab,
          ]),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) handlers.current.onChange(update.state.doc.toString())
          }),
        ],
      }),
    })
    view.current = made
    return () => {
      made.destroy()
      view.current = null
    }
  }, [])

  useEffect(() => {
    const current = view.current
    if (!current || current.state.doc.toString() === value) return
    current.dispatch({ changes: { from: 0, to: current.state.doc.length, insert: value } })
  }, [value])

  useEffect(() => {
    view.current?.dispatch({ effects: parts.current.grammar.reconfigure(grammar(language)) })
  }, [language])

  useEffect(() => {
    view.current?.dispatch({ effects: parts.current.locked.reconfigure([EditorState.readOnly.of(readOnly), EditorView.editable.of(!readOnly)]) })
  }, [readOnly])

  useEffect(() => {
    const current = view.current
    if (!current || !jump || jump.line < 1 || jump.line > current.state.doc.lines) return
    const line = current.state.doc.line(jump.line)
    current.dispatch({ selection: { anchor: line.from + (line.text.length - line.text.trimStart().length) }, effects: EditorView.scrollIntoView(line.from, { y: 'center' }) })
    current.focus()
  }, [jump])

  return <div ref={host} data-code-editor={language} className={cn('h-full min-h-0 overflow-hidden', className)} />
}
