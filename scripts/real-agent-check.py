#!/usr/bin/env python3
"""Drive a real ACP agent through acpd the way the phone does, and report what happened.

The script starts `acpd start` with the inherited environment (provide provider
credentials there, never in the registry), pairs a throwaway device, creates
sessions over HTTP and talks ACP over the `acpd.v1` WebSocket. It prints a
summary per step and writes every sent/received WebSocket message to a local
JSONL capture for diagnosis. The pairing code and bearer token are kept in
memory only and are never printed or captured.

Linux only: process checks read /proc. Requires Python 3.10+ and
`pip install websockets` (tested with 15.0.1). Model requests cost money;
the default steps send about ten short prompts.

Example (see docs/real-agent-testing.md):
  python3 scripts/real-agent-check.py --init-fixture /path/to/root/calc
  OPENAI_API_KEY="$(cat /path/to/key)" python3 scripts/real-agent-check.py \
      --acpd target/debug/acpd --config /path/to/config.toml \
      --agent goose --workspace /path/to/root/calc --capture capture.jsonl
"""
import argparse, asyncio, json, os, re, signal, subprocess, sys, time, urllib.error, urllib.request

try:
    import websockets
except ImportError:  # pragma: no cover - guidance only
    sys.exit("install the websockets package: pip install websockets")

FIXTURE = {
    "calc.py": '"""Tiny calculator used to exercise a coding agent."""\n\n\ndef add(a, b):\n    return a - b\n\n\ndef multiply(a, b):\n    return a * b\n',
    "test_calc.py": "import unittest\n\nfrom calc import add, multiply\n\n\nclass CalcTest(unittest.TestCase):\n    def test_add(self):\n        self.assertEqual(add(2, 3), 5)\n\n    def test_multiply(self):\n        self.assertEqual(multiply(4, 5), 20)\n\n\nif __name__ == \"__main__\":\n    unittest.main()\n",
    "README.md": "# calc\n\nA tiny calculator with `add` and `multiply`. Run the tests with:\n\n    python3 -m unittest -v\n",
}
STEPS = ["inspect", "approve", "deny", "terminal", "reconnect", "load", "cancel", "outside"]


def init_fixture(path):
    os.makedirs(path, exist_ok=True)
    for name, text in FIXTURE.items():
        with open(os.path.join(path, name), "w") as handle:
            handle.write(text)
    print(f"fixture written to {path}; tests fail until add() is fixed")


def proc_table():
    table = {}
    for entry in os.listdir("/proc"):
        if not entry.isdigit():
            continue
        try:
            with open(f"/proc/{entry}/stat") as handle:
                stat = handle.read()
            with open(f"/proc/{entry}/cmdline", "rb") as handle:
                cmd = handle.read().replace(b"\0", b" ").decode(errors="replace").strip()
        except OSError:
            continue
        rest = stat[stat.rfind(")") + 2:].split()
        table[int(entry)] = (int(rest[1]), rest[0], cmd)
    return table


def descendants(root):
    table, found, todo = proc_table(), [], [root]
    while todo:
        parent = todo.pop()
        for pid, (ppid, state, cmd) in table.items():
            if ppid == parent and state not in "ZX":
                found.append((pid, cmd[:100]))
                todo.append(pid)
    return found


class Capture:
    def __init__(self, path):
        self.handle = open(path, "a") if path else None

    def write(self, direction, value):
        if self.handle:
            self.handle.write(json.dumps({"t": round(time.time(), 3), "dir": direction, "msg": value}) + "\n")
            self.handle.flush()


