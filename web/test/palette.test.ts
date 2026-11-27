import { describe, expect, test } from 'bun:test'
import type { Item } from '../src/api/contract'
import { filterEntries, fold, itemPath, paletteEntries, type PaletteLabels } from '../src/app/palette'

const labels: PaletteLabels = {
  deliveries: 'Entregas',
  security: 'Segurança',
  settings: 'Configurações',
  tab: (tab) => ({ connections: 'Conexões', applications: 'Aplicações', permissions: 'Permissões', memory: 'Memória', browser: 'Navegador', usage: 'Uso', appearance: 'Personalização', account: 'Conta' })[tab],
  title: (it) => it.title,
}

const item = (id: string, title: string, key: string | null, archived = false) => ({ id, title, issue: key ? { key } : null, archivedAt: archived ? '2026-01-01T00:00:00Z' : null }) as unknown as Item

const entries = paletteEntries([item('a', 'Corrigir login com SSO', 'PAY-12'), item('b', 'Exportar relatório de uso', 'PAY-120'), item('c', 'Item arquivado', 'OLD-1', true), item('d', 'Sem tarefa', null)], labels)

describe('paleta de comandos', () => {
  test('lista páginas, cada aba das configurações e os itens não arquivados', () => {
    expect(entries.filter((e) => e.kind === 'page').map((e) => e.to)).toEqual(['/', '/security', '/settings/connections', '/settings/applications', '/settings/permissions', '/settings/memory', '/settings/browser', '/settings/usage', '/settings/appearance', '/settings/account'])
    expect(entries.filter((e) => e.kind === 'item').map((e) => e.to)).toEqual(['/items/a', '/items/b', '/items/d'])
  })

  test('sem busca, devolve tudo na ordem', () => {
    expect(filterEntries(entries, '   ')).toEqual(entries)
  })

  test('ignora acentos e maiúsculas', () => {
    expect(fold('  Memória ')).toBe('memoria')
    expect(filterEntries(entries, 'SEGURANCA').map((e) => e.to)).toEqual(['/security'])
    expect(filterEntries(entries, 'conexoes').map((e) => e.to)).toEqual(['/settings/connections'])
  })

  test('acha item pelo título e pela chave da tarefa, a chave exata primeiro', () => {
    expect(filterEntries(entries, 'login sso').map((e) => e.to)).toEqual(['/items/a'])
    expect(filterEntries(entries, 'pay-12').map((e) => e.to)).toEqual(['/items/a', '/items/b'])
    expect(filterEntries(entries, 'pay-120').map((e) => e.to)).toEqual(['/items/b'])
  })

  test('quem começa com a busca vem antes de quem só a contém', () => {
    expect(filterEntries(entries, 'uso').map((e) => e.to)).toEqual(['/settings/usage', '/items/b'])
  })

  test('nada encontrado e limite', () => {
    expect(filterEntries(entries, 'zzz')).toEqual([])
    expect(filterEntries(entries, '', 3)).toHaveLength(3)
  })

  test('caminho do item escapa o id e a etapa', () => {
    expect(itemPath('a/b')).toBe('/items/a%2Fb')
    expect(itemPath('a', 'plan x')).toBe('/items/a/plan%20x')
  })
})
