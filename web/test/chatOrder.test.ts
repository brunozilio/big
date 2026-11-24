import { describe, expect, test } from 'bun:test'
import type { SessionEvent } from '../src/api/contract'
import { segmentsOf, withoutRepeatedPlans } from '../src/chat/ChatView'
import { parsePlan, storied, stressed, structured, subtasksOf } from '../src/chat/planDoc'

const ev = (id: string, type: string, extra: Record<string, unknown> = {}) => ({ id, at: '', type, ...extra }) as unknown as SessionEvent

describe('ordem do chat', () => {
  test('respostas e trabalho ficam na ordem em que aconteceram', () => {
    const events = [
      ev('t1', 'thinking', { steps: [{ text: 'pensa', done: true }], seconds: 1, live: false }),
      ev('a1', 'assistant', { text: 'vou ler' }),
      ev('k1', 'tools', { calls: [], seconds: 0, live: false }),
      ev('k2', 'tools', { calls: [], seconds: 0, live: false }),
      ev('a2', 'assistant', { text: 'pronto' }),
    ]
    const order = segmentsOf(events, false).map((s) => (s.kind === 'human' ? s.event.id : s.events.map((e) => e.id).join('+')))
    expect(order).toEqual(['t1', 'a1', 'k1+k2', 'a2'])
  })

  test('o mesmo plano não aparece duas vezes', () => {
    const plan = (id: string, resolution: string) => ev(id, 'plan', { text: '# P', resolution })
    expect(withoutRepeatedPlans([plan('e14', 'approved'), plan('ask', 'approved')]).map((e) => e.id)).toEqual(['ask'])
  })
})

describe('plano no formato do Jira', () => {
  const text = '# Busca\n\n## Descrição\nComo cliente, eu quero buscar para que eu ache.\n\n## Critérios de aceite\n- Dado que estou na busca, quando filtro, então vejo só o filtrado.\n\n## Subtarefas\n### Criar endpoint\nGET /busca.\n\n### Tela\nCampo de busca.'

  test('reconhece história, critérios e subtarefas', () => {
    const doc = parsePlan(text)
    expect(doc.title).toBe('Busca')
    expect(doc.sections.map((s) => s.kind)).toEqual(['story', 'criteria', 'subtasks'])
    expect(structured(doc)).toBe(true)
    expect(subtasksOf(doc.sections[2].body)).toEqual([{ title: 'Criar endpoint', details: 'GET /busca.' }, { title: 'Tela', details: 'Campo de busca.' }])
  })

  test('destaca Dado, Quando e Então', () => {
    expect(stressed('Dado que estou na busca, quando filtro, então vejo')).toBe('**Dado que** estou na busca, **Quando** filtro, **Então** vejo')
  })

  test('destaca Como, Eu quero e Para que', () => {
    expect(storied('Como cliente, eu quero buscar para que eu ache.')).toBe('**Como** cliente, **Eu quero** buscar **Para que** eu ache.')
    expect(storied('Como pessoa da API, eu quero gerenciar produtos para manter o catálogo.')).toBe('**Como** pessoa da API, **Eu quero** gerenciar produtos **Para** manter o catálogo.')
  })
})
