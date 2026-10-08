# Remaining work

Updated 8 October 2026. Implementation resumed by user authorization on 8 October 2026. This checklist contains active and suspended work; completed items and chronological verification history are in [TODO_DONE.md](TODO_DONE.md). Historical passing checks do not establish production completion.

Local agent: Goose ACP. Phone connections use authenticated REST/WebSocket; the host communicates with agents over ACP stdio. Follow [HANDOFF.md](HANDOFF.md) and [the requirements audit](docs/requirements-audit.md) for context and scope. The supplied mockup guide is [docs/ui-reference.md](docs/ui-reference.md).

## Next priorities

Maintenance: periodically move completed checklist items and their verification notes to [TODO_DONE.md](TODO_DONE.md), preferably at each finished increment or handoff. Preserve their wording, evidence and limits; keep remaining follow-up work here. Do not mark unverified work complete merely to archive it.

Scope update, 8 October 2026: tablet/landscape acceptance is excluded from the current work. Physical-phone checks are suspended and owned by the user. Signed APK production/verification is suspended. Supported-build verification must run in GitHub Actions rather than locally. Historical results in TODO_DONE.md remain evidence of earlier checks, not current task requirements.

- [ ] Finish Connection logs TalkBack/accessibility review and physical-network recovery. Current semantics/layout checks use component events rather than live connection failures.
- [ ] Verify Agents TalkBack and lifecycle acceptance, then remaining page recovery and previous-task restoration.
- [ ] Broaden MainActivity task recovery to more routes, conversations/pending permissions and failure states, then remaining error/accessibility states and the page matrix. Abrupt-kill recovery launches a fresh Activity through instrumentation; it does not restore the prior task. Current refinements follow `docs/ui-reference.md`; the supplied mockup folder remains absent at its recorded path.
- [ ] Broaden folder browsing across physical-network transitions and configuration changes/process death. Controlled HTTP navigation covers New session and Workspace access; it does not prove physical-device lifecycle behavior.
- [ ] Broaden UI-state acceptance across page scroll positions, expanded activity, configuration changes and full-session process death. Physical agent-cache network transitions and prompt-history keyboard/cache/process-death acceptance remain open.
- [ ] Complete remaining operational gaps: deeper doctor TLS/provider/free-space diagnostics and a bounded host logging sink. Listener/storage probes and runtime-limit reporting now pass scoped checks. Raw verbose ACP logging is optional and must preserve privacy.
- [ ] Broaden rich-content and thought acceptance checks across device codecs, accessibility, real agents and lifecycle/process death.
- [ ] Broaden repository/ViewModel recovery coverage and route-specific error presentation. Local mock-host credential renewal passes. Physical-device pairing/network/controller checks are suspended below for the user.
- [ ] Broaden offline conversation acceptance across pending-decision reconciliation after process death, message-cache settings, document providers and small/large-font keyboard layouts. Physical-network checks are suspended below for the user.
- [ ] Finish remaining auxiliary-state limits and stress/heap checks. Timeline, terminal/session-info, permission, Android incoming/outgoing queues, host command/reader queues and request-cache content now have scoped limits and tests. Remaining configuration/model correlation fields, broadcasts, waiting callers and allocator overhead need review. Verify permission overflow through repository/device navigation and profile bounded attachment serialization on the UI thread.
- [ ] Verify the complete real Goose ACP workflow: file read/write, full diff review, terminal callbacks, permission rejection/approval, cancel and explicit resume. A real Goose handshake and simple prompt have passed; the broader workflow currently uses the mock agent.

## Product and release checks

- [ ] Review every implemented page against all 16 supplied mockups. Check large fonts, accessibility, keyboard behavior, and dark/light loading, empty and error states. Broaden concurrent-session process-death acceptance using isolated emulator fixtures.
- [ ] Complete persistence checks: Room migrations, process death, restart metadata and explicit load. Decide whether optional host delivery-journal persistence is needed. Unsent attachment payloads intentionally remain in memory and must be selected again after process death.
- [ ] Close the Windows process-launch-to-JobObject assignment gap and verify Unix process cleanup, metadata ownership and workspace containment. The existing policy controls are not an OS sandbox.
- [ ] Configure GitHub Actions to verify the declared Rust minimum version, Linux/container builds and reproducible Windows builds. Verify actual workflow results on GitHub; do not perform these supported-build checks locally.
- [ ] Finish runtime/path/authentication diagnostics. Disabled custom registry templates and configuration guidance are documented and CLI-parsed; actual adapter negotiation remains unverified.
- [ ] Finish an acceptance report covering the current scope. Installation/trusted-TLS instructions are documented; physical TLS deployment and signed delivery remain suspended and unverified. Keep optional biometric unlock, QR pairing and future protocol features separate.

## Suspended work

- Physical Android phone testing is user-owned and not required from the agent for now: trusted TLS, Wi-Fi/mobile-data switching, lock/unlock, backgrounding, offline cache, credential renewal/expiry/revocation and competing controllers. Emulator evidence does not verify these behaviors.
- Producing and verifying a signed release APK is not needed for now. Resume only when requested; ordinary debug builds and implementation checks remain in scope.
