import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { OptionsPanel } from '../src/OptionPicker'
import { selectorsFromAcp, setRequest } from '../src/agentSetup'

const raw = {
  configOptions: [
    { id: 'model', name: 'Model', type: 'select', currentValue: 'a', options: [{ value: 'a', name: 'A' }, { value: 'b', name: 'B' }] },
    { id: 'web', name: 'Web search', type: 'boolean', currentValue: false },
    { id: 'think', name: 'Show thinking', type: 'boolean', currentValue: true },
    { id: 'odd', name: 'Odd', type: 'range', currentValue: 3 },
  ],
}

describe('boolean config options', () => {
  it('are read next to the selects, and unknown kinds stay ignored', () => {
    const selectors = selectorsFromAcp(raw)
    expect(selectors.map((s) => s.id)).toEqual(['model', 'web', 'think'])
    expect(selectors[1]).toMatchObject({ boolean: true, current: 'false' })
    expect(selectors[2]).toMatchObject({ boolean: true, current: 'true' })
    expect(selectors[0].boolean).toBeUndefined()
  })

  it('are set with a typed boolean value, selects keep the plain value', () => {
    const [model, web] = selectorsFromAcp(raw)
    expect(setRequest(web, 'true')).toEqual({ method: 'session/set_config_option', params: { configId: 'web', type: 'boolean', value: true } })
    expect(setRequest(web, 'false').params.value).toBe(false)
    expect(setRequest(model, 'b')).toEqual({ method: 'session/set_config_option', params: { configId: 'model', value: 'b' } })
  })

  it('show as switches that call back with the other value, without opening a list', () => {
    const onPick = vi.fn()
    const selectors = selectorsFromAcp(raw)
    render(<OptionsPanel selectors={selectors} valueOf={(s) => s.current} onPick={onPick} />)
    const web = screen.getByRole('switch', { name: 'Web search' })
    const think = screen.getByRole('switch', { name: 'Show thinking' })
    expect(web.getAttribute('aria-checked')).toBe('false')
    expect(think.getAttribute('aria-checked')).toBe('true')
    fireEvent.click(web)
    expect(onPick).toHaveBeenLastCalledWith(expect.objectContaining({ id: 'web' }), 'true')
    fireEvent.click(think)
    expect(onPick).toHaveBeenLastCalledWith(expect.objectContaining({ id: 'think' }), 'false')
    expect(screen.queryByRole('group', { name: 'Web search' })).toBeNull()
  })

  it('a lone boolean is a switch, not a two-item list', () => {
    render(<OptionsPanel selectors={selectorsFromAcp({ configOptions: [raw.configOptions[1]] })} valueOf={(s) => s.current} onPick={() => {}} />)
    expect(screen.getByRole('switch', { name: 'Web search' })).toBeTruthy()
  })
})
