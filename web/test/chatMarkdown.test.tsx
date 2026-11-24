import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, render } from '@testing-library/react'
import { ChatMarkdown } from '../src/ui/chatMarkdown'

afterEach(cleanup)

describe('markdown do chat', () => {
  test('bloco cercado vira bloco de código com a linguagem', () => {
    const view = render(<ChatMarkdown text={'Veja:\n\n```json\n{\n  "a": 1\n}\n```'} />)
    expect(view.getByText('JSON')).toBeTruthy()
    const pre = view.container.querySelector('pre')
    expect(pre?.textContent).toContain('"a":')
    expect(pre?.children.length).toBe(3)
  })

  test('bloco sem linguagem que parece JSON é tratado como JSON', () => {
    const view = render(<ChatMarkdown text={'```\n[1, 2]\n```'} />)
    expect(view.getByText('JSON')).toBeTruthy()
  })

  test('bloco de outra linguagem mostra o nome dela', () => {
    const view = render(<ChatMarkdown text={'```rust\nfn main() {}\n```'} />)
    expect(view.getByText('rust')).toBeTruthy()
  })

  test('código inline continua inline', () => {
    const view = render(<ChatMarkdown text={'use `cargo` aqui'} />)
    expect(view.container.querySelector('pre')).toBeNull()
    expect(view.container.querySelector('code')?.textContent).toBe('cargo')
  })
})
