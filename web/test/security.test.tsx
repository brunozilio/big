import { afterEach, beforeEach, describe, expect, test } from 'bun:test'
import { act, cleanup, fireEvent, render, waitFor, within } from '@testing-library/react'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import { setApi } from '../src/api'
import type { Api, Application, Block } from '../src/api/contract'
import { mockApi } from '../src/api/mock'
import { boot, stop } from '../src/api/store'
import { SecurityView, filtered, findingOf, targetsOf } from '../src/screens/security/SecurityView'
import { TooltipProvider } from '../src/ui/overlay'

const fresh = () => mockApi({ tickMs: 0, delayMs: 0 })

async function started(api: Api = fresh()) {
  setApi(api)
  await act(async () => {
    await boot(api)
  })
  return api
}

beforeEach(() => localStorage.clear())
afterEach(() => {
  cleanup()
  stop()
  setApi(null)
})

describe('página de segurança', () => {
  test('cada repositório vinculado mostra o relatório de qualidade e o de segurança', async () => {
    const asked: string[] = []
    const base = fresh()
    await started({ ...base, call: ((connection, action, args) => (asked.push(`${connection} ${action}`), base.call(connection, action, args))) as typeof base.call })
    const view = render(<TooltipProvider><SecurityView /></TooltipProvider>)
    const quality = await view.findByTestId('report-app-checkout/r-checkout/quality', {}, { timeout: 3000 })
    const security = await view.findByTestId('report-app-checkout/r-checkout/security', {}, { timeout: 3000 })
    expect(within(quality).getAllByText(/Coverage on New Code/).length).toBeGreaterThan(0)
    expect(within(security).getAllByText(/SQL Injection/).length).toBeGreaterThan(0)
    expect(asked).toContain('c-sonar quality.report')
    expect(asked).toContain('c-veracode security.findings')
    expect(asked.some((call) => call.startsWith('c-github'))).toBe(false)
  })

  test('só entra o que tem vínculo de relatório, e os filtros cortam por severidade e texto', () => {
    const apps: Application[] = [{ id: 'a', name: 'A', repositories: [
      { id: 'r1', directory: '/a', run: [], verify: [], bindings: { code: { connection: 'g', value: 'acme/a' }, security: { connection: 'v', value: 'guid' } } },
      { id: 'r2', directory: '/b', run: [], verify: [], bindings: { code: { connection: 'g', value: 'acme/b' } } },
    ] }]
    expect(targetsOf(apps).map((t) => t.key)).toEqual(['a/r1/security'])
    const findings = [findingOf({ id: '1', title: 'SQL Injection', severity: 5, file: 'src/a.ts' }), findingOf({ id: '2', title: 'Log', severity: 2, file: 'src/log.ts' })]
    expect(findings[0].suggested).toBe('fix')
    const block: Block = { type: 'findings', connection: 'v', project: 'guid', scannedAt: null, url: null, triage: null, findings }
    const base = { application: 'all', source: 'all', severity: 0, search: '' }
    const bySeverity = filtered(block, { ...base, severity: 4 })
    expect(bySeverity?.type === 'findings' && bySeverity.findings.map((f) => f.id)).toEqual(['1'])
    const byText = filtered(block, { ...base, search: 'log.ts' })
    expect(byText?.type === 'findings' && byText.findings.map((f) => f.id)).toEqual(['2'])
    expect(filtered(block, { ...base, search: 'nada' })).toBeNull()
  })

  test('sem nenhum vínculo a página explica onde escolher o projeto', async () => {
    const base = fresh()
    await started({ ...base, applications: async () => [] })
    const view = render(<MemoryRouter initialEntries={['/security']}><TooltipProvider><Routes><Route path="/security" element={<SecurityView />} /><Route path="/settings/applications" element={<p>aplicações abertas</p>} /></Routes></TooltipProvider></MemoryRouter>)
    await waitFor(() => expect(view.getByText('Nada para mostrar ainda')).toBeTruthy())
    expect(view.getByText(/Vincule um projeto de segurança ou de qualidade/)).toBeTruthy()
    fireEvent.click(view.getByRole('button', { name: 'Abrir aplicações' }))
    expect(view.getByText('aplicações abertas')).toBeTruthy()
  })
})
