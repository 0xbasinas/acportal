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

Useful flags: `--steps inspect,approve` to run a subset; `--outside-file` for a
harmless path outside the workspace (default `/etc/hostname`).

## Results (8 October 2026, Linux)

One continuous run through all eight steps, with the host on the branch tip that
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
| 4. Terminal | **partial** | Goose usually sends a shell string as `terminal/create.command` (for example `cd … && python3 -m unittest -v`) with no `args`. The host correctly requires a resolved executable and returns `-32602`. When Goose did send a plain argv (`pwd`, `ls`) the full host path ran: consent (`host-terminal` / `Run once`), create, output, wait, release, with exit code 0 and the retained output published. Running the project's tests through Goose therefore did not succeed in this run; the host terminal path itself is covered by the mock-agent tests and by those argv cases. |
| 5. Reconnect | **pass** | Client detached after the first message chunk, then reconnected with `?after=`. Replay reported `gap: false` and delivered the remaining chunks plus the final `end_turn` response. |
| 6. Session load | **pass** | After deleting the host session, `POST /v1/sessions` with `loadSessionId` returned `ready` with the same ACP session id. The new attachment replayed prior user/agent/tool history; a follow-up prompt answered `calc.py` as the file changed earlier. |
| 7. Cancel mid write | **pass** | While a host `Write CANCEL_MARKER.txt` consent was outstanding, the client sent `session/cancel` and answered the consent with `cancelled`. Prompt `stopReason` was `cancelled`; `CANCEL_MARKER.txt` was never created. The host answered the agent's pending `fs/write_text_file` with `-32800`. |
| 8. Outside-workspace read | **pass** | Goose requested `/etc/hostname`; host `fs/read_text_file` returned `-32602` with `Workspace text read denied or unavailable`. Goose reported the denial and printed no contents. |
| Cleanup | **pass** | After `DELETE /v1/sessions/:id` and host shutdown, no `goose acp` process remained under the host. |
| Frame metadata log | **pass** | With `frame_metadata = true` the 0600 log held only `direction`/`kind`/`method`/`update`/`bytes` (and `code` on errors). No prompt text, reply text, path, pairing code, bearer token or provider key. |

Doctor against the same Goose install, run separately earlier the same day, reported
`ready` (exit 0) once Gemini was disabled. See also the sign-in / frame-metadata
checkpoint in `AGENTS.md`.

## Bugs found and fixed

| Issue | Status |
| --- | --- |
| Host answered refused `fs/read_text_file` / `fs/write_text_file` (and some other host failures) with JSON-RPC `-32000`. ACP reserves that code for `auth_required`, so Goose told the model "Authentication required" for a path outside the workspace. | Fixed on this branch: refusals use `-32602`, not-ready/capacity use `-32603`, cancel of a pending callback uses `-32800`. Client-facing "Agent request failed" keeps the agent's own code. Tests cover an outside read, a denied write and an agent error forwarded over the WebSocket. |

## Known limitations (not host bugs)

- **Goose shell strings.** Goose's shell tool often puts a whole shell line in
  `terminal/create.command` instead of a resolved executable plus `args`. The host
  rejects that, by design (literal argv, no shell). Plain commands such as `pwd`
  and `ls` do go through.
- **Doctor cannot see missing provider credentials for Goose.** Goose creates
  sessions without talking to the provider, so `doctor --agent` reports `ready`
  even with an unconfigured provider; the first prompt then fails with `-32603`.
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

## See also

- [Adding agents and Goose setup](agents.md)
- [Installation and doctor](installation.md)
- [Development log](development.md)
- [HANDOFF](../HANDOFF.md) and [AGENTS](../AGENTS.md) for the latest checkpoints
