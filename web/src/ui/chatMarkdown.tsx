import { createContext, isValidElement, memo, useContext, useEffect, useRef, useState, type ComponentPropsWithoutRef, type MouseEvent, type ReactNode } from 'react'
import ReactMarkdown, { defaultUrlTransform, type Components } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import type { FileRef } from '@/api/contract'
import { CodeBlock } from './display'

function textOf(node: ReactNode): string {
  if (typeof node === 'string' || typeof node === 'number') return String(node)
  if (Array.isArray(node)) return node.map(textOf).join('')
  if (isValidElement<{ children?: ReactNode }>(node)) return textOf(node.props.children)
  return ''
}

export function fencedCode(children: ReactNode): { code: string; language?: string } {
  const inner = Array.isArray(children) ? children[0] : children
  const className = isValidElement<{ className?: string }>(inner) ? inner.props.className ?? '' : ''
  const language = /language-([\w+#-]+)/.exec(className)?.[1]?.toLowerCase()
  const code = textOf(children).replace(/\n$/, '')
  const looksJson = !language && /^\s*[[{]/.test(code) && /[\]}]\s*$/.test(code)
  return { code, language: language ?? (looksJson ? 'json' : undefined) }
}

type HastNode = { type: string; tagName?: string; value?: string; properties?: Record<string, unknown>; children?: HastNode[] }

type Stream = { seen: number; counted: number; last: number }

const GAP = 30
const MAX_LAG = 700
const SETTLE = 1600

const StreamContext = createContext<Stream | null>(null)

const untouched = new Set(['code', 'pre', 'svg'])

export function splitWords(node: HastNode, counter: { n: number }): void {
  if (!node.children || (node.tagName && untouched.has(node.tagName))) return
  node.children = node.children.flatMap((child) => {
    if (child.type !== 'text') {
      splitWords(child, counter)
      return [child]
    }
    return (child.value ?? '').split(/(\s+)/).filter(Boolean).map((part) =>
      /^\s+$/.test(part) ? { type: 'text', value: part } : { type: 'element', tagName: 'span', properties: { dataWord: counter.n++ }, children: [{ type: 'text', value: part }] },
    )
  })
}

export function countWords(node: HastNode): number {
  if (node.tagName && untouched.has(node.tagName)) return 0
  if (node.type === 'text') return (node.value ?? '').split(/\s+/).filter(Boolean).length
  let total = 0
  for (const child of node.children ?? []) total += countWords(child)
  return total
}

function rehypeWords(stream: Stream) {
  return () => (tree: HastNode) => {
    const counter = { n: 0 }
    splitWords(tree, counter)
    stream.counted = counter.n
  }
}

function rehypeCount(stream: Stream) {
  return () => (tree: HastNode) => {
    stream.counted = countWords(tree)
  }
}

function startOf(stream: Stream, index: number): number | null {
  if (index < stream.seen) return null
  const at = performance.now()
  const start = Math.min(Math.max(at, stream.last + GAP), at + MAX_LAG)
  stream.last = start
  return start - at
}

function Word({ index, children }: { index: number; children: ReactNode }) {
  const stream = useContext(StreamContext)
  const [delay] = useState(() => (stream ? startOf(stream, index) : null))
  if (delay === null) return <span>{children}</span>
  return <span className="t-stream-w" style={{ animationDelay: `${Math.round(delay)}ms` }}>{children}</span>
}

const WEB = /^(https?|mailto|tel|data|javascript|vbscript|blob):/i

export function fileRef(href: string): FileRef | null {
  let raw = href.trim()
  if (!raw || raw.startsWith('#') || raw.startsWith('?') || WEB.test(raw)) return null
  if (/^file:\/\//i.test(raw)) raw = raw.slice('file://'.length)
  else if (/^[a-z][\w+.-]*:\/\//i.test(raw)) return null
  try {
    raw = decodeURI(raw)
  } catch {
    return null
  }
  const found = /#L(\d+)(?:C(\d+))?(?:-L?\d+(?:C\d+)?)?$/.exec(raw) ?? /:(\d+)(?::(\d+))?(?:-\d+)?$/.exec(raw)
  const path = found ? raw.slice(0, found.index) : raw
  if (!path) return null
  return { path, line: found ? Number(found[1]) : null, column: found?.[2] ? Number(found[2]) : null }
}

export const OpenFileContext = createContext<((file: FileRef) => void) | null>(null)

function urlTransform(url: string): string {
  return fileRef(url) ? url : defaultUrlTransform(url)
}

function Link({ node: _node, href, children, ...props }: ComponentPropsWithoutRef<'a'> & { node?: unknown }) {
  const openFile = useContext(OpenFileContext)
  const file = href ? fileRef(href) : null
  if (!file || !openFile) return <a href={href} target="_blank" rel="noreferrer" {...props}>{children}</a>
  const open = (event: MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault()
    openFile(file)
  }
  return <a href={href} title={file.line ? `${file.path}:${file.line}` : file.path} onClick={open} {...props}>{children}</a>
}

const components: Components = {
  a: Link,
  pre: ({ children }) => {
    const { code, language } = fencedCode(children)
    return <CodeBlock code={code} language={language} className="not-prose my-3 border" />
  },
  table: ({ children }) => (
    <div className="my-3 overflow-x-auto">
      <table>{children}</table>
    </div>
  ),
  span: ({ node: _node, ...props }: ComponentPropsWithoutRef<'span'> & { node?: unknown; 'data-word'?: number | string }) => {
    const index = props['data-word']
    if (index === undefined) return <span {...props} />
    return <Word index={Number(index)}>{props.children}</Word>
  },
}

const remark = [remarkGfm]

function useStreaming(text: string): boolean {
  const last = useRef(text)
  const streaming = useRef(false)
  const [, settle] = useState(0)
  if (last.current !== text) {
    last.current = text
    streaming.current = true
  }
  useEffect(() => {
    if (!streaming.current) return
    const timer = setTimeout(() => {
      streaming.current = false
      settle((n) => n + 1)
    }, SETTLE)
    return () => clearTimeout(timer)
  }, [text])
  return streaming.current
}

export const ChatMarkdown = memo(function ChatMarkdown({ text }: { text: string }) {
  const stream = useRef<Stream>({ seen: 0, counted: 0, last: 0 }).current
  const plugins = useRef({ split: [rehypeWords(stream)], count: [rehypeCount(stream)] }).current
  const streaming = useStreaming(text)
  useEffect(() => {
    stream.seen = stream.counted
  })
  return (
    <StreamContext.Provider value={stream}>
      <ReactMarkdown remarkPlugins={remark} rehypePlugins={streaming ? plugins.split : plugins.count} components={components} urlTransform={urlTransform}>
        {text}
      </ReactMarkdown>
    </StreamContext.Provider>
  )
})
