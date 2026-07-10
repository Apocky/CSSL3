#!/usr/bin/env python3
"""
§ dev_observe — local command evidence + constructive critique ledger.

Writes path-redacted JSONL telemetry for development runs and emits CSL review
summaries. Designed for local-only practical use by Codex/Grok/MCP clients.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


SCHEMA = "apocky.dev_observe.v1"
THIS = Path(__file__).resolve()
ROOT = THIS.parents[2]
LOG_DIR = THIS.parent / "logs"
RUNS_JSONL = LOG_DIR / "dev_runs.jsonl"
LATEST_JSON = LOG_DIR / "latest.json"

KNOWN_ROOTS = {
    "CSSLv3": ROOT,
    "LoA_v13": ROOT.parent / "LoA v13",
    "LoA_v14": ROOT.parent / "LoA v14",
    "repos": ROOT.parent,
    "user": Path.home(),
}

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def sha16(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8", errors="replace")).hexdigest()[:16]


def redact(text: str) -> str:
    out = text
    roots = sorted(KNOWN_ROOTS.items(), key=lambda kv: len(str(kv[1])), reverse=True)
    for label, root in roots:
        raw = str(root)
        if raw:
            out = out.replace(raw, f"<{label}>")
            out = out.replace(raw.replace("\\", "/"), f"<{label}>")
    return out


def tail(text: str, limit: int = 4000) -> str:
    text = redact(text)
    if len(text) <= limit:
        return text
    return text[-limit:]


def status_from_exit(exit_code: int) -> str:
    return "pass" if exit_code == 0 else "fail"


def benign_stderr(text: str) -> bool:
    stripped = text.strip()
    if not stripped:
        return True
    lines = [line.strip() for line in stripped.splitlines() if line.strip()]
    cargo_prefixes = (
        "Blocking waiting for file lock",
        "Compiling ",
        "Checking ",
        "Finished ",
        "Running ",
        "Doc-tests ",
        "Downloading ",
        "Downloaded ",
        "Updating ",
        "Locking ",
        "Adding ",
    )

    def benign_line(line: str) -> bool:
        lower = line.lower()
        if "warning:" in lower or "error:" in lower or "panicked" in lower:
            return False
        if line.startswith("csslc: check ") and line.endswith(" : OK"):
            return True
        if "LoA-v13 starting" in line and "pure-CSSL native" in line:
            return True
        return line.startswith(cargo_prefixes)

    return bool(lines) and all(benign_line(line) for line in lines)


def stdout_needs_attention(text: str) -> bool:
    return "pass_benchmark_budget_fail_production_evidence" in text or "status: fail_budget" in text


def critique_for(event: dict[str, Any]) -> list[str]:
    critique: list[str] = []
    if event["status"] != "pass":
        critique.append("✗ command failed ; inspect stderr_tail + rerun after single-variable fix")
    if event["duration_ms"] > 60_000:
        critique.append("⚠ slow feedback loop >60s ; split command or cache build artifacts")
    if event["stderr_tail"].strip() and event["status"] == "pass" and not benign_stderr(event["stderr_tail"]):
        critique.append("◐ passing command emitted stderr ; classify warnings before treating green as clean")
    if event["status"] == "pass" and stdout_needs_attention(event["stdout_tail"]):
        critique.append("◐ benchmark command ran, but report still has production-evidence gaps")
    if event["stdout_bytes"] == 0 and event["stderr_bytes"] == 0:
        critique.append("◐ no process output ; keep exit-code but add stronger observable where possible")
    if not critique:
        critique.append("✓ evidence adequate for this slice")
    return critique


def next_for(event: dict[str, Any]) -> list[str]:
    if event["status"] != "pass":
        return ["reproduce with narrower command", "record root cause", "add regression gate"]
    if event["stderr_tail"].strip() and not benign_stderr(event["stderr_tail"]):
        return ["triage warnings", "decide suppress vs fix"]
    if stdout_needs_attention(event["stdout_tail"]):
        return ["close benchmark evidence gaps before production readiness claim"]
    return ["promote evidence into handoff/spec when load-bearing"]


def append_event(event: dict[str, Any]) -> None:
    LOG_DIR.mkdir(parents=True, exist_ok=True)
    with RUNS_JSONL.open("a", encoding="utf-8") as f:
        f.write(json.dumps(event, ensure_ascii=False, sort_keys=True) + "\n")
    LATEST_JSON.write_text(json.dumps(event, ensure_ascii=False, indent=2, sort_keys=True), encoding="utf-8")


def load_events(limit: int) -> list[dict[str, Any]]:
    if not RUNS_JSONL.exists():
        return []
    lines = RUNS_JSONL.read_text(encoding="utf-8").splitlines()
    out: list[dict[str, Any]] = []
    for line in lines[-limit:]:
        if not line.strip():
            continue
        try:
            out.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return out


def run_command(args: argparse.Namespace) -> int:
    cwd_arg = Path(args.cwd) if args.cwd else ROOT
    cwd = cwd_arg.resolve() if cwd_arg.is_absolute() else (ROOT / cwd_arg).resolve()
    command = args.command
    if command and command[0] == "--":
        command = command[1:]
    started = time.perf_counter()
    proc = subprocess.run(
        command,
        cwd=cwd,
        text=True,
        encoding="utf-8",
        errors="replace",
        capture_output=True,
        shell=args.shell,
    )
    duration_ms = int((time.perf_counter() - started) * 1000)
    command_text = command if isinstance(command, str) else " ".join(command)
    event: dict[str, Any] = {
        "schema": SCHEMA,
        "kind": "command",
        "ts_utc": utc_now(),
        "phase": args.phase,
        "repo": args.repo,
        "cwd_label": redact(str(cwd)),
        "cwd_hash": sha16(str(cwd)),
        "command_preview": redact(command_text),
        "command_hash": sha16(command_text),
        "exit_code": proc.returncode,
        "status": status_from_exit(proc.returncode),
        "duration_ms": duration_ms,
        "stdout_bytes": len(proc.stdout.encode("utf-8", errors="replace")),
        "stderr_bytes": len(proc.stderr.encode("utf-8", errors="replace")),
        "stdout_tail": tail(proc.stdout),
        "stderr_tail": tail(proc.stderr),
    }
    event["critique"] = critique_for(event)
    event["next"] = next_for(event)
    append_event(event)
    print(json.dumps(event, ensure_ascii=False, indent=2, sort_keys=True))
    return proc.returncode


def review(args: argparse.Namespace) -> int:
    events = load_events(args.limit)
    if not events:
        print("§ dev.observe.review\n  status: ○ no-events\n∎")
        return 1
    total = len(events)
    failed = [e for e in events if e.get("status") != "pass"]
    warned = [
        e
        for e in events
        if (e.get("stderr_tail", "").strip() and not benign_stderr(e.get("stderr_tail", "")))
        or stdout_needs_attention(e.get("stdout_tail", ""))
    ]
    slowest = max(events, key=lambda e: e.get("duration_ms", 0))
    print("§ dev.observe.review")
    print(f"  window.events: {total}")
    print(f"  pass: {total - len(failed)}")
    print(f"  fail: {len(failed)}")
    print(f"  warn.stderr: {len(warned)}")
    print(f"  slowest: {slowest.get('repo')}:{slowest.get('phase')} {slowest.get('duration_ms')}ms")
    print("  § critique")
    if failed:
        for e in failed[:5]:
            print(f"    ✗ {e.get('repo')}:{e.get('phase')} exit={e.get('exit_code')} cmd={e.get('command_preview')}")
    if warned:
        for e in warned[:5]:
            print(f"    ◐ {e.get('repo')}:{e.get('phase')} evidence gap or warning present ; classify before readiness claim")
    if not failed and not warned:
        print("    ✓ recent evidence clean")
    print("  § next")
    if failed:
        print("    W! fix first failing command before widening scope")
    elif warned:
        print("    R! triage warning debt before claiming production-clean")
    else:
        print("    R! move to next smallest verified slice")
    print("∎")
    return 0 if not failed else 1


def latest(_: argparse.Namespace) -> int:
    if not LATEST_JSON.exists():
        print("{}")
        return 1
    print(LATEST_JSON.read_text(encoding="utf-8"))
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="local development evidence ledger")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_run = sub.add_parser("run", help="run command and append JSONL telemetry")
    p_run.add_argument("--repo", default="CSSLv3")
    p_run.add_argument("--phase", default="dev")
    p_run.add_argument("--cwd", default=str(ROOT))
    p_run.add_argument("--shell", action="store_true")
    p_run.add_argument("command", nargs=argparse.REMAINDER)
    p_run.set_defaults(func=run_command)

    p_review = sub.add_parser("review", help="print CSL constructive review")
    p_review.add_argument("--limit", type=int, default=20)
    p_review.set_defaults(func=review)

    p_latest = sub.add_parser("latest", help="print latest JSON event")
    p_latest.set_defaults(func=latest)

    args = parser.parse_args(argv)
    if args.cmd == "run" and not args.command:
        parser.error("run requires command")
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
