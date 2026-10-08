# Protocol and host API

## Implemented ACP boundary

Session creation accepts optional `workspaceAccess` with the three required booleans `readFiles`, `writeFiles` and `terminal`. Omission preserves the previous all-enabled default. A provided partial or unknown-field policy is rejected. The effective immutable policy is returned in session metadata and persisted in the restart catalog. It determines both the ACP initialization capabilities and host-side callback enforcement. Disabled callbacks return JSON-RPC method-unavailable errors without publishing permission requests or running the operation. Changing saved phone preferences affects a new or explicitly loaded agent process, not reconnection to an existing process.

ACP v1 uses UTF-8 JSON-RPC 2.0 messages delimited by newline on stdio. Requests have numeric or string IDs; notifications have no ID. Agent callbacks may arrive while a prompt request is pending. The host must read continuously while awaiting a response. See the [official transport specification](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/protocol/v1/transports.mdx).

The host explicitly sends `protocolVersion: 1` and `clientInfo`, and rejects a different negotiated version. Managed sessions advertise `clientCapabilities.fs.readTextFile`, `writeTextFile` and `terminal` from their effective workspace policy. All three default to true. The standalone probe advertises an empty capability set because it has no managed workspace services. Optional agent capabilities default to unsupported. See [v1 initialization](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/protocol/v1/initialization.mdx).

