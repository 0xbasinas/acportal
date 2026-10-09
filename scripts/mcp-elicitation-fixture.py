#!/usr/bin/env python3
"""Minimal stdio MCP server whose one tool asks the user a form question.

Used by `real-agent-check.py --steps elicitation` to see whether an agent turns an MCP
server's `elicitation/create` into an ACP elicitation for the client. The tool
`ask_preference` sends a form request (a colour choice and a count) to its MCP client
(the agent) and returns the answer it got back. Each received answer is appended as one
JSON line to the file named by MCP_ELICITATION_LOG, if set. Standard library only; no
network access; nothing is read from disk.
"""
import json, os, sys

SCHEMA = {
    "type": "object",
    "properties": {
        "colour": {"type": "string", "title": "Colour", "enum": ["red", "green", "blue"]},
        "count": {"type": "integer", "title": "Count", "minimum": 1, "maximum": 5},
    },
    "required": ["colour"],
}


def send(message):
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


def note(entry):
    path = os.environ.get("MCP_ELICITATION_LOG")
    if path:
        with open(path, "a") as handle:
            handle.write(json.dumps(entry) + "\n")


def main():
    client_capabilities = {}
    pending = None  # (tool call id) while waiting for the elicitation answer
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        message = json.loads(line)
        method = message.get("method")
        if method == "initialize":
            params = message.get("params") or {}
            client_capabilities = params.get("capabilities") or {}
            note({"event": "initialize", "clientElicitation": client_capabilities.get("elicitation")})
            send({"jsonrpc": "2.0", "id": message["id"], "result": {
                "protocolVersion": params.get("protocolVersion", "2025-06-18"),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "elicitation-fixture", "version": "1.0.0"}}})
        elif method == "tools/list":
            send({"jsonrpc": "2.0", "id": message["id"], "result": {"tools": [{
                "name": "ask_preference",
                "description": "Ask the user which colour and how many items they want. Returns their answer.",
                "inputSchema": {"type": "object", "properties": {}}}]}})
        elif method == "tools/call":
            if "elicitation" not in client_capabilities:
                note({"event": "no_client_elicitation"})
                send({"jsonrpc": "2.0", "id": message["id"], "result": {"isError": True, "content": [
                    {"type": "text", "text": "The MCP client did not advertise elicitation support."}]}})
                continue
            pending = message["id"]
            send({"jsonrpc": "2.0", "id": "fixture-elicitation", "method": "elicitation/create", "params": {
                "message": "Which colour should the report use, and how many copies?",
                "requestedSchema": SCHEMA}})
        elif message.get("id") == "fixture-elicitation" and pending is not None:
            result = message.get("result")
            note({"event": "answer", "result": result, "error": message.get("error")})
            text = f"User answer: {json.dumps(result if result is not None else {'error': message.get('error')})}"
            send({"jsonrpc": "2.0", "id": pending, "result": {"content": [{"type": "text", "text": text}]}})
            pending = None
        elif "id" in message and method is not None:
            send({"jsonrpc": "2.0", "id": message["id"], "error": {"code": -32601, "message": "Method not found"}})


if __name__ == "__main__":
    main()
