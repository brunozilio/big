import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, render } from '@testing-library/react'
import type { SessionEvent } from '@/api/contract'
import { Transcript, withoutRepeatedPlans } from '@/chat/ChatView'
import { ChatMarkdown } from '@/ui/chatMarkdown'
import { TooltipProvider } from '@/ui/overlay'

afterEach(cleanup)

const respond = () => Promise.resolve()
const at = '2026-11-24T13:46:53Z'
const user = (id: string, text: string) => ({ type: 'user', id, at, text, attachments: 0, author: { id: 'you', name: 'Você', avatar: null, you: true } }) as unknown as SessionEvent
const assistant = (id: string, text: string) => ({ type: 'assistant', id, at, text }) as unknown as SessionEvent

function counted(id: string, text: string) {
  const reads = { n: 0 }
  const event = { type: 'assistant', id, at }
  Object.defineProperty(event, 'text', { enumerable: true, get: () => {
    reads.n++
    return text
  } })
  return { event: event as unknown as SessionEvent, reads }
}

const show = (events: SessionEvent[]) => <TooltipProvider><Transcript events={events} running respond={respond} /></TooltipProvider>

describe('renderização do chat', () => {
  test('atualizar um evento não renderiza de novo uma mensagem já assentada', () => {
    const old = counted('a1', 'Resposta antiga e **pronta**')
    const first = [user('u1', 'oi'), old.event, user('u2', 'mais'), assistant('a2', 'Nova')]
    const view = render(show(first))
    const before = old.reads.n
    const settledNode = view.getByText('pronta')
    expect(before).toBeGreaterThan(0)
    for (const text of ['Nova resposta', 'Nova resposta chegando', 'Nova resposta chegando aos poucos']) {
      view.rerender(show([first[0], first[1], first[2], assistant('a2', text)]))
    }
    expect(old.reads.n).toBe(before)
    expect(view.getByText('pronta')).toBe(settledNode)
    expect(view.container.querySelector('[data-event="assistant"]')?.querySelectorAll('[class~="t-stream-w"]').length).toBe(0)
    const rows = view.container.querySelectorAll('[data-event="assistant"]')
    expect([...rows[1].querySelectorAll('.t-stream-w')].map((w) => w.textContent)).toEqual(['resposta', 'chegando', 'aos', 'poucos'])
  })

  test('mensagem do mesmo turno que não mudou também não renderiza de novo', () => {
    const old = counted('a1', 'Primeira parte')
    const view = render(show([user('u1', 'oi'), old.event, assistant('a2', 'Segunda')]))
    const before = old.reads.n
    view.rerender(show([user('u1', 'oi'), old.event, assistant('a2', 'Segunda parte')]))
    expect(old.reads.n).toBe(before)
  })

  test('histórico é markdown puro, sem um span por palavra', () => {
    const view = render(<ChatMarkdown text={'Um texto **já pronto** com `código`'} />)
    expect(view.container.querySelectorAll('span').length).toBe(0)
    expect(view.container.textContent).toBe('Um texto já pronto com código')
  })

  test('mensagem que parou de crescer larga os spans de palavra', async () => {
    const view = render(<ChatMarkdown text="Olá" />)
    view.rerender(<ChatMarkdown text="Olá mundo novo" />)
    expect(view.container.querySelectorAll('span').length).toBe(3)
    await new Promise((done) => setTimeout(done, 1800))
    expect(view.container.querySelectorAll('span').length).toBe(0)
    expect(view.container.textContent).toBe('Olá mundo novo')
  })

  test('planos repetidos: fica só o último de cada texto', () => {
    const plan = (id: string, text: string) => ({ type: 'plan', id, at, text, resolution: 'approved' }) as unknown as SessionEvent
    const kept = withoutRepeatedPlans([plan('p1', 'A'), assistant('a1', 'x'), plan('p2', 'B'), plan('p3', 'A'), plan('p4', 'B')])
    expect(kept.map((e) => e.id)).toEqual(['a1', 'p3', 'p4'])
  })
})
