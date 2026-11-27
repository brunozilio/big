import { expect, test } from 'bun:test'
import { getState, setBranding, setLanguage } from '../src/api/store'
import { currentLanguage } from '../src/i18n'

test('sem branding, a língua escolhida na tela passa a valer', () => {
  const before = getState().branding
  setBranding(null as never)
  setLanguage('en')
  expect(currentLanguage()).toBe('en')
  setLanguage('es')
  expect(currentLanguage()).toBe('es')
  if (before) setBranding(before)
})
