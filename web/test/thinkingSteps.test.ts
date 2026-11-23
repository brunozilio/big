import { describe, expect, test } from 'bun:test'
import { visibleSteps } from '../src/chat/events'

describe('passos do pensamento', () => {
  test('um passo ainda sem texto não vira uma linha só com o spinner', () => {
    const steps = [{ text: 'Lendo o plano', done: true }, { text: '', done: false }, { text: '  ', done: false }]
    expect(visibleSteps(steps)).toEqual([{ text: 'Lendo o plano', done: true }])
  })
})
