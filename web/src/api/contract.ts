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
  agent?: boolean
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

export interface Said {
  key: string
  vars?: Record<string, string | number>
}

export interface Impediment {
  reason: string
  said?: Said | null
  detail?: string
  connect: ProviderMark | null
}

export interface Attention {
  badge: { text: string; tone: Tone; icon?: string; said?: Said | null } | null
  diff: DiffSummary | null
  action: { label: string; icon?: string; step: string; said?: Said | null } | null
}

export interface Activity {
  text: string
  said?: Said | null
  since: Iso
  tone?: 'danger'
}

export interface ItemStep {
  id: string
  label: string
  icon: string
  logo?: string
  requires?: string
  parent?: string
  depth?: number
  provider?: string
  state: StepState
  attempts: number
  seconds: number
}

export type SourceKind = 'issue' | 'pull_request' | 'branch'

export interface ItemSource {
  kind: SourceKind | (string & {})
  key: string
  connection: string | null
  repository: string | null
  provider: string | null
  name: string | null
  logo: string | null
  url: string | null
}

export interface ItemGit {
  repository: string
  name: string
  base: string | null
  head: string | null
}

export interface NewSource {
  kind: SourceKind
  connection: string | null
  subject: string
  repository?: string
}

export type NewGit = Record<string, { base?: string; head?: string }>

export interface Item {
  id: string
  title: string
  untitled?: boolean
  workflow: string
  group: BoardGroup
  status: string | null
  issue: ExternalRef | null
  change: ChangeRef | null
  sources: ItemSource[]
  git: ItemGit[]
  steps: ItemStep[]
  current: string | null
  live: boolean
  activity: Activity | null
  impediment: Impediment | null
  attention: Attention | null
  recap: string | null
  createdAt: Iso
  updatedAt: Iso
  doneAt: Iso | null
  archivedAt: Iso | null
  links?: string[]
  trail?: StageView[] | null
  accepting?: boolean
  joinable?: boolean
}

export interface NewItem {
  application?: string | null
  text: string
  issue: { connection: string; key: string } | null
  sources?: NewSource[]
  git?: NewGit
  workflow?: string
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
  headsUp: HeadsUp | null
}

export interface HeadsUp {
  title: string
  explain: string
  urgent: boolean
}

export type CommandState = 'todo' | 'running' | 'done' | 'failed'

export interface Command {
  name: string
  state: CommandState
  seconds: number | null
  log: string
  session?: string | null
  files?: EvidenceFile[]
}

export interface EvidenceFile {
  id: string
  name: string
  size?: number
}

