# Architecture

9 October 2026: [native daemon control and soft reload](daemon-and-reload.md) adds operator-only lifecycle control and immutable launch settings. Existing sessions retain their authorization; immediate root revocation requires stop/restart. See [the ADR](adr-background-control.md) and [verification history](../TODO_DONE.md). Platform Actions acceptance remains open.

## Product boundary

The phone and the desktop app are ACP client interfaces. The development host owns execution, workspaces, agent configuration, and session processes. Android learns names, availability, capabilities, modes, models, and configuration options from the host and ACP responses. No Kotlin branch will select behavior by agent name.

```mermaid
flowchart TD
    UI[Android Compose interface] --> Domain[Session repository and ACP reducer]
    Desktop[Desktop Electron app] --> WS
    Domain --> Transport[AgentTransport]
    Transport --> WS[Authenticated WebSocket]
    WS --> Host[acpd management and session service]
    Host --> Registry[Registry and executable discovery]
    Host --> Sessions[Session manager]
    Sessions --> Process[ACP process actor]
    Process -->|ACP v1 JSON-RPC over stdio| Agent[Configured native agent or adapter]
```

The Android and network blocks are implemented. The independently tested Android protocol module contains wire models, the reducer and diff logic. Transport, data, storage, security and presentation currently live in app packages rather than separate Gradle modules; this keeps the working app small without coupling protocol logic to Android.

## Repository structure

```text
/
  acpd/                    Host (library, CLI `acpd`, mock agent, integration tests)
    src/api.rs, src/api/   REST and WebSocket API; bridge.rs (peer ACP bridge), checks.rs,
                           git.rs (scoped git), shared.rs (drafts, titles, history)
    src/session.rs, connection.rs, session_catalog.rs   Sessions, ACP process actor, restart catalog
    src/registry.rs, discovery.rs, userdata.rs           Agents, executable discovery, phone-added settings
    src/terminal.rs, shell.rs, shell_review.rs, auto_approve.rs, win_sandbox.rs, process_tree.rs
                           Host terminals, Windows shell choice, scope check, auto-review, sandboxes, cleanup
    src/orchestrator/      Delegated tasks, supervisor, worktrees, task records (off by default)
    src/peers.rs, peer_link.rs, peer_cli.rs              Peer hosts
    src/security.rs, launcher.rs, daemon.rs, doctor.rs   Pairing/tokens, setup, background mode, diagnostics
    src/bin/mock_acp_agent.rs
    tests/
  protocol/                Shared Rust wire types (ACP v1)
  tray/                    System-tray controller (separate Cargo workspace)
  android/universal-acp/
    app/                   Compose app: transport, data, storage, security and presentation packages
    core/protocol/         JVM-tested ACP models, reducer and diff logic
  desktop/                 Electron client: electron/ (main process: host API, sockets, git, secrets,
                           approval watcher), src/ (React renderer), test/ (node and Vitest tests)
  docs/  examples/  scripts/  docker/  assets/
```

The Android app keeps transport, data, storage, security and presentation in app packages. Create a Gradle module only when its boundary has working code and tests, not one per screen, event variant or agent. The desktop app talks to the same host API as the phone; see [desktop/README](../desktop/README.md).

## Key data models

| Model | Owner | Purpose |
| --- | --- | --- |
| `AgentDefinition` | Host registry | ID, display name, command, argument array, env, transport, enabled state, cwd, icon |
| `Discovery` | Host registry | Resolved executable, installed flag, status, optional version |
| `Config` / `RuntimeConfig` | Host | Allowed workspace roots and bounded resource limits |
| `AcpConnection` | Host protocol | Request routing, callback responses, streams and process lifecycle |
| `AcpSession` | Host session | ACP ID, workspace, connection, prompts and cancellation |
| `SessionMetadata` | Host session | Host UUID distinct from ACP session ID, agent ID, initialization/setup, status |
| `Event` / `Replay` | Host session | Sequence cursor, original message, replay gap and unresolved permissions |
| `HostProfile` | Android | Host identity, address, device identity and credential alias |
| `SessionState` | Android | Streaming content, patched tools, permissions, capabilities and generation state |
| Credential alias | Android | Reference to Keystore-encrypted credentials, never plaintext Room fields |

Upstream ACP v1 types define initialization, session responses, capabilities, permissions, updates, tools, files, terminals and content. Forwarded payloads use JSON values so unknown fields survive storage and replay. Validate known messages without stripping their original representation.

## Rust module responsibilities

`registry` loads the custom file (there are no built-in agents). Duplicate IDs within a file fail validation; agents the phone registered are added from `user-settings.json`. Discovery resolves explicit paths and absolute PATH entries without executing the target. It does not guess ACP support for every executable on the machine.

`connection` launches an executable with literal arguments. One actor owns stdin and process lifecycle. A separate bounded stdout reader preserves partial frames across actor select branches. The actor correlates internal request IDs, detects exit, enforces a deadline, drains stderr without logging its contents, and fails outstanding requests when the process closes. There is one outstanding prompt per process.

`session` validates canonical workspaces, negotiates version 1, retains original initialization capabilities, and creates or loads a session. Creation is serialized to enforce limits. Each host session currently has its own process, which isolates request IDs, cancellation, and crashes. A manager retains sessions when subscribers detach.

`api` exposes lifecycle operations and transport metadata. It forwards ACP requests without translating coding actions into agent-specific HTTP endpoints. Authentication belongs at the host API boundary; provider authentication remains ACP/agent-owned.

`process_tree.rs` owns Windows cleanup jobs (processes start suspended and resume only after assignment) or Unix process groups for agent and terminal lifetimes. `terminal.rs` supplies the bounded terminal runtime. The connection actor brokers command consent, routes all five terminal callbacks, waits for exits without blocking its reader and publishes retained output for the phone. Executable and WebSocket regression tests run on Windows and Linux; descendant cleanup has platform-specific tests on both.

## Ownership and recovery

```mermaid
sequenceDiagram
    participant Phone
    participant Host as acpd
    participant Agent
    Phone->>Host: Create session with agent ID and workspace
    Host->>Agent: initialize version 1
    Agent-->>Host: Capabilities
    Host->>Agent: session/new
    Agent-->>Host: ACP session ID
    Phone->>Host: Connect to host session
    Phone->>Host: ACP session/prompt
    Host->>Agent: ACP session/prompt
    Agent-->>Host: session/update
    Host-->>Phone: Sequenced original ACP message
    Note over Phone,Host: Phone disconnects; host keeps agent alive
    Agent-->>Host: session/request_permission
    Note over Host,Agent: Permission waits; no implicit approval
    Phone->>Host: Reconnect with cursor
    Host-->>Phone: Replay and unresolved permission
    Phone->>Host: Explicit permission response
    Host->>Agent: ACP response with selected option ID
```

Ownership and replay are tested through subprocess and WebSocket integration tests. Network hosts retain a protected session metadata catalog across restarts. Recovered records are interrupted and have no live process, event journal or pending permissions. An explicit resume creates a new host record through capability-checked `session/load`; an arbitrary agent process cannot be restored by inventing a new session.
