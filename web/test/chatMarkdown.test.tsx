import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, fireEvent, render } from '@testing-library/react'
import type { FileRef } from '../src/api/contract'
import { ChatMarkdown, fileRef, OpenFileContext } from '../src/ui/chatMarkdown'

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

  test('referência a arquivo vira caminho, linha e coluna', () => {
    expect(fileRef('test/calculadora.test.js:91')).toEqual({ path: 'test/calculadora.test.js', line: 91, column: null })
    expect(fileRef('calc.js:9:4')).toEqual({ path: 'calc.js', line: 9, column: 4 })
    expect(fileRef('src/a.rs#L12-L20')).toEqual({ path: 'src/a.rs', line: 12, column: null })
    expect(fileRef('file:///Users/me/Application%20Support/a.rs')).toEqual({ path: '/Users/me/Application Support/a.rs', line: null, column: null })
    expect(fileRef('https://example.com/a.rs:3')).toBeNull()
    expect(fileRef('mailto:x@y.z')).toBeNull()
    expect(fileRef('#secao')).toBeNull()
  })

  test('clicar num link de arquivo pede para abrir na IDE', () => {
    const opened: FileRef[] = []
    const view = render(
      <OpenFileContext.Provider value={(file) => opened.push(file)}>
        <ChatMarkdown text={'Veja [calc.test.js:91](calc.test.js:91) e [site](https://example.com).'} />
      </OpenFileContext.Provider>,
    )
    fireEvent.click(view.getByText('calc.test.js:91'))
    expect(opened).toEqual([{ path: 'calc.test.js', line: 91, column: null }])
    expect(view.getByText('site').closest('a')?.getAttribute('target')).toBe('_blank')
  })
})
