import type { Application, ApplicationDraft, CloudAccount, CloudChoice, CodeRepo, DirListing } from '../contract'

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
  if (!folders) throw new Error('Essa pasta não existe.')
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
      tracker: { connection: 'c-jira', project: 'APP' },
      repositories: [
        { id: 'r-checkout', code: { connection: 'c-github', repo: 'acme/checkout' }, directory: `${MOCK_HOME}/projetos/checkout`, run: ['bun dev'], verify: ['bun test', 'bun run lint', 'npx playwright test'] },
        { id: 'r-infra', code: { connection: 'c-github', repo: 'acme/infra' }, directory: `${MOCK_HOME}/projetos/infra`, run: [], verify: ['terraform validate'] },
      ],
    },
  ]
}

let counter = 0

export const CLOUD_ACCOUNTS: CloudAccount[] = [
  { id: '111122223333', name: 'acme-prod', email: 'ops@acme.com.br', roles: ['AdministratorAccess', 'Deploy'] },
  { id: '444455556666', name: 'acme-dev', email: 'dev@acme.com.br', roles: ['Developer'] },
]

function checkedCloud(chosen: CloudChoice | null | undefined): CloudChoice | null {
  if (!chosen) return null
  const picked = { connection: chosen.connection.trim(), account: chosen.account.trim(), role: chosen.role.trim() }
  return picked.connection && picked.account ? picked : null
}

export function checkedApplication(draft: ApplicationDraft, id: string): Application {
  const name = draft.name.trim()
  if (!name) throw new Error('Dê um nome à aplicação.')
  if (draft.repositories.length === 0) throw new Error('Adicione ao menos um repositório.')
  const repositories = draft.repositories.map((r) => {
    if (!r.directory.trim()) throw new Error('Escolha a pasta de cada repositório.')
    counter += 1
    return {
      id: r.id || `r-${Date.now()}-${counter}`,
      code: r.code && r.code.repo && r.code.connection ? r.code : null,
      directory: r.directory.trim(),
      run: r.run.map((v) => v.trim()).filter(Boolean),
      verify: r.verify.map((v) => v.trim()).filter(Boolean),
    }
  })
  const tracker = draft.tracker && draft.tracker.project && draft.tracker.connection ? draft.tracker : null
  return { id, name, tracker, cloud: checkedCloud(draft.cloud), repositories, skipDependabot: draft.skipDependabot !== false }
}
