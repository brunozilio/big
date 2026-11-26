import { afterEach, describe, expect, test } from 'bun:test'
import { act, cleanup, render } from '@testing-library/react'
import type { Item } from '../src/api/contract'
import { stepSignature, useSettled } from '../src/api/store'
import { ticking, useMinutesSince } from '../src/screens/items/ticker'

afterEach(cleanup)

const item = (over: Partial<Item> = {}, seconds = 5) =>
  ({ id: 'i1', current: 'build', live: true, updatedAt: 'a', doneAt: null, impediment: null, attention: null, change: null, activity: null, steps: [{ id: 'plan', state: 'passed', attempts: 1, seconds: 30 }, { id: 'build', state: 'running', attempts: 1, seconds }], ...over }) as unknown as Item

describe('quando a etapa busca de novo', () => {
  test('ignora atualizações do item que não tocam a etapa', () => {
    const base = stepSignature(item(), 'plan')
    expect(stepSignature(item({ updatedAt: 'b', activity: { text: 'Compilando', since: 'x' } }), 'plan')).toBe(base)
    expect(stepSignature(item({}, 9), 'build')).toBe(stepSignature(item({ updatedAt: 'c' }, 12), 'build'))
  })

  test('muda com estado, tentativas, tempo em faixas e etapa atual', () => {
    const base = stepSignature(item(), 'build')
    expect(stepSignature(item({}, 45), 'build')).not.toBe(base)
    expect(stepSignature(item({ current: 'plan' }), 'build')).not.toBe(base)
    expect(stepSignature(item({ live: false }), 'build')).not.toBe(base)
    const failed = item()
    failed.steps[1] = { ...failed.steps[1], state: 'failed', attempts: 2 }
    expect(stepSignature(failed, 'build')).not.toBe(base)
  })

  test('uma rajada vira uma mudança só e trocar de etapa não espera', async () => {
    const seen: string[] = []
    function Probe({ value, scope }: { value: string; scope: string }) {
      seen.push(useSettled(value, scope, 30))
      return null
    }
    const view = render(<Probe value="a" scope="s1" />)
    view.rerender(<Probe value="b" scope="s1" />)
    view.rerender(<Probe value="c" scope="s1" />)
    expect(new Set(seen)).toEqual(new Set(['a']))
    await act(() => new Promise((resolve) => setTimeout(resolve, 60)))
    expect(seen[seen.length - 1]).toBe('c')
    expect(seen.includes('b')).toBe(false)
    view.rerender(<Probe value="z" scope="s2" />)
    expect(seen[seen.length - 1]).toBe('z')
  })
})

describe('relógio compartilhado', () => {
  test('só roda enquanto alguém mostra minutos', () => {
    function Clock({ since }: { since: string | null }) {
      return <span>{useMinutesSince(since)}</span>
    }
    expect(ticking()).toBe(false)
    const view = render(<><Clock since={new Date().toISOString()} /><Clock since={new Date().toISOString()} /><Clock since={null} /></>)
    expect(ticking()).toBe(true)
    view.unmount()
    expect(ticking()).toBe(false)
  })
})
