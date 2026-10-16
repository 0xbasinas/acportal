// Pure helpers for "Run on several agents" and the pre-merge checklist (unit-tested).
import type { DiffBlock } from './acp'

export type RunEntry = { agentId: string; sessionId: string; acpSessionId: string; branch: string; path: string; error?: string }
export type Run = { id: string; hostId: string; root: string; base: string; prompt: string; createdAt: number; entries: RunEntry[]; state: 'running' | 'merged' | 'discarded'; winner?: string }
export type CheckResult = { command: string; code: number | null; output: string; ms: number; timedOut?: boolean; truncated?: boolean }
export type Checks = { passed: boolean; sandboxed: boolean; results: CheckResult[]; at: number }
export type ChecksConfig = { commands: string[]; aiReview: boolean; reviewAgent?: string }
export type Review = { state: 'running' | 'pass' | 'fail' | 'unclear'; sessionId?: string; text?: string }

/** A short branch-safe name for one run of one agent: `run/<slug>-<n>-<agent>`. */
export function runBranch(prompt: string, agentId: string, n: number, stamp: string): string {
  const slug = prompt.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 24).replace(/-+$/, '') || 'task'
  const agent = agentId.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '') || 'agent'
  return `run/${slug}-${stamp}-${n}-${agent}`
}

/** Split a multi-file `git diff` into one block per file. */
export function splitDiff(unified: string): DiffBlock[] {
  const out: DiffBlock[] = []
  for (const part of unified.split(/^(?=diff --git )/m)) {
    if (!part.startsWith('diff --git ')) continue
    const m = /^diff --git a\/(.+?) b\/(.+)$/m.exec(part)
    const path = m ? m[2] : 'file'
    const isNew = /^new file mode/m.test(part)
    out.push({ path, oldText: isNew ? null : '', newText: '', unified: part })
  }
  return out
}

/** Added and removed line counts of a unified diff. */
export function diffTotals(unified: string): { files: number; add: number; del: number } {
  let add = 0, del = 0, files = 0
  for (const l of unified.split('\n')) {
    if (l.startsWith('diff --git ')) files++
    else if (l.startsWith('+') && !l.startsWith('+++')) add++
    else if (l.startsWith('-') && !l.startsWith('---')) del++
  }
  return { files, add, del }
}

/** The reviewer's verdict: the last `VERDICT: PASS|FAIL` line wins. */
export function verdictOf(reply: string | undefined): 'pass' | 'fail' | undefined {
  if (!reply) return undefined
  const all = [...reply.matchAll(/VERDICT:\s*\**\s*(PASS|FAIL)\b/gi)]
  const last = all[all.length - 1]
  return last ? (last[1].toUpperCase() === 'PASS' ? 'pass' : 'fail') : undefined
}

export function reviewPrompt(diff: string, prompt: string): string {
  const cut = diff.length > 60_000 ? diff.slice(0, 60_000) + '\n… diff cut for length' : diff
  return `You are reviewing a change before it is merged. Do not edit files or run commands.\nThe change was made for this request:\n> ${prompt.replace(/\n/g, '\n> ')}\n\nCheck it for bugs, missing pieces, security problems and anything that breaks existing behaviour. Be brief: list concrete problems, if any.\nEnd your reply with exactly one line: VERDICT: PASS or VERDICT: FAIL.\n\n\`\`\`diff\n${cut}\n\`\`\``
}

/** Green only when everything configured ran and passed; red when anything failed; otherwise pending. */
export function gate(cfg: ChecksConfig | undefined, checks: Checks | undefined, review: Review | undefined): { state: 'green' | 'red' | 'pending' | 'none'; why: string } {
  const wantChecks = (cfg?.commands.length ?? 0) > 0
  const wantReview = Boolean(cfg?.aiReview)
  if (!wantChecks && !wantReview) return { state: 'none', why: 'No checks configured for this workspace' }
  if (wantChecks && checks && !checks.passed) return { state: 'red', why: `${checks.results.filter((r) => r.code !== 0).length} check(s) failed` }
  if (wantReview && review && (review.state === 'fail' || review.state === 'unclear')) return { state: 'red', why: review.state === 'fail' ? 'AI review found problems' : 'AI review gave no verdict' }
  if ((wantChecks && !checks) || (wantReview && (!review || review.state === 'running'))) return { state: 'pending', why: 'Checks have not finished' }
  return { state: 'green', why: 'All checks passed' }
}

export function money(c?: { amount: number; currency: string }): string {
  if (!c) return '—'
  return `${c.currency === 'USD' ? '$' : c.currency + ' '}${c.amount < 0.01 ? c.amount.toFixed(4) : c.amount.toFixed(2)}`
}