class Host:
    def __init__(self, args):
        self.args, self.base = args, f"http://{args.listen}"
        self.token, self.process = None, None

    def acpd(self, *extra):
        command = [self.args.acpd, "--config", self.args.config]
        if self.args.registry:
            command += ["--registry", self.args.registry]
        return command + list(extra)

    def start(self):
        self.log = open(self.args.host_output, "a")
        self.process = subprocess.Popen(self.acpd("start"), stdout=self.log, stderr=subprocess.STDOUT)
        deadline = time.time() + 20
        while time.time() < deadline:
            try:
                urllib.request.urlopen(self.base + "/v1/status", timeout=1)
            except urllib.error.HTTPError:
                break  # 401 means it is listening
            except OSError:
                time.sleep(0.2)
        else:
            raise SystemExit("acpd did not start; see host output")
        output = subprocess.run(self.acpd("pair"), capture_output=True, text=True, check=True).stdout
        code = re.search(r"Pairing code: (\S+)", output).group(1)
        self.token = self.http("POST", "/v1/pair", {"code": code, "deviceName": "real-agent-check"}, auth=False)["token"]
        print(f"host pid {self.process.pid} listening on {self.base}; paired (token kept in memory)")

    def http(self, method, path, body=None, auth=True):
        headers = {"Content-Type": "application/json"}
        if auth:
            headers["Authorization"] = f"Bearer {self.token}"
        request = urllib.request.Request(self.base + path, method=method, headers=headers,
                                         data=None if body is None else json.dumps(body).encode())
        try:
            with urllib.request.urlopen(request, timeout=300) as reply:
                raw = reply.read()
                return json.loads(raw) if raw else reply.status
        except urllib.error.HTTPError as failure:
            return {"httpStatus": failure.code, "error": failure.read().decode(errors="replace")[:300]}

    def stop(self):
        if self.token:
            self.http("DELETE", "/v1/device")
        if self.process and self.process.poll() is None:
            children = descendants(self.process.pid)
            self.process.send_signal(signal.SIGINT)
            try:
                self.process.wait(20)
            except subprocess.TimeoutExpired:
                self.process.kill()
            time.sleep(1)
            table = proc_table()
            alive = [(pid, cmd) for pid, cmd in children if pid in table and table[pid][1] not in "ZX"]
            print(f"host stopped (exit {self.process.returncode}); processes it had started still running: {alive}")


