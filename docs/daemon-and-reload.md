# Background host and soft reload

9 October 2026, implemented locally on `codex/session-model-picker`. Use the updated `acpd` executable and the same operator account/profile as setup.

## Commands

From `C:/Users/basin/acportal/target/debug` on this development machine:

```powershell
.\acpd.exe start --background
.\acpd.exe daemon status
.\acpd.exe pair
```

The start command waits for listener/TLS initialization and then returns. The host runs without keeping the launching terminal open. `acpd daemon start` is an alias for background startup. This is a detached user process, not a Windows service, boot task, automatic crash-restart policy or systemd installation. Agent/provider configuration remains that of the launching operator.

Edit the existing host TOML or agent registry, then explicitly apply it:

```powershell
.\acpd.exe daemon reload
```

Stop it gracefully:

```powershell
.\acpd.exe daemon stop
```

Stop interrupts active conversations, closes agents through the existing cleanup boundary and preserves device credentials/interrupted metadata. Restart with `start --background`; running processes and pending prompts are never automatically resumed.

Configured foreground `acpd start` also exposes the same operator control commands. Ctrl+C remains available; Unix SIGTERM enters graceful shutdown. Without a selected/existing config file, the legacy implicit-default foreground mode has no managed control record. Run setup with an existing config before using background mode.

`--config` and `--profile` must select the same configuration identity used at startup. Setup can be rerun; switching to a different config selects a different managed instance. Stop the old instance first or explicitly select its original config/profile to manage it. Control commands still work when the host TOML is invalid or missing. Changing setup's remembered address/name affects future pairing forms, not an already running listener.

## Reload rules

| Setting | Result |
| --- | --- |
| Registry definitions, enabled agents, executable/arguments/environment and own-tools declarations | Fresh discovery and future launches use the new registry; existing agents retain their launch definitions |
| Workspace roots | Browse/new launches use new roots; existing and in-flight sessions keep their original authorization |
| Request timeout, session limit, history event/byte limits | Future launches use new values; lowering the session limit does not evict existing sessions |
| Shell review settings | Future launches only; pending approvals are unchanged |
| Listener, TLS paths, token lifetime, state directory, frame size, logging | Whole reload rejected with restart-required message |

Config and registry are both parsed/validated before an atomic settings snapshot is published. Invalid input or any restart-only change keeps the whole previous snapshot. TLS certificate/key content replacement also needs restart, even if their paths are unchanged. An explicit startup `--registry` override remains the registry source during reload.

Reload is not an immediate revocation of existing session workspace permissions. Stop/restart for that effect. It neither sends a prompt nor approves, rejects or resends a pending decision. There is no automatic file watcher or mutation retry.

## Local control, logging and recovery

An ephemeral IPv4 loopback listener accepts only status/stop/reload with a random operator capability from a protected control record. This is separate from the authenticated phone REST/WebSocket API and cannot execute arbitrary shell commands. Control records/locks live in `daemons/` beside the selected launcher profile, keyed by the absolute config path. Keep these files private; they contain a local capability, never a phone token or provider credential. Do not copy them into logs, repository artifacts or shared backups.

Requests are limited to 4 KiB, replies to 2 KiB, 16 simultaneous handlers and bounded deadlines. Failed/unauthorized input is discarded without payload logs. Status authenticates a live control connection rather than trusting a PID file. Stop never terminates a saved PID. Stale records are replaced only after the worker acquires the exclusive instance lock; startup timeout cleanup uses only its own newly created process handle.

Configure the existing bounded `[logging] file` sink before background startup if persistent lifecycle diagnostics are wanted. Without it, detached stdout/stderr are unavailable; no unbounded capture file is created. Startup failures direct the operator to foreground start/doctor. An unreachable control record may be stale or the worker may be unavailable; it is not proof that an arbitrary PID should be killed. Timed-out mutations are not automatically retried.

Scoped Windows implementation checks and limitations are recorded in [TODO_DONE](../TODO_DONE.md). Unix detachment/SIGTERM, macOS, Linux/minimum-Rust/container/reproducibility must still be verified in GitHub Actions. Physical phone/provider behavior and OS service installation are not established by the isolated fixtures.
