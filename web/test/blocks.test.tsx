import { afterEach, describe, expect, test } from 'bun:test'
import { cleanup, render } from '@testing-library/react'
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join } from 'node:path'
import type { BlockType } from '../src/api/contract'
import { BlockView, registry } from '../src/blocks/registry'

afterEach(cleanup)

const TYPES: BlockType[] = ['chat', 'issue', 'change', 'review', 'pipeline', 'merge', 'markdown', 'logs', 'form', 'actions']

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f)
    return statSync(p).isDirectory() ? files(p) : [p]
  })
}

describe('registro de blocos', () => {
  test('todo tipo de bloco do contrato tem componente', () => {
    expect(Object.keys(registry).sort()).toEqual([...TYPES].sort())
  })

  test('blocos simples renderizam a partir dos dados', () => {
    const view = render(
      <div>
        <BlockView block={{ type: 'markdown', text: 'Ainda não começou.', muted: true }} />
        <BlockView block={{ type: 'logs', commands: [{ name: 'bun test', state: 'failed', seconds: 12, log: '$ bun test\n1 fail' }] }} />
        <BlockView block={{ type: 'actions', buttons: [{ action: 'deploy.approve', label: 'Aprovar' }] }} />
      </div>,
    )
    expect(view.getByText('Ainda não começou.')).toBeTruthy()
    expect(view.getByText('bun test')).toBeTruthy()
    expect(view.getByText('Aprovar')).toBeTruthy()
  })

  test('blocos não citam provedor pelo nome', () => {
    const names = /jira|github|atlassian|aws|gitlab|linear/i
    const offenders = files(join(import.meta.dir, '../src/blocks')).filter((f) => names.test(readFileSync(f, 'utf8')))
    expect(offenders).toEqual([])
  })
})