class Session:
    """One WebSocket attachment to a host session."""

    def __init__(self, host, capture, meta):
        self.host, self.capture, self.meta = host, capture, meta
        self.acp_id, self.ws, self.last_sequence, self.seen = meta["acpSessionId"], None, 0, set()
        self.backlog = []

    async def connect(self, after=None):
        """Attach and read the replay. After a reconnect the replayed envelopes are also
        queued so a resumed prompt() sees events delivered while detached."""
        query = f"?after={after}" if after is not None else ""
        uri = self.host.base.replace("http", "ws", 1) + f"/v1/sessions/{self.meta['id']}/connect{query}"
        self.ws = await websockets.connect(uri, subprotocols=["acpd.v1"], max_size=8 << 20,
                                           additional_headers={"Authorization": f"Bearer {self.host.token}"})
        replay = {"events": 0, "kinds": {}, "pending": []}
        while True:
            envelope = await self.receive()
            if envelope["type"] == "replay_start":
                replay["start"] = {k: envelope.get(k) for k in ("gap", "latestSequence", "processing")}
            elif envelope["type"] == "event":
                replay["events"] += 1
                kind = describe(envelope["message"])
                replay["kinds"][kind] = replay["kinds"].get(kind, 0) + 1
            elif envelope["type"] == "pending_permission":
                replay["pending"].append(envelope["message"])
            elif envelope["type"] == "replay_complete":
                return replay
            if after is not None and envelope["type"] in ("event", "pending_permission"):
                self.backlog.append(envelope)

    async def next(self, timeout=300):
        return self.backlog.pop(0) if self.backlog else await self.receive(timeout)

    async def receive(self, timeout=300):
        envelope = json.loads(await asyncio.wait_for(self.ws.recv(), timeout))
        self.capture.write("recv", envelope)
        if envelope.get("type") == "event":
            self.last_sequence = max(self.last_sequence, envelope["sequence"])
        return envelope

    async def send(self, value):
        self.capture.write("send", value)
        await self.ws.send(json.dumps(value))

    async def close(self):
        await self.ws.close()

    async def prompt(self, request_id, text, decide=lambda request: None, on_update=None, resume=None):
        """Send a prompt (unless resuming) and follow it to its response.

        decide(request) returns an optionId, or None to answer `cancelled`.
        on_update(session, update) may act on streamed updates and return "detach" to stop reading.
        """
        if resume is None:
            await self.send({"jsonrpc": "2.0", "id": request_id, "method": "session/prompt",
                             "params": {"sessionId": self.acp_id, "prompt": [{"type": "text", "text": text}]}})
            report = {"updates": {}, "reply": "", "tools": [], "permissions": [], "duplicates": 0, "started": time.time()}
        else:
            report = resume
        while True:
            envelope = await self.next()
            if envelope["type"] in ("event", "pending_permission"):
                message = envelope["message"]
                sequence = envelope.get("sequence")
                if sequence is not None:
                    if sequence in self.seen:
                        report["duplicates"] += 1
                        continue
                    self.seen.add(sequence)
            else:
                report.setdefault("envelopes", []).append(envelope)
                continue
            method = message.get("method")
            if method == "session/update":
                update = message["params"]["update"]
                kind = update.get("sessionUpdate")
                report["updates"][kind] = report["updates"].get(kind, 0) + 1
                if kind == "agent_message_chunk":
                    report["reply"] += update.get("content", {}).get("text", "")
                if kind in ("tool_call", "tool_call_update"):
                    report["tools"].append({k: update.get(k) for k in ("toolCallId", "title", "kind", "status") if update.get(k) is not None})
                if on_update and await on_update(self, update) == "detach":
                    return report
            elif method == "session/request_permission":
                if any(p["id"] == message["id"] for p in report["permissions"]):
                    continue
                option = decide(message)
                params = message["params"]
                report["permissions"].append({"id": message["id"], "source": (params.get("_meta") or {}).get("acpdSource", "agent"),
                                              "title": params["toolCall"].get("title"), "kind": params["toolCall"].get("kind"),
                                              "options": [o["optionId"] for o in params["options"]], "chosen": option})
                if option == "hold":
                    # The caller answers later; give its hook a chance to act now (e.g. cancel).
                    if on_update:
                        await on_update(self, {"sessionUpdate": "_permission_held", "request": message})
                    continue
                outcome = {"outcome": "selected", "optionId": option} if option else {"outcome": "cancelled"}
                await self.send({"jsonrpc": "2.0", "id": message["id"], "result": {"outcome": outcome}})
            elif message.get("id") == request_id and envelope.get("direction") == "agent":
                report["final"] = message.get("result") or {"error": message.get("error")}
                report["seconds"] = round(time.time() - report.pop("started"), 1)
                return report


def describe(message):
    if message.get("method") == "session/update":
        return "update:" + str(message["params"]["update"].get("sessionUpdate"))
    if "method" in message:
        return str(message["method"])
    return "response" if "result" in message else "error"


def choose(kind):
    """Pick the first option of the given ACP kind (allow_once, reject_once, ...)."""
    def decide(request):
        for option in request["params"]["options"]:
            if option["kind"] == kind:
                return option["optionId"]
        return None
    return decide


def deny_host_writes(request):
    """Let the agent propose an edit, but refuse the host filesystem write consent.
    Other permissions (reads, agent-level prompts) are allowed once."""
    source = (request["params"].get("_meta") or {}).get("acpdSource")
    return choose("reject_once" if source == "host-filesystem" else "allow_once")(request)


RESULTS = []


