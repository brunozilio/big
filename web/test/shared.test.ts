import { describe, expect, test } from 'bun:test'
import { isAdmin, isShared } from '../src/api/me'
import { tabShown } from '../src/app/settingsNav'
import { draftOf, editingOf, reduce } from '../src/screens/settings/applicationDraft'

describe('servidor compartilhado', () => {
  test('quem não é admin não vê as abas da empresa e só o admin vê as pessoas', () => {
    expect(tabShown('permissions', true, false)).toBe(false)
    expect(tabShown('usage', true, false)).toBe(true)
    expect(tabShown('people', true, true)).toBe(true)
    expect(tabShown('people', false, true)).toBe(false)
    expect(tabShown('permissions', false, true)).toBe(true)
  })

  test('rodando local todo mundo é admin', () => {
    expect(isAdmin(null)).toBe(true)
    expect(isAdmin({ mode: 'local', user: { user: 'local', email: '', name: '' }, admin: false })).toBe(true)
    expect(isAdmin({ mode: 'shared', user: { user: 'u', email: 'a@b.co', name: '' }, admin: false })).toBe(false)
    expect(isShared({ mode: 'shared', user: { user: 'u', email: 'a@b.co', name: '' }, admin: true })).toBe(true)
  })

  test('sem escolher pasta, um repositório vazio entra para receber o endereço', () => {
    const editing = reduce(editingOf(draftOf(null)), { type: 'blank' })
    expect(editing.repositories.length).toBe(2)
    expect(editing.repositories[1].value.directory).toBe('')
  })
})
