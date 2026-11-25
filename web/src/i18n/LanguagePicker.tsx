import { api } from '@/api'
import type { Language } from '@/api/contract'
import { setBranding } from '@/api/store'
import { Select } from '@/ui/form'
import { Icon } from '@/ui/icon'
import { isLanguage, LANGUAGES, useLanguage, useT } from '.'
import { applyLanguage } from './sync'

export function saveLanguage(language: Language) {
  applyLanguage(language)
  return api()
    .saveBranding({ language })
    .then(setBranding)
}

export function LanguagePicker({ className }: { className?: string }) {
  const t = useT()
  const language = useLanguage()
  return (
    <Select
      size="sm"
      label={t('common.language')}
      className={className}
      value={language}
      leading={<Icon name="translate" size={14} className="text-muted" />}
      options={LANGUAGES.map((l) => ({ value: l.value, label: l.label }))}
      onValueChange={(value) => {
        if (isLanguage(value)) saveLanguage(value).catch(() => undefined)
      }}
    />
  )
}
