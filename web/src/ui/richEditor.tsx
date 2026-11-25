import * as RPopover from '@radix-ui/react-popover'
import Image from '@tiptap/extension-image'
import Link from '@tiptap/extension-link'
import Placeholder from '@tiptap/extension-placeholder'
import Table from '@tiptap/extension-table'
import TableCell from '@tiptap/extension-table-cell'
import TableHeader from '@tiptap/extension-table-header'
import TableRow from '@tiptap/extension-table-row'
import TaskItem from '@tiptap/extension-task-item'
import TaskList from '@tiptap/extension-task-list'
import { EditorContent, useEditor, type Editor } from '@tiptap/react'
import StarterKit from '@tiptap/starter-kit'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Markdown } from 'tiptap-markdown'
import { Button } from './button'
import { useT, type T } from '@/i18n'
import { cn } from './cn'
import { Input } from './form'
import { Icon } from './icon'
import { improveMarkdown } from './markdownOps'
import { proseClass } from './markdownProse'
import { Tool, ToolButton, ToolSep, Toolbar, tipText } from './markdownToolbar'
import { IconSwap } from './motion'
import { Menu, MenuContent, MenuItem, MenuLabel, MenuSeparator, MenuTrigger, Tooltip, popIn } from './overlay'

export function editorMarkdown(editor: Editor): string {
  const storage = editor.storage as { markdown?: { getMarkdown: () => string } }
  return storage.markdown?.getMarkdown() ?? editor.getText()
}

const emojis = ['👍', '👎', '✅', '❌', '⚠️', '🚀', '🐛', '🔥', '💡', '📌', '📝', '🎯', '⏳', '🔒', '🙏', '👀', '🎉', '🤔', '💬', '📈', '🧪', '🛠️', '📦', '⭐']

function styleName(t: T, level: 1 | 2 | 3 | undefined): string {
  return level === undefined ? t('editor.paragraph') : t('editor.heading', { level })
}

const check = <Icon name="check" size={15} className="text-fg" />

function UrlForm({ initial, label, submit, onDone, withText }: { initial: string; label: string; submit: (url: string, text: string) => void; onDone: () => void; withText?: string }) {
  const t = useT()
  const [url, setUrl] = useState(initial)
  const [text, setText] = useState(withText ?? '')
  return (
    <form
      className="w-72 space-y-2 p-1"
      onSubmit={(e) => {
        e.preventDefault()
        submit(url.trim(), text.trim())
        onDone()
      }}
    >
      <Input autoFocus value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://" aria-label={label} />
      {withText !== undefined && <Input value={text} onChange={(e) => setText(e.target.value)} placeholder={t('editor.textOptional')} aria-label={t('editor.text')} />}
      <div className="flex justify-end gap-1.5">
        <Button type="button" size="sm" variant="ghost" onClick={onDone}>
          {t('editor.cancel')}
        </Button>
        <Button type="submit" size="sm" variant="primary">
          {t('editor.apply')}
        </Button>
      </div>
    </form>
  )
}

function PopoverTool({ label, shortcut, icon, open, onOpenChange, children, disabled }: { label: string; shortcut?: string; icon: ReactNode; open: boolean; onOpenChange: (o: boolean) => void; children: ReactNode; disabled?: boolean }) {
  return (
    <RPopover.Root open={open} onOpenChange={onOpenChange}>
      <Tooltip content={tipText(label, shortcut)}>
        <RPopover.Trigger asChild>
          <ToolButton label={label} disabled={disabled}>
            {icon}
          </ToolButton>
        </RPopover.Trigger>
      </Tooltip>
      <RPopover.Portal>
        <RPopover.Content align="start" sideOffset={8} onCloseAutoFocus={(e) => e.preventDefault()} className={cn('z-50 rounded-card bg-surface p-1.5 shadow-pop outline-none', popIn)}>
          {children}
        </RPopover.Content>
      </RPopover.Portal>
    </RPopover.Root>
  )
}

