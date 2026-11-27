export function basePath(): string {
  if (typeof document === 'undefined') return ''
  const href = document.querySelector('base')?.getAttribute('href') ?? '/'
  const path = href.replace(/\/+$/, '')
  return path.startsWith('/') && !path.startsWith('//') ? path : ''
}

export function at(path: string): string {
  return path.startsWith('/') && !path.startsWith('//') ? basePath() + path : path
}

export function appPath(): string {
  if (typeof window === 'undefined') return '/'
  const base = basePath()
  const path = window.location.pathname
  return base && path.startsWith(base) ? path.slice(base.length) || '/' : path
}

export function toLogin(): string {
  return at(`/login?next=${encodeURIComponent(appPath() + window.location.search)}`)
}