`fs/read_text_file` follows the [stable filesystem specification](https://agentclientprotocol.com/protocol/v1/file-system). Only the attached ACP session ID and absolute paths inside its selected workspace are accepted. Line ranges start at 1 and preserve newline characters. The service rejects directories, binary/invalid UTF-8 input, and files larger than `(max_frame_bytes - 256) / 6`; this reserves room for worst-case JSON escaping. A read has a five-second deadline and the host allows at most eight concurrent filesystem workers. A timed-out worker retains its slot until the OS operation finishes. Reads during `session/load` are supported before the load response arrives. Denials use a generic JSON-RPC error without leaking an outside path or file content. Refused reads, writes and terminal requests use `-32602`; a callback the host cannot serve yet (session not ready, permission capacity) uses `-32603`; a callback still pending at `session/cancel` gets `-32800`. ACP reserves `-32000` for `auth_required`, so the host never uses it for these refusals; real Goose reported the earlier `-32000` answers to the model as "Authentication required". When the host answers a client request on an agent's behalf, it keeps the agent's numeric code and replaces the message with "Agent request failed"; host-side failures (request cache or retained-response limits) use `-32603`.

`fs/write_text_file` accepts bounded UTF-8 text at an absolute workspace path after session setup completes. Each old/new text is limited to `(max_frame_bytes - 256) / 18`, reserving space for the permission request and diff. The host creates a `session/request_permission` message with a unique host ID, `_meta.acpdSource: "host-filesystem"`, a proposed diff and allow-once/deny-once choices. The original agent callback stays pending until that decision. These messages use `direction: host`; they do not invent a separate file-edit API. A reconnect restores the pending choice. A duplicate answer cannot commit twice.

Before replacement, the host checks that the file contents still match the approved snapshot. It writes through the workspace directory capability to a temporary file, syncs it, then atomically renames it over the destination. It rejects leaf symlinks, directories, binary files and outside parents. Content checks reduce accidental overwrite but are not an atomic compare-and-swap against other programs. Windows replacements preserve the existing DACL. Unix replacements preserve mode bits; other ownership and extended metadata need further verification. Preparation and commit share the read service's worker limit and five-second deadlines. A timed-out commit has an ambiguous outcome and must not be retried automatically. Writes during initial creation/loading are rejected because the controller cannot yet review consent.

`session/new` includes a canonical `cwd` and an empty `mcpServers` array. `session/load` is called only when `agentCapabilities.loadSession` is true. Prompts currently contain text content blocks. Streaming messages preserve the original JSON values, including unknown payload fields. The schema dependency has no unstable features enabled.

If initial session setup returns ACP `auth_required` and the initialized agent advertises authentication methods, the host retains that process as a managed record with `status: authentication_required` and an empty `acpSessionId`. The phone shows its protocol-driven methods. It sends the standard `authenticate` request with the advertised `methodId`, then the host completes the original new/load operation on that same connection. The phone refreshes session metadata after success and during reconnect replay. Until setup finishes, prompts are unavailable. Terminal and unknown authentication types are not sent to `authenticate`; terminal authentication is not advertised by this host. Provider credentials stay in the agent's own login flow. See [ACP authentication](https://agentclientprotocol.com/protocol/v1/authentication).

The phone's Session options page uses the agent's `configOptions` in their supplied order, with grouped select values and descriptions. When configuration selectors are supplied, legacy mode/model controls are replaced. Unknown option types are ignored; boolean capability is not advertised. A successful `session/set_config_option` response replaces the complete configuration array, including dependent options. Legacy model changes use the matching successful JSON-RPC response before updating the displayed selection. Configuration errors do not mark an unrelated prompt complete. See [ACP session configuration](https://agentclientprotocol.com/protocol/v1/session-config-options).

The connection actor assigns monotonically increasing internal request IDs. Agent permission callback IDs belong to the opposite request direction and remain untouched. Responses from the local user must match a pending callback and one of its supplied option IDs. Duplicate answers fail. On prompt cancellation, the host responds to pending permissions with a cancelled outcome before forwarding `session/cancel`.

Malformed or oversized agent output closes the connection and fails waiting requests. Initialization and setup use absolute request deadlines. Prompts use an inactivity deadline renewed by agent updates; it pauses while permission or a terminal exit callback is pending and renews after that wait completes. Terminal commands still have a one-hour runtime limit. A timeout terminates the connection because blindly retrying a prompt could run an operation twice. Oversized caller requests fail without killing a healthy agent. Raw agent error messages are not copied into diagnostics.

The terminal runtime in `acpd/src/terminal.rs` is independently implemented and tested. It prepares a command for consent, resolves its executable, launches literal argument arrays, captures both output streams with separate UTF-8 decoders, retains bounded output and supports exit waiting, kill and release. It limits each session to eight handles and the host to 32 live terminal processes. Runtime shutdown and service drop terminate ordinary descendants through the process cleanup guard. A one-hour runtime ceiling and bounded post-exit draining prevent indefinite command retention.

All five [stable ACP terminal methods](https://agentclientprotocol.com/protocol/v1/terminals) are routed after session setup. `terminal/create` waits for a host `session/request_permission` before launching anything. Denial, cancellation or session end (pending consent answered with `-32800`) prevents creation. Approval authorizes that request once; reconnect replay restores an unresolved choice without launching it again. Commands requested during initial creation/loading are rejected because a controller cannot yet review their consent.

Two consent shapes exist:

- **Plain program** (`_meta.acpdSource: "host-terminal"`). The agent sent a command plus a separate `args` array, or a single word with no shell metacharacters. Consent shows the resolved executable, literal arguments, cwd and environment variable names. Options are `acpd-command-deny` / `acpd-command-allow` ("Run once"). Approval launches that executable with those args; no shell.
- **Shell line** (`_meta.acpdSource: "host-shell-command"`). The agent sent a command string with empty `args` that contains whitespace or shell metacharacters (`& | ; < > ( ) $ \` " ' * ? [ ] { } ~ ! # % ^`, and on Unix a backslash). Consent title is "Run shell command". `rawInput` carries `shellLine` (the exact text), `shell` (`/bin/sh -c` on Unix, `cmd.exe /D /S /C` on Windows), `executable`, `cwd` and `environmentNames` — never env values, and never the plain `command`/`args` keys so older clients do not show a truncated preview. Options are `acpd-shell-deny` / `acpd-shell-allow` ("Run this shell line once"). There is no remembered or blanket approval: each line needs its own phone choice. Approval runs exactly that string through the fixed shell (`/bin/sh -c <line>` on Unix; on Windows `%SystemRoot%\System32\cmd.exe` with `/D /S /C "<line>"` via `raw_arg`, rejecting CR/LF in the line). A plain-command option id cannot approve a shell consent, and vice versa.

Detection is conservative: any non-empty `args` stays on the literal path even if the text looks like shell syntax. A single unresolvable word with no metacharacters still fails resolution, as before. Existing protections (cwd containment, process-group / job cleanup, output and handle limits, metadata-only logging of the method) apply to both shapes. Session cancel still calls `stop_all` on live terminals so a running shell and its grandchildren are cleaned up.

`terminal/output`, `wait_for_exit`, `kill` and `release` validate the attached session and handle. Exit waits run separately so callbacks and cancellation remain responsive. Changed output is published at most every 250 milliseconds as standard tool updates with a terminal reference and host `_meta.acpdTerminal` snapshot. Final output and exit status are published before release. Snapshots are bounded to 64 KiB or the configured frame budget, whichever is smaller; Android retains the latest 16 terminal snapshots and trusts these extensions only from host delivery events. Working-directory checks and explicit consent do not sandbox command arguments or filesystem access.

## Implemented management API

All routes except pairing require an Authorization bearer header. Pairing codes are created by the local operator CLI.

| Method | Route | Purpose |
| --- | --- | --- |
| GET | `/v1/status` | Version, host identity and health |
| GET | `/v1/agents` | Registry metadata and executable discovery |
| GET | `/v1/workspaces` | Allowed folders and recent workspaces |
| GET | `/v1/workspaces/browse` | Canonical child folders inside allowed roots |
| GET | `/v1/sessions` | Host-owned session metadata |
| POST | `/v1/sessions` | Create or advertised load with agent ID and workspace |
| GET | `/v1/sessions/:id` | Initialization, setup, status and ACP session ID |
| DELETE | `/v1/sessions/:id` | Explicitly terminate process and remove host record |
| POST | `/v1/pair` | Rate-limited one-use code exchange over TLS |
| DELETE | `/v1/device` | Revoke the current device's credential |
| WS | `/v1/sessions/:id/connect` | ACP messages and bounded reconnect replay |

Creation input:

```json
{"agentId":"goose","workspace":"/home/user/projects/app","loadSessionId":null}
```

The host UUID in these paths identifies a managed process/session record. It is distinct from the agent's ACP session ID. Management errors use an HTTP status and stable machine-readable code. Raw registry env, authorization headers, and process arguments must never appear in discovery or error responses.

Network hosts persist session metadata under the protected state directory. After restart, retained records appear in list/detail responses with `status: interrupted`; their WebSocket endpoints have no live process. `activeSessions` counts managed processes only. The phone shows an interrupted-session page with an explicit resume action when loading was advertised, or a new-session action otherwise. Resume repeats initialization and checks the current agent's load capability before calling `session/load`. It creates a new host UUID and preserves the requested ACP session ID. Neither prompts nor unresolved permission decisions are replayed across a daemon restart. Deleting a retained record removes it from subsequent restarts.

The catalog uses atomic protected writes, a lifetime single-daemon file lease, a versioned format and limits of 256 records and 64 MiB. Corruption or unsupported versions fail startup rather than silently erase history. Current workspace policy is revalidated before exposing recovered metadata. The local standalone chat does not create a host catalog. Persisting the delivery journal remains separate work; optional phone caches and the agent's own loaded history currently provide conversation continuity.

## WebSocket framing

Use the subprotocol `acpd.v1`, an Authorization bearer header, and a reconnect cursor in connection setup. Do not put credentials in URL query parameters. Android sends plain ACP requests, notifications, and permission responses as JSON text frames. The gateway routes request IDs to the session actor and returns the original client ID in responses.

Host events have an outer delivery envelope:

```json
{
  "type": "event",
  "sequence": 42,
  "direction": "agent",
  "message": {
    "jsonrpc": "2.0",
    "method": "session/update",
    "params": {
      "sessionId": "agent-session-id",
      "update": {
        "sessionUpdate": "agent_message_chunk",
        "content": {"type":"text","text":"Inspecting the project."}
      }
    }
  }
}
```

The outer sequence is host delivery metadata. The inner message retains ACP semantics. No host-specific `runCommand`, `editFile`, or `askAgent` coding methods will be introduced.

`POST /v1/sessions` accepts optional `mcpServers` in the stable ACP wire format. The host validates a maximum of 16 definitions and 12,000 serialized bytes, unique names, absolute stdio executable paths, literal arguments, environment/header values and HTTP/SSE URLs. HTTP and SSE are checked against `agentCapabilities.mcpCapabilities` after initialization. Unsupported transports produce the safe API code `unsupported_mcp_transport` and leave no live session. Accepted definitions reach `session/new` or `session/load`, including setup retried after advertised authentication. Definitions remain in process memory; the host does not add them to restart metadata.

This follows the [ACP session setup specification](https://agentclientprotocol.com/protocol/v1/session-setup). MCP programs and HTTP requests are run by the agent on the selected host. The client does not claim that these servers are connected merely because their definitions were saved.

Subscribe before reading replay. Return replay through its latest sequence, then ignore queued live events at or below that sequence. A cursor older than the retained journal yields an explicit gap; never silently claim complete history. Include unresolved permissions separately from the bounded history so eviction cannot hide an outstanding decision. Android reducers deduplicate permission IDs and tool-call IDs as well as sequence numbers.

One controlling client lease is allowed per session. Observers explicitly request `readOnly=true`; a second controller gets HTTP 409. Each session-scoped message must target the attached ACP session ID. Connect with `?after=42` to replay after a cursor. Delivery includes `replay_start`, individual `event` and `pending_permission` envelopes, then `replay_complete`. Accepted client prompts and permission answers are journaled with `direction: client`; agent output uses `direction: agent`. Pending/completed client request IDs are retained in a bounded memory cache for duplicate protection. Clients must not automatically resend prompts after a socket closes. A transport loss does not cancel the prompt or approve permission.

Live delivery holds only weak references to journal events. If the journal evicts an event before a slow socket receives it, or the live queue overflows, the host sends `reconnect_required` with reason `consumer_lagged` and the client reconnects from its cursor; the replay reports the gap. Host-bound requests beyond a per-session byte budget of max(8 MiB, 2 × max_frame_bytes) fail immediately with a retryable error; cancellation is never refused for this reason. Ping/pong, bounded frames, backpressure and credential revalidation are implemented. Android reconnects with capped exponential backoff and jitter. Closing a transport releases its lease, not the session process. Rust network tests cover authentication, permission recovery, replay, duplicate prompts, mismatched session IDs and revocation.

Android retains incoming text, image, audio, embedded resources and resource links from message chunks and tool content. The same standard blocks are used for capability-checked outgoing attachments. Optional message IDs preserve message boundaries. Session metadata patches and complete usage snapshots follow the current [prompt-turn update definitions](https://agentclientprotocol.com/protocol/v1/prompt-turn) and [session metadata semantics](https://agentclientprotocol.com/protocol/v1/session-list). Unsupported content variants remain visible through a fallback; no agent URL is fetched automatically. Media bytes are omitted from Markdown export while embedded text and resource identities remain readable.