export interface FormField {
  name: string
  label: string
  kind: 'text' | 'textarea' | 'select'
  value?: string
  placeholder?: string
  options?: { value: string; label: string }[]
  mono?: boolean
  secret?: boolean
  help?: string
  optional?: boolean
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

export interface TestCaseView {
  title: string
  kind: 'api' | 'ui'
  objective: string | null
  ref: string | null
  url: string | null
  given: string[]
  when: string[]
  then: string[]
}

export type MergeMethod = 'squash' | 'merge' | 'rebase'

export type FindingKind = 'static' | 'dependency' | 'dynamic' | 'manual'

export type FindingChoice = 'fix' | 'task' | 'ignore'

export type IgnoreReason = 'false_positive' | 'accepted_risk'

export interface LinkRef {
  key: string
  url: string | null
}

export interface FindingDecision {
  choice: FindingChoice
  reason: IgnoreReason | null
  task: LinkRef | null
  pr: LinkRef | null
}

export interface Finding {
  id: string
  title: string
  severity: number
  kind: FindingKind
  new: boolean
  policy: boolean
  cwe: { id: string; name: string } | null
  cve: { id: string; cvss: number | null } | null
  file: string | null
  line: number | null
  component: string | null
  version: string | null
  fixedIn: string | null
  description: string
  held: LinkRef | null
  suggested: FindingChoice
  decision: FindingDecision | null
  url: string | null
}

export type QualityGrade = 'A' | 'B' | 'C' | 'D' | 'E'

export interface QualityCondition {
  metric: string
  label?: string
  value: string
  threshold: string
  passed: boolean
}

export interface QualityIssue {
  key: string
  type: 'bug' | 'vulnerability' | 'code_smell' | 'hotspot'
  severity: 'blocker' | 'critical' | 'major' | 'minor' | 'info'
  message: string
  file: string | null
  line: number | null
  url: string | null
}

export interface QualityReport {
  gate: 'passed' | 'failed'
  ratings: { reliability: QualityGrade; security: QualityGrade; maintainability: QualityGrade }
  counts: { bugs: number; vulnerabilities: number; smells: number; hotspots: number; hotspotsReviewed: number }
  coverage: number | null
  duplication: number | null
  conditions: QualityCondition[]
  issues: QualityIssue[]
  total: number
}

export type Block =
  | { type: 'chat'; session: string; composer: boolean; title?: string }
  | { type: 'issue'; connection: string; key: string; comment: boolean }
  | { type: 'change'; connection: string; change: string }
  | { type: 'review'; connection: string; change: string; editable: boolean; session: string | null; submit: string }
  | { type: 'pipeline'; connection: string; run: string; gate: { approve: string; reject: string } | null }
  | { type: 'merge'; connection: string; change: string; ready: boolean; methods: MergeMethod[]; action: string; checks: DetailRow[]; result: DetailRow[] | null }
  | { type: 'markdown'; text: string; muted?: boolean }
  | { type: 'cases'; cases: TestCaseView[]; session: string | null; editable: boolean; approve: string | null; changes: string | null }
  | { type: 'logs'; commands: Command[] }
  | { type: 'calls'; calls: ToolCall[] }
  | { type: 'form'; title?: string; fields: FormField[]; submit: { action: string; label: string } }
  | { type: 'actions'; buttons: ActionButton[] }
  | { type: 'findings'; connection: string; project: string; scannedAt: Iso | null; url: string | null; triage: string | null; findings: Finding[] }
  | { type: 'quality'; connection: string; project: string; branch: string; analyzedAt: Iso | null; url: string | null; report: QualityReport }
  | { type: 'stages'; stages: StageView[]; live: boolean }
  | { type: 'members'; items: MemberView[]; removable: boolean }
  | { type: 'tasks'; tasks: ChangeTaskView[]; kinds: TaskKindView[]; locked: boolean; environment: string | null; owners?: TaskOwner[] }

export type BlockType = Block['type']

export interface StageAction {
  id: string
  label: string
  icon?: string
  variant?: string
  payload?: unknown
  disabled?: boolean
}

export interface StageView {
  id: string
  label: string
  short: string
  icon: string
  state: StepState
  gate: 'none' | 'merge' | 'change'
  window: string | null
  picker: boolean
  change: { key: string; status: string; label: string; url: string | null } | null
  busy: string | null
  actions: StageAction[]
}

export interface MemberView {
  id: string
  title: string
  issue: string | null
}

export interface TaskFile {
  name: string
  size: number
}

export interface TaskOwner {
  value: string
  label: string
}

export interface ChangeTaskView {
  id: string
  kind: string
  title: string
  detail: string
  owner: string | null
  link: string | null
  start: string
  end: string
  files: TaskFile[]
  service?: string
  template?: string
}

export interface TaskKindView {
  id?: string
  kind: string
  label?: string
  icon: string
  tone: string
  attachable?: boolean
  editable?: boolean
}

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
  type?: string | null
  sprint?: boolean | null
}

export interface TrackerProject {
  id: string
  key: string
  name: string
  avatar: string | null
}

export interface CodeRepo {
  id: string
  ref: string
  name: string
  url: string | null
  defaultBranch: string | null
  private: boolean
}

export interface OpenPull {
  key: string
  number: number
  title: string
  head: string
  base: string
  author: string
  url: string | null
  draft: boolean
}

export interface CodeBranch {
  name: string
  default: boolean
}

