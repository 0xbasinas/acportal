import { test } from 'node:test'
import assert from 'node:assert/strict'
import { reduce, emptyTimeline, changedFiles, diffLines, unifiedLines } from '../src/acp'
import { validateAddress } from '../electron/host'
import { moveTo } from '../src/order'

const ev = (sequence: number, message: unknown, direction = 'agent') => ({ type: 'event', sequence, direction, message })
const upd = (update: unknown) => ({ jsonrpc: '2.0', method: 'session/update', params: { sessionId: 's', update } })

test('chunks merge, replayed sequences are ignored', () => {
  let t = emptyTimeline()
  t = reduce(t, ev(1, { jsonrpc: '2.0', id: 7, method: 'session/prompt', params: { prompt: [{ type: 'text', text: 'hi' }] } }, 'client'))
  t = reduce(t, ev(2, upd({ sessionUpdate: 'agent_message_chunk', content: { type: 'text', text: 'Hel' } })))
  t = reduce(t, ev(3, upd({ sessionUpdate: 'agent_message_chunk', content: { type: 'text', text: 'lo' } })))
  t = reduce(t, ev(3, upd({ sessionUpdate: 'agent_message_chunk', content: { type: 'text', text: 'lo' } })))
  assert.equal(t.items.length, 2)
  assert.equal(t.items[1].kind === 'agent' && t.items[1].text, 'Hello')
  assert.equal(t.busy, true)
  t = reduce(t, ev(4, { jsonrpc: '2.0', id: 7, result: { stopReason: 'end_turn' } }))
  assert.equal(t.busy, false)
})

test('a finished turn settles a card the host never journaled, and a real answer stays', () => {
  const req = { jsonrpc: '2.0', id: 4, method: 'session/request_permission', params: { toolCall: { title: 'Write' }, options: [{ optionId: 'allow', name: 'Allow', kind: 'allow_once' }] } }
  const prompt = { jsonrpc: '2.0', id: 7, method: 'session/prompt', params: { prompt: [{ type: 'text', text: 'hi' }] } }
  let open = reduce(emptyTimeline(), ev(1, prompt, 'client'))
  open = reduce(open, ev(2, req))
  open = reduce(open, ev(3, { jsonrpc: '2.0', id: 7, result: { stopReason: 'end_turn' } }))
  const settled = open.items.find((i) => i.kind === 'permission')
  assert.ok(settled && settled.kind === 'permission' && settled.answered === 'settled')

  let kept = reduce(emptyTimeline(), ev(1, prompt, 'client'))
  kept = reduce(kept, ev(2, req))
  kept = reduce(kept, ev(3, { jsonrpc: '2.0', id: 4, result: { outcome: { outcome: 'selected', optionId: 'allow' } } }, 'client'))
  kept = reduce(kept, ev(4, { jsonrpc: '2.0', id: 7, result: { stopReason: 'end_turn' } }))
  const answered = kept.items.find((i) => i.kind === 'permission')
  assert.ok(answered && answered.kind === 'permission' && answered.answered === 'allow')

  let cancelled = reduce(emptyTimeline(), ev(1, prompt, 'client'))
  cancelled = reduce(cancelled, ev(2, req))
  cancelled = reduce(cancelled, ev(3, { jsonrpc: '2.0', id: 7, result: { stopReason: 'cancelled' } }))
  const dropped = cancelled.items.find((i) => i.kind === 'permission')
  assert.ok(dropped && dropped.kind === 'permission' && dropped.answered === 'cancelled')

  // The request id and the answer id can differ in JSON type. A host error is not an answer.
  let typed = reduce(emptyTimeline(), ev(1, { ...req, id: 9 }))
  typed = reduce(typed, ev(2, { jsonrpc: '2.0', id: '9', error: { code: -32602, message: 'Permission response rejected' } }, 'host'))
  const still = typed.items.find((i) => i.kind === 'permission')
  assert.ok(still && still.kind === 'permission' && still.answered === undefined)
  typed = reduce(typed, ev(3, { jsonrpc: '2.0', id: '9', result: { outcome: { outcome: 'selected', optionId: 'allow' } } }, 'client'))
  const matched = typed.items.find((i) => i.kind === 'permission')
  assert.ok(matched && matched.kind === 'permission' && matched.answered === 'allow')
})

