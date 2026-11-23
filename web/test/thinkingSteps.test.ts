import { describe, expect, test } from 'bun:test'
import { emptyThinking, visibleSteps } from '../src/chat/events'

describe('passos do pensamento', () => {
  test('um passo ainda sem texto não vira uma linha só com o spinner', () => {
    const steps = [{ text: 'Lendo o plano', done: true }, { text: '', done: false }, { text: '  ', done: false }]
    expect(visibleSteps(steps)).toEqual([{ text: 'Lendo o plano', done: true }])
  })
})

describe('pensamento vazio', () => {
  const thinking = (live: boolean, text: string) => ({ id: 't', at: '2026-11-23T00:00:00Z', type: 'thinking' as const, steps: [{ text, done: true }], seconds: 0, live })

  test('terminado e sem texto some; ao vivo ou com texto fica', () => {
    expect(emptyThinking(thinking(false, ''))).toBe(true)
    expect(emptyThinking(thinking(true, ''))).toBe(false)
    expect(emptyThinking(thinking(false, 'Lendo'))).toBe(false)
  })
})

test('sessão parada não deixa pensamento vazio girando', () => {
  const stuck = { id: 't', at: '2026-11-23T00:00:00Z', type: 'thinking' as const, steps: [{ text: '', done: false }], seconds: 0, live: true }
  expect(emptyThinking(stuck, false)).toBe(true)
  expect(emptyThinking(stuck, true)).toBe(false)
})
