//! Optional per-agent overrides for knowledge the ACP and MCP protocols cannot express.
//!
//! Default behaviour is driven only by what a session advertises and what each permission
//! request carries (see design section "Protocol-uniform handling"). The fields below are the
//! only agent-specific knobs the orchestrator reads; each defaults to the protocol path when
//! unset.
//!
//! | Override | Where it is set | Effect when set |
//! | --- | --- | --- |
//! | `ownTools` | registry agent entry | Agent-originated `session/request_permission` is human-only (the host cannot see the agent's own edits). Host-raised consents are unchanged. |
//! | `[shell_review.<id>]` | config | Opt-in shell-line rules and optional model reviewer for that agent id. Absent = every shell line waits for the phone. |
//! | `orchestrate` / `delegable` lists | `[orchestrator]` | Which agent ids get the MCP delegation tools, and which may be children. Empty = none. |
//!
//! Nothing here names a particular product (Goose, OpenCode, …). Product-specific prompts,
//! env vars and trial fixtures live only in opt-in tests (`tests/real_goose.rs`) and examples.

/// Snapshot of the overrides that apply to one session, taken when the session starts.
#[derive(Clone, Copy, Debug, Default)]
pub struct SessionOverrides {
    /// Registry `ownTools`: see the module docs.
    pub own_tools: bool,
}

impl SessionOverrides {
    pub fn from_registry(own_tools: bool) -> Self {
        Self { own_tools }
    }
}