test('permissions dedupe by id across pending_permission and event', () => {
  const req = { jsonrpc: '2.0', id: 'p1', method: 'session/request_permission', params: { toolCall: { title: 'Run', rawInput: { shellLine: 'git status\nls' } }, options: [{ optionId: 'a', name: 'Allow', kind: 'allow_once' }], _meta: { acpdSource: 'host-shell-command' } } }
  let t = reduce(emptyTimeline(), { type: 'pending_permission', message: req })
  t = reduce(t, ev(5, req))
  assert.equal(t.items.length, 1)
  const p = t.items[0]
  assert.ok(p.kind === 'permission' && p.lines.length === 2 && p.source === 'host-shell-command')
  t = reduce(t, ev(6, { jsonrpc: '2.0', id: 'p1', result: { outcome: { outcome: 'selected', optionId: 'a' } } }, 'client'))
  assert.equal(t.items[0].kind === 'permission' && t.items[0].answered, 'a')
})

test('tool diffs feed changed files, newest text per path', () => {
  let t = emptyTimeline()
  t = reduce(t, ev(1, upd({ sessionUpdate: 'tool_call', toolCallId: 'x', title: 'Write', content: [{ type: 'diff', path: '/w/a.py', oldText: 'a\nb\n', newText: 'a\nB\n' }] })))
  t = reduce(t, ev(2, upd({ sessionUpdate: 'tool_call_update', toolCallId: 'x', status: 'completed' })))
  const files = changedFiles(t)
  assert.equal(files.length, 1)
  assert.deepEqual([files[0].added, files[0].removed], [1, 1])
  assert.deepEqual(diffLines(files[0].diff).filter((l) => l.kind !== 'ctx').map((l) => l.kind), ['del', 'add'])
})

test('address rules match the Android app', () => {
  assert.equal(validateAddress('https://host.example:8765').origin, 'https://host.example:8765')
  assert.equal(validateAddress('http://127.0.0.1:8798').protocol, 'http:')
  assert.throws(() => validateAddress('http://192.168.1.4:8765'), /HTTPS/)
  assert.throws(() => validateAddress('https://u:p@host.example'), /path or credentials/)
  assert.throws(() => validateAddress('https://host.example/v1'), /path or credentials/)
})

