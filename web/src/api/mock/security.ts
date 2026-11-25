import type { Finding, FindingChoice, FindingDecision, IgnoreReason, QualityReport } from '../contract'
import type { SimItem } from './sim'

export const VERACODE_LOGO = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 25 25" fill="none"><defs><linearGradient id="veracode-mark" x1="15.3969" y1="-6.31621" x2="15.3969" y2="41.625" gradientUnits="userSpaceOnUse"><stop stop-color="#00B9FF"/><stop offset="1" stop-color="#0065DD"/></linearGradient></defs><path d="M0 0.312256L8.47516 23.7648L10.9971 16.4219L5.4591 0.312256H0Z" fill="currentColor"/><path d="M18.8443 0.312256L12.3078 19.3316H12.2941L10.4687 24.6535H15.5092L24.3034 0.312256H18.8443Z" fill="url(#veracode-mark)"/></svg>'

const PORTAL = 'https://analysiscenter.veracode.com/auth/index.jsp#ReviewResultsAllFlaws'

type Seed = Omit<Finding, 'held' | 'suggested' | 'decision' | 'url' | 'new' | 'policy'>

const base = { cwe: null, cve: null, file: null, line: null, component: null, version: null, fixedIn: null }

const STATIC: Seed[] = [
  { ...base, id: '1043', title: 'SQL Injection', severity: 5, kind: 'static', cwe: { id: '89', name: 'Improper Neutralization of Special Elements used in an SQL Command' }, file: 'src/orders/repository.ts', line: 118, description: 'The query is built by concatenating the order filter received in the request. An attacker can change the query and read or delete other customers’ orders. Use a parameterized query.' },
  { ...base, id: '1051', title: 'Cross-Site Scripting', severity: 4, kind: 'static', cwe: { id: '79', name: 'Improper Neutralization of Input During Web Page Generation' }, file: 'web/src/checkout/Summary.tsx', line: 54, description: 'The coupon name is rendered with dangerouslySetInnerHTML without encoding. A crafted coupon runs script in the customer’s browser.' },
  { ...base, id: '1038', title: 'Path Traversal', severity: 4, kind: 'static', cwe: { id: '22', name: 'Improper Limitation of a Pathname to a Restricted Directory' }, file: 'src/files/download.ts', line: 31, description: 'The invoice file name comes from the query string and is joined to the storage folder without normalization, so ../ sequences reach other files.' },
  { ...base, id: '1066', title: 'Use of a Broken or Risky Cryptographic Algorithm', severity: 3, kind: 'static', cwe: { id: '327', name: 'Use of a Broken or Risky Cryptographic Algorithm' }, file: 'src/auth/token.ts', line: 12, description: 'Reset tokens are hashed with MD5. Use SHA-256 or a dedicated token generator.' },
  { ...base, id: '1072', title: 'Insertion of Sensitive Information into Log File', severity: 3, kind: 'static', cwe: { id: '532', name: 'Insertion of Sensitive Information into Log File' }, file: 'src/payments/gateway.ts', line: 207, description: 'The full gateway response, including the card holder name and the last digits, is written to the application log.' },
  { ...base, id: '1080', title: 'Improper Output Neutralization for Logs', severity: 2, kind: 'static', cwe: { id: '117', name: 'Improper Output Neutralization for Logs' }, file: 'src/api/middleware/log.ts', line: 44, description: 'The User-Agent header is logged as is; line breaks in it forge log entries.' },
]

const DEPENDENCY: Seed[] = [
  { ...base, id: '2011', title: 'path-to-regexp', severity: 4, kind: 'dependency', cve: { id: 'CVE-2024-45296', cvss: 7.5 }, component: 'path-to-regexp', version: '0.1.7', fixedIn: '0.1.10', description: 'Backtracking regular expressions are generated for routes with two parameters in one segment, which allows a denial of service with a crafted URL.' },
  { ...base, id: '2014', title: 'ws', severity: 4, kind: 'dependency', cve: { id: 'CVE-2024-37890', cvss: 7.5 }, component: 'ws', version: '8.11.0', fixedIn: '8.17.1', description: 'A request with more headers than the server limit crashes the WebSocket server.' },
  { ...base, id: '2019', title: 'tough-cookie', severity: 3, kind: 'dependency', cve: { id: 'CVE-2023-26136', cvss: 6.5 }, component: 'tough-cookie', version: '4.1.2', fixedIn: '4.1.3', description: 'Prototype pollution when the cookie jar is used in loose mode.' },
  { ...base, id: '2023', title: 'micromatch', severity: 3, kind: 'dependency', cve: { id: 'CVE-2024-4067', cvss: 5.3 }, component: 'micromatch', version: '4.0.5', fixedIn: '4.0.8', description: 'Inefficient regular expression in micromatch.braces can be exploited with long inputs.' },
]

export type Triage = Record<string, { choice: FindingChoice; reason: IgnoreReason | null }>

const suggest = (severity: number): FindingChoice => (severity >= 4 ? 'fix' : 'task')

const prOf = (n: number) => ({ key: `#${n}`, url: `https://github.com/acme/checkout/pull/${n}` })

function decide(it: SimItem, chosen: { choice: FindingChoice; reason: IgnoreReason | null }, index: number): FindingDecision {
  const task = chosen.choice === 'task' ? { key: `APP-${300 + (it.h % 40) + index}`, url: `https://acme.atlassian.net/browse/APP-${300 + (it.h % 40) + index}` } : null
  const pr = chosen.choice === 'fix' ? prOf(1200 + (it.h % 60)) : null
  return { ...chosen, task, pr }
}

