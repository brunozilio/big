import { describe, expect, test } from 'bun:test'
import { fireEvent, render, screen } from '@testing-library/react'
import { BackgroundWork, hasWork } from '@/chat/BackgroundWork'
import { ToolCallItem } from '@/chat/events'

describe('trabalho do agente em andamento', () => {
  test('mostra os jobs em segundo plano e as tarefas com o estado', () => {
    const work = { background: ['bash_1', 'agent_2'], tasks: [{ id: '1', subject: 'rodar os testes', status: 'in_progress' }, { id: '2', subject: 'abrir o PR', status: 'completed' }] }
    render(<BackgroundWork work={work} />)
    expect(screen.getByText('2 tarefas em segundo plano')).toBeTruthy()
    expect(screen.getByText('rodar os testes')).toBeTruthy()
    expect(screen.getByText('Fazendo')).toBeTruthy()
    expect(screen.getByText('Feito')).toBeTruthy()
  })

  test('sem nada em andamento o painel não aparece', () => {
    expect(hasWork(undefined)).toBe(false)
    expect(hasWork({ background: [], tasks: [] })).toBe(false)
  })

  test('um subagente abre os passos que deu', () => {
    const call = { id: 'agent-0', kind: 'subagent' as const, title: 'procura o bug', tag: 'explore', state: 'ok' as const, durationMs: 0, steps: ['Read src/a.rs', 'Grep erro'] }
    render(<ol><ToolCallItem call={call} /></ol>)
    fireEvent.click(screen.getByText('procura o bug'))
    expect(screen.getByLabelText('Passos do subagente').textContent).toContain('Grep erro')
  })
})
