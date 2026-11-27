import type { LibraryStep, Library, LibraryAssist, LibraryCard, LibraryChange, LibraryDraft, LibraryField, LibraryItem, LibraryTurn, LibraryValue } from '../contract'

interface Kept {
  section: string
  path: string
  title: string
  description: string
  avatar: LibraryCard['avatar']
  origin: LibraryCard['origin']
  files: string[]
  values: Record<string, LibraryValue>
  body: string | null
  source: string
}

const SECTIONS = [
  { key: 'agents', label: 'Agentes', hint: 'Quem faz o trabalho em cada etapa', icon: 'bot', creatable: true },
  { key: 'skills', label: 'Skills', hint: 'Conhecimento que os agentes carregam quando precisam', icon: 'idea', creatable: true },
  { key: 'prompts', label: 'Prompts', hint: 'Os textos que o sistema envia ao modelo', icon: 'note', creatable: false },
  { key: 'workflows', label: 'Workflows', hint: 'As etapas que um item percorre', icon: 'workflow', creatable: true },
]

const TOOLS = ['Read', 'Grep', 'Glob', 'Bash', 'Edit', 'Write', 'WebFetch', 'WebSearch', 'Agent', 'mcp__*']
const SKILLS = ['verify', 'simplify', 'run', 'code-review']

function agent(name: string, label: string, description: string, shape: string, color: string, tools: string[], body: string): Kept {
  return { section: 'agents', path: `agents/${name}.ts`, title: label, description, avatar: { shape, color }, origin: 'shipped', files: [], values: { label, description, tools, skills: ['verify'], permissionMode: 'auto', 'avatar.color': color, 'avatar.shape': shape, disallowedTools: [], observer: [], model: '' }, body, source: '' }
}

function skill(name: string, description: string, body: string, files: string[] = []): Kept {
  return { section: 'skills', path: `skills/${name}/SKILL.md`, title: name, description, avatar: null, origin: 'shipped', files, values: { name, description, 'disable-model-invocation': false }, body, source: '' }
}

function code(section: string, path: string, title: string, description: string, source: string): Kept {
  return { section, path, title, description, avatar: null, origin: 'shipped', files: [], values: {}, body: null, source }
}

const WORKFLOW = `import { workflow, on } from "sdk";
import { plan, develop, checks, pr, review, merge } from "sdk/steps";

export default workflow({
  name: "entrega",
  accepts: ["issue", "text"],
  triggers: [on("tracker.issue.created")],
  steps: [
    plan({ agent: "planner", status: { category: "todo" } }),
    develop({ agent: "developer", status: { category: "doing" } }),
    checks({ agent: "verifier", onFail: "develop", retries: 3 }),
    pr(),
    review({ agent: "reviewer", onChanges: "develop" }),
    merge({ method: "squash" }),
  ],
});
`

function initial(): Kept[] {
  return [
    agent('planner', 'Planner', 'Turns an item into a delivery plan the Developer can carry out.', 'triangle', '#e94b9b', ['Read', 'Grep', 'Glob'], 'You are the Planner.\n\n## What you receive\n- The item: title, description and acceptance criteria.\n\n## How to work\n1. **Understand the request.**\n2. **Actually read the code.**\n3. **Write the plan.**\n'),
    agent('developer', 'Developer', "Implements an item's plan on its branch, with tests, until the checks pass.", 'squircle', '#3b82f6', ['Read', 'Grep', 'Glob', 'Bash', 'Edit', 'Write'], 'You are the Developer.\n\n## How to work\n1. **Read before changing.**\n2. **Change in small steps.**\n3. **Test.**\n'),
    agent('reviewer', 'Reviewer', 'Reviews the change like a careful colleague and asks for what is missing.', 'circle', '#4fb39c', ['Read', 'Grep', 'Glob'], 'You are the Reviewer.\n'),
    skill('verify', 'Verify that a change does what it should by driving the real flow.', '**Verification is runtime observation.**\n\n## Find the change\n\n```bash\ngit diff --stat\n```\n', ['skills/verify/examples/cli.md']),
    skill('code-review', 'Review the current changes for correctness bugs.', '# Code review\n'),
    { ...code('prompts', 'prompts/chore-title.js', 'chore-title', '', ''), body: 'You are naming a coding session so the user can pick it out of a long list.\n' },
    code('prompts', 'prompts/system.js', 'system', '', 'const role = `You are an interactive agent.`;\n\nexport default function ({ tools }) {\n  return [role, tools].join("\\n\\n");\n}\n'),
    code('workflows', 'workflows/default.ts', 'entrega', 'plan → develop → checks → pr → review → merge', WORKFLOW),
  ]
}

