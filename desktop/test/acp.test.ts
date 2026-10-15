import { test } from 'node:test'
import assert from 'node:assert/strict'
import { reduce, emptyTimeline, changedFiles, diffLines } from '../src/acp'
import { validateAddress } from '../electron/host'

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