export interface IssueData {
  key: string
  url: string | null
  title: string
  status: string | null
  type?: string | null
  parent?: string | null
  assignee: Person | null
  description: string
  attachments: { name: string; size: string; url?: string }[]
  subtasks: { key: string; title: string; done: boolean; byAgent: boolean; type?: string | null }[]
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
  | { action: 'tests.projects'; args: { query?: string }; result: TrackerProject[] }
  | { action: 'security.projects'; args: { query?: string }; result: TrackerProject[] }
  | { action: 'code.repos'; args: { query?: string }; result: CodeRepo[] }
  | { action: 'code.pulls'; args: { repo: string }; result: OpenPull[] }
  | { action: 'code.branches'; args: { repo: string }; result: CodeBranch[] }
  | { action: 'tracker.issue'; args: { key: string }; result: IssueData }
  | { action: 'tracker.comment'; args: { key: string; body: string }; result: Comment }
  | { action: 'code.change'; args: { change: string }; result: ChangeData }
  | { action: 'ci.run'; args: { run: string }; result: PipelineRun }
  | { action: 'cloud.accounts_and_roles'; args: Record<string, never>; result: CloudAccount[] }
  | { action: 'mcp.tools'; args: Record<string, never>; result: McpTool[] }
  | { action: 'security.findings'; args: { project: string; open?: boolean; limit?: number }; result: Record<string, unknown>[] }
  | { action: 'security.status'; args: { project: string; since?: string }; result: { status: string; scannedAt: string | null; running?: boolean } }
  | { action: 'quality.report'; args: { project: string; pullRequest?: string; branch?: string }; result: Record<string, unknown> & { status: string } }
  | { action: `${string}.options`; args: { query?: string; field?: string }; result: BindingOption[] }

export interface McpTool {
  name: string
}

export type CallAction = ConnectionCall['action']
export type CallArgs<A extends CallAction> = Extract<ConnectionCall, { action: A }>['args']
export type CallResult<A extends CallAction> = Extract<ConnectionCall, { action: A }>['result']

export type SessionStatus = 'idle' | 'running' | 'waiting'
export type SessionMode = 'auto' | 'plan'

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
  steps?: string[]
  job?: string
  jobRunning?: boolean
  jobStoppable?: boolean
}

export interface AskQuestion {
  question: string
  header: string
  multiSelect: boolean
  options: { label: string; description?: string }[]
}

export type PermissionDecision = 'allowed' | 'always' | 'denied'

export type SessionEvent =
  | { id: string; at: Iso; type: 'user'; author: Person; text: string; attachments: number; agent?: string }
  | { id: string; at: Iso; type: 'thinking'; seconds: number; steps: ThinkingStep[]; live: boolean }
  | { id: string; at: Iso; type: 'tools'; calls: ToolCall[]; seconds: number; live: boolean; agent?: string }
  | { id: string; at: Iso; type: 'assistant'; text: string; agent?: string }
  | { id: string; at: Iso; type: 'status'; status: 'info' | 'error'; text: string }
  | { id: string; at: Iso; type: 'ask'; questions: AskQuestion[]; answers: string[] | null; agent?: string }
  | { id: string; at: Iso; type: 'permission'; tool: string; detail: string; risk: 'low' | 'medium' | 'high'; logo?: string; rememberable?: boolean; resolution: 'pending' | PermissionDecision }
  | { id: string; at: Iso; type: 'plan'; text: string; resolution: 'pending' | 'approved' | 'revised'; feedback?: string; agent?: string }
  | { id: string; at: Iso; type: 'artifact'; artifactId: string; title: string; name: string; kind: ArtifactKind | null; agent?: string }

export type ArtifactKind = 'page' | 'markdown' | 'pdf' | 'image' | 'text'

export interface Session {
  id: string
  title: string
  status: SessionStatus
  mode: SessionMode
  model: string
  agents: Agent[]
  events: SessionEvent[]
  work?: SessionWork
  suggestion: string | null
}

export interface SessionTask {
  id: string
  subject: string
  status: string
}

export interface BackgroundJob {
  id: string
  command: string
}

export interface SessionWork {
  background: BackgroundJob[]
  stoppable?: boolean
  tasks: SessionTask[]
}

export interface NewMessage {
  text: string
  model: string | null
  effort: Effort
  fast: boolean
  attachments: string[]
}

export interface SideQuestion {
  text: string
  model?: string | null
}

export interface SideAnswer {
  answer: string
}

export interface Btw {
  text: string
  application: string | null
  model?: string | null
}

export type AskResponse =
  | { kind: 'ask'; answers: string[] }
  | { kind: 'permission'; decision: PermissionDecision }
  | { kind: 'plan'; decision: 'approve' | 'revise'; feedback?: string }

export type SignInKind = 'oauth' | 'device' | 'sso' | 'browser' | 'token'

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
  alternatives?: SignInAlternative[]
  ai?: boolean
  bindings?: BindingSpec[]
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

export type UsageDays = 7 | 30 | 90

export interface UsageTotals {
  costUsd: number
  input: number
  output: number
  cacheRead: number
  cacheWrite: number
  calls: number
  sessions: number
}

export interface UsageDay {
  date: string
  costUsd: number
  tokens: number
  calls: number
  byProvider: Record<string, number>
  byModel: Record<string, number>
}

