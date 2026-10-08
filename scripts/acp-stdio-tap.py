#!/usr/bin/env python3
"""Transparent stdio tap for diagnosing an ACP agent behind acpd.

Register it in a private registry in place of the agent command, for example
  "command": "/usr/bin/python3",
  "args": ["/abs/path/scripts/acp-stdio-tap.py", "/abs/path/to/goose", "acp"],
  "env": {"TAP_FILE": "/abs/path/tap.jsonl", ...}
It runs the real agent unchanged and appends every JSON-RPC line, in both
directions, to TAP_FILE. The tap holds full ACP payloads: prompts, file contents,
command lines and tool output. Keep it local, delete it after use and never
commit it. It does not read the environment, so provider keys passed through the
environment are not recorded unless the agent itself puts them in a frame.
"""
import json, os, subprocess, sys, threading, time

tap = open(os.environ.get("TAP_FILE", "tap.jsonl"), "a")
os.chmod(tap.name, 0o600)
lock = threading.Lock()
child = subprocess.Popen(sys.argv[1:], stdin=subprocess.PIPE, stdout=subprocess.PIPE)


def log(direction, line):
    with lock:
        record = {"t": round(time.time(), 3), "dir": direction, "line": line.decode(errors="replace").rstrip("\n")}
        tap.write(json.dumps(record) + "\n")
        tap.flush()


def pump_in():
    for line in sys.stdin.buffer:
        log("host_to_agent", line)
        child.stdin.write(line)
        child.stdin.flush()
    child.stdin.close()


threading.Thread(target=pump_in, daemon=True).start()
for line in child.stdout:
    log("agent_to_host", line)
    sys.stdout.buffer.write(line)
    sys.stdout.buffer.flush()
sys.exit(child.wait())
