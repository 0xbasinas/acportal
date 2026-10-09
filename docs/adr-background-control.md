# ADR: managed local worker and immutable launch snapshots

Status: Accepted for this implementation. Date: 9 October 2026. Requested by the user as native background mode and soft config reload.

## Context

Foreground-only startup requires an open terminal. Existing credentials, interrupted metadata, live ACP actors, controller leases and pending approvals must survive a configuration reload without implicit execution or decisions. A saved PID cannot establish process identity or authorize stopping it.

## Decision

Launch a detached copy of the existing acpd binary with literal arguments. Windows uses CreateProcessW with handle inheritance disabled; Unix creates a new session. The worker holds an exclusive operator-owned lock and publishes a protected capability for an ephemeral loopback lifecycle channel. It reports ready after listener/TLS initialization. Graceful stop uses the existing session shutdown boundary. No service installation or auto-restart is introduced.

SessionManager owns an atomic Arc snapshot of launch config and registry behind a short RwLock. Discovery/browse and future launches read the current snapshot. In-flight/active sessions retain their initial settings and permissions. Reload validates the full candidate and rejects all of it if listener/TLS/auth-storage/frame/logging settings differ. See [operational rules](daemon-and-reload.md).

## Alternatives

A Windows service/systemd installation would provide boot/restart supervision, but requires separate account, installation and provider-environment decisions. A PID-only stop command can hit an unrelated reused PID and was rejected. Rebuilding the entire Host on reload would interrupt conversations and controller state, so it was rejected. A file watcher adds implicit mutations and partial-write races; explicit reload makes failures reviewable.

## Consequences

The CLI can manage a foreground or detached configured host without phone credentials. Live sessions remain stable through reload, with scoped request and concurrency bounds. Root/review changes apply to future sessions; immediate revocation needs a graceful stop/restart. Detached logging uses the existing optional bounded sink. Platform lifecycle verification and service installation remain distinct acceptance work.
