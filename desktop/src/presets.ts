// Permission presets from GET /v1/presets. A preset only sets the read/write/terminal
// flags the composer can already toggle. MCP and the agent's own mode stay as they are.

export type Preset = {
  id: string
  label: string
  effects: string[]
  workspaceAccess: { readFiles: boolean; writeFiles: boolean; terminal: boolean; manualApproval?: boolean }
}

export type Rights = { read: boolean; write: boolean; terminal: boolean; /** The host's own approvers stay off for this session. */ manual?: boolean }

export const presetKey = (agent: string): string => `acportal.preset.${agent}`

/** Copy the preset's flags onto an access value. Other fields, including MCP, stay. */
export function applyPreset<T extends Rights>(access: T, preset: Preset): T {
  return {
    ...access,
    read: preset.workspaceAccess.readFiles,
    write: preset.workspaceAccess.writeFiles,
    terminal: preset.workspaceAccess.terminal,
    manual: Boolean(preset.workspaceAccess.manualApproval),
  }
}

export function sameRights(access: Rights, preset: Preset): boolean {
  return access.read === preset.workspaceAccess.readFiles
    && access.write === preset.workspaceAccess.writeFiles
    && access.terminal === preset.workspaceAccess.terminal
    && Boolean(access.manual) === Boolean(preset.workspaceAccess.manualApproval)
}
