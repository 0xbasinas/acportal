# testy end-to-end tests

[testy](https://agentclientprotocol.github.io/rust-sdk/testy.html) is the deterministic ACP test agent from the official Rust SDK ([agentclientprotocol/rust-sdk](https://github.com/agentclientprotocol/rust-sdk), crate `agent-client-protocol-test`). The prompt text picks a scenario. `acpd/tests/testy.rs` registers testy as an ordinary agent through the registry and drives it through the real host router and the phone WebSocket (`acpd.v1`), answering consents the way a phone would.

## Pinned revision

`scripts/build-testy.sh` pins rust-sdk tag **v3.2.0** (commit `5c41d62297eb74cce06daac8b3a487c6c453d552`, 8 October 2026). v3.0.0 through v3.2.0 lock `agent-client-protocol-schema` 1.10.2, the exact version acpd pins (`=1.10.2`); v3.2.0 is the newest of them. When acpd moves to a new schema version, move the pin to a rust-sdk release that locks the same schema, then rerun the tests. Some assertions count testy's updates per scenario; update them if testy changes.

## Run locally

```sh
ACPD_TESTY_BIN="$(scripts/build-testy.sh)" bash scripts/run-testy-isolated.sh
```

The build script clones rust-sdk into `${ACPD_TESTY_CACHE:-~/.cache/acpd-testy}` outside the repository, checks out the pinned commit, builds `testy` with the SDK's own lockfile and prints only the binary path. Without `ACPD_TESTY_BIN` every test prints a skip message and passes immediately. The tests are Unix-only.

The Linux runner requires `jq`, `sudo`, `unshare`, `mount` and `setpriv`. It compiles the test executable, creates a private mount namespace and mounts a fresh tmpfs at `/tmp`, then drops back to the invoking user's UID/GID before starting the tests and agent. This isolates testy's fixed `/tmp/testy-write.txt` and terminal cwd from the machine's files. Namespace teardown discards fixture files even after a failed assertion. The callback test refuses a direct run unless the private-mount marker and separate `/tmp` tmpfs are present, and no longer deletes a pre-existing callback target. If mount setup fails, the runner stops; it does not fall back to the machine's `/tmp`. Keep builds and the testy binary outside `/tmp`, since the private mount hides that directory.

For this project's current scope, execute Linux verification through GitHub Actions. [Rust run 37818450569](https://github.com/0xbasinas/acportal/actions/runs/37818450569) at merged commit `981a97a` verifies the revised namespace runner on Ubuntu 24.04: all five testy scenarios pass, including callback/full approval and denial in the private `/tmp` mount. Earlier local evidence below predates this isolation change.

## CI (manual)

The Rust workflow has a `testy` job (Linux, 30-minute timeout, testy build cached by the script's hash). It runs on manual runs (input `testy`, on by default) and on pushes to `main`, never on PRs. Both workflows were re-enabled by the user on 8 October 2026. Start it with:

```sh
gh workflow run rust.yml -R 0xbasinas/acportal --ref <branch> -f testy=true
```

## What is covered (8 October 2026, all passing locally)

| Test | Scenarios | Checks |
| --- | --- | --- |
| `testy_echo_wait_for_cancel_and_cancel_status` | `echo`, `wait_for_cancel`, `cancel_status` | Echo text and `end_turn`; the phone's `session/cancel` ends `wait_for_cancel` with `cancelled`; a later `cancel_status` reports `not_cancelled`. |
| `testy_session_updates_tool_calls_modes_config_and_auth_pass_through` | `session_updates`, `tool_calls`, `content`, plus `session/set_mode`, `session/set_config_option`, `authenticate`, `logout` | session/new modes and config options reach the phone. All 19, 7 and 8 agent updates arrive (session info, current mode, config option, available commands, usage, thought and message chunks, plan, tool call and updates, every stable content block). A valid mode switch and config change pass through, an invented mode returns testy's `-32602`, and authenticate and logout pass through. |
| `testy_elicitation_is_not_advertised_and_fails_cleanly` | `elicitations` | acpd does not advertise `elicitation` in `clientCapabilities`, so testy returns its deterministic `-32602` ("client does not support form elicitation") before sending any request. No consent is shown, and the session keeps working. |
| `testy_callbacks_outside_the_workspace_are_refused_without_consent` | `callbacks` | With a temporary workspace, testy's `/tmp` file and terminal callbacks are refused by the host without asking the phone. Only testy's own permission request is forwarded. |
| `testy_callbacks_and_full_inside_the_workspace_follow_phone_decisions` (Linux) | `callbacks` ×2, `full` | Workspace `/tmp`. Approved: agent permission `allow_once`, host write consent, read back, host command consent, then terminal output/wait_for_exit/kill/release, with output published to the phone. Denied: `reject_once`; the write is refused and the file is unchanged; the command is refused. `full`: both commands (the tool-content terminal and the callback terminal) need approval, and all update kinds arrive. |

`callbacks` and `full` always end with testy's elicitation `-32602`, because the elicitation part runs last; the tests check the callbacks before that point.

## Known gaps, not fixed

- **Elicitation** (`elicitation/create`, `elicitation/complete`) is not supported. acpd does not advertise it, and an unadvertised `elicitation/create` would get `-32601` ("Client capability not supported"). Supporting it needs a phone UI for forms and URLs.
- **Session management methods** that testy implements (`session/list`, `session/delete`, `session/resume`, `session/close`) are not used: acpd owns session lifecycle (one agent process per session) and does not let the phone send them.
- **MCP** pass-through to testy's MCP tools (`mcp-echo-server`) is not exercised.
- **Draft protocol v2** is not exercised (testy's `unstable_protocol_v2` feature is not built).

No acpd bug was found by these scenarios. A stdio tap showed every testy callback getting the response the ACP schema expects.

## ACP TCK against the mock agent (optional, local only)

[acp-tck](https://github.com/agentclientprotocol/acp-tck) at `fa00ccdc13dfaa4b288a91877c1f3e468af4aa7d`, run with `uv run acp-tck --agent-cwd <dir> -- target/debug/mock-acp-agent`: **VERDICT: CONFORMANT**. Mandatory 21 passed / 0 failed; capability 2 passed / 17 skipped (not advertised by the mock: auth flow, logout, image/audio/embedded context, resume, list, delete, close, additional directories, modes/config options); advisory 11 passed / 1 skipped (DELETE-002); informational 4 passed. This tests acpd's mock agent, not the host. It is not in CI.