import { parseStatus, parseDiff, parseGraph, parseWorktrees, parseBranches, parseCompare, githubRepo } from '../src/gitparse'
test('git porcelain parsers', () => {
  const s = parseStatus('## main...origin/main [ahead 2, behind 1]\nM  a.py\n M b.py\nMM c.py\n?? new.txt\nUU d.py\nR  old.py -> new.py\n')
  assert.equal(s.branch, 'main'); assert.equal(s.ahead, 2); assert.equal(s.behind, 1)
  assert.deepEqual(s.files.filter((f) => f.staged).map((f) => f.path), ['a.py', 'c.py', 'new.py'])
  assert.deepEqual(s.files.filter((f) => f.unstaged).map((f) => f.path), ['b.py', 'c.py', 'new.txt'])
  assert.deepEqual(s.files.filter((f) => f.conflict).map((f) => f.path), ['d.py'])
  const d = parseDiff('diff --git a/x.py b/x.py\nindex 1..2 100644\n--- a/x.py\n+++ b/x.py\n@@ -1,2 +1,2 @@\n-a\n+b\n c\n@@ -10,1 +10,2 @@\n z\n+y\n')
  assert.equal(d[0].path, 'x.py'); assert.equal(d[0].hunks.length, 2)
  assert.ok(d[0].hunks[1].patch.startsWith('diff --git a/x.py b/x.py\n') && d[0].hunks[1].patch.endsWith('+y\n'))
  const g = parseGraph('* abc1234\u001f (HEAD -> main)\u001ffix\u001fme\u001f2m ago\n|\\  \n')
  assert.equal(g[0].hash, 'abc1234'); assert.equal(g[0].refs, 'HEAD -> main'); assert.equal(g[1].graph, '|\\  ')
  const w = parseWorktrees('worktree /r\nHEAD 0123456789\nbranch refs/heads/main\n\nworktree /r-wt\nHEAD abcdef0123\ndetached\n')
  assert.equal(w.length, 2); assert.equal(w[0].branch, 'main'); assert.ok(w[1].detached)
})
test('branches, compare and GitHub remotes', () => {
  const b = parseBranches('main\u001fabc1234\u001f*\u001f[ahead 1]\u001f2 hours ago\nfeat/x\u001fdef5678\u001f \u001f\u001f3 days ago\norigin/HEAD\u001fabc\u001f \u001f\u001f\norigin/main\u001fabc1234\u001f \u001f\u001f2 hours ago\n')
  assert.deepEqual(b.map((x) => [x.name, x.current, x.remote, x.track]), [['main', true, false, 'ahead 1'], ['feat/x', false, false, ''], ['origin/main', false, true, '']])
  const c = parseCompare('>\u001fa1\u001fmine\u001f1m\n<\u001fb2\u001ftheirs\u001f2m\n')
  assert.equal(c.ahead[0].subject, 'mine'); assert.equal(c.behind[0].hash, 'b2')
  assert.equal(githubRepo('git@github.com:0xbasinas/acportal.git'), 'https://github.com/0xbasinas/acportal')
  assert.equal(githubRepo('https://github.com/a/b'), 'https://github.com/a/b')
  assert.equal(githubRepo('https://gitlab.com/a/b.git'), undefined)
  assert.equal(parseStatus('## release/v1.2...origin/release/v1.2 [behind 2]\n').branch, 'release/v1.2')
})

test('unified diffs from the host drop the trailing blank line', () => {
  const lines = unifiedLines('diff --git a/h b/h\n--- /dev/null\n+++ b/h\n@@ -0,0 +1 @@\n+hi\n')
  assert.deepEqual(lines, [{ kind: 'add', b: 1, text: 'hi' }])
})

test('several hosts report the worst state and how many are connected', async () => {
  const { aggregate } = await import('../src/fleet')
  const h = (id: string) => ({ id, hostId: id, name: id, address: 'http://127.0.0.1:1', deviceId: '', expiresAt: 0, addedAt: 0 })
  const e = (state: 'connected' | 'offline' | 'repair') => ({ state, agents: [], sessions: [], workspaces: [], failures: 0 })
  assert.deepEqual(aggregate([h('a'), h('b')], { a: e('connected'), b: e('connected') }), { state: 'connected', label: 'All 2 hosts connected' })
  assert.deepEqual(aggregate([h('a'), h('b')], { a: e('connected'), b: e('offline') }), { state: 'offline', label: '1 of 2 hosts connected' })
  assert.equal(aggregate([h('a')], { a: e('repair') }).label, 'Needs re-pair')
})

test('scope metadata on permission requests becomes an outside-workspace warning', () => {
  const req = { jsonrpc: '2.0', id: 'p9', method: 'session/request_permission', params: { toolCall: { title: 'Run', rawInput: { shellLine: 'cat /etc/passwd' } }, options: [], _meta: { acpdSource: 'host-shell-command', acpdScope: { outsideWorkspace: ['/etc/passwd', 7], sandboxed: true } } } }
  const t = reduce(emptyTimeline(), ev(1, req, 'host'))
  const p = t.items.find((i) => i.kind === 'permission')
  assert.ok(p && p.kind === 'permission')
  assert.deepEqual(p.scope, { outside: ['/etc/passwd'], sandboxed: true, agentTool: false })
  // An agent-direction request can never claim the host sandbox.
  const forged = reduce(emptyTimeline(), ev(1, req)).items.find((i) => i.kind === 'permission')
  assert.ok(forged && forged.kind === 'permission')
  assert.deepEqual(forged.scope, { outside: ['/etc/passwd'], sandboxed: false, agentTool: true })
  const plain = reduce(emptyTimeline(), ev(1, { ...req, id: 'p10', params: { ...req.params, _meta: {} } }))
  const q = plain.items.find((i) => i.kind === 'permission')
  assert.ok(q && q.kind === 'permission' && q.scope === undefined)
  const win = { ...req, id: 'p11', params: { ...req.params, _meta: { acpdScope: { outsideWorkspace: [], sandboxed: true, shell: 'powershell', shellNote: 'Git Bash cannot run inside the Windows sandbox; runs in PowerShell.' } } } }
  const w = reduce(emptyTimeline(), ev(1, win, 'host')).items.find((i) => i.kind === 'permission')
  assert.ok(w && w.kind === 'permission')
  assert.equal(w.scope?.shell, 'powershell')
  assert.match(w.scope?.shellNote ?? '', /PowerShell/)
  // The agent's own tool does not get host shell details.
  const own = reduce(emptyTimeline(), ev(1, win)).items.find((i) => i.kind === 'permission')
  assert.ok(own && own.kind === 'permission' && own.scope?.shell === undefined)
})

