"""Owned loopback-only recovery fixture. Never launches agents or writes workspaces.

Uses only Python's standard library. Control endpoints and summaries contain counts,
never client payloads. The deliberately public credential authenticates this fixture
only; it is not a pairing token or production credential.
"""
import argparse
import base64
import hashlib
import json
import struct
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit


class Fixture:
    def __init__(self):
        self.lock = threading.Lock()
        self.release = threading.Event()
        self.release.set()
        self.metadata_release = threading.Event()
        self.metadata_release.set()
        self.restoring = False
        self.connections = {"live-1": 0, "live-2": 0}
        self.mutations = 0
        self.decisions = []
        self.handlers = {}

    def snapshot(self):
        with self.lock:
            return dict(connections=dict(self.connections), mutations=self.mutations,
                        decisions=list(self.decisions), held=not self.release.is_set())


def info(session):
    return dict(id=session, agentId="fixture-agent", acpSessionId="acp-"+session,
                workspace="/fixture/"+session, status="running")


def permission(request_id, title):
    return dict(jsonrpc="2.0", id=request_id, method="session/request_permission",
                params=dict(toolCall=dict(title=title), options=[
                    dict(optionId="deny", name="Deny fixture request", kind="reject_once")]))


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):
        pass

    def reply(self, value, status=200):
        body = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        path = urlsplit(self.path).path
        if path == "/fixture/hold":
            self.server.fixture.restoring = True
            self.server.fixture.release.clear()
            self.server.fixture.metadata_release.clear()
            self.reply({"held": True})
        elif path == "/fixture/release":
            self.server.fixture.release.set()
            self.reply({"held": False})
        elif path == "/fixture/metadata":
            self.server.fixture.metadata_release.set()
            self.reply({"metadataReleased": True})
        elif path in ("/fixture/overflow-count", "/fixture/overflow-bytes"):
            handler = self.server.fixture.handlers.get("live-2")
            if handler is None:
                self.reply({}, 409)
                return
            try:
                for index in range(129 if path.endswith("count") else 5):
                    message = permission("overflow-"+str(index), "Overflow fixture approval")
                    if path.endswith("bytes"):
                        message["params"]["fixturePadding"] = "x"*600000
                    handler.frame(dict(type="event", sequence=14+index, direction="agent", message=message))
                    time.sleep(0.02)
                # A trailing replay_complete must never undo overflow detachment.
                handler.frame(dict(type="replay_complete", latestSequence=200))
            except OSError:
                pass
            self.reply({"sent": True})
        else:
            with self.server.fixture.lock:
                self.server.fixture.mutations += 1
            self.reply({}, 405)

    def do_DELETE(self):
        with self.server.fixture.lock:
            self.server.fixture.mutations += 1
        self.reply({}, 405)

    def do_GET(self):
        path = urlsplit(self.path).path
        if path == "/fixture/status":
            self.reply(self.server.fixture.snapshot())
            return
        if self.headers.get("Authorization") != "Bearer recovery-fixture-only":
            self.reply({}, 401)
            return
        if path.endswith("/connect"):
            session = path.split("/")[-2]
            if session not in self.server.fixture.connections:
                self.reply({}, 404)
                return
            self.websocket(session)
        elif path == "/v1/status":
            self.reply(dict(hostId="task-host", name="Recovery fixture", version="fixture"))
        elif path == "/v1/sessions":
            self.reply([info("live-1"), info("live-2")])
        elif path.startswith("/v1/sessions/") and path.split("/")[-1] in self.server.fixture.connections:
            if not self.server.fixture.metadata_release.wait(120):
                self.reply({}, 503)
                return
            self.reply(info(path.split("/")[-1]))
        elif path in ("/v1/agents", "/v1/workspaces"):
            self.reply([])
        else:
            self.reply({}, 404)

    def frame(self, value):
        payload = json.dumps(value).encode()
        length = len(payload)
        header = bytes([0x81, length]) if length < 126 else (bytes([0x81, 126])+struct.pack("!H", length) if length < 65536 else bytes([0x81, 127])+struct.pack("!Q", length))
        with self.frame_lock:
            self.wfile.write(header+payload)
            self.wfile.flush()

    def read_exact(self, length):
        value = self.rfile.read(length)
        if len(value) != length:
            raise EOFError()
        return value

    def websocket(self, session):
        key = self.headers.get("Sec-WebSocket-Key", "")
        accept = base64.b64encode(hashlib.sha1((key+"258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()).digest()).decode()
        self.send_response(101)
        self.send_header("Upgrade", "websocket")
        self.send_header("Connection", "Upgrade")
        self.send_header("Sec-WebSocket-Accept", accept)
        self.send_header("Sec-WebSocket-Protocol", "acpd.v1")
        self.end_headers()
        fixture = self.server.fixture
        self.frame_lock = threading.Lock()
        with fixture.lock:
            fixture.connections[session] += 1
            fixture.handlers[session] = self
        try:
            self.frame(dict(type="replay_start", processing=session == "live-1"))
            # Hold after replay_start so restored cached approvals are cleared, while
            # the composer stays disabled until the authoritative snapshot completes.
            if not fixture.release.wait(120):
                return
            request_id = "fresh-1" if fixture.restoring and session == "live-1" else "old-"+session
            if not fixture.restoring or session == "live-1":
                self.frame(dict(type="pending_permission", message=permission(request_id,
                           "Fresh authoritative request" if fixture.restoring else "Original "+session+" approval")))
            self.frame(dict(type="replay_complete", latestSequence=12))
            while True:
                first, second = self.read_exact(2)
                length = second & 127
                if length == 126:
                    length = struct.unpack("!H", self.read_exact(2))[0]
                elif length == 127:
                    length = struct.unpack("!Q", self.read_exact(8))[0]
                if length > 1024*1024 or not second & 128:
                    return
                mask = self.read_exact(4)
                payload = self.read_exact(length)
                payload = bytes(value ^ mask[index % 4] for index, value in enumerate(payload))
                opcode = first & 15
                if opcode == 8:
                    return
                if opcode == 9:
                    self.wfile.write(bytes([0x8A, len(payload)])+payload)
                    self.wfile.flush()
                elif opcode == 1:
                    message = json.loads(payload)
                    with fixture.lock:
                        fixture.mutations += 1
                        # Retain only scoped ids/outcomes, never prompts or form values.
                        if "result" in message:
                            fixture.decisions.append(dict(session=session, id=message.get("id")))
                    self.frame(dict(type="event", sequence=13, direction="client", message=message))
                    if session == "live-1" and "result" in message:
                        self.frame(dict(type="event", sequence=14, direction="agent", message=dict(jsonrpc="2.0", id="fixture-turn", result=dict(stopReason="end_turn"))))
        except (EOFError, OSError, ValueError):
            pass
        finally:
            self.close_connection = True


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=0)
    args = parser.parse_args()
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    server.daemon_threads = True
    server.fixture = Fixture()
    print(json.dumps({"port": server.server_port}), flush=True)
    server.serve_forever()
