import { afterEach, describe, expect, test } from 'bun:test'
import { act, renderHook } from '@testing-library/react'
import { onFirstLine, onLastLine, rememberPrompt, storedPrompts, usePromptHistory } from '../src/chat/promptHistory'

afterEach(() => {
  localStorage.clear()
})

describe('prompt history', () => {
  test('keeps sent prompts newest last without repeats', () => {
    rememberPrompt('first')
    rememberPrompt('second')
    rememberPrompt('  first  ')
    rememberPrompt('   ')
    expect(storedPrompts()).toEqual(['second', 'first'])
  })

  test('walks back and forth and restores the draft', () => {
    rememberPrompt('one')
    rememberPrompt('two')
    const { result } = renderHook(() => usePromptHistory())
    const h = result.current
    expect(h.older('draft')?.text).toBe('two')
    expect(h.older('two')?.text).toBe('one')
    expect(h.older('one')).toBeNull()
    expect(h.newer()?.text).toBe('two')
    expect(h.newer()?.text).toBe('draft')
    expect(h.newer()).toBeNull()
  })

  test('recording restarts from the newest prompt', () => {
    rememberPrompt('old')
    const { result } = renderHook(() => usePromptHistory())
    act(() => {
      result.current.older('')
      result.current.record('new')
    })
    expect(result.current.older('')?.text).toBe('new')
  })

  test('empty history does nothing', () => {
    const { result } = renderHook(() => usePromptHistory())
    expect(result.current.older('typed')).toBeNull()
    expect(result.current.newer()).toBeNull()
  })

  test('only browses from the edge lines', () => {
    const text = 'a\nb'
    expect(onFirstLine(text, 1, 1)).toBe(true)
    expect(onFirstLine(text, 3, 3)).toBe(false)
    expect(onLastLine(text, 3, 3)).toBe(true)
    expect(onLastLine(text, 1, 1)).toBe(false)
    expect(onFirstLine(text, 0, 2)).toBe(false)
  })
})