test('drag reorder moves a host onto the target position', () => {
  assert.deepEqual(moveTo(['a', 'b', 'c', 'd'], 'a', 'c'), ['b', 'c', 'a', 'd'])
  assert.deepEqual(moveTo(['a', 'b', 'c', 'd'], 'd', 'b'), ['a', 'd', 'b', 'c'])
  assert.deepEqual(moveTo(['a', 'b'], 'a', 'a'), ['a', 'b'])
  assert.deepEqual(moveTo(['a', 'b'], 'x', 'a'), ['a', 'b'])
})

import { diffTotals, gate, runBranch, splitDiff, verdictOf } from '../src/runs'

test('multi-agent runs: branch names, diff split and totals', () => {
  assert.equal(runBranch('Add a --verbose flag!', 'Goose Worker', 2, 'k3f1'), 'run/add-a-verbose-flag-k3f1-2-goose-worker')
  assert.equal(runBranch('???', '', 1, 'a'), 'run/task-a-1-agent')
  const d = 'diff --git a/a.py b/a.py\nindex 1..2\n--- a/a.py\n+++ b/a.py\n@@ -1 +1,2 @@\n-x\n+y\n+z\ndiff --git a/n.md b/n.md\nnew file mode 100644\n--- /dev/null\n+++ b/n.md\n@@ -0,0 +1 @@\n+hi\n'
  const parts = splitDiff(d)
  assert.deepEqual(parts.map((p) => [p.path, p.oldText]), [['a.py', ''], ['n.md', null]])
  assert.deepEqual(diffTotals(d), { files: 2, add: 3, del: 1 })
  assert.deepEqual(splitDiff(''), [])
})

test('pre-merge gate: green only when everything configured passed', () => {
  const ok = { passed: true, sandboxed: true, results: [], at: 1 }
  const bad = { passed: false, sandboxed: true, results: [{ command: 't', code: 1, output: '', ms: 1 }], at: 1 }
  assert.equal(gate(undefined, undefined, undefined).state, 'none')
  assert.equal(gate({ commands: ['t'], aiReview: false }, undefined, undefined).state, 'pending')
  assert.equal(gate({ commands: ['t'], aiReview: false }, ok, undefined).state, 'green')
  assert.equal(gate({ commands: ['t'], aiReview: false }, bad, undefined).state, 'red')
  assert.equal(gate({ commands: ['t'], aiReview: true }, ok, { state: 'running' }).state, 'pending')
  assert.equal(gate({ commands: ['t'], aiReview: true }, ok, { state: 'fail' }).state, 'red')
  assert.equal(gate({ commands: [], aiReview: true }, undefined, { state: 'unclear' }).state, 'red')
  assert.equal(gate({ commands: [], aiReview: true }, undefined, { state: 'pass' }).state, 'green')
  assert.equal(verdictOf('looks fine\nVERDICT: PASS'), 'pass')
  assert.equal(verdictOf('VERDICT: PASS\n...actually\n**VERDICT: FAIL**'), 'fail')
  assert.equal(verdictOf('no verdict here'), undefined)
})

