import type { Application, ApplicationDraft, BindingOption, Bindings, CloudAccount, CodeBranch, CodeRepo, DirListing, OpenPull } from '../contract'
import { say } from './say'

export const MOCK_HOME = '/Users/voce'

interface Folder {
  name: string
  remote?: string
}

const TREE: Record<string, Folder[]> = {
  '/': [{ name: 'Users' }],
  '/Users': [{ name: 'voce' }],
  [MOCK_HOME]: [{ name: 'Documents' }, { name: 'Downloads' }, { name: 'projetos' }],
  [`${MOCK_HOME}/Documents`]: [{ name: 'notas' }],
  [`${MOCK_HOME}/Downloads`]: [],
  [`${MOCK_HOME}/Documents/notas`]: [],
  [`${MOCK_HOME}/projetos`]: [{ name: 'checkout', remote: 'acme/checkout' }, { name: 'infra', remote: 'acme/infra' }, { name: 'rascunhos' }, { name: 'web', remote: 'acme/web' }],
  [`${MOCK_HOME}/projetos/checkout`]: [{ name: 'src' }],
  [`${MOCK_HOME}/projetos/infra`]: [],
  [`${MOCK_HOME}/projetos/rascunhos`]: [],
  [`${MOCK_HOME}/projetos/web`]: [{ name: 'src' }],
  [`${MOCK_HOME}/projetos/checkout/src`]: [],
  [`${MOCK_HOME}/projetos/web/src`]: [],
}

export const CODE_REPOS: CodeRepo[] = ['checkout', 'web', 'infra'].map((name, i) => ({
  id: String(100 + i),
  ref: `acme/${name}`,
  name,
  url: `https://github.com/acme/${name}`,
  defaultBranch: 'main',
  private: true,
}))

function parentOf(path: string): string | null {
  if (path === '/') return null
  const cut = path.slice(0, path.lastIndexOf('/'))
  return cut || '/'
}

function join(path: string, name: string): string {
  return path === '/' ? `/${name}` : `${path}/${name}`
}

function remoteOf(path: string): string | null {
  const parent = parentOf(path)
  if (parent === null) return null
  return TREE[parent]?.find((f) => join(parent, f.name) === path)?.remote ?? null
}

export function listDirs(path: string | null): DirListing {
  const wanted = (path ?? MOCK_HOME).replace(/\/+$/, '') || '/'
  const folders = TREE[wanted]
  if (!folders) throw new Error(say('folderMissing'))
  const ref = remoteOf(wanted)
  return {
    path: wanted,
    parent: parentOf(wanted),
    home: MOCK_HOME,
    repo: ref !== null,
    remote: { url: ref ? `git@github.com:${ref}.git` : null, ref },
    entries: folders.map((f) => ({ name: f.name, path: join(wanted, f.name), repo: !!f.remote })),
  }
}

export function initialApplications(): Application[] {
  return [
    {
      id: 'app-checkout',
      name: 'Checkout',
      bindings: { tracker: { connection: 'c-jira', value: 'APP', label: 'App · APP' } },
      repositories: [
        { id: 'r-checkout', bindings: { code: { connection: 'c-github', value: 'acme/checkout', label: 'acme/checkout' }, security: { connection: 'c-veracode', value: 'a1b2c3d4-0000-4000-8000-000000000001', label: 'Checkout · a1b2c3d4' }, quality: { connection: 'c-sonar', value: 'acme_checkout', label: 'Checkout · acme_checkout' } }, directory: `${MOCK_HOME}/projetos/checkout`, run: ['bun dev'], verify: ['bun test', 'bun run lint'], e2e: ['npx cypress run'], e2eDirectory: 'e2e' },
        { id: 'r-infra', bindings: { code: { connection: 'c-github', value: 'acme/infra', label: 'acme/infra' } }, directory: `${MOCK_HOME}/projetos/infra`, run: [], verify: ['terraform validate'] },
      ],
      env: { CYPRESS_BASE_URL: 'http://localhost:3000' },
    },
  ]
}

let counter = 0

export const CLOUD_ACCOUNTS: CloudAccount[] = [
  { id: '111122223333', name: 'acme-prod', email: 'ops@acme.com.br', roles: ['AdministratorAccess', 'Deploy'] },
  { id: '444455556666', name: 'acme-dev', email: 'dev@acme.com.br', roles: ['Developer'] },
]

function checkedBindings(given: Bindings | undefined): Bindings {
  const kept: Bindings = {}
  for (const [capability, bound] of Object.entries(given ?? {})) {
    const connection = bound.connection.trim()
    const value = bound.value.trim()
    const fields = Object.keys(bound.fields ?? {}).length > 0 ? { fields: bound.fields } : {}
    const profiles = (bound.profiles ?? []).length > 0 ? { profiles: bound.profiles } : {}
    if (capability && connection && (value || fields.fields || profiles.profiles)) kept[capability] = { connection, value, ...(bound.label ? { label: bound.label } : {}), ...fields, ...profiles }
  }
  return kept
}

export function cloudOptions(): BindingOption[] {
  return CLOUD_ACCOUNTS.flatMap((a) => a.roles.map((role) => ({ value: `${a.id}/${role}`, label: `${a.name} · ${role}` })))
}

export function checkedApplication(draft: ApplicationDraft, id: string): Application {
  const name = draft.name.trim()
  if (!name) throw new Error(say('appNameRequired'))
  if (draft.repositories.length === 0) throw new Error(say('addRepository'))
  const repositories = draft.repositories.map((r) => {
    if (!r.directory.trim()) throw new Error(say('chooseFolder'))
    counter += 1
    return {
      id: r.id || `r-${Date.now()}-${counter}`,
      directory: r.directory.trim(),
      run: r.run.map((v) => v.trim()).filter(Boolean),
      verify: r.verify.map((v) => v.trim()).filter(Boolean),
      e2e: (r.e2e ?? []).map((v) => v.trim()).filter(Boolean),
      e2eDirectory: (r.e2eDirectory ?? '').trim().replace(/^\/+|\/+$/g, ''),
      bindings: checkedBindings(r.bindings),
    }
  })
  const env = Object.fromEntries(Object.entries(draft.env ?? {}).map(([k, v]) => [k.trim(), v] as const).filter(([k]) => k))
  return { id, name, bindings: checkedBindings(draft.bindings), repositories, env }
}

export function openPulls(repo: string): OpenPull[] {
  return [
    { key: `${repo}#42`, number: 42, title: say('pullFreight'), head: 'fix/frete', base: 'main', author: 'ana', url: null, draft: false },
    { key: `${repo}#38`, number: 38, title: say('pullOnePage'), head: 'feat/checkout', base: 'main', author: 'bia', url: null, draft: true },
  ]
}

export function codeBranches(): CodeBranch[] {
  return ['main', 'develop', 'release/2.4', 'fix/frete', 'feat/checkout'].map((name) => ({ name, default: name === 'main' }))
}