export function findingsFor(it: SimItem, triage: Triage | null): Finding[] {
  if (it.done && it.h % 5 === 0) return []
  const seeds = [...STATIC, ...DEPENDENCY].filter((_, i) => ((it.h >> (i % 12)) & 3) !== 0 || i < 2)
  const sorted = seeds.sort((a, b) => b.severity - a.severity)
  const heldAt = 1 + (it.h % 3)
  return sorted.map((seed, i): Finding => {
    const held = i === heldAt ? prOf(1100 + (it.h % 80)) : null
    const suggested = suggest(seed.severity)
    const chosen = held ? null : triage ? triage[seed.id] ?? null : null
    return { ...seed, held, suggested, decision: chosen ? decide(it, chosen, i) : null, url: PORTAL, new: ((it.h >> i) & 1) === 1, policy: seed.severity >= 4 }
  })
}

export function autoTriage(it: SimItem): Triage {
  return Object.fromEntries(findingsFor(it, null).filter((f) => !f.held).map((f, i) => [f.id, { choice: i === 3 && f.severity <= 3 ? 'ignore' : f.suggested, reason: i === 3 && f.severity <= 3 ? 'false_positive' : null }]))
}

export const SONAR_LOGO = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"><path d="M4 18c3-8 9-12 16-12" stroke="#126ed3" stroke-width="2.4" stroke-linecap="round"/><path d="M7 20c2-5 6-8 12-9" stroke="#126ed3" stroke-width="2" stroke-linecap="round" opacity=".7"/><path d="M11 21c1-3 4-5 8-5.5" stroke="#126ed3" stroke-width="1.8" stroke-linecap="round" opacity=".45"/></svg>'

export function qualityFor(it: SimItem, failed: boolean): QualityReport {
  const coverage = failed ? 71.4 : 82 + (it.h % 14)
  const hotspots = 2 + (it.h % 3)
  const reviewed = failed ? 1 : hotspots
  return {
    gate: failed ? 'failed' : 'passed',
    ratings: { reliability: failed ? 'C' : 'A', security: 'A', maintainability: 'A' },
    counts: { bugs: failed ? 3 : it.h % 2, vulnerabilities: 0, smells: 8 + (it.h % 30), hotspots, hotspotsReviewed: reviewed },
    coverage,
    duplication: failed ? 4.2 : 1 + (it.h % 20) / 10,
    conditions: [
      { metric: 'new_coverage', label: 'Coverage on New Code', value: `${coverage.toFixed(1)}%`, threshold: '≥ 80%', passed: coverage >= 80 },
      { metric: 'new_duplicated_lines_density', label: 'Duplicated Lines (%) on New Code', value: failed ? '4.2%' : `${(1 + (it.h % 20) / 10).toFixed(1)}%`, threshold: '≤ 3%', passed: !failed },
      { metric: 'new_security_hotspots_reviewed', label: 'Security Hotspots Reviewed on New Code', value: `${Math.round((reviewed / hotspots) * 100)}%`, threshold: '100%', passed: reviewed === hotspots },
      { metric: 'new_reliability_rating', label: 'Reliability Rating on New Code', value: failed ? 'C' : 'A', threshold: '≥ A', passed: !failed },
      { metric: 'new_security_rating', label: 'Security Rating on New Code', value: 'A', threshold: '≥ A', passed: true },
    ],
    issues: [
      ...(failed ? [
        { key: 'AZ1', type: 'bug' as const, severity: 'major' as const, message: 'Promise returned by quote() is not handled', file: 'src/shipping/quote.ts', line: 88, url: null },
        { key: 'AZ2', type: 'bug' as const, severity: 'major' as const, message: 'This condition always evaluates to true', file: 'src/shipping/rules.ts', line: 140, url: null },
      ] : []),
      { key: 'AZ3', type: 'hotspot', severity: 'critical', message: 'Make sure this regular expression cannot lead to denial of service', file: 'src/shipping/zip.ts', line: 23, url: null },
      { key: 'AZ4', type: 'code_smell', severity: 'critical', message: 'Refactor this function to reduce its Cognitive Complexity from 24 to 15', file: 'src/shipping/quote.ts', line: 41, url: null },
      { key: 'AZ5', type: 'code_smell', severity: 'minor', message: 'Remove this duplicated block of code', file: 'src/shipping/table.ts', line: 12, url: null },
    ],
    total: failed ? 47 : 12 + (it.h % 20),
  }
}

const hashOf = (text: string) => [...text].reduce((h, c) => (h * 31 + c.charCodeAt(0)) >>> 0, 7)

export function scannedFindings(project: string): Record<string, unknown>[] {
  const h = hashOf(project)
  return [...STATIC, ...DEPENDENCY]
    .filter((_, i) => ((h >> (i % 12)) & 3) !== 0 || i < 2)
    .map((seed, i) => ({ ...seed, status: 'open', new: ((h >> i) & 1) === 1, policy: seed.severity >= 4, url: PORTAL }))
}

export function scanStatusOf(project: string) {
  const day = 18 + (hashOf(project) % 7)
  return { status: 'done', scannedAt: `2026-11-${day}T09:30:00.000Z`, running: false }
}

export function qualityReportOf(project: string) {
  const h = hashOf(project)
  const report = qualityFor({ h, done: true } as unknown as SimItem, h % 3 === 0)
  return { status: 'done', ...report, url: `https://sonarcloud.io/dashboard?id=${encodeURIComponent(project)}`, analyzedAt: null }
}