export interface UsageRow {
  id: string
  name: string
  costUsd: number
  tokens: number
  calls: number
}

export interface UsageProviderRow extends UsageRow {
  logo: string
}

export interface UsageAccountRow {
  id: string
  provider: string
  account: string
  costUsd: number
  tokens: number
  calls: number
}

export interface Usage {
  days: number
  from: string
  to: string
  totals: UsageTotals
  daily: UsageDay[]
  byProvider: UsageProviderRow[]
  byModel: UsageRow[]
  byAccount: UsageAccountRow[]
}

export interface ItemSpent {
  costUsd: number | null
  input: number
  output: number
  cacheRead: number
  cacheWrite: number
  calls: number
}

export interface ItemContext {
  used: number
  window: number
  percent: number
}

export interface ItemUsage {
  spent: ItemSpent
  context: ItemContext | null
}

export interface UsageWindow {
  id: string
  label: string
  usedPercent: number
  resetsAt: string | null
  startsAt: string | null
}

export interface UsageLimitAccount {
  provider: string
  name: string
  logo: string
  account: string
  status: 'ok' | 'unsupported' | 'error'
  error: string | null
  plan: string | null
  windows: UsageWindow[]
}

export interface UsageLimits {
  accounts: UsageLimitAccount[]
}

export type SignInStart =
  | { flow: string; kind: 'redirect'; url: string; paste: boolean; returns?: boolean }
  | { flow: string; kind: 'device'; url: string; code: string; expiresIn: number; interval: number }
  | { flow: string; kind: 'token' }

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

export type Theme = 'system' | 'light' | 'dark'

export type Language = 'en' | 'es' | 'pt-BR'

export interface OutputStyle {
  key: string
  name: string
  description: string
}

export interface Branding {
  name: string
  logo: string | null
  favicon: string | null
  editor: string
  theme: Theme
  language: Language
  output_style: string | null
  output_styles: OutputStyle[]
}

export interface BrandingPatch {
  theme?: Theme
  language?: Language
  output_style?: string | null
}

export interface CloudAccount {
  id: string
  name: string
  email: string
  roles: string[]
}

export interface BindingChoice {
  value: string
  label?: string
}

export interface Binding {
  connection: string
  value: string
  label?: string
  more?: BindingChoice[]
  fields?: Record<string, BindingChoice>
  profiles?: Record<string, BindingChoice>[]
}

export type Bindings = Record<string, Binding>

export type BindingScope = 'application' | 'repository'

export interface BindingSpec {
  capability: string
  scope: BindingScope
  label: string
  multiple?: boolean
  fields?: BindingFieldSpec[]
  each?: BindingEach
}

export interface BindingFieldSpec {
  key: string
  label: string
  required?: boolean
}

export interface BindingEach {
  key: string
  label: string
  fields: BindingFieldSpec[]
}

export interface BindingOption {
  value: string
  label: string
  detail?: string | null
}

export interface ApplicationRepository {
  id: string
  directory: string
  run: string[]
  verify: string[]
  e2e?: string[]
  e2eDirectory?: string
  bindings?: Bindings
  flow?: string
}

export interface Application {
  id: string
  name: string
  bindings?: Bindings
  repositories: ApplicationRepository[]
  skipDependabot?: boolean
  env?: Record<string, string>
}

export type ApplicationDraft = Omit<Application, 'id'> & { id?: string }

export interface DirEntry {
  name: string
  path: string
  repo: boolean
}

export interface DirListing {
  path: string
  parent: string | null
  home: string
  repo: boolean
  remote: { url: string | null; ref: string | null }
  entries: DirEntry[]
}

export type LiveEvent =
  | { type: 'item'; item: Item }
  | { type: 'item.removed'; id: string }
  | { type: 'session'; session: string; event: SessionEvent }
  | { type: 'session.event.removed'; session: string; id: string }
  | { type: 'session.status'; session: string; status: SessionStatus }
  | { type: 'session.suggestion'; session: string; suggestion: string | null }
  | { type: 'session.work'; session: string; work: SessionWork }
  | { type: 'connections'; connections: Connections }
  | { type: 'branding'; branding: Branding }
  | { type: 'resync' }

