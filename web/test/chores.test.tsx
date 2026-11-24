import { afterEach, describe, expect, mock, test } from 'bun:test'
import { act, cleanup, fireEvent, render, renderHook, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { setApi } from '../src/api'
import { isItem, isLiveEvent, isSession, isStepView } from '../src/api/guards'
import { mockApi } from '../src/api/mock'
import { applyLive, boot, getState, stop, useSession } from '../src/api/store'
import { btwOf, Composer } from '../src/chat/Composer'
import { BtwChat, useBtw } from '../src/chat/BtwChat'
import { SideAnswers, useSideQuestions } from '../src/chat/SideQuestions'
import { HeadsUpNote } from '../src/screens/items/ItemModal'
import { ItemsView } from '../src/screens/items/ItemsView'
import { TooltipProvider } from '../src/ui/overlay'

afterEach(() => {
  cleanup()
  stop()
  setApi(null)
})

async function started() {
  const api = mockApi({ tickMs: 0, delayMs: 0 })
  setApi(api)
  await act(() => boot(api))
  return api
}

function composer(props: Partial<Parameters<typeof Composer>[0]>) {
  const onSubmit = mock(() => undefined)
  const view = render(
    <MemoryRouter>
      <TooltipProvider>
        <Composer onSubmit={onSubmit} {...props} />
      </TooltipProvider>
    </MemoryRouter>,
  )
  return { view, onSubmit }
}

describe('resumo do item', () => {
  test('o card da lista mostra só o título, sem o resumo', async () => {
    const api = await started()
    const items = await api.items()
    const recaps = items.filter((i) => i.recap)
    expect(recaps.length).toBeGreaterThan(0)
    expect(recaps.every((i) => i.group === 'you' && isItem(i))).toBe(true)
    const view = render(<TooltipProvider><ItemsView onOpen={() => undefined} onConnect={() => undefined} fresh={null} /></TooltipProvider>)
    for (const it of recaps) expect(view.queryByText(it.recap ?? '')).toBeNull()
  })
})

describe('aviso da etapa', () => {
  test('mostra título e explicação, mais forte quando urgente', () => {
    const view = render(<HeadsUpNote headsUp={{ title: 'Migração em staging', explain: 'Altera tabelas compartilhadas.', urgent: true }} />)
    const note = view.getByRole('note')
    expect(note.textContent).toContain('Você deveria saber')
    expect(view.getByText('Migração em staging').className).toContain('font-medium')
    expect(note.getAttribute('data-urgent')).toBe('true')
  })

  test('o mock só traz avisos válidos', async () => {
    const api = await started()
    const items = await api.items()
    const views = await Promise.all(items.flatMap((it) => it.steps.map((s) => api.step(it.id, s.id, null))))
    expect(views.every(isStepView)).toBe(true)
    expect(views.some((v) => v.headsUp)).toBe(true)
  })
})

describe('sugestão de próxima mensagem', () => {
  test('clicar preenche o campo sem enviar', () => {
    const { view, onSubmit } = composer({ suggestion: 'Rode os testes de novo' })
    fireEvent.click(view.getByLabelText('Usar sugestão: Rode os testes de novo'))
    expect((view.getByLabelText('Mensagem') as HTMLTextAreaElement).value).toBe('Rode os testes de novo')
    expect(onSubmit).not.toHaveBeenCalled()
  })

  test('some com texto no campo ou com o agente rodando', () => {
    const { view } = composer({ suggestion: 'Rode os testes de novo', running: true })
    expect(view.queryByLabelText('Usar sugestão: Rode os testes de novo')).toBeNull()
    cleanup()
    const idle = composer({ suggestion: 'Rode os testes de novo' })
    fireEvent.change(idle.view.getByLabelText('Mensagem'), { target: { value: 'outra coisa' } })
    expect(idle.view.queryByLabelText('Usar sugestão: Rode os testes de novo')).toBeNull()
  })

  test('sessões do mock seguem o contrato com a sugestão', async () => {
    const api = await started()
    const items = await api.items()
    const sessions = await Promise.all(items.map((i) => api.session(`s-${i.id}`)))
    expect(sessions.every(isSession)).toBe(true)
    expect(sessions.some((s) => s.status === 'idle' && s.suggestion)).toBe(true)
  })
})

describe('pergunta à parte', () => {
  test('com o agente rodando, Enter manda a mensagem ao agente e só o botão pergunta à parte', async () => {
    const onAside = mock((_text: string) => undefined)
    const { view, onSubmit } = composer({ running: true, onAside })
    const box = view.getByLabelText('Mensagem') as HTMLTextAreaElement
    fireEvent.change(box, { target: { value: 'Inclua os testes' } })
    await waitFor(() => expect((view.getByLabelText('Enviar') as HTMLButtonElement).disabled).toBe(false))
    fireEvent.keyDown(box, { key: 'Enter' })
    expect(onSubmit).toHaveBeenCalledTimes(1)
    expect(onSubmit).toHaveBeenCalledWith(expect.objectContaining({ text: 'Inclua os testes' }))
    expect(onAside).not.toHaveBeenCalled()
    expect(box.value).toBe('')
    expect(view.getByLabelText('Parar')).toBeTruthy()
    fireEvent.change(box, { target: { value: 'O que falta?' } })
    fireEvent.click(view.getByText('Perguntar à parte'))
    expect(onAside).toHaveBeenCalledWith('O que falta?')
    expect(onSubmit).toHaveBeenCalledTimes(1)
    expect(box.value).toBe('')
  })

  test('a resposta aparece numa bolha efêmera com a pergunta', async () => {
    const side = mock(async (_session: string, _question: { text: string; model: string }) => ({ answer: 'Está rodando os testes.' }))
    function Harness() {
      const q = useSideQuestions('s1', (text) => side('s1', { text, model: 'opus' }))
      return (
        <>
          <button onClick={() => q.ask('O que falta?')}>perguntar</button>
          <SideAnswers asked={q.asked} onDismiss={q.dismiss} />
        </>
      )
    }
    const view = render(<Harness />)
    fireEvent.click(view.getByText('perguntar'))
    await waitFor(() => expect(view.getByText('Está rodando os testes.')).toBeTruthy())
    expect(side).toHaveBeenCalledWith('s1', { text: 'O que falta?', model: 'opus' })
    const bubble = view.getByTestId('side-question')
    expect(bubble.textContent).toContain('Pergunta à parte')
    expect(bubble.textContent).toContain('O que falta?')
    fireEvent.click(view.getByLabelText('Fechar pergunta à parte'))
    await waitFor(() => expect(view.queryByTestId('side-question')).toBeNull())
  })

  test('/btw pergunta à parte sem enviar, mesmo com o agente parado', () => {
    const onBtw = mock((_text: string) => undefined)
    const { view, onSubmit } = composer({ onBtw })
    const box = view.getByLabelText('Mensagem') as HTMLTextAreaElement
    fireEvent.change(box, { target: { value: '/btw o swagger está atualizado?' } })
    expect(view.getByText('Perguntar à parte')).toBeTruthy()
    fireEvent.keyDown(box, { key: 'Enter' })
    expect(onBtw).toHaveBeenCalledWith('o swagger está atualizado?')
    expect(onSubmit).not.toHaveBeenCalled()
    expect(box.value).toBe('')
    fireEvent.change(box, { target: { value: '/btw' } })
    fireEvent.keyDown(box, { key: 'Enter' })
    expect(onBtw).toHaveBeenCalledTimes(1)
    expect(view.getByRole('alert').textContent).toContain('/btw <pergunta>')
  })

  test('sem quem trate o /btw, o texto segue como mensagem', () => {
    const { view, onSubmit } = composer({})
    const box = view.getByLabelText('Mensagem') as HTMLTextAreaElement
    fireEvent.change(box, { target: { value: '/btw oi' } })
    fireEvent.keyDown(box, { key: 'Enter' })
    expect(onSubmit).toHaveBeenCalled()
  })

  test('só /btw no começo vira pergunta à parte', () => {
    expect(btwOf('/btw  qual arquivo? ')).toBe('qual arquivo?')
    expect(btwOf('/btw\nlinha dois')).toBe('linha dois')
    expect(btwOf('/btw')).toBe('')
    expect(btwOf('/btwx oi')).toBeNull()
    expect(btwOf('faça /btw depois')).toBeNull()
  })

  test('o /btw abre uma conversa livre que responde e continua', async () => {
    await started()
    function Harness() {
      const btw = useBtw('a1', null)
      return (
        <MemoryRouter>
          <TooltipProvider>
            <button onClick={() => btw.ask('o swagger está atualizado?')}>perguntar</button>
            {btw.state && <BtwChat state={btw.state} onClose={btw.close} />}
          </TooltipProvider>
        </MemoryRouter>
      )
    }
    const view = render(<Harness />)
    fireEvent.click(view.getByText('perguntar'))
    await waitFor(() => expect(view.getByText(/está tudo em dia/)).toBeTruthy(), { timeout: 3000 })
    expect(view.getByTestId('btw-chat').textContent).toContain('Pergunta à parte')
    expect(view.getByTestId('btw-chat').textContent).not.toContain('não cria atividade')
    const box = view.getByLabelText('Mensagem') as HTMLTextAreaElement
    fireEvent.change(box, { target: { value: 'e o readme?' } })
    fireEvent.keyDown(box, { key: 'Enter' })
    await waitFor(() => expect(view.getAllByText(/está tudo em dia/)).toHaveLength(2), { timeout: 3000 })
    fireEvent.click(view.getByLabelText('Fechar pergunta à parte'))
    await waitFor(() => expect(view.queryByTestId('btw-chat')).toBeNull())
  })

  test('o mock abre o /btw da aplicação e recusa sem pergunta ou aplicação', async () => {
    const api = await started()
    const before = (await api.items()).length
    const { session } = await api.btw({ text: 'o swagger está atualizado?', application: 'a1' })
    expect((await api.session(session)).mode).toBe('plan')
    expect(await api.items()).toHaveLength(before)
    await expect(api.btw({ text: ' ', application: 'a1' })).rejects.toThrow('Escreva a pergunta.')
    await expect(api.btw({ text: 'oi', application: null })).rejects.toThrow('Escolha uma aplicação')
  })

  test('o mock responde e recusa pergunta vazia', async () => {
    const api = await started()
    const [first] = await api.items()
    expect((await api.side(`s-${first.id}`, { text: 'Como está?' })).answer.length).toBeGreaterThan(0)
    await expect(api.side(`s-${first.id}`, { text: '  ' })).rejects.toThrow('Escreva a pergunta.')
  })
})

describe('sugestão ao vivo', () => {
  test('a sugestão que o servidor anuncia chega à sessão aberta e some quando ela volta a rodar', async () => {
    const api = await started()
    const items = await api.items()
    const id = `s-${items[0].id}`
    const opened = renderHook(() => useSession(id))
    await waitFor(() => expect(opened.result.current.session?.id).toBe(id))
    act(() => applyLive({ type: 'session.status', session: id, status: 'idle' }))
    act(() => applyLive({ type: 'session.suggestion', session: id, suggestion: 'rode os testes' }))
    expect(getState().sessions[id]?.suggestion).toBe('rode os testes')
    act(() => applyLive({ type: 'session.status', session: id, status: 'running' }))
    expect(getState().sessions[id]?.suggestion).toBeNull()
    expect(isLiveEvent({ type: 'session.suggestion', session: id, suggestion: null })).toBe(true)
    expect(isLiveEvent({ type: 'session.suggestion', session: id, suggestion: 3 })).toBe(false)
  })
})
