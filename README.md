# ACP Portal

9 October 2026 daemon/reload increment: native background mode and operator-only local status/stop/reload are implemented. SessionManager publishes coherent launch snapshots; discovery/browse/future sessions use new settings while current sessions and approvals remain unchanged. Listener/TLS/token/storage/frame/logging changes require restart. See [daemon commands and reload rules](docs/daemon-and-reload.md), the ADR and TODO_DONE.md for actual verification and limits.

9 October 2026 native CLI/motion increment: `acpd setup` remembers an existing config and HTTPS address; `acpd start`, `acpd pair` and `acpd doctor` then use that setup. No wrapper script is required. Android ordinary push/Back now use brief overlapping fades, and selected-tab taps no longer rebuild the route. See [native CLI and motion](docs/native-cli-and-motion.md) and TODO_DONE.md for scoped evidence and remaining phone profiling.

9 October 2026 easier pairing and Markdown on `codex/session-model-picker`: Android offers QR scanning and simpler manual entry; `acpd pair --address HTTPS-ADDRESS --qr --name NAME` prints a terminal QR without changing authentication. Scans fill a reviewable form and never pair automatically. Agent replies render native Markdown with bounded off-Main parsing, literal HTML, no remote image fetch and confirmed web links. See [pairing and Markdown](docs/pairing-and-markdown.md) and TODO_DONE.md for verification and limits.

9 October 2026 All sessions creation implemented on `codex/session-model-picker`: + opens connection selection, then the approved connection/workspace/agent form. Fresh discovery gates Start; pairing, unavailable reads and explicit recovery reuse existing boundaries. Isolated checks pass 14 component regressions, one actual-navigation/failed-creation case and four new-screen cases at actual font 2.0. 52 app JVM tests, both debug APKs and lint pass (0 errors, 16 warnings). Real-phone and native TalkBack verification of this flow remain open. See TODO_DONE.md and docs/session-creation-design.md.

9 October 2026 phone-feedback fixes on `codex/session-model-picker`: catalogs over 40 choices use a searchable lazy dialog; smaller catalogs keep their dropdown. Choice mappings are remembered, advertised IDs remain exact, and opening/filtering/cancelling sends no configuration request. Conversation status now aligns with the subtitle inside the header, above the divider. See TODO_DONE.md for scoped verification and remaining phone checks.

9 October 2026 PR #10 review: metadata refresh now updates only metadata, preserving original oversized history/drafts and archive/timestamps instead of writing guarded read placeholders back. Summary invalidation uses a bounded SHA-256 scan, covering equal-length/equal-timestamp changes and metadata hash collisions; production missing-summary rendering no longer decodes history on Main. Verification: 64 core/52 app JVM tests with zero failures/errors/skips, both debug APK builds and lint (0 errors, 16 dependency/toolchain warnings). Two isolated Room regressions and four dark/light Sessions layout/native-keyboard cases pass on small_phone, API 36, 720x1280, density 320, font 1.0. Full old-build upgrade, precise heap profiling, real-agent Android screens and broader route acceptance remain open. Original platform Actions run 37940712823 was inspected, including macOS RSS/process cleanup, container TLS/restart/revocation and identical cross-runner hashes; Rust sources were unchanged by the review and not locally rerun.

Historical PR #10 checkpoint, 9 October 2026: PR #10 merged on main at `6ec49de`, after review at `d4ba9f2`. Local checks pass 64 core/52 app JVM tests, both debug APK builds and lint; six isolated emulator storage/Sessions tests pass. Current-head [Android Actions](https://github.com/0xbasinas/acportal/actions/runs/37949743658) and [Rust Actions](https://github.com/0xbasinas/acportal/actions/runs/37949743959) pass. This does not establish production completion.

Remaining work is tracked in [TODO.md](TODO.md). Current verification and platform limits are in the [acceptance report](docs/acceptance-report.md); historical results are in [TODO_DONE.md](TODO_DONE.md).

An Android client and Rust host for coding agents that speak Agent Client Protocol. Agent commands live in a host registry. The Android client will never need a release to learn a new agent's executable or arguments.