function fieldsOf(kept: Kept): LibraryField[] {
  const value = (key: string) => kept.values[key]
  if (kept.section === 'agents') {
    return [
      { key: 'label', label: 'Nome', kind: 'text', value: value('label'), required: true, max: 64 },
      { key: 'description', label: 'Descrição', kind: 'area', value: value('description'), max: 1024 },
      { key: 'tools', label: 'Ferramentas', kind: 'chips', value: value('tools'), options: TOOLS },
      { key: 'skills', label: 'Skills', kind: 'chips', value: value('skills'), options: SKILLS },
      { key: 'permissionMode', label: 'Modo', kind: 'select', value: value('permissionMode'), options: ['auto', 'plan'] },
      { key: 'avatar.color', label: 'Cor', kind: 'color', value: value('avatar.color') },
      { key: 'avatar.shape', label: 'Forma', kind: 'select', value: value('avatar.shape'), options: ['circle', 'squircle', 'triangle', 'cloud', 'drop', 'pill'] },
      { key: 'disallowedTools', label: 'Ferramentas bloqueadas', kind: 'chips', value: value('disallowedTools'), options: TOOLS, more: true },
      { key: 'observer', label: 'Observadores', kind: 'chips', value: value('observer'), options: ['observer'], more: true },
      { key: 'model', label: 'Modelo', kind: 'select', value: value('model'), options: ['', 'inherit'], from: 'models', more: true },
    ]
  }
  if (kept.section === 'skills') {
    return [
      { key: 'name', label: 'Nome', kind: 'text', value: value('name'), required: true, max: 64, pattern: '^[a-z0-9]+(-[a-z0-9]+)*$', hint: 'Minúsculas, dígitos e hífens simples', readOnly: true },
      { key: 'description', label: 'Quando usar', kind: 'area', value: value('description'), required: true, max: 1024 },
      { key: 'disable-model-invocation', label: 'Só quando chamada pelo nome', kind: 'switch', value: value('disable-model-invocation') },
    ]
  }
  return []
}

const text = (value: LibraryValue | undefined, fallback: string) => (typeof value === 'string' && value ? value : fallback)

export function stepsOf(source: string): LibraryStep[] {
  const lines = source.split('\n')
  const from = lines.findIndex((line) => /^\s*steps:\s*\[\s*$/.test(line))
  if (from < 0) return []
  const found: LibraryStep[] = []
  for (let at = from + 1; at < lines.length && !/^\s*\],?\s*$/.test(lines[at]); at += 1) {
    const call = /^\s*([A-Za-z_]\w*)\((.*)\),?\s*$/.exec(lines[at])
    if (!call) continue
    const back = /\bon(?:Fail|Changes):\s*"([^"]+)"/.exec(call[2])?.[1]
    const retries = /\bretries:\s*(\d+)/.exec(call[2])?.[1]
    found.push({ line: at + 1, name: call[1], agent: /\bagent:\s*"([^"]+)"/.exec(call[2])?.[1] ?? null, notes: back ? [`↩ ${back}${retries ? ` ×${retries}` : ''}`] : [] })
  }
  return found
}