function MenuTool({ label, icon, children, caretOnly, align = 'start' }: { label: string; icon?: ReactNode; children: ReactNode; caretOnly?: boolean; align?: 'start' | 'end' }) {
  return (
    <Menu modal={false}>
      <Tooltip content={label}>
        <MenuTrigger asChild>
          <ToolButton label={label} caret={!caretOnly || undefined} className={caretOnly ? 'w-5' : undefined}>
            {caretOnly ? <Icon name="down" size={12} className="text-faint" /> : icon}
          </ToolButton>
        </MenuTrigger>
      </Tooltip>
      <MenuContent align={align}>{children}</MenuContent>
    </Menu>
  )
}

export function RichEditor({
  value,
  onChange,
  placeholder,
  autoFocus,
  label,
  compact,
  minHeight,
  maxHeight,
  fill,
  readOnly,
  assist,
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
  readOnly?: boolean
  assist?: string
  trailing?: ReactNode
  onSubmit?: () => void
}) {
  const last = useRef(value)
  const changeRef = useRef(onChange)
  changeRef.current = onChange
  const submitRef = useRef(onSubmit)
  submitRef.current = onSubmit
  const t = useT()
  const [linkOpen, setLinkOpen] = useState(false)
  const [imageOpen, setImageOpen] = useState(false)
  const [emojiOpen, setEmojiOpen] = useState(false)
  const [thinking, setThinking] = useState(false)
  const timer = useRef<number>()

  const editor = useEditor({
    extensions: [
      StarterKit.configure({ heading: { levels: [1, 2, 3] } }),
      Placeholder.configure({ placeholder: placeholder ?? t('editor.startWriting') }),
      Link.configure({ openOnClick: false, autolink: true }),
      Image,
      Table.configure({ resizable: false }),
      TableRow,
      TableHeader,
      TableCell,
      TaskList,
      TaskItem.configure({ nested: true }),
      Markdown.configure({ html: false, tightLists: true, linkify: true, transformPastedText: true, transformCopiedText: true }),
    ],
    content: value,
    editable: !readOnly,
    autofocus: autoFocus ? 'end' : false,
    editorProps: {
      attributes: { 'aria-label': label, 'aria-multiline': 'true', role: 'textbox', class: cn(proseClass, compact ? 'px-3 py-2.5' : 'px-5 py-4') },
      handleKeyDown: (_view, event) => {
        const mod = event.metaKey || event.ctrlKey
        if (mod && event.key.toLowerCase() === 'k') {
          event.preventDefault()
          setLinkOpen(true)
          return true
        }
        if (mod && event.key === 'Enter' && submitRef.current) {
          event.preventDefault()
          submitRef.current()
          return true
        }
        return false
      },
    },
    onUpdate: ({ editor: ed }) => {
      const md = editorMarkdown(ed)
      last.current = md
      changeRef.current(md)
    },
  })

  useEffect(() => {
    if (!editor || value === last.current) return
    last.current = value
    editor.commands.setContent(value, false)
  }, [editor, value])

  useEffect(() => {
    editor?.setEditable(!readOnly)
  }, [editor, readOnly])

  useEffect(() => () => window.clearTimeout(timer.current), [])

  const chain = () => editor?.chain().focus()
  const is = (name: string, attrs?: Record<string, unknown>) => !!editor?.isActive(name, attrs)

  const improve = () => {
    if (!editor || thinking) return
    setThinking(true)
    timer.current = window.setTimeout(() => {
      editor.chain().focus().setContent(improveMarkdown(editorMarkdown(editor)), true).run()
      setThinking(false)
    }, 900)
  }

  const activeLevel = ([1, 2, 3] as const).find((l) => is('heading', { level: l }))
  const styleLabel = styleName(t, activeLevel)

  const area = (
    <div
      className={cn('md-rich overflow-auto', fill && 'min-h-0 flex-1', thinking && 'pointer-events-none opacity-60 transition-opacity')}
      style={fill ? undefined : { minHeight, maxHeight }}
      onClick={(e) => e.target === e.currentTarget && editor?.commands.focus('end')}
    >
      <EditorContent editor={editor} className="h-full" style={fill ? undefined : { minHeight }} />
    </div>
  )

  if (readOnly) return area

  return (
    <div className={cn('flex flex-col', fill && 'min-h-0 flex-1')}>
      <Toolbar label={t('editor.formatting')} end={trailing}>
        {assist && (
          <>
            <Tool label={assist} onClick={improve} disabled={thinking} className="text-accent hover:text-accent">
              <IconSwap k={thinking ? 'busy' : 'idle'}>{thinking ? <Icon name="loading" size={17} className="animate-spin" /> : <Icon name="sparkles" size={17} />}</IconSwap>
            </Tool>
            <ToolSep />
          </>
        )}
        <MenuTool label={t('editor.textStyleOf', { style: styleLabel })} icon={<Icon name="textStyle" size={17} />}>
          <MenuLabel>{t('editor.textStyle')}</MenuLabel>
          <MenuItem onSelect={() => chain()?.setParagraph().run()} right={activeLevel === undefined ? check : undefined}>
            {t('editor.paragraph')}
          </MenuItem>
          {([1, 2, 3] as const).map((level) => (
            <MenuItem key={level} onSelect={() => chain()?.setHeading({ level }).run()} right={is('heading', { level }) ? check : <span className="text-xs text-muted">⌘⌥{level}</span>}>
              <span className={cn('text-fg', level === 1 && 'text-lg font-medium', level === 2 && 'text-md font-medium', level === 3 && 'font-semibold')}>{t('editor.heading', { level })}</span>
            </MenuItem>
          ))}
        </MenuTool>
        <Tool label={t('editor.bold')} shortcut="⌘B" active={is('bold')} aria-pressed={is('bold')} onClick={() => chain()?.toggleBold().run()}>
          <Icon name="bold" size={17} />
        </Tool>
        <MenuTool label={t('editor.moreFormatting')} caretOnly>
          <MenuItem icon={<Icon name="italic" size={16} />} onSelect={() => chain()?.toggleItalic().run()} right={is('italic') ? check : <span className="text-xs text-muted">⌘I</span>}>
            {t('editor.italic')}
          </MenuItem>
          <MenuItem icon={<Icon name="strike" size={16} />} onSelect={() => chain()?.toggleStrike().run()} right={is('strike') ? check : <span className="text-xs text-muted">⌘⇧S</span>}>
            {t('editor.strike')}
          </MenuItem>
          <MenuItem icon={<Icon name="code" size={16} />} onSelect={() => chain()?.toggleCode().run()} right={is('code') ? check : <span className="text-xs text-muted">⌘E</span>}>
            {t('editor.inlineCode')}
          </MenuItem>
          <MenuSeparator />
          <MenuItem icon={<Icon name="close" size={16} />} onSelect={() => chain()?.unsetAllMarks().clearNodes().run()}>
            {t('editor.clearFormatting')}
          </MenuItem>
        </MenuTool>
        <MenuTool label={t('editor.lists')} icon={<Icon name={is('orderedList') ? 'listOrdered' : is('taskList') ? 'checklist' : 'list'} size={17} />}>
          <MenuItem icon={<Icon name="list" size={16} />} onSelect={() => chain()?.toggleBulletList().run()} right={is('bulletList') ? check : <span className="text-xs text-muted">⌘⇧8</span>}>
            {t('editor.bulletList')}
          </MenuItem>
          <MenuItem icon={<Icon name="listOrdered" size={16} />} onSelect={() => chain()?.toggleOrderedList().run()} right={is('orderedList') ? check : <span className="text-xs text-muted">⌘⇧7</span>}>
            {t('editor.orderedList')}
          </MenuItem>
          <MenuItem icon={<Icon name="checklist" size={16} />} onSelect={() => chain()?.toggleTaskList().run()} right={is('taskList') ? check : <span className="text-xs text-muted">⌘⇧9</span>}>
            {t('editor.checklist')}
          </MenuItem>
        </MenuTool>
        <ToolSep />
        <PopoverTool label={t('editor.image')} icon={<Icon name="image" size={17} />} open={imageOpen} onOpenChange={setImageOpen}>
          <UrlForm initial="" label={t('editor.imageUrl')} withText="" submit={(src, alt) => src && chain()?.setImage({ src, alt: alt || undefined }).run()} onDone={() => setImageOpen(false)} />
        </PopoverTool>
        <Tool label={t('editor.codeBlock')} shortcut="⌘⌥C" active={is('codeBlock')} aria-pressed={is('codeBlock')} onClick={() => chain()?.toggleCodeBlock().run()}>
          <Icon name="codeBlock" size={17} />
        </Tool>
        <PopoverTool label={t('editor.emoji')} icon={<Icon name="emoji" size={17} />} open={emojiOpen} onOpenChange={setEmojiOpen}>
          <div className="grid grid-cols-8 gap-0.5" role="group" aria-label={t('editor.emojis')}>
            {emojis.map((em) => (
              <button
                key={em}
                type="button"
                aria-label={t('editor.insertEmoji', { emoji: em })}
                onClick={() => {
                  chain()?.insertContent(em).run()
                  setEmojiOpen(false)
                }}
                className="flex size-8 items-center justify-center rounded-control-sm text-lg transition-colors hover:bg-hover"
              >
                {em}
              </button>
            ))}
          </div>
        </PopoverTool>
        <MenuTool label={t('editor.insert')} icon={<Icon name="plus" size={17} />}>
          <MenuItem icon={<Icon name="table" size={16} />} onSelect={() => chain()?.insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run()}>
            {t('editor.table')}
          </MenuItem>
          <MenuItem icon={<Icon name="rule" size={16} />} onSelect={() => chain()?.setHorizontalRule().run()}>
            {t('editor.divider')}
          </MenuItem>
          <MenuItem icon={<Icon name="quote" size={16} />} onSelect={() => chain()?.toggleBlockquote().run()} right={is('blockquote') ? check : <span className="text-xs text-muted">⌘⇧B</span>}>
            {t('editor.quote')}
          </MenuItem>
          {is('table') && (
            <>
              <MenuSeparator />
              <MenuItem onSelect={() => chain()?.addRowAfter().run()}>{t('editor.addRow')}</MenuItem>
              <MenuItem onSelect={() => chain()?.addColumnAfter().run()}>{t('editor.addColumn')}</MenuItem>
              <MenuItem danger onSelect={() => chain()?.deleteTable().run()}>
                {t('editor.removeTable')}
              </MenuItem>
            </>
          )}
        </MenuTool>
        <PopoverTool label={t('editor.link')} shortcut="⌘K" icon={<Icon name="link" size={17} />} open={linkOpen} onOpenChange={setLinkOpen}>
          <UrlForm
            key={linkOpen ? 'open' : 'closed'}
            initial={(editor?.getAttributes('link').href as string | undefined) ?? ''}
            label={t('editor.linkUrl')}
            submit={(href) => {
              if (!editor) return
              if (!href) {
                chain()?.extendMarkRange('link').unsetLink().run()
                return
              }
              if (editor.state.selection.empty && !is('link')) {
                chain()?.insertContent({ type: 'text', text: href, marks: [{ type: 'link', attrs: { href } }] }).run()
                return
              }
              chain()?.extendMarkRange('link').setLink({ href }).run()
            }}
            onDone={() => setLinkOpen(false)}
          />
        </PopoverTool>
        <ToolSep />
        <Tool label={t('editor.undo')} shortcut="⌘Z" disabled={!editor?.can().undo()} onClick={() => chain()?.undo().run()}>
          <Icon name="undo" size={17} />
        </Tool>
        <Tool label={t('editor.redo')} shortcut="⌘⇧Z" disabled={!editor?.can().redo()} onClick={() => chain()?.redo().run()}>
          <Icon name="redo" size={17} />
        </Tool>
      </Toolbar>
      {area}
    </div>
  )
}
