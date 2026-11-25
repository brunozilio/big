import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { BackgroundWork, hasWork } from '@/chat/BackgroundWork'
import { ToolCallItem } from '@/chat/events'
import { JobsContext } from '@/chat/jobs'

afterEach(cleanup)

describe('trabalho do agente em andamento', () => {
  test('mostra os jobs em segundo plano e as tarefas com o estado', () => {
    const work = { background: [{ id: 'bash_1', command: 'npm start' }, { id: 'bash_2', command: 'cargo watch' }], stoppable: false, tasks: [{ id: '1', subject: 'rodar os testes', status: 'in_progress' }, { id: '2', subject: 'abrir o PR', status: 'completed' }] }
    render(<BackgroundWork work={work} />)
    expect(screen.getByText('2 processos rodando em segundo plano')).toBeTruthy()
    expect(screen.getByText('npm start')).toBeTruthy()
    expect(screen.queryByText('Parar')).toBeNull()
    expect(screen.getByText('rodar os testes')).toBeTruthy()
    expect(screen.getByText('Fazendo')).toBeTruthy()
    expect(screen.getByText('Feito')).toBeTruthy()
  })

  test('sem nada em andamento o painel não aparece', () => {
    expect(hasWork(undefined)).toBe(false)
    expect(hasWork({ background: [], tasks: [] })).toBe(false)
  })

  test('um processo rodando em segundo plano se distingue e pode ser parado', async () => {
    const stopped: string[] = []
    const call = { id: 'b1', kind: 'command' as const, title: 'PORT=3100 npm start', tag: 'Bash', state: 'ok' as const, durationMs: 0, job: 'bash_33', jobRunning: true, jobStoppable: true }
    render(
      <JobsContext.Provider value={async (job) => void stopped.push(job)}>
        <ol><ToolCallItem call={call} /></ol>
      </JobsContext.Provider>,
    )
    expect(screen.getByText('Rodando')).toBeTruthy()
    expect(screen.queryByText('Bash')).toBeNull()
    fireEvent.click(screen.getByText('Parar'))
    await waitFor(() => expect(stopped).toEqual(['bash_33']))
  })

  test('um processo que terminou mostra encerrado e não oferece parar', () => {
    const call = { id: 'b1', kind: 'command' as const, title: 'npm start', tag: 'Bash', state: 'ok' as const, durationMs: 0, job: 'bash_33', jobRunning: false, jobStoppable: false }
    render(<JobsContext.Provider value={async () => undefined}><ol><ToolCallItem call={call} /></ol></JobsContext.Provider>)
    expect(screen.getByText('Encerrado')).toBeTruthy()
    expect(screen.queryByText('Parar')).toBeNull()
  })

  test('um subagente abre os passos que deu', () => {
    const call = { id: 'agent-0', kind: 'subagent' as const, title: 'procura o bug', tag: 'explore', state: 'ok' as const, durationMs: 0, steps: ['Read src/a.rs', 'Grep erro'] }
    render(<ol><ToolCallItem call={call} /></ol>)
    fireEvent.click(screen.getByText('procura o bug'))
    expect(screen.getByLabelText('Passos do subagente').textContent).toContain('Grep erro')
  })
})
