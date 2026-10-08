# ACP Portal

An Android client and Rust host for coding agents that speak Agent Client Protocol. Agent commands live in a host registry. The Android client will never need a release to learn a new agent's executable or arguments.

The repository includes the Rust host, authenticated WebSocket API, pairing service, and a compiling Android client. Secure WebSocket is the phone connection transport. The Android pages cover hosts, pairing, agent/workspace selection, sessions, settings, streamed conversation, permissions, tool output and basic diffs. See [milestones](docs/milestones.md) for verification status and remaining work.

The implemented local client uses **Goose ACP** by default. Tests use a deterministic ACP executable with no model subscription. Filesystem tests write only inside temporary test workspaces.

## Run locally

Requires Rust 1.88 or newer and the Goose CLI on `PATH`. The supplied config allows this repository as a workspace and sets `GOOSE_MODE=approve` in the registry. Configure your provider through Goose itself. ACP Portal does not read or copy provider credentials.

```powershell
cargo build --locked --workspace --bins
cargo run --locked -p acpd -- --config examples/config.toml agents
cargo run --locked -p acpd -- --config examples/config.toml probe --workspace .
cargo run --locked -p acpd -- --config examples/config.toml chat --workspace .
```

`probe` negotiates ACP v1 without a model prompt. `chat` creates a real Goose session. Enter prompts, respond to numbered permission options, and use `/quit` to leave. Ctrl+C during a prompt sends ACP cancellation. A one-shot prompt is also supported:

```powershell
cargo run --locked -p acpd -- --config examples/config.toml chat --workspace . --prompt "Say hello without using tools or changing files."
```

The local client prints the ACP session ID. Use `chat --workspace . --load "session-id"` with the same config and agent to load it later when the agent advertises that capability. Real Goose session loading has been verified locally.

On PowerShell 7, the equivalent developer commands are:

```powershell
./scripts/dev.ps1 setup
./scripts/dev.ps1 acpd
./scripts/dev.ps1 test
./scripts/dev.ps1 probe
./scripts/dev.ps1 chat
./scripts/dev.ps1 mock
```

On Unix, use `make setup`, `make acpd`, `make test`, `make probe`, and `make chat`. Windows batch wrappers are intentionally not spawned through a shell. Configure an actual executable for adapters on Windows.

## What exists

- Extensible JSON registry, built-in Goose and Gemini definitions, custom overrides, and executable discovery.
- Official stable ACP v1 schema types, JSON-RPC validation, and lossless unknown payload preservation.
- Subprocess actor with bounded frames, request correlation, deadlines, stderr draining, crash detection, Windows cleanup jobs and Unix process groups.
- Capability negotiation, session creation, advertised `session/load`, prompts, streaming updates, and cancellation.
- Agent-provided sign-in choices when authentication is required before session creation or loading.
- Explicit permission choices, validation of agent-supplied options, and cancellation of pending permissions.
- Multiple host-owned sessions, bounded event replay, history gap detection, and retained unresolved permissions.
- Local CLI with discovery, configuration diagnostics, ACP probe, and interactive sessions.
- Unit and executable integration tests for normal lifecycle and failure paths.
- Authenticated REST and WebSocket API, expiring device credentials, revocation, one-use pairing codes, controller leases and reconnect replay.
- Kotlin/Compose Android app with Room session storage, DataStore preferences and Keystore-encrypted credentials.

## Run the Android client

```powershell
./scripts/dev.ps1 start
# In another terminal, with the same config:
./scripts/dev.ps1 pair
./scripts/dev.ps1 android
./scripts/dev.ps1 android-test
```

Install `android/universal-acp/app/build/outputs/apk/debug/app-debug.apk`. In the app, choose **Add host**, enter the address and the pairing code. The code expires after 60 seconds. For an Android emulator connected to the host's default loopback listener, use `http://10.0.2.2:8765`. That cleartext exception is restricted to local development addresses in debug builds.

For a physical phone, configure a reachable listener and TLS certificate/private key in `[server]`. Remote listeners require TLS and the app verifies the certificate using Android's system trust store. Use a trusted certificate; there is no certificate-validation bypass. Release builds accept HTTPS only. Do not expose the development loopback listener by changing its address without TLS.

Managed sessions advertise workspace-scoped `fs.readTextFile`, `fs.writeTextFile` and `terminal`. Reads validate the session ID, line ranges, UTF-8 and size through an opened directory capability. Every write waits for a host permission request with a reviewable diff. Approval applies only to that proposed write; denial, cancellation and changed file contents prevent it. Terminal creation also requires explicit host consent. The phone shows bounded command output and exit status, including after the agent releases its terminal handle. The default mock reports simulated tool content; `scripts/mock-host.ps1 -Terminal` exercises actual ACP terminal callbacks using its own fixture executable.

Active processes and delivery journals are in memory. Dropping a phone connection does not terminate an agent. Phone reconnection replays retained events and unresolved permissions. The phone saves session metadata, drafts and optional message history. The host retains a protected restart catalog: after daemon restart, prior sessions appear interrupted and require explicit capability-checked loading into a new process. Restart never repeats prompts or permission decisions. Exiting the local CLI terminates its agents.

