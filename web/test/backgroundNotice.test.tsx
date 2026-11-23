import { describe, expect, test } from 'bun:test'
import { render, screen } from '@testing-library/react'
import { backgroundNotice, StatusLine } from '@/chat/events'

const raw = "[background bash_8] `npm test --silent -- '/Users/x/Library/Application Support/Big/worktrees/big_92ec--565e/tests/db/migrate.test.ts'` finished (exit code 0)."

describe('aviso de tarefa em segundo plano', () => {
  test('vira uma linha de tarefa com o comando curto', () => {
    expect(backgroundNotice(raw)).toEqual({ command: 'npm test --silent -- tests/db/migrate.test.ts', ok: true, code: 0 })
    render(<StatusLine event={{ id: 'e1', type: 'status', status: 'info', text: raw, at: '' } as never} />)
    expect(screen.getByText('Tarefa em segundo plano concluída')).toBeTruthy()
    expect(screen.getByText('npm test --silent -- tests/db/migrate.test.ts')).toBeTruthy()
  })

  test('uma tarefa que falhou mostra o código', () => {
    expect(backgroundNotice('[background bash_7] `go test .` finished (exit code 1).')).toEqual({ command: 'go test .', ok: false, code: 1 })
  })

  test('outro aviso continua como texto', () => {
    expect(backgroundNotice('Auto: claude, medium effort')).toBeNull()
  })
})