def show(name, report):
    """Print a short human summary; the full report goes to --results."""
    RESULTS.append({"step": name, **report})
    final = report.get("final", {})
    print(f"== {name}: stopReason={final.get('stopReason')} error={final.get('error')} seconds={report.get('seconds')} usage={final.get('usage')}")
    print(f"   updates: {report.get('updates')}")
    for permission in report.get("permissions", []):
        print(f"   permission [{permission['source']}] {str(permission['title'])[:70]!r} -> {permission['chosen']}")
    titles = [tool["title"] for tool in report.get("tools", []) if tool.get("title")]
    statuses = [tool["status"] for tool in report.get("tools", []) if tool.get("status")]
    print(f"   tools: {titles} statuses: {statuses}")
    for key in ("disk", "replay_on_reconnect", "history_replayed_on_load", "cancel_sent", "marker_exists",
                "processes_before_cancel", "processes_after", "agent_processes_after", "duplicates"):
        if key in report:
            print(f"   {key}: {report[key]}")
    print(f"   reply: {report.get('reply', '')[:300]!r}")


def git_status(workspace):
    result = subprocess.run(["git", "-C", workspace, "status", "--porcelain"], capture_output=True, text=True)
    return result.stdout.strip() if result.returncode == 0 else "(not a git repository)"


def run_tests(workspace):
    result = subprocess.run([sys.executable, "-m", "unittest", "-q"], cwd=workspace, capture_output=True, text=True)
    return result.stderr.strip().splitlines()[-1] if result.stderr.strip() else f"exit {result.returncode}"


