export type SettingsTab = 'connections' | 'library' | 'browser' | 'permissions' | 'memory' | 'applications' | 'usage' | 'appearance' | 'account' | 'people'

export type SettingsGroup = 'workspace' | 'agent' | 'preferences'

export const SETTINGS_GROUPS: readonly { key: SettingsGroup; tabs: readonly SettingsTab[] }[] = [
  { key: 'workspace', tabs: ['connections', 'applications', 'permissions', 'memory'] },
  { key: 'agent', tabs: ['library', 'browser', 'usage'] },
  { key: 'preferences', tabs: ['appearance', 'account', 'people'] },
]

export const SETTINGS_TABS: readonly SettingsTab[] = SETTINGS_GROUPS.flatMap((g) => g.tabs)

const ADMIN_ONLY: readonly SettingsTab[] = ['permissions', 'memory', 'browser', 'library']

export function tabShown(tab: SettingsTab, shared: boolean, admin: boolean): boolean {
  if (tab === 'people') return shared && admin
  return admin || !ADMIN_ONLY.includes(tab)
}

export function settingsTab(raw: string | undefined): SettingsTab {
  return SETTINGS_TABS.find((x) => x === raw) ?? 'connections'
}