export interface Api {
  items(): Promise<Item[]>
  createItem(input: NewItem): Promise<Item>
  rerun(item: string, step: string | null): Promise<void>
  cancel(item: string): Promise<void>
  openFolders(item: string): Promise<void>
  archive(item: string): Promise<void>
  unarchive(item: string): Promise<void>
  step(item: string, step: string, attempt: number | null): Promise<StepView>
  input(item: string, step: string, input: HumanInput): Promise<void>
  itemUsage(item: string): Promise<ItemUsage>
  call<A extends CallAction>(connection: string, action: A, args: CallArgs<A>): Promise<CallResult<A>>
  session(id: string): Promise<Session>
  send(session: string, message: NewMessage): Promise<void>
  side(session: string, question: SideQuestion): Promise<SideAnswer>
  btw(question: Btw): Promise<Item>
  respond(session: string, event: string, response: AskResponse): Promise<void>
  stop(session: string): Promise<void>
  stopJob(session: string, job: string): Promise<void>
  openFile(session: string, file: FileRef): Promise<void>
  connections(): Promise<Connections>
  startSignIn(provider: string, fields: Record<string, string>, method?: string): Promise<SignInStart>
  pollSignIn(request: SignInPollRequest): Promise<SignInPoll>
  disconnect(connection: string): Promise<void>
  models(): Promise<Models>
  usage(days: UsageDays): Promise<Usage>
  usageLimits(): Promise<UsageLimits>
  branding(): Promise<Branding>
  saveBranding(patch: BrandingPatch): Promise<Branding>
  applications(): Promise<Application[]>
  flows(): Promise<string[]>
  createApplication(application: ApplicationDraft): Promise<Application>
  saveApplication(application: Application): Promise<Application>
  deleteApplication(id: string): Promise<void>
  dirs(path: string | null): Promise<DirListing>
  upload(file: File): Promise<{ id: string; name: string; size: number }>
  live(onEvent: (event: LiveEvent) => void, onState: (connected: boolean) => void): () => void
}

export type FileRef = { path: string; line: number | null; column: number | null }

export const BTW_WORKFLOW = 'btw'

export const ROUTES = {
  items: '/api/items',
  rerun: (id: string) => `/api/items/${encodeURIComponent(id)}/rerun`,
  cancel: (id: string) => `/api/items/${encodeURIComponent(id)}/cancel`,
  openFolders: (id: string) => `/api/items/${encodeURIComponent(id)}/open`,
  archive: (id: string) => `/api/items/${encodeURIComponent(id)}/archive`,
  itemUsage: (id: string) => `/api/items/${encodeURIComponent(id)}/usage`,
  step: (id: string, step: string) => `/api/items/${encodeURIComponent(id)}/steps/${encodeURIComponent(step)}`,
  input: (id: string, step: string) => `/api/items/${encodeURIComponent(id)}/steps/${encodeURIComponent(step)}/input`,
  call: (connection: string) => `/api/connections/${encodeURIComponent(connection)}/call`,
  session: (id: string) => `/api/sessions/${encodeURIComponent(id)}`,
  messages: (id: string) => `/api/sessions/${encodeURIComponent(id)}/messages`,
  side: (id: string) => `/api/sessions/${encodeURIComponent(id)}/side`,
  btw: '/api/btw',
  respond: (id: string) => `/api/sessions/${encodeURIComponent(id)}/respond`,
  stop: (id: string) => `/api/sessions/${encodeURIComponent(id)}/stop`,
  stopJob: (id: string, job: string) => `/api/sessions/${encodeURIComponent(id)}/jobs/${encodeURIComponent(job)}/stop`,
  openFile: (id: string) => `/api/sessions/${encodeURIComponent(id)}/open`,
  connections: '/api/connections',
  connection: (id: string) => `/api/connections/${encodeURIComponent(id)}`,
  start: (provider: string) => `/api/connections/${encodeURIComponent(provider)}/start`,
  poll: '/api/connections/device/poll',
  models: '/api/models',
  usage: (days: number) => `/api/usage?days=${days}`,
  usageLimits: '/api/usage/limits',
  branding: '/api/branding',
  applications: '/api/applications',
  flows: '/api/flows',
  application: (id: string) => `/api/applications/${encodeURIComponent(id)}`,
  dirs: (path: string | null) => (path ? `/api/fs/dirs?path=${encodeURIComponent(path)}` : '/api/fs/dirs'),
  attachments: '/api/attachments',
  attachment: (id: string) => `/api/attachments/${encodeURIComponent(id)}`,
  artifactContent: (id: string, download = false) => `/api/artifacts/${encodeURIComponent(id)}/content${download ? '?download=1' : ''}`,
  live: '/api/live',
} as const