The repository includes the Rust host, authenticated WebSocket API, pairing service, and a compiling Android client. Secure WebSocket is the phone connection transport. The Android pages cover hosts, pairing, agent/workspace selection, sessions, settings, streamed conversation, permissions, tool output and basic diffs. See [milestones](docs/milestones.md) for verification status and remaining work.

The implemented local client uses **Goose ACP** by default. Tests use a deterministic ACP executable with no model subscription. Filesystem tests write only inside temporary test workspaces.

Opt-in shell review requires manual consent for agent-supplied terminal environment overrides and filename patterns. The Linux testy callback runner isolates upstream fixed `/tmp` paths in a private mount; see [testy verification and limits](docs/testy.md).

[UI/accessibility work](docs/ui-accessibility.md) is active. Scoped emulator checks cover review badges, conversation restoration, conversation/history, pairing and MCP keyboards, and native TalkBack including automatic recovery/Agent focus at normal and actual 2.0 font scale. Broader pages, forms and route focus-restoration acceptance remain open. [HANDOFF.md](HANDOFF.md) records the current continuation checkpoint.

Merged `main` at `981a97a` passes [Rust Actions verification](https://github.com/0xbasinas/acportal/actions/runs/37818450569) and [Android Actions verification](https://github.com/0xbasinas/acportal/actions/runs/37818455265), including the private-mount testy scenarios, Linux shell-review regressions and 47 core/35 app JVM tests. See [verification history](TODO_DONE.md) for coverage and remaining acceptance limits.

## Run locally

After [native setup](docs/native-cli-and-motion.md), run these commands from the directory containing your updated `acpd.exe`:

```powershell
.\acpd.exe start --background
.\acpd.exe daemon status
# After editing the existing config or registry:
.\acpd.exe daemon reload
.\acpd.exe daemon stop
```

Background startup returns once the listener is ready. Reload changes discovery and future-session settings while preserving live conversations and pending approvals. Listener/TLS, token settings, storage, frame size and logging changes require stop/restart. Automatic startup at boot is not installed. See [background operation and reload rules](docs/daemon-and-reload.md).

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

See [installation and trusted TLS](docs/installation.md) for private host configuration, certificate paths, debug installation, pairing and revocation. [Custom registry templates](examples/agents.custom.json) are disabled placeholders for installed native agents and Node adapters.

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

Use JDK 21 for local Gradle commands. Supported minimum-Rust, Linux/container and Windows reproducibility checks belong in GitHub Actions. See [development](docs/development.md) for commands and [the acceptance report](docs/acceptance-report.md) for completed platform checks.

The project is not production complete. Active follow-ups are real-agent Android workflows/recovery, heap/provider profiling, a full old-build upgrade, remaining page/focus/content acceptance, custom-adapter diagnostics and containment gaps. Physical-phone and signed-release checks are suspended; tablet/landscape is excluded. See [TODO](TODO.md) for exact scope.

## Documentation

Checklist maintenance: periodically move completed items and verification notes from [TODO](TODO.md) to [TODO_DONE](TODO_DONE.md), preferably at each finished increment or handoff. Preserve evidence and limits; keep remaining and suspended work in TODO.

- [Architecture and key models](docs/architecture.md)
- [ACP forwarding and host API](docs/protocol.md)
- [Adding agents and Goose setup](docs/agents.md)
- [Android modules and pages](docs/android.md)
- [Security boundaries](docs/security.md)
- [Development and testing](docs/development.md)
- [Real-agent testing through the host](docs/real-agent-testing.md)
- [Implementation milestones](docs/milestones.md)
- [Original request audit and remaining acceptance gates](docs/requirements-audit.md)
- [Contributor context and test entry points](AGENTS.md)
- [Current continuation checklist](TODO.md)
- [Previous-task recovery verification](docs/task-recovery.md)

ACP v1 details follow the [official protocol repository](https://github.com/agentclientprotocol/agent-client-protocol) and its [v1 initialization specification](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/protocol/v1/initialization.mdx). Schema package versions and negotiated ACP wire versions are separate. This project pins `agent-client-protocol-schema` 1.10.2 and negotiates wire version 1.
