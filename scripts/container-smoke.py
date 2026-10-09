#!/usr/bin/env python3
"""Container smoke check: TLS, persistent storage and agent execution through acpd.

Run against a host started from the container image (see the `container` job in
.github/workflows/rust.yml) with the repository's mock agent registered as `mock`
(arguments `--terminal-during-prompt`). Two phases:

  first   pair with the code in ACPD_PAIR_CODE, check status and agent discovery,
          create a session, run one prompt over the acpd.v1 WebSocket (host terminal
          consent and agent permission approved), and keep the session.
  second  after the container restarts on the same state mount: the saved device
          credential still works and the session is still listed; then revoke it.

Certificate verification stays on (the job's own test CA). The pairing code and the
token are never printed; the token is kept in --token-file (mode 0600) between the
two phases and the file is removed after revocation. Requires `pip install websockets`.
"""
import argparse, asyncio, json, os, ssl, sys, urllib.error, urllib.request

import websockets


def http(args, context, method, path, body=None, token=None):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    request = urllib.request.Request(args.base + path, method=method, headers=headers,
                                     data=None if body is None else json.dumps(body).encode())
    try:
        with urllib.request.urlopen(request, timeout=60, context=context) as reply:
            raw = reply.read()
            return reply.status, (json.loads(raw) if raw else None)
    except urllib.error.HTTPError as failure:
        return failure.code, failure.read().decode(errors="replace")[:300]


def check(condition, message):
    if not condition:
        sys.exit(f"FAIL: {message}")
    print(f"ok: {message}")


async def prompt(args, context, token, session):
    uri = args.base.replace("https", "wss", 1) + f"/v1/sessions/{session['id']}/connect"
    seen = {"terminal_output": False, "reply": "", "permissions": []}
    async with websockets.connect(uri, subprotocols=["acpd.v1"], ssl=context, max_size=8 << 20,
                                  additional_headers={"Authorization": f"Bearer {token}"}) as ws:
        while json.loads(await asyncio.wait_for(ws.recv(), 30))["type"] != "replay_complete":
            pass
        await ws.send(json.dumps({"jsonrpc": "2.0", "id": "p1", "method": "session/prompt", "params": {
            "sessionId": session["acpSessionId"], "prompt": [{"type": "text", "text": "container smoke"}]}}))
        while True:
            envelope = json.loads(await asyncio.wait_for(ws.recv(), 60))
            if envelope.get("type") not in ("event", "pending_permission"):
                continue
            message = envelope["message"]
            method = message.get("method")
            if method == "session/update":
                update = message["params"]["update"]
                if update.get("sessionUpdate") == "agent_message_chunk":
                    seen["reply"] += update.get("content", {}).get("text", "")
                if "ACP terminal output verified." in json.dumps(update):
                    seen["terminal_output"] = True
            elif method == "session/request_permission" and envelope.get("direction") != "client":
                if message["id"] in [p[0] for p in seen["permissions"]]:
                    continue
                source = (message["params"].get("_meta") or {}).get("acpdSource", "agent")
                option = next(o["optionId"] for o in message["params"]["options"] if o.get("kind") == "allow_once")
                seen["permissions"].append((message["id"], source))
                await ws.send(json.dumps({"jsonrpc": "2.0", "id": message["id"],
                                          "result": {"outcome": {"outcome": "selected", "optionId": option}}}))
            elif message.get("id") == "p1" and envelope.get("direction") == "agent":
                seen["final"] = message.get("result") or {"error": message.get("error")}
                return seen


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("phase", choices=["first", "second"])
    parser.add_argument("--base", default="https://localhost:8443")
    parser.add_argument("--ca", required=True, help="PEM CA that signed the host certificate")
    parser.add_argument("--workspace", default="/workspace", help="workspace path inside the container")
    parser.add_argument("--token-file", required=True)
    parser.add_argument("--session-file", required=True)
    args = parser.parse_args()
    context = ssl.create_default_context(cafile=args.ca)

    # Plain HTTP must not reach a TLS listener, and an untrusted client must fail the handshake.
    try:
        urllib.request.urlopen(args.base + "/v1/status", timeout=10, context=ssl.create_default_context())
        sys.exit("FAIL: the host certificate was trusted without the test CA")
    except urllib.error.URLError as failure:
        check(isinstance(failure.reason, ssl.SSLCertVerificationError), "default trust store rejects the test certificate")

    if args.phase == "first":
        code = os.environ.pop("ACPD_PAIR_CODE")
        status, reply = http(args, context, "POST", "/v1/pair", {"code": code, "deviceName": "container-smoke"})
        check(status == 200 and "token" in reply, "pairing over TLS")
        token = reply["token"]
        descriptor = os.open(args.token_file, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(descriptor, "w") as handle:
            handle.write(token)
        status, reply = http(args, context, "GET", "/v1/status", token=token)
        check(status == 200, "authenticated status")
        status, agents = http(args, context, "GET", "/v1/agents", token=token)
        mock = next((agent for agent in agents if agent["id"] == "mock"), None)
        check(status == 200 and mock and mock.get("status") == "available", "mock agent discovered in the image")
        status, session = http(args, context, "POST", "/v1/sessions", {"agentId": "mock", "workspace": args.workspace}, token=token)
        check(status == 201 and session.get("status") == "ready", f"session created in {args.workspace}")
        with open(args.session_file, "w") as handle:
            handle.write(session["id"])
        seen = asyncio.run(prompt(args, context, token, session))
        check(seen.get("final", {}).get("stopReason") == "end_turn", "prompt ended with end_turn")
        check([source for _, source in seen["permissions"]] == ["host-terminal", "agent"],
              f"host terminal consent then agent permission ({seen['permissions']})")
        check(seen["terminal_output"], "agent-launched terminal ran inside the container")
        check("Mock lifecycle complete" in seen["reply"], "agent reply streamed")
    else:
        with open(args.token_file) as handle:
            token = handle.read().strip()
        with open(args.session_file) as handle:
            session_id = handle.read().strip()
        status, _ = http(args, context, "GET", "/v1/status", token=token)
        check(status == 200, "device credential survived the restart (state mount)")
        status, sessions = http(args, context, "GET", "/v1/sessions", token=token)
        listed = [s for s in sessions if s["id"] == session_id] if status == 200 else []
        check(len(listed) == 1, f"session survived the restart (status {listed[0]['status'] if listed else None})")
        status, _ = http(args, context, "DELETE", "/v1/device", token=token)
        check(status in (200, 204), "device revoked")
        status, _ = http(args, context, "GET", "/v1/status", token=token)
        check(status == 401, "revoked credential refused")
        os.remove(args.token_file)


if __name__ == "__main__":
    main()