async def main(args):
    capture = Capture(args.capture)
    host = Host(args)
    host.start()
    steps = args.steps.split(",")
    try:
        meta = host.http("POST", "/v1/sessions", {"agentId": args.agent, "workspace": args.workspace})
        print("session:", {k: meta.get(k) for k in ("status", "agentId", "httpStatus", "error")}, "agent processes:", descendants(host.process.pid))
        session = Session(host, capture, meta)
        await session.connect()
        if "inspect" in steps:
            show("inspect", await session.prompt("inspect", "List the files in this project and explain in two sentences what it does and whether the tests should pass. Do not modify anything.", choose("allow_once")))
        if "approve" in steps:
            before = run_tests(args.workspace)
            report = await session.prompt("approve", "Fix the bug in calc.py so that test_add passes. Edit only calc.py. Do not run anything.", choose("allow_once"))
            report["disk"] = {"tests_before": before, "tests_after": run_tests(args.workspace), "git_status": git_status(args.workspace)}
            show("approve", report)
        if "deny" in steps:
            with open(os.path.join(args.workspace, "README.md")) as handle:
                readme = handle.read()
            report = await session.prompt("deny", "Using the edit/write tool only (no shell), append the line 'Maintained by the calc team.' to README.md. Do not run any command.", deny_host_writes)
            with open(os.path.join(args.workspace, "README.md")) as handle:
                report["disk"] = {"readme_unchanged": handle.read() == readme, "git_status": git_status(args.workspace)}
            show("deny", report)
        if "terminal" in steps:
            report = await session.prompt("terminal", "Run `python3 -m unittest -v` in this project and tell me how many tests passed.", choose("allow_once"))
            report["agent_processes_after"] = descendants(host.process.pid)
            show("terminal", report)
        if "reconnect" in steps:
            async def detach(current, update):
                return "detach" if update.get("sessionUpdate") in ("agent_message_chunk", "tool_call") else None
            report = await session.prompt("reconnect", "In one short paragraph, describe what multiply() does and name one more test worth adding. Do not modify or run anything.", choose("allow_once"), detach)
            cursor = session.last_sequence
            await session.close()
            print(f"detached after sequence {cursor}; reconnecting with ?after={cursor}")
            await asyncio.sleep(1)
            replay = await session.connect(after=cursor)
            report["replay_on_reconnect"] = replay
            report = await session.prompt("reconnect", "", choose("allow_once"), resume=report)
            show("reconnect", report)
        if "load" in steps:
            await session.close()
            print("delete original session:", host.http("DELETE", f"/v1/sessions/{meta['id']}"))
            loaded = host.http("POST", "/v1/sessions", {"agentId": args.agent, "workspace": args.workspace, "loadSessionId": meta["acpSessionId"]})
            print("load session:", {k: loaded.get(k) for k in ("status", "httpStatus", "error")}, "same ACP id:", loaded.get("acpSessionId") == meta["acpSessionId"])
            if "httpStatus" not in loaded:
                session = Session(host, capture, loaded)
                replay = await session.connect()
                report = await session.prompt("load", "Which file did you change earlier in this session? Answer with just the file name.", choose("reject_once"))
                report["history_replayed_on_load"] = replay
                show("load", report)
                meta = loaded
        if "cancel" in steps:
            state = {"sent": False}

            def cancel_before_write(request):
                source = (request["params"].get("_meta") or {}).get("acpdSource")
                if source == "host-filesystem" and not state["sent"]:
                    state["pending"] = request
                    return "hold"  # leave it unanswered; the on_update path cancels
                return choose("allow_once")(request)

            async def cancel_on_write_consent(current, update):
                if not state["sent"] and state.get("pending"):
                    state["procs_before_cancel"] = descendants(host.process.pid)
                    await current.send({"jsonrpc": "2.0", "method": "session/cancel", "params": {"sessionId": current.acp_id}})
                    # also answer the outstanding consent so the agent is not stuck
                    pending = state["pending"]
                    await current.send({"jsonrpc": "2.0", "id": pending["id"], "result": {"outcome": {"outcome": "cancelled"}}})
                    state["sent"] = True
            report = await session.prompt(
                "cancel",
                "Create a new file named CANCEL_MARKER.txt containing the single word cancelled. Edit only that new file.",
                cancel_before_write,
                cancel_on_write_consent,
            )
            await asyncio.sleep(1)
            report["cancel_sent"] = state["sent"]
            report["processes_before_cancel"] = state.get("procs_before_cancel")
            report["processes_after"] = descendants(host.process.pid)
            report["marker_exists"] = os.path.exists(os.path.join(args.workspace, "CANCEL_MARKER.txt"))
            show("cancel", report)
        if "outside" in steps:
            outside = args.outside_file
            report = await session.prompt("outside", f"Read the file {outside} and print its contents verbatim. Do not use any other tool.", choose("allow_once"))
            show("outside", report)
        await session.close()
        print("delete session:", host.http("DELETE", f"/v1/sessions/{meta['id']}"))
        await asyncio.sleep(1)
        print("processes under host after delete:", descendants(host.process.pid))
    finally:
        host.stop()
        if args.results:
            with open(args.results, "w") as handle:
                json.dump(RESULTS, handle, indent=1, default=str)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--init-fixture", metavar="DIR", help="write the calc fixture to DIR and exit")
    parser.add_argument("--acpd", default="target/debug/acpd")
    parser.add_argument("--config")
    parser.add_argument("--registry")
    parser.add_argument("--listen", default="127.0.0.1:8799", help="must match [server] listen in the config")
    parser.add_argument("--agent", default="goose")
    parser.add_argument("--workspace", help="absolute path inside workspace_roots")
    parser.add_argument("--steps", default=",".join(STEPS), help="comma-separated subset of " + ",".join(STEPS))
    parser.add_argument("--outside-file", default="/etc/hostname", help="harmless file outside the workspace")
    parser.add_argument("--capture", help="append sent/received WebSocket messages to this JSONL file")
    parser.add_argument("--host-output", default="acpd-host.out")
    parser.add_argument("--results", help="write full per-step reports as JSON")
    options = parser.parse_args()
    if options.init_fixture:
        init_fixture(options.init_fixture)
        sys.exit(0)
    if not options.config or not options.workspace:
        parser.error("--config and --workspace are required")
    asyncio.run(main(options))
