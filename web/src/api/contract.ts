export type Iso = string

export type BoardGroup = 'you' | 'running' | 'delivered'
export const BOARD_GROUPS: BoardGroup[] = ['you', 'running', 'delivered']

export type StepState = 'pending' | 'running' | 'waiting_human' | 'passed' | 'failed' | 'skipped'
export const STEP_STATES: StepState[] = ['pending', 'running', 'waiting_human', 'passed', 'failed', 'skipped']

export type Tone = 'neutral' | 'info' | 'success' | 'warning' | 'danger' | 'accent'

export type AgentShape = 'circle' | 'squircle' | 'triangle' | 'cloud' | 'drop' | 'pill'

export type AvatarLook = { kind: 'agent'; shape: AgentShape; color: string } | { kind: 'image'; url: string }

export interface Person {
  id: string
  name: string
  avatar: AvatarLook | null
  you?: boolean
}

export interface ProviderMark {
  provider: string
  name: string
  logo: string
}

export interface ExternalRef extends ProviderMark {
  connection: string
  key: string
  url: string | null
}

export type ChecksState = 'pending' | 'running' | 'passed' | 'failed'

export interface ChangeRef extends ExternalRef {
  checks: ChecksState | null
  merged: boolean
}

export interface DiffSummary {
  files: number
  additions: number
  deletions: number
  url: string | null
}

export interface Impediment {
  reason: string
  connect: ProviderMark | null
}

export interface Attention {
  badge: { text: string; tone: Tone; icon?: string } | null
  diff: DiffSummary | null
  action: { label: string; icon?: string; step: string } | null
}

export interface Activity {
  text: string
  since: Iso
  tone?: 'danger'
}

export interface ItemStep {
  id: string
  label: string
  icon: string
  logo?: string
  state: StepState
  attempts: number
  seconds: number
}

export interface Item {
  id: string
  title: string
  workflow: string
  group: BoardGroup
  status: string | null
  issue: ExternalRef | null
  change: ChangeRef | null
  steps: ItemStep[]
  current: string | null
  live: boolean
  activity: Activity | null
  impediment: Impediment | null
  attention: Attention | null
  createdAt: Iso
  updatedAt: Iso
  doneAt: Iso | null
}

export interface NewItem {
  text: string
  issue: { connection: string; key: string } | null
  project?: { connection: string; key: string } | null
  model: string
  effort: Effort
  fast: boolean
  attachments: string[]
}

export interface Attempt {
  n: number
  state: StepState
  startedAt: Iso
  endedAt: Iso | null
}

export interface StepView {
  item: string
  step: string
  attempt: number
  attempts: Attempt[]
  blocks: Block[]
}

export type CommandState = 'todo' | 'running' | 'done' | 'failed'

export interface Command {
  name: string
  state: CommandState
  seconds: number | null
  log: string
}

export interface FormField {
  name: string
  label: string
  kind: 'text' | 'textarea' | 'select'
  value?: string
  placeholder?: string
  options?: { value: string; label: string }[]
  mono?: boolean
}

export type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger'

export interface ActionButton {
  action: string
  label: string
  icon?: string
  variant?: ButtonVariant
  payload?: unknown
}

export interface DetailRow {
  label: string
  text: string
  person?: Person
  mono?: boolean
  url?: string
  note?: string
}

export type MergeMethod = 'squash' | 'merge' | 'rebase'

export type Block =
  | { type: 'chat'; session: string; composer: boolean; title?: string }
  | { type: 'issue'; connection: string; key: string; comment: boolean }
  | { type: 'change'; connection: string; change: string }
  | { type: 'review'; connection: string; change: string; editable: boolean; session: string | null; submit: string }
  | { type: 'pipeline'; connection: string; run: string; gate: { approve: string; reject: string } | null }
  | { type: 'merge'; connection: string; change: string; ready: boolean; methods: MergeMethod[]; action: string; checks: DetailRow[]; result: DetailRow[] | null }
  | { type: 'markdown'; text: string; muted?: boolean }
  | { type: 'logs'; commands: Command[] }
  | { type: 'form'; title?: string; fields: FormField[]; submit: { action: string; label: string } }
  | { type: 'actions'; buttons: ActionButton[] }

export type BlockType = Block['type']

export interface HumanInput {
  action: string
  payload: unknown
}

export interface Comment {
  id: string
  author: Person
  body: string
  at: Iso
  kind: 'comment' | 'approved' | 'changes'
  file?: string
  line?: number
}

export interface IssueSummary {
  key: string
  title: string
  status: string | null
}

