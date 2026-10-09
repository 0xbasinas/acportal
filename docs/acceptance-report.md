# Acceptance report

Status on 9 October 2026 (Europe/Athens), branch `remaining-work-batch-6` (PR #10) on top of
main `cf8fa26`. This is a record of what has been checked and where. It is not a claim that the
product is ready for production. Results from other checks are in [TODO_DONE.md](../TODO_DONE.md),
[android-acceptance.md](android-acceptance.md), [ui-accessibility.md](ui-accessibility.md) and
[real-agent-testing.md](real-agent-testing.md). Open work is in [TODO.md](../TODO.md).

## How to read the status column

- **Verified**: a named check ran and passed on the platform listed, and its output was read.
- **CI only**: verified in GitHub Actions only, not on a local machine or a device.
- **Partial**: some of the behaviour is checked and the rest is listed as open.
- **Not verified**: there is no evidence yet. The item is open or has been suspended.

## Merged main

| Commit | Workflow | Result |
| --- | --- | --- |
| `cf8fa26` (PR #9 merge) | [Android 37939942253](https://github.com/0xbasinas/acportal/actions/runs/37939942253) | success: debug build and JVM tests. PR #9 touched only Android, so the Rust workflow (path filters) did not run on this commit. |
| `ada4798` (PR #8 merge) | [Rust 37925483694](https://github.com/0xbasinas/acportal/actions/runs/37925483694) | success: Linux stable (fmt, strict Clippy including the Windows target, tests), Linux Rust 1.88, Windows stable and the testy end-to-end job. The container and reproducibility jobs are manual-only and were skipped. |
| `ada4798` | [Android 37925483582](https://github.com/0xbasinas/acportal/actions/runs/37925483582) | success |

The Rust code on main has not changed since `ada4798`, so that Rust run still describes the
host on main.

## Platform verification (PR #10, manual dispatch)

[Rust run 37940712823](https://github.com/0xbasinas/acportal/actions/runs/37940712823) was dispatched
on commit `404bb7f` with `extras=true` and `testy=true`. All 9 jobs passed. The [PR run 37940716023](https://github.com/0xbasinas/acportal/actions/runs/37940716023)
on the same commit passed its 3 PR jobs.

| Area | Status | Evidence |
| --- | --- | --- |
| macOS host tests | **CI only** | `macos-latest` ran the full `cargo test` (every suite passed; 1 test ignored, the same one as on Linux). |
| macOS process cleanup | **CI only** | These tests now run on all Unix systems (ps-based process-group checks where there is no `/proc`): `unix_descendants_stop_on_session_removal_agent_crash_and_request_timeout` and `unix_terminal_kill_release_and_service_drop_stop_descendants`. Both passed, in the full run and again when run alone. |
| macOS host RSS | **CI only** | `memory_rss` (resident size from `proc_pidinfo`, peak from `getrusage`): 312 MiB streamed over 4 sessions. Resident memory went from 46,160 to 47,312 KiB (+1,152 KiB) and the peak was the same. |
| Windows reproducibility across runners | **CI only** | Two independent `windows-2022` jobs used different checkout paths (`src-first-runner`, `src-second-runner`). Each built twice and got matching hashes. A comparison job found the same `acpd.exe` SHA256 on both: `8C617C3FB7221338ABC35F565BAFD4A10AD70C6D50A6F619E5AF5210411C5FBA`. This applies to these hosted runners and toolchain only. Other machines or toolchains were not checked. |
| Container: TLS | **CI only** | The host listened on `0.0.0.0:8443` with TLS from a CA generated for the test. Plain HTTP was refused. The default trust store rejected the certificate, and the CA-pinned client was accepted. |
| Container: mounted storage | **CI only** | The device token and session were still there after a container restart (the session showed as `interrupted`). Revoking the device made it get 401. State files on the bind mount were owned by uid 10001, with the directory at 700 and files at 600. |
| Container: agent execution | **CI only** | A mock agent inside the container ran a prompt over WSS. A host terminal consent came first, then an agent permission, then the terminal output and the reply, ending with `end_turn`. The mock agent is only in the `smoke` image target; the default production image does not include it (this is checked). |
| Linux stable, Linux Rust 1.88, Windows stable | **CI only** for Rust 1.88 and Windows | All three jobs passed in the dispatch run and the PR run. Linux stable was also verified locally (see below). |

## Host and real agents

| Area | Status | Evidence |
| --- | --- | --- |
| Rust format, strict Clippy (Linux + Windows target), tests | **Verified** (box, Linux) | `cargo fmt --check`; `cargo clippy -D warnings` for host and `x86_64-pc-windows-gnu`; 117 passed, 1 ignored. |
| testy pinned agent | **Verified** (box) | `scripts/run-testy-isolated.sh`: 9 passed. In CI the testy job passed in the dispatch run and on main. |
| Goose 1.53.0 + DeepSeek `deepseek-flash`, host side | **Verified** (box, Linux) | One run on 9 October 2026, 17:06–17:07 Athens, covering inspect, approve, deny, shell-line terminal, kill, reconnect, load, cancel mid-write, outside-workspace read and **form elicitation (accept and decline)**. Details are in [real-agent-testing.md](real-agent-testing.md#batch-6-run-9-october-2026-linux-1707-athens). |
| Goose elicitation | **Verified** (host side) | Goose forwards an MCP server's `elicitation/create` to the ACP client as a form elicitation, but only when the client advertised `elicitation.form`. The answers (accept with content, and decline) reach the MCP server unchanged. |
| Android screens with a real agent | **Not verified** | The checks above drive the host API only. |

## Android

| Area | Status | Evidence |
| --- | --- | --- |
| JVM tests, debug and test APK builds, lint | **Verified** (box) | 115 core and app JVM tests (0 failures, 0 skipped); `assembleDebug`, `assembleDebugAndroidTest` and `lintDebug` pass. Android CI runs on the PR. |
| Older-data safety (oversized or unreadable rows, draft and metadata write limits) | **Partial** | The guarded Room queries, tolerant decoding and write limits are covered by JVM tests (`StoredLimitsTest`). The Room and CursorWindow behaviour itself needs a device: run `OversizedSessionRowsTest` (it compiles, but has not been run). |
| Sessions list memory (decode only changed rows, off the main thread, bounded search text) | **Verified** (JVM) | `SessionListSummaryTest`. |
| Timeline retention without re-serialising history | **Verified** (JVM) | `TimelineRetentionCostTest`. |
| Heap profiling on a device | **Not verified** | Steps are in [android-acceptance.md](android-acceptance.md#heap-profiling-android-studio). |
| Emulator UI, TalkBack and process-death recovery | **Partial** | These are scoped emulator results from PR #8 and PR #9 (Codex). They are recorded in [android-acceptance.md](android-acceptance.md) and were not re-run in this batch. |

## Suspended or owned by the user

- Testing on a physical phone: trusted TLS on the device, network switching, lock/unlock, competing controllers.
- Deploying TLS on a real network with a real certificate.
- Producing and verifying a signed release APK.
- Tablet and landscape layouts are excluded from the current scope.
