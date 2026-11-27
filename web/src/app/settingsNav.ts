export type SettingsTab = 'connections' | 'browser' | 'permissions' | 'memory' | 'applications' | 'usage' | 'appearance' | 'account'

export type SettingsGroup = 'workspace' | 'agent' | 'preferences'

export const SETTINGS_GROUPS: readonly { key: SettingsGroup; tabs: readonly SettingsTab[] }[] = [
  { key: 'workspace', tabs: ['connections', 'applications', 'permissions', 'memory'] },
  { key: 'agent', tabs: ['browser', 'usage'] },
  { key: 'preferences', tabs: ['appearance', 'account'] },
]

export const SETTINGS_TABS: readonly SettingsTab[] = SETTINGS_GROUPS.flatMap((g) => g.tabs)

export function settingsTab(raw: string | undefined): SettingsTab {
  return SETTINGS_TABS.find((x) => x === raw) ?? 'connections'
}
