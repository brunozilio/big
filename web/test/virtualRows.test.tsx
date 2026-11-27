import { afterEach, describe, expect, test } from 'bun:test'
import { act, cleanup, render, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import type { Item, SessionEvent } from '@/api/contract'
import { PAST_TURN, Transcript } from '@/chat/ChatView'
import { settings } from '@/app/settings'
import { Rows } from '@/screens/items/ItemsView'
import { TooltipProvider } from '@/ui/overlay'
import { spanOf, useVirtualRows } from '@/ui/virtual'

afterEach(cleanup)

const nothing = () => undefined
const item = (n: number) => ({ id: `i${n}`, workflow: 'default', archivedAt: null, title: `Item ${n}`, issue: null, change: null, sources: [], git: [], steps: [], group: 'running' }) as unknown as Item

const rows = (items: Item[]) => (
  <MemoryRouter>
    <TooltipProvider>
      <Rows items={items} empty="vazio" fresh={null} accepting={false} onOpen={nothing} onConnect={nothing} onMove={nothing} />
    </TooltipProvider>
  </MemoryRouter>
)

function place(node: Element, top: number) {
  Object.defineProperty(node, 'getBoundingClientRect', { configurable: true, value: () => ({ top, bottom: top, left: 0, right: 0, width: 0, height: 0, x: 0, y: top, toJSON: () => ({}) }) })
}

describe('janela de linhas', () => {
  test('mostra só o que cabe na tela mais a margem e soma o resto como espaço', () => {
    const sizes = Array.from({ length: 100 }, () => 50)
    expect(spanOf(sizes, 0, 200, 2)).toEqual({ start: 0, end: 6, before: 0, after: 4700 })
    expect(spanOf(sizes, 1000, 200, 2)).toEqual({ start: 18, end: 26, before: 900, after: 3700 })
    expect(spanOf(sizes, 99999, 200, 2)).toEqual({ start: 98, end: 100, before: 4900, after: 0 })
    expect(spanOf([], 0, 200, 2)).toEqual({ start: 0, end: 0, before: 0, after: 0 })
  })

  test('linhas de alturas diferentes contam a altura de cada uma', () => {
    expect(spanOf([10, 300, 10, 10, 10], 305, 10, 0)).toEqual({ start: 1, end: 3, before: 10, after: 20 })
  })

  test('desligado, o gancho devolve a lista inteira', () => {
    const seen: number[] = []
    function Probe() {
      const span = useVirtualRows<HTMLUListElement>(['a', 'b', 'c'], { estimate: 10, enabled: false }).span
      seen.push(span.end - span.start)
      return null
    }
    render(<Probe />)
    expect(seen.at(-1)).toBe(3)
  })
})

describe('lista de itens longa', () => {
  test('lista curta continua inteira e animada', () => {
    const view = render(rows(Array.from({ length: 10 }, (_, n) => item(n))))
    expect(view.container.querySelectorAll('li').length).toBe(10)
  })

  test('até o limite da configuração a lista ainda monta todas as linhas', () => {
    const view = render(rows(Array.from({ length: settings.virtualRowsFrom }, (_, n) => item(n))))
    expect(view.container.querySelectorAll('li').length).toBe(settings.virtualRowsFrom)
  })

  test('lista longa monta só as linhas visíveis e acompanha a rolagem', async () => {
    const items = Array.from({ length: settings.virtualRowsFrom * 5 }, (_, n) => item(n))
    const view = render(rows(items))
    const shown = () => [...view.container.querySelectorAll('li')].map((li) => li.getAttribute('data-virtual-key'))
    expect(shown().length).toBeLessThan(40)
    expect(shown()[0]).toBe('i0')
    const list = view.container.querySelector('ul') as HTMLUListElement
    expect(Number.parseFloat(list.style.paddingBottom)).toBeGreaterThan(10000)
    place(list, -52 * 200)
    await act(async () => {
      window.dispatchEvent(new Event('scroll'))
    })
    await waitFor(() => expect(shown()).toContain('i200'))
    expect(shown()).not.toContain('i0')
    expect(Number.parseFloat(list.style.paddingTop)).toBeGreaterThan(52 * 180)
    expect(view.getByText('Item 200')).toBeTruthy()
  })
})

describe('transcrição longa', () => {
  const at = '2026-11-24T13:46:53Z'
  const user = (id: string) => ({ type: 'user', id, at, text: id, attachments: 0, author: { id: 'you', name: 'Você', avatar: null, you: true } }) as unknown as SessionEvent
  const assistant = (id: string) => ({ type: 'assistant', id, at, text: id }) as unknown as SessionEvent

  test('as voltas passadas pulam a pintura fora da tela e a última fica sempre viva', () => {
    const events = Array.from({ length: 4 }, (_, n) => [user(`u${n}`), assistant(`a${n}`)]).flat()
    const view = render(<TooltipProvider><Transcript events={events} running respond={() => Promise.resolve()} /></TooltipProvider>)
    const sections = [...view.container.querySelectorAll('section')].filter((node) => node.parentElement === view.container.firstElementChild || node.parentElement === view.container)
    expect(sections.length).toBe(4)
    expect(sections.map((node) => node.classList.contains(PAST_TURN))).toEqual([true, true, true, false])
  })
})
