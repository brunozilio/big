import { afterEach, describe, expect, test } from 'bun:test'
import { act, renderHook } from '@testing-library/react'
import { rememberedModel, rememberedTuning, useRememberedModel, useRememberedTuning } from '../src/chat/remembered'

afterEach(() => {
  localStorage.clear()
})

describe('remembered composer choice', () => {
  test('model survives a reload', () => {
    const first = renderHook(() => useRememberedModel())
    act(() => first.result.current[1]('claude-opus'))
    expect(rememberedModel()).toBe('claude-opus')
    const reloaded = renderHook(() => useRememberedModel())
    expect(reloaded.result.current[0]).toBe('claude-opus')
  })

  test('choosing the default forgets the model', () => {
    const hook = renderHook(() => useRememberedModel())
    act(() => hook.result.current[1]('claude-opus'))
    act(() => hook.result.current[1](null))
    expect(rememberedModel()).toBeNull()
  })

  test('tuning survives a reload', () => {
    const first = renderHook(() => useRememberedTuning({ effort: 'medium', fast: false }))
    act(() => first.result.current[1]({ effort: 'high', fast: true }))
    const reloaded = renderHook(() => useRememberedTuning({ effort: 'medium', fast: false }))
    expect(reloaded.result.current[0]).toEqual({ effort: 'high', fast: true })
  })

  test('corrupt tuning falls back', () => {
    localStorage.setItem('composer.tuning', '{"effort":"huge","fast":"yes"}')
    expect(rememberedTuning({ effort: 'medium', fast: false })).toEqual({ effort: 'medium', fast: false })
  })
})
