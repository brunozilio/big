import { describe, expect, test } from 'bun:test'
import { render } from '@testing-library/react'
import { ChatMarkdown } from '@/ui/chatMarkdown'

describe('texto em streaming', () => {
  test('o que já estava na tela não anima; só as palavras que chegam depois', () => {
    const view = render(<ChatMarkdown text="Olá mundo" />)
    expect(view.container.querySelectorAll('.t-stream-w').length).toBe(0)
    view.rerender(<ChatMarkdown text="Olá mundo, tudo **bem** por aqui" />)
    const fresh = [...view.container.querySelectorAll('.t-stream-w')].map((w) => w.textContent)
    expect(fresh).toEqual(['tudo', 'bem', 'por', 'aqui'])
    expect(view.container.textContent).toBe('Olá mundo, tudo bem por aqui')
  })

  test('código não é quebrado em palavras', () => {
    const view = render(<ChatMarkdown text="a" />)
    view.rerender(<ChatMarkdown text={'a `x y`\n\n```\nfoo bar\n```'} />)
    expect([...view.container.querySelectorAll('.t-stream-w')].map((w) => w.textContent)).toEqual([])
  })
})