## Repository

```text
acpd/                Rust host library, local CLI, mock agent and lifecycle tests
protocol/            Shared Rust protocol boundary and upstream ACP v1 exports
docs/                Architecture, protocol, Android plan, security and milestones
examples/            Working Goose registry and host configuration
scripts/             PowerShell developer commands
docker/              Host CLI container recipe
android/universal-acp/ Compose Android app and independently tested JVM protocol module
Cargo.toml           Rust workspace
Cargo.lock           Reproducible dependency resolution
Makefile             Unix developer commands
```

Implementation boundaries are documented in [architecture](docs/architecture.md), [protocol](docs/protocol.md), and [Android](docs/android.md).

## Verification

Current scope, 8 October 2026: tablet/landscape acceptance is excluded for now. Physical-phone testing is suspended and user-owned; signed APK production/verification is also suspended. Supported-build verification will run in GitHub Actions rather than locally. TODO lists active and suspended work separately; suspended checks remain unverified.

Latest device checkpoint, 8 October 2026: four repository/recovery tests pass without skips on emulator-5554. Actual local WebSocket permission overflow detaches the client, retains previous approvals and blocks decisions/prompts despite a late replay completion or error dismissal. Both APK builds/install and lint pass. The fixture owns its Room/DataStore/vault and removes its files. Device byte-overflow, page navigation and physical-host acceptance remain open.

Latest Android checkpoint, 8 October 2026: 45 core and 31 app JVM tests, debug build and lint pass with no test failures or skips. Incoming WebSocket queues have an 8 MiB byte budget; outgoing admission caps queued content at 4 MiB. Terminal snapshots and additive session metadata each have a conservative 2 MiB budget. Pending/replay permissions have a combined 8 MiB estimate and 128 entries per map; overflow rejects the whole update and detaches without sending a decision. Prior consent remains intact. Local queue/replay tests pass; repository/device permission-overflow and physical stress acceptance remain open.

Latest host source checks pass 73 Rust tests, strict Clippy and formatting; an extended oversized-input lifecycle regression also passes. Duplicate-request cache and reader/command queues now have content budgets and pre-enqueue frame validation. Doctor reports listener availability, storage write/remove probe results and effective runtime limits. Missing storage probes its existing parent without creating state directories. Logging, deeper diagnostics, remaining auxiliary fields, waiting callers and whole-process heap measurements remain open. Detailed chronological evidence is in TODO and HANDOFF.
```text
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
```

See [development](docs/development.md) for test coverage and the local verification record. Docker is optional and remains unverified here. Its image defaults to help; run the host explicitly with mounted configuration, workspace and state directories and configured TLS for a remote listener.

Latest Android checkpoint, 7 October 2026: 25 app JVM tests, debug/test APK builds, lint and release manifest generation pass. The external task-recovery workflow also passes on actual MainActivity with an isolated debug repository. It restores the same task in a distinct process, returns to the MCP host route and preserves Back navigation through the chooser to Settings. Unsaved MCP drafts are discarded; saved definitions remain available.

Two separate JUnit storage phases verify fixture data/alias and unchanged production hosts, sessions, MCP aliases and durable UI preferences. Three normal debug lifecycle/layout regressions also pass. The recorded fixture task, callbacks and storage are cleaned up, and the normal repository is restored. Release keeps the production Application. See [task recovery](docs/task-recovery.md) for the exact procedure and limits.

Earlier increments cover compact dark/light Connections, logs, Agents, Sessions and Changes controls, MCP encrypted-storage failures, long values and Activity recreation. Detailed chronological results are in [TODO](TODO.md) and [screen acceptance](docs/screen-acceptance.md). Earlier 40 core protocol JVM and 67 Rust test results were not rerun for these Android increments.

The project is not production complete. More lifecycle routes and pending-permission recovery, page/accessibility checks, memory limits, host hardening, full real Goose workflows, physical-device trusted TLS/network/background behavior, supported-platform reproducibility and signed release delivery remain open. See the [scope audit](docs/requirements-audit.md) and [handoff](HANDOFF.md).

## Documentation

- [Architecture and key models](docs/architecture.md)
- [ACP forwarding and host API](docs/protocol.md)
- [Adding agents and Goose setup](docs/agents.md)
- [Android modules and pages](docs/android.md)
- [Security boundaries](docs/security.md)
- [Development and testing](docs/development.md)
- [Implementation milestones](docs/milestones.md)
- [Original request audit and remaining acceptance gates](docs/requirements-audit.md)
- [Contributor context and test entry points](AGENTS.md)
- [Current continuation checklist](TODO.md)
- [Previous-task recovery verification](docs/task-recovery.md)

ACP v1 details follow the [official protocol repository](https://github.com/agentclientprotocol/agent-client-protocol) and its [v1 initialization specification](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/protocol/v1/initialization.mdx). Schema package versions and negotiated ACP wire versions are separate. This project pins `agent-client-protocol-schema` 1.10.2 and negotiates wire version 1.