export function mockLibrary() {
  let kept = initial()
  const extras = new Map<string, string>([['skills/verify/examples/cli.md', '# CLI\n\nRun the command and read what it prints.\n']])
  const find = (path: string) => kept.find((item) => item.path === path)
  const owner = (path: string) => kept.find((item) => item.path === path || item.files.includes(path))
  const missing = () => new Error('Esse item não existe.')

  const shown = (path: string): LibraryItem => {
    const item = owner(path)
    if (!item) throw missing()
    const base = { item: item.path, section: item.section, title: item.title, avatar: item.avatar, files: item.files, editable: true, assist: { ideas: ['Deixe mais curto', 'Revise e aponte ambiguidades', 'Explique o que isto faz'] } }
    if (path !== item.path) return { ...base, outline: null, path, origin: 'shipped', source: extras.get(path) ?? '', mode: 'code', language: 'markdown', fields: [], body: null }
    const form = item.body !== null
    return { ...base, outline: item.section === 'workflows' ? stepsOf(item.source) : null, path, origin: item.origin, source: item.source, mode: form ? 'form' : 'code', language: path.endsWith('.md') ? 'markdown' : path.endsWith('.ts') ? 'typescript' : 'javascript', fields: fieldsOf(item), body: form ? { label: item.section === 'skills' ? 'Instruções' : 'Prompt', text: item.body ?? '', language: 'markdown' } : null }
  }

  const list = (): Library => ({
    editable: true,
    sections: SECTIONS.map((section) => ({
      ...section,
      items: kept.filter((item) => item.section === section.key).map(({ path, title, description, avatar, origin, files }) => ({ path, title, description, avatar, origin, files })),
    })),
  })

  const save = (path: string, change: LibraryChange): LibraryItem => {
    const item = find(path)
    if (!item) {
      if (!owner(path) || !('source' in change)) throw missing()
      extras.set(path, change.source)
      return shown(path)
    }
    const origin = item.origin === 'own' ? 'own' : 'edited'
    if ('source' in change) kept = kept.map((it) => (it === item ? { ...item, source: change.source, origin } : it))
    else {
      const values = { ...item.values, ...change.values }
      const avatar = item.avatar ? { shape: text(values['avatar.shape'], item.avatar.shape ?? ''), color: text(values['avatar.color'], item.avatar.color ?? '') } : null
      const next: Kept = { ...item, values, avatar, origin, body: change.body ?? item.body, title: text(values.label ?? values.name, item.title), description: text(values.description, '') }
      kept = kept.map((it) => (it === item ? next : it))
    }
    return shown(path)
  }

  const create = (section: string, name: string): LibraryItem => {
    const slug = name.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '')
    if (!slug) throw new Error('O nome precisa de uma letra ou um número.')
    const made = section === 'agents' ? agent(slug, name.trim(), '', 'circle', '#8b5cf6', ['Read', 'Grep', 'Glob'], `You are ${name.trim()}.\n`) : section === 'skills' ? skill(slug, name.trim(), `# ${name.trim()}\n`) : code(section, `${section}/${slug}.ts`, slug, '', section === 'workflows' ? WORKFLOW : '')
    if (find(made.path)) throw new Error('Já existe um item com esse nome.')
    kept = [...kept, { ...made, origin: 'own' }]
    return shown(made.path)
  }

  const restore = (path: string) => {
    const item = find(path)
    if (!item) throw missing()
    const original = initial().find((it) => it.path === path)
    kept = original ? kept.map((it) => (it === item ? original : it)) : kept.filter((it) => it !== item)
  }

  const assist = (path: string, draft: LibraryDraft, messages: LibraryTurn[]): LibraryAssist => {
    const item = shown(path)
    const asked = messages[messages.length - 1]?.text ?? ''
    const text = item.mode === 'form' ? (draft.body ?? '') : draft.source
    if (/\?$|expli|revis/i.test(asked.trim())) return { reply: 'Este item está claro. Eu só deixaria explícito o que fazer quando os testes falham.', values: {}, body: null, source: null }
    const shorter = `${text.split('\n\n')[0].trim()}\n`
    const changed = shorter === text ? `${text.trimEnd()}\n\nSeja direto.\n` : shorter
    return { reply: 'Pronto: enxuguei o texto e mantive a primeira parte.', values: {}, body: item.mode === 'form' ? changed : null, source: item.mode === 'code' ? changed : null }
  }

  const createFile = (itemPath: string, name: string): LibraryItem => {
    const item = find(itemPath)
    const folder = itemPath.slice(0, itemPath.lastIndexOf('/') + 1)
    const path = `${folder}${name.trim()}`
    if (!item || folder.split('/').length < 3 || !name.trim()) throw missing()
    if (owner(path)) throw new Error('Já existe um item com esse nome.')
    kept = kept.map((it) => (it === item ? { ...item, files: [...item.files, path] } : it))
    extras.set(path, '')
    return shown(path)
  }

  const act = (path: string, action: string): LibraryItem => {
    shown(path)
    throw new Error(`Não foi salvo: ${path} não tem a ação ${action}`)
  }

  return { list, shown, save, create, restore, assist, createFile, act }
}
