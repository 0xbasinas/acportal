import { describe, expect, it } from 'vitest'
import { joinArgs, splitArgs } from '../src/Settings'

describe('agent arguments in the edit form', () => {
  it('round-trips arguments, including ones with spaces', () => {
    const args = ['acp', '--config', 'C:/Program Files/x/cfg.json', '']
    expect(joinArgs(args)).toBe('acp --config "C:/Program Files/x/cfg.json" ""')
    expect(splitArgs(joinArgs(args))).toEqual(args)
    expect(splitArgs('  --acp   x ')).toEqual(['--acp', 'x'])
  })
})
