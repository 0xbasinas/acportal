"""Owned real-Goose reload fixture. No payload/credential logs or automatic approvals."""
import argparse
import asyncio
import json
import re
import socket
import subprocess
import tempfile
import time
import threading
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import requests
import websockets


async def verify(acpd, goose, idle=False, adb=None, serial="emulator-5554", elicitation=False, authorization=None):
    with tempfile.TemporaryDirectory(prefix="acportal-real-reload-") as directory:
        root = Path(directory)
        registry = root / "agents.json"
        definitions = [{"id": "goose", "name": "Before", "command": str(goose),
                        "args": ["acp"], "env": {"GOOSE_MODE": "approve"}, "enabled": True}]
        registry.write_text(json.dumps(definitions), encoding="utf-8")
        with socket.socket() as reserved:
            reserved.bind(("127.0.0.1", 0))
            port = reserved.getsockname()[1]
        config = root / "config.toml"
        config.write_text("registry='agents.json'\nstate_directory='state'\nworkspace_roots=['.']\n"
                          f"[server]\nlisten='127.0.0.1:{port}'\n"
                          "[runtime]\nrequest_timeout_seconds=300\n", encoding="utf-8")
        profile = root / "launcher.json"
        command = [str(acpd), "--config", str(config), "--profile", str(profile)]
        def operator(*args):
            result = subprocess.run(command + list(args), capture_output=True, text=True, timeout=20)
            if result.returncode:
                raise RuntimeError("owned operator command failed")
            return result.stdout
        process = subprocess.Popen(command + ["start"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        http = requests.Session()
        http.trust_env = False
        base = f"http://127.0.0.1:{port}"
        ws = None
        try:
            deadline = time.monotonic() + 20
            while True:
                if process.poll() is not None:
                    raise RuntimeError("owned host exited before readiness")
                try:
                    response = http.get(base + "/v1/status", timeout=1)
                    if response.status_code == 401:
                        break
                except requests.ConnectionError:
                    pass
                if time.monotonic() > deadline:
                    raise RuntimeError("owned host readiness deadline")
                await asyncio.sleep(.1)
            output = operator("pair")
            code = re.search(r"Pairing code: (\S+)", output).group(1)
            paired = http.post(base + "/v1/pair", json={"code": code, "deviceName": "reload-fixture"}, timeout=10)
            paired.raise_for_status()
            token = paired.json()["token"]
            http.headers["Authorization"] = "Bearer " + token
            creation = {"agentId": "goose", "workspace": str(root)}
            if elicitation:
                creation.update(mcpServers=[{"name": "elicitation-fixture", "command": sys.executable,
                    "args": [str(Path(__file__).with_name("mcp-elicitation-fixture.py").resolve())], "env": []}],
                    workspaceAccess={"readFiles": True, "writeFiles": True, "terminal": True, "formElicitation": True})
            created = http.post(base + "/v1/sessions", json=creation, timeout=120)
            created.raise_for_status()
            session = created.json()
            print("CHECK real session initialized", flush=True)
            url = base.replace("http:", "ws:") + f"/v1/sessions/{session['id']}/connect?after=0"
            async def connect():
                return await websockets.connect(url, subprotocols=["acpd.v1"],
                    additional_headers={"Authorization": "Bearer " + token}, max_size=8 << 20)
            replay_counts = {"prompts": 0, "decisions": 0}
            permission_id = None
            async def receive_until(predicate):
                async def read():
                    while True:
                        envelope = json.loads(await ws.recv())
                        message = envelope.get("message", {})
                        if envelope.get("type") == "event" and envelope.get("direction") == "client":
                            if message.get("method") == "session/prompt":
                                replay_counts["prompts"] += 1
                            if permission_id is not None and message.get("id") == permission_id and "method" not in message:
                                replay_counts["decisions"] += 1
                        if predicate(envelope):
                            return envelope
                return await asyncio.wait_for(read(), 120)
            ws = await connect()
            await receive_until(lambda e: e.get("type") == "replay_complete")
            print("CHECK initial replay complete", flush=True)
            await ws.send(json.dumps({"jsonrpc": "2.0", "id": "owned-write", "method": "session/prompt",
                "params": {"sessionId": session["acpSessionId"], "prompt": [{"type": "text",
                    "text": "Answer only fixture ready, without tools." if idle else
                    "Call ask_preference exactly once, then report its result. Do not use any other tool." if elicitation else
                    "Create reload-fixture.txt in this workspace with the text fixture. Use a file-write tool; wait for approval."}]}}))
            permission = await receive_until(lambda e: e.get("message", {}).get("method") in ("session/request_permission", "elicitation/create")
                or (e.get("message", {}).get("id") == "owned-write" and "method" not in e.get("message", {})))
            if elicitation and authorization and permission["message"].get("method") == "session/request_permission":
                params = permission["message"]["params"]
                call = params.get("toolCall", {})
                # Fail closed if Goose asks for any other tool. The marker consumes
                # one explicit human authorization, even if delivery subsequently fails.
                title = call.get("title", "")
                # Goose 1.53.0 format_tool_name replaces underscores with spaces;
                # permission updates use that deterministic title, not generated text.
                # Require our exact MCP namespace and its empty input schema.
                if title != "elicitation-fixture: ask preference" or call.get("rawInput") != {}:
                    print("UNVERIFIED authorized fixture: pending tool title does not match ask_preference", flush=True)
                    raise RuntimeError("unexpected tool approval")
                option = next((o for o in params.get("options", []) if o.get("kind") == "allow_once"), None)
                if option is None:
                    raise RuntimeError("one-time fixture approval unavailable")
                with authorization.open("x", encoding="utf-8") as marker:
                    marker.write("Consumed one human-authorized ask_preference fixture call.\n")
                await ws.send(json.dumps({"jsonrpc": "2.0", "id": permission["message"]["id"],
                    "result": {"outcome": {"outcome": "selected", "optionId": option["optionId"]}}}))
                print("CHECK one explicitly authorized ask_preference approval sent", flush=True)
                permission = await receive_until(lambda e: e.get("message", {}).get("method") in ("elicitation/create", "session/request_permission")
                    or (e.get("message", {}).get("id") == "owned-write" and "method" not in e.get("message", {})))
            if idle:
                assert "result" in permission["message"]
            elif permission["message"].get("method") != ("elicitation/create" if elicitation else "session/request_permission"):
                observed=permission["message"].get("method")
                print("UNVERIFIED target request: observed=" + (observed if observed in ("session/request_permission", "elicitation/create") else "turn response")
                      + "; error=" + str("error" in permission["message"]), flush=True)
                raise RuntimeError("provider did not expose a pending approval")
            permission_id = None if idle else permission["message"]["id"]
            second = None
            if adb:
                created_second = http.post(base + "/v1/sessions", json={"agentId": "goose", "workspace": str(root)}, timeout=120)
                created_second.raise_for_status()
                second = created_second.json()
            definitions[0].update(name="After", enabled=False)
            registry.write_text(json.dumps(definitions), encoding="utf-8")
            operator("daemon", "reload")
            print("CHECK reload accepted", flush=True)
            await ws.close()
            ws = await connect()
            if not idle:
                pending = await receive_until(lambda e: e.get("type") == "pending_permission")
                assert pending["message"]["id"] == permission_id
                print("CHECK authoritative permission replay matched", flush=True)
            await receive_until(lambda e: e.get("type") == "replay_complete")
            agents = http.get(base + "/v1/agents", timeout=10)
            agents.raise_for_status()
            agent = next(a for a in agents.json() if a["id"] == "goose")
            assert agent["name"] == "After" and agent["enabled"] is False
            existing = http.get(base + f"/v1/sessions/{session['id']}", timeout=10)
            existing.raise_for_status()
            assert existing.json()["acpSessionId"] == session["acpSessionId"]
            if adb:
                await ws.close()
                ws = None
                bootstrap = {"token": token, "address": base, "sessions": [session, second],
                             "permissionLabels": ["Submit", "Decline", "Cancel"] if elicitation else
                                [o["name"] for o in permission["message"]["params"]["options"]],
                             "permissionHeading": "The agent is asking you" if elicitation else ""}
                class BootstrapHandler(BaseHTTPRequestHandler):
                    def log_message(self, *_):
                        pass
                    def do_GET(self):
                        if self.path != "/bootstrap":
                            self.send_error(404); return
                        body = json.dumps(bootstrap).encode()
                        self.send_response(200)
                        self.send_header("Content-Type", "application/json")
                        self.send_header("Content-Length", str(len(body)))
                        self.end_headers(); self.wfile.write(body)
                server = ThreadingHTTPServer(("127.0.0.1", 0), BootstrapHandler)
                thread = threading.Thread(target=server.serve_forever, daemon=True)
                thread.start()
                try:
                    result = subprocess.run(["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(Path(__file__).with_name("verify-real-android-recovery.ps1")),
                        "-AdbPath", str(adb), "-Serial", serial, "-BootstrapPort", str(server.server_port), "-HostPort", str(port)],
                        capture_output=True, text=True, timeout=240)
                    if result.returncode:
                        # No tool/provider payload or bootstrap credential is printed.
                        print("FAIL real Android recovery runner", flush=True)
                        print(result.stdout.strip(), flush=True)
                        # The helper handles only fixture labels/ports, not provider configuration.
                        # Redact its disposable pairing credential even if a future error echoes it.
                        detail = result.stderr.replace(token, "[redacted]").replace(code, "[redacted]")
                        print("CHECK runner diagnostic: " + detail[:1600], flush=True)
                        for reason in ("Real recovery preparation failed", "Owned recovery adb operation failed", "Owned acceptance app is not foreground", "Owned recovery UI did not settle", "Owned recovery control unavailable", "Real approval did not reach Android", "Owned task identity missing", "Owned app did not background", "Owned process still alive", "Distinct restored process not proved", "Previous task not restored"):
                            if reason in result.stderr:
                                print("CHECK runner failure: " + reason, flush=True)
                        raise RuntimeError("owned Android recovery failed")
                    print(result.stdout.strip(), flush=True)
                finally:
                    server.shutdown(); server.server_close(); thread.join(5)
                replay_counts.update(prompts=0, decisions=0)
                ws = await connect()
                pending = await receive_until(lambda e: e.get("type") == "pending_permission")
                assert pending["message"]["id"] == permission_id
                await receive_until(lambda e: e.get("type") == "replay_complete")
                assert replay_counts == {"prompts": 1, "decisions": 0}
                print("CHECK real approval still pending after Android process recovery", flush=True)
            if idle:
                await ws.send(json.dumps({"jsonrpc": "2.0", "id": "after-reload", "method": "session/prompt",
                    "params": {"sessionId": session["acpSessionId"], "prompt": [{"type": "text", "text": "Answer only fixture continued, without tools."}]}}))
                response = await receive_until(lambda e: e.get("message", {}).get("id") == "after-reload" and "method" not in e.get("message", {}))
                assert "result" in response["message"]
                print("PASS real Goose idle reload: same session/credential, new discovery disabled, explicit follow-up turn completed; pending approvals unverified")
                return
            # Explicitly cancel the pending request; never approve or answer it here.
            await ws.send(json.dumps({"jsonrpc": "2.0", "id": permission_id,
                                      "result": {"action": "cancel"} if elicitation else {"outcome": {"outcome": "cancelled"}}}))
            await ws.send(json.dumps({"jsonrpc": "2.0", "method": "session/cancel",
                                      "params": {"sessionId": session["acpSessionId"]}}))
            await receive_until(lambda e: e.get("message", {}).get("id") == "owned-write"
                                and "method" not in e.get("message", {}))
            assert not (root / "reload-fixture.txt").exists()
            print("PASS real Goose: pending " + ("form elicitation" if elicitation else "approval") + " identity, credential/session continuity, future discovery reload, explicit cancellation, no fixture write")
        finally:
            if ws is not None:
                await ws.close()
            if process.poll() is None:
                try:
                    operator("daemon", "stop")
                    process.wait(20)
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait(20)
            http.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--acpd", type=Path, required=True)
    parser.add_argument("--goose", type=Path, required=True)
    parser.add_argument("--idle", action="store_true", help="Check two no-tool turns; does not verify pending approvals")
    parser.add_argument("--elicitation", action="store_true", help="Check a read-only MCP form request; any tool approval needs explicit separate human authorization")
    parser.add_argument("--authorize-elicitation-once", type=Path, help="Only with explicit human authorization: exclusive consumption-marker path for one matching fixture call")
    parser.add_argument("--adb", type=Path, help="Also run separately prepared real Android process-death recovery")
    parser.add_argument("--serial", default="emulator-5554")
    args = parser.parse_args()
    try:
        if args.idle and args.elicitation:
            parser.error("Choose idle or elicitation")
        if args.authorize_elicitation_once and not args.elicitation:
            parser.error("One-time authorization is restricted to elicitation")
        if args.authorize_elicitation_once and args.authorize_elicitation_once.exists():
            parser.error("This one-time fixture authorization has already been consumed")
        asyncio.run(verify(args.acpd.resolve(), args.goose.resolve(), args.idle, args.adb, args.serial, args.elicitation, args.authorize_elicitation_once))
    except Exception as error:
        # Exceptions may carry authenticated URLs or provider text. Report only the class.
        print("FAIL owned real reload fixture: " + type(error).__name__)
        raise SystemExit(1)
