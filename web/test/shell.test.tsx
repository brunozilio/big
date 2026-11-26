import { afterEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import { Reconnecting } from '../src/app/connection'
import { escapeGoesBack, typing } from '../src/app/escape'
import { needsOnboarding, returnedTo } from '../src/app/gate'
import { settingsTab } from '../src/app/settingsNav'
import * as onboarding from '../src/screens/onboarding/Onboarding'
import { Collapse } from '../src/ui/motion'
import { Dialog } from '../src/ui/overlay'

afterEach(cleanup)

const sleep = (ms: number) => act(() => new Promise<void>((r) => setTimeout(r, ms)))

describe('indicador de reconexão', () => {
  test('só aparece depois do atraso e some ao reconectar', async () => {
    const view = render(<Reconnecting down delay={60} />)
    expect(screen.queryByText('Reconectando…')).toBeNull()
    await sleep(20)
    expect(screen.queryByText('Reconectando…')).toBeNull()
    await sleep(80)
    expect(screen.getByText('Reconectando…')).toBeTruthy()
    view.rerender(<Reconnecting down={false} delay={60} />)
    expect(screen.queryByText('Reconectando…')).toBeNull()
  })

  test('queda curta não mostra nada', async () => {
    const view = render(<Reconnecting down delay={60} />)
    await sleep(20)
    view.rerender(<Reconnecting down={false} delay={60} />)
    await sleep(80)
    expect(screen.queryByText('Reconectando…')).toBeNull()
  })
})

describe('esc', () => {
  test('volta só sem sobreposição aberta e fora de campo de texto', () => {
    expect(escapeGoesBack({ key: 'Escape', defaultPrevented: false }, document)).toBe(true)
    expect(escapeGoesBack({ key: 'Enter', defaultPrevented: false }, document)).toBe(false)
    expect(escapeGoesBack({ key: 'Escape', defaultPrevented: true }, document)).toBe(false)
    render(<input aria-label="campo" />)
    screen.getByLabelText('campo').focus()
    expect(typing(document.activeElement)).toBe(true)
    expect(escapeGoesBack({ key: 'Escape', defaultPrevented: false }, document)).toBe(false)
    cleanup()
    render(<Dialog open onOpenChange={() => undefined} title="Título">corpo</Dialog>)
    expect(escapeGoesBack({ key: 'Escape', defaultPrevented: false }, document)).toBe(false)
  })
})

describe('diálogo', () => {
  test('só o corpo rola; cabeçalho e rodapé ficam fixos', () => {
    render(<Dialog open onOpenChange={() => undefined} title="Título" footer={<button>Salvar</button>}>corpo</Dialog>)
    const panel = screen.getByRole('dialog')
    expect(panel.className).toContain('flex-col')
    expect(panel.className).toContain('overflow-hidden')
    expect(screen.getByText('corpo').className).toContain('overflow-y-auto')
    expect(screen.getByText('Salvar').parentElement?.className).toContain('shrink-0')
  })

  test('sem moldura, o painel inteiro rola', () => {
    render(<Dialog open bare onOpenChange={() => undefined} title="Título">corpo</Dialog>)
    const panel = screen.getByRole('dialog')
    expect(panel.className).toContain('overflow-y-auto')
    expect(panel.className).not.toContain('flex-col')
  })
})

describe('collapse', () => {
  test('monta ao abrir e desmonta depois da transição ao fechar', async () => {
    const view = render(<Collapse open={false}>dentro</Collapse>)
    expect(screen.queryByText('dentro')).toBeNull()
    view.rerender(<Collapse open>dentro</Collapse>)
    expect(screen.getByText('dentro')).toBeTruthy()
    expect(view.container.querySelector('.fold')?.getAttribute('data-open')).toBe('true')
    view.rerender(<Collapse open={false}>dentro</Collapse>)
    expect(view.container.querySelector('.fold')?.getAttribute('data-open')).toBe('false')
    await sleep(320)
    expect(screen.queryByText('dentro')).toBeNull()
  })
})

describe('rotas da casca', () => {
  test('aba desconhecida cai em conexões', () => {
    expect(settingsTab('usage')).toBe('usage')
    expect(settingsTab('nada')).toBe('connections')
    expect(settingsTab(undefined)).toBe('connections')
  })

  test('a porta do onboarding decide igual à tela', () => {
    const cases: [boolean, [] | null, { skipped: boolean } | null][] = [[true, [], null], [true, [], { skipped: true }], [false, [], null], [true, null, null]]
    for (const [ready, apps, state] of cases) expect(needsOnboarding(ready, apps, state)).toBe(onboarding.needsOnboarding(ready, apps, state))
    for (const [path, search] of [['/settings/connections', '?connect=github&flow=f1'], ['/settings/connections', ''], ['/', '?connect=github']]) expect(returnedTo(path, search)).toEqual(onboarding.returnedTo(path, search))
  })
})

describe('tecla', () => {
  test('esc dentro de um diálogo fecha só ele', () => {
    let closed = 0
    render(<Dialog open onOpenChange={(o) => { if (!o) closed += 1 }} title="Título"><input aria-label="campo" /></Dialog>)
    fireEvent.keyDown(screen.getByLabelText('campo'), { key: 'Escape' })
    expect(closed).toBe(1)
  })
})
