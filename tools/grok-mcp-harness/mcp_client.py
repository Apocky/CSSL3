#!/usr/bin/env python3
"""
mcp_client.py · minimal terminal MCP client for the apocky-harness.

USAGE
    python mcp_client.py [URL]

    URL defaults to http://localhost:8080/mcp · accepts cloudflared-tunnel-URLs.

INTERACTIVE PROMPT
    > fs_read_file path=specs/55_NORMAL_ENGINE_PIVOT.csl
    > csl_parse spec="§ test ::"
    > infinity_engine_status
    > list             # list available tools
    > help             # show this
    > quit             # exit

Each line is parsed as `tool_name [k=v ...]`. Quoted values supported.
"""

from __future__ import annotations

import json
import os
import shlex
import sys
import uuid
from typing import Any

try:
    import httpx
except ImportError:
    print("ERROR: httpx not installed. Run: pip install httpx", file=sys.stderr)
    sys.exit(1)


DEFAULT_URL = "http://localhost:8080/mcp"


def load_bearer_token() -> str | None:
    """Load BEARER_TOKEN from sibling .env if present."""
    env_path = os.path.join(os.path.dirname(__file__), ".env")
    if not os.path.isfile(env_path):
        return os.environ.get("BEARER_TOKEN")
    with open(env_path) as f:
        for line in f:
            line = line.strip()
            if line.startswith("BEARER_TOKEN="):
                return line.split("=", 1)[1].strip()
    return None


def parse_kv(arg_str: str) -> dict[str, Any]:
    """Parse `key=value key2="quoted value"` into dict."""
    if not arg_str.strip():
        return {}
    out = {}
    for token in shlex.split(arg_str):
        if "=" not in token:
            continue
        k, v = token.split("=", 1)
        # try JSON parse for ints/floats/bools/lists
        try:
            v_parsed = json.loads(v)
        except (json.JSONDecodeError, ValueError):
            v_parsed = v
        out[k.strip()] = v_parsed
    return out


def jsonrpc_call(client: httpx.Client, url: str, method: str, params: dict[str, Any], headers: dict[str, str]) -> dict[str, Any]:
    """Send a JSON-RPC 2.0 request to the MCP server."""
    payload = {
        "jsonrpc": "2.0",
        "id": str(uuid.uuid4()),
        "method": method,
        "params": params,
    }
    resp = client.post(url, json=payload, headers=headers, timeout=60.0)
    resp.raise_for_status()
    # MCP servers may stream SSE; httpx returns raw text. Parse as JSON if possible.
    try:
        return resp.json()
    except json.JSONDecodeError:
        # Try SSE-style "data: {...}" lines
        for line in resp.text.splitlines():
            if line.startswith("data: "):
                try:
                    return json.loads(line[6:])
                except json.JSONDecodeError:
                    continue
        return {"error": {"message": "non-JSON response", "raw": resp.text[:500]}}


def list_tools(client: httpx.Client, url: str, headers: dict[str, str]) -> list[dict[str, Any]]:
    res = jsonrpc_call(client, url, "tools/list", {}, headers)
    if "result" in res:
        return res["result"].get("tools", [])
    return []


def call_tool(client: httpx.Client, url: str, headers: dict[str, str], name: str, args: dict[str, Any]) -> Any:
    res = jsonrpc_call(client, url, "tools/call", {"name": name, "arguments": args}, headers)
    if "result" in res:
        return res["result"]
    return res.get("error", res)


def initialize(client: httpx.Client, url: str, headers: dict[str, str]) -> dict[str, Any]:
    """Send the initial MCP handshake."""
    return jsonrpc_call(
        client, url, "initialize",
        {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "apocky-mcp-client", "version": "1.0.0"},
        },
        headers,
    )


HELP_TEXT = """
Available commands :
  list                       # list tools available on the server
  help                       # show this help
  quit / exit                # exit
  <tool_name> k1=v1 k2=v2... # invoke a tool

Common tool examples (verify with `list`) :
  fs_read_file path=specs/55_NORMAL_ENGINE_PIVOT.csl
  fs_write_file path=tmp/test.txt content="hello"
  csl_parse spec="§ test ::"
  cssl_compile path=examples/hello_main.cssl emit=object
  infinity_engine_status
  infinity_engine_sync action=ping
"""


def main() -> int:
    url = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_URL
    if not url.endswith("/mcp"):
        url = url.rstrip("/") + "/mcp"

    bearer = load_bearer_token()
    headers = {
        "Content-Type": "application/json",
        "Accept": "application/json, text/event-stream",
    }
    if bearer:
        headers["Authorization"] = f"Bearer {bearer}"
        print(f"[mcp_client] using bearer-token from .env (len={len(bearer)})")
    print(f"[mcp_client] connecting to {url}")

    with httpx.Client() as client:
        try:
            init_res = initialize(client, url, headers)
            print(f"[mcp_client] initialized · server-info={init_res.get('result', init_res)}")
        except httpx.HTTPError as e:
            print(f"[mcp_client] ERROR connecting : {e}", file=sys.stderr)
            return 1

        print("Type 'list' to see tools · 'help' for usage · 'quit' to exit.\n")

        while True:
            try:
                line = input("apocky> ").strip()
            except (EOFError, KeyboardInterrupt):
                print()
                break
            if not line:
                continue
            cmd, _, args = line.partition(" ")
            cmd = cmd.lower()
            if cmd in ("quit", "exit"):
                break
            if cmd == "help":
                print(HELP_TEXT)
                continue
            if cmd == "list":
                tools = list_tools(client, url, headers)
                if not tools:
                    print("(no tools listed · server may still be initializing)")
                    continue
                for t in tools:
                    name = t.get("name", "?")
                    desc = (t.get("description") or "").splitlines()[0][:80]
                    print(f"  {name:<32} {desc}")
                continue

            try:
                parsed = parse_kv(args)
            except ValueError as e:
                print(f"[mcp_client] arg parse error : {e}")
                continue

            try:
                result = call_tool(client, url, headers, cmd, parsed)
            except httpx.HTTPError as e:
                print(f"[mcp_client] HTTP error : {e}")
                continue

            # Pretty-print result
            if isinstance(result, dict) and "content" in result:
                for item in result["content"]:
                    if item.get("type") == "text":
                        print(item.get("text", ""))
                    else:
                        print(json.dumps(item, indent=2))
            else:
                print(json.dumps(result, indent=2, default=str))

    print("[mcp_client] bye.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