export interface TrackerProject {
  id: string
  key: string
  name: string
  avatar: string | null
}

export interface IssueData {
  key: string
  url: string | null
  title: string
  status: string | null
  assignee: Person | null
  description: string
  attachments: { name: string; size: string; url?: string }[]
  subtasks: { key: string; title: string; done: boolean; byAgent: boolean }[]
  comments: Comment[]
}

export interface FileDiff {
  path: string
  additions: number
  deletions: number
  diff: string
  firstLine: number
}

export interface ChangeData {
  key: string
  number: number
  title: string
  url: string | null
  state: 'open' | 'merged' | 'closed'
  author: Person
  head: string
  base: string
  body: string
  files: FileDiff[]
  comments: Comment[]
}

export type JobState = 'queued' | 'running' | 'passed' | 'failed' | 'skipped' | 'waiting' | 'rejected'

export interface Job {
  id: string
  name: string
  stage: number
  needs: string[]
  state: JobState
  seconds: number | null
  condition: string | null
  gate: string | null
  waitingSince: Iso | null
  steps: Command[]
}

export interface PipelineRun {
  id: string
  title: string
  file: string
  number: number
  event: string
  ref: string
  url: string | null
  state: JobState
  jobs: Job[]
}

export type ConnectionCall =
  | { action: 'tracker.search'; args: { query: string; project?: string }; result: IssueSummary[] }
  | { action: 'tracker.projects'; args: { query?: string }; result: TrackerProject[] }
  | { action: 'tracker.issue'; args: { key: string }; result: IssueData }
  | { action: 'tracker.comment'; args: { key: string; body: string }; result: Comment }
  | { action: 'code.change'; args: { change: string }; result: ChangeData }
  | { action: 'ci.run'; args: { run: string }; result: PipelineRun }

export type CallAction = ConnectionCall['action']
export type CallArgs<A extends CallAction> = Extract<ConnectionCall, { action: A }>['args']
export type CallResult<A extends CallAction> = Extract<ConnectionCall, { action: A }>['result']

export type SessionStatus = 'idle' | 'running' | 'waiting'
export type SessionMode = 'ask' | 'accept_edits' | 'plan' | 'bypass'

export interface Agent {
  id: string
  name: string
  avatar: AvatarLook | null
}

export interface ThinkingStep {
  text: string
  done: boolean
}

export type ToolKind = 'command' | 'edit' | 'read' | 'search' | 'call' | 'subagent'

export interface ToolCall {
  id: string
  kind: ToolKind
  title: string
  tag: string
  logo?: string
  state: 'running' | 'ok' | 'error'
  input?: unknown
  output?: string
  diff?: string
  durationMs: number
}

export interface AskQuestion {
  question: string
  header: string
  multiSelect: boolean
  options: { label: string; description?: string }[]
}

export type PermissionDecision = 'allowed' | 'always' | 'denied'

export type SessionEvent =
  | { id: string; at: Iso; type: 'user'; author: Person; text: string; attachments: number }
  | { id: string; at: Iso; type: 'thinking'; seconds: number; steps: ThinkingStep[]; live: boolean }
  | { id: string; at: Iso; type: 'tools'; calls: ToolCall[]; seconds: number; live: boolean; agent?: string }
  | { id: string; at: Iso; type: 'assistant'; text: string; agent?: string }
  | { id: string; at: Iso; type: 'status'; status: 'info' | 'error'; text: string }
  | { id: string; at: Iso; type: 'ask'; questions: AskQuestion[]; answers: string[] | null; agent?: string }
  | { id: string; at: Iso; type: 'permission'; tool: string; detail: string; risk: 'low' | 'medium' | 'high'; logo?: string; resolution: 'pending' | PermissionDecision }
  | { id: string; at: Iso; type: 'plan'; text: string; resolution: 'pending' | 'approved' | 'revised'; feedback?: string; agent?: string }

export type SessionEventType = SessionEvent['type']

export interface Session {
  id: string
  title: string
  status: SessionStatus
  mode: SessionMode
  model: string
  agents: Agent[]
  events: SessionEvent[]
}

export interface NewMessage {
  text: string
  model: string | null
  effort: Effort
  fast: boolean
  attachments: string[]
}

export type AskResponse =
  | { kind: 'ask'; answers: string[] }
  | { kind: 'permission'; decision: PermissionDecision }
  | { kind: 'plan'; decision: 'approve' | 'revise'; feedback?: string }

export type SignInKind = 'oauth' | 'device' | 'sso' | 'browser'

export interface SignInAlternative {
  key: string
  label: string
  button: string
  site: string
  signIn: SignInKind
  fields: FormField[]
}