test('the reply to an option change replaces the whole option list; a refusal shows the agent value again', () => {
  let t = emptyTimeline()
  t = reduce(t, ev(1, { jsonrpc: '2.0', id: 30, method: 'session/set_config_option', params: { sessionId: 's', configId: 'provider', value: 'beta' } }, 'client'))
  const list = [
    { id: 'provider', name: 'Provider', type: 'select', currentValue: 'beta', options: [{ value: 'alpha', name: 'Alpha' }, { value: 'beta', name: 'Beta' }] },
    { id: 'model', name: 'Model', type: 'select', currentValue: 'b-fast', options: [{ value: 'b-fast', name: 'Beta Fast' }] }
  ]
  t = reduce(t, ev(2, { jsonrpc: '2.0', id: 30, result: { configOptions: list } }))
  assert.deepEqual(t.configOptions, list)
  assert.deepEqual(t.config, { provider: 'beta', model: 'b-fast' })
  assert.deepEqual(t.configRequests, {})
  // A refused change: the optimistic value is dropped and a notice says so.
  t = { ...t, config: { ...t.config, model: 'nope' } }
  t = reduce(t, ev(3, { jsonrpc: '2.0', id: 31, method: 'session/set_config_option', params: { sessionId: 's', configId: 'model', value: 'nope' } }, 'client'))
  t = reduce(t, ev(4, { jsonrpc: '2.0', id: 31, error: { code: -32602, message: 'Invalid' } }))
  assert.equal(t.config?.model, undefined)
  assert.match(t.items.at(-1)?.kind === 'notice' ? (t.items.at(-1) as { text: string }).text : '', /did not accept that model \(-32602\)/)
})

test('applying an event leaves the previous timeline untouched, so a repeated update keeps it', () => {
  // React may call a state updater twice with the same previous state (StrictMode in development).
  const before = emptyTimeline()
  const prompt = ev(1, { jsonrpc: '2.0', id: 9, method: 'session/prompt', params: { sessionId: 's', prompt: [{ type: 'text', text: 'Hello?' }] } }, 'client')
  const first = reduce(before, prompt)
  const second = reduce(before, prompt)
  assert.equal(before.seen.size, 0)
  assert.deepEqual(second.items, first.items)
  assert.equal(second.items[0]?.kind, 'user')
  // The same event applied to the new timeline is still ignored.
  assert.equal(reduce(first, prompt), first)
})

test('commit graph lanes: a branch, its merge and a tip that starts its own lane', async () => {
  const { parseDag, layoutDag } = await import('../src/gitparse')
  // m merges b2 into a1; b2 <- b1 <- base; a1 <- base; t is a separate tip on top of m.
  const line = (h: string, p: string, refs = '') => [h, p, refs, 'subject ' + h, 'me', 'now'].join('\u001f')
  const commits = parseDag([line('t', 'm', ' (HEAD -> main)'), line('m', 'a1 b2'), line('b2', 'b1'), line('a1', 'base'), line('b1', 'base'), line('base', '')].join('\n'))
  assert.equal(commits[0].refs, 'HEAD -> main')
  assert.deepEqual(commits[1].parents, ['a1', 'b2'])
  const { rows, lanes } = layoutDag(commits)
  const at = (h: string) => rows.find((r) => r.commit.hash === h)!
  assert.equal(at('t').col, 0)
  assert.equal(at('m').col, 0)
  assert.equal(at('m').merge, true)
  assert.deepEqual(at('m').out.map((o) => o.to), [0, 1]) // first parent straight down, merged branch in lane 1
  assert.equal(at('b2').col, 1)
  assert.deepEqual(at('b2').through.map((t) => t.lane), [0]) // main passes beside the branch
  assert.equal(at('base').into.length, 2) // both lanes end in the common ancestor
  assert.equal(at('base').col, 0)
  assert.equal(lanes, 2)
  // Colours: a lane keeps its colour down to the ancestor; the branch has another.
  assert.notEqual(at('b2').color, at('a1').color)
})