export interface Provider {
  key: string
  name: string
  group: string
  description: string
  logo: string
  button?: string
  site?: string
  signIn: SignInKind
  fields: FormField[]
  multiple: boolean
  alternative?: SignInAlternative | null
  ai?: boolean
}

export interface Connection {
  id: string
  provider: string
  label: string
  account: string
  capabilities: string[]
  status: 'ok' | 'error'
}

export interface Connections {
  providers: Provider[]
  connections: Connection[]
}

export type SignInStart =
  | { flow: string; kind: 'redirect'; url: string; paste: boolean }
  | { flow: string; kind: 'device'; url: string; code: string; expiresIn: number; interval: number }

export interface SignInPollRequest {
  flow: string
  code?: string
  choice?: string
}

export type SignInPoll =
  | { state: 'pending' }
  | { state: 'choose'; title: string; choices: { id: string; label: string; detail: string }[] }
  | { state: 'done'; connection: Connection }
  | { state: 'failed'; message: string }

export const EFFORTS = ['low', 'medium', 'high', 'xhigh', 'max'] as const

export type Effort = (typeof EFFORTS)[number]

export interface Model {
  id: string
  name: string
  provider: string
  logo: string
  description?: string
  efforts: Effort[]
  fast: boolean
}

export interface Models {
  models: Model[]
  default: string | null
}

export const AUTO_MODEL = 'auto'

export type Theme = 'system' | 'light' | 'dark'

export interface Branding {
  name: string
  logo: string | null
  theme: Theme
}

export interface BrandingPatch {
  logo?: string | null
  theme?: Theme
}

export type LiveEvent =
  | { type: 'item'; item: Item }
  | { type: 'item.removed'; id: string }
  | { type: 'session'; session: string; event: SessionEvent }
  | { type: 'session.status'; session: string; status: SessionStatus }
  | { type: 'connections'; connections: Connections }
  | { type: 'branding'; branding: Branding }
  | { type: 'resync' }

export interface Api {
  items(): Promise<Item[]>
  createItem(input: NewItem): Promise<Item>
  rerun(item: string, step: string | null): Promise<void>
  cancel(item: string): Promise<void>
  step(item: string, step: string, attempt: number | null): Promise<StepView>
  input(item: string, step: string, input: HumanInput): Promise<void>
  call<A extends CallAction>(connection: string, action: A, args: CallArgs<A>): Promise<CallResult<A>>
  session(id: string): Promise<Session>
  send(session: string, message: NewMessage): Promise<void>
  respond(session: string, event: string, response: AskResponse): Promise<void>
  stop(session: string): Promise<void>
  connections(): Promise<Connections>
  startSignIn(provider: string, fields: Record<string, string>, method?: string): Promise<SignInStart>
  pollSignIn(request: SignInPollRequest): Promise<SignInPoll>
  disconnect(connection: string): Promise<void>
  models(): Promise<Models>
  branding(): Promise<Branding>
  saveBranding(patch: BrandingPatch): Promise<Branding>
  upload(file: File): Promise<{ id: string; name: string; size: number }>
  live(onEvent: (event: LiveEvent) => void, onState: (connected: boolean) => void): () => void
}

export const ROUTES = {
  items: '/api/items',
  item: (id: string) => `/api/items/${encodeURIComponent(id)}`,
  rerun: (id: string) => `/api/items/${encodeURIComponent(id)}/rerun`,
  cancel: (id: string) => `/api/items/${encodeURIComponent(id)}/cancel`,
  step: (id: string, step: string) => `/api/items/${encodeURIComponent(id)}/steps/${encodeURIComponent(step)}`,
  input: (id: string, step: string) => `/api/items/${encodeURIComponent(id)}/steps/${encodeURIComponent(step)}/input`,
  call: (connection: string) => `/api/connections/${encodeURIComponent(connection)}/call`,
  session: (id: string) => `/api/sessions/${encodeURIComponent(id)}`,
  messages: (id: string) => `/api/sessions/${encodeURIComponent(id)}/messages`,
  respond: (id: string) => `/api/sessions/${encodeURIComponent(id)}/respond`,
  stop: (id: string) => `/api/sessions/${encodeURIComponent(id)}/stop`,
  connections: '/api/connections',
  connection: (id: string) => `/api/connections/${encodeURIComponent(id)}`,
  start: (provider: string) => `/api/connections/${encodeURIComponent(provider)}/start`,
  poll: '/api/connections/device/poll',
  models: '/api/models',
  branding: '/api/branding',
  attachments: '/api/attachments',
  live: '/api/live',
} as const
