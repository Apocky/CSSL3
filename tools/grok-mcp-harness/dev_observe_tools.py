#!/usr/bin/env python3
"""FastMCP tools for the local dev-observe ledger."""

from __future__ import annotations

import json
import importlib.util
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from fastmcp import FastMCP
from pydantic import BaseModel, Field


ROOT = Path(__file__).resolve().parents[2]
LEDGER = ROOT / "tools" / "dev-observe" / "logs" / "dev_runs.jsonl"
LATEST = ROOT / "tools" / "dev-observe" / "logs" / "latest.json"


class Observation(BaseModel):
    repo: str = "CSSLv3"
    phase: str = "dev"
    status: str = Field(pattern="^(pass|fail|warn|info)$")
    summary: str
    critique: list[str] = []
    next: list[str] = []


class RenderV3BenchRequest(BaseModel):
    log_path: str
    image_path: str | None = None
    compare_image_path: str | None = None
    compare_label: str | None = None
    diff_image_path: str | None = None
    production_shape: bool = False
    rich_scene: bool = False
    mixed_sim: bool = False
    input_telemetry: bool = False
    accepted_utilization: bool = False
    include_csl: bool = True


def _read_events(limit: int) -> list[dict[str, Any]]:
    if not LEDGER.exists():
        return []
    events: list[dict[str, Any]] = []
    for line in LEDGER.read_text(encoding="utf-8").splitlines()[-limit:]:
        if not line.strip():
            continue
        try:
            events.append(json.loads(line))
        except json.JSONDecodeError:
            continue
    return events


def _append_event(event: dict[str, Any]) -> None:
    LEDGER.parent.mkdir(parents=True, exist_ok=True)
    with LEDGER.open("a", encoding="utf-8") as f:
        f.write(json.dumps(event, ensure_ascii=False, sort_keys=True) + "\n")
    LATEST.write_text(json.dumps(event, ensure_ascii=False, indent=2, sort_keys=True), encoding="utf-8")


def _repo_path(value: str | None) -> Path | None:
    if value is None:
        return None
    path = Path(value)
    resolved = path.resolve() if path.is_absolute() else (ROOT / path).resolve()
    root = ROOT.resolve()
    if resolved != root and root not in resolved.parents:
        raise PermissionError(f"path outside CSSLv3 repo: {value}")
    return resolved


def _load_render_v3_bench() -> Any:
    script = ROOT / "tools" / "render-v3" / "v3_present_bench.py"
    spec = importlib.util.spec_from_file_location("render_v3_bench", script)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load render benchmark analyzer: {script}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def _benign_stderr(text: str) -> bool:
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


def _stdout_needs_attention(text: str) -> bool:
    return "pass_benchmark_budget_fail_production_evidence" in text or "status: fail_budget" in text


def register_dev_observe_tools(mcp: FastMCP) -> None:
    """Register read/write tools for local dev-observe telemetry."""

    @mcp.tool
    def dev_observe_latest() -> dict[str, Any]:
        """Read the latest local development observation."""
        if not LATEST.exists():
            return {"status": "empty", "event": None}
        return {"status": "ok", "event": json.loads(LATEST.read_text(encoding="utf-8"))}

    @mcp.tool
    def dev_observe_recent(limit: int = 20) -> dict[str, Any]:
        """Read recent local command telemetry events."""
        events = _read_events(max(1, min(limit, 200)))
        return {"status": "ok", "count": len(events), "events": events}

    @mcp.tool
    def dev_observe_review(limit: int = 20) -> str:
        """Return a CSL constructive review over recent development telemetry."""
        events = _read_events(max(1, min(limit, 200)))
        if not events:
            return "§ dev.observe.review\n  status: ○ no-events\n∎"
        failed = [e for e in events if e.get("status") not in ("pass", "info")]
        warned = [
            e
            for e in events
            if e.get("status") == "warn"
            or (e.get("stderr_tail", "").strip() and not _benign_stderr(e.get("stderr_tail", "")))
            or _stdout_needs_attention(e.get("stdout_tail", ""))
        ]
        slowest = max(events, key=lambda e: e.get("duration_ms", 0))
        lines = [
            "§ dev.observe.review",
            f"  window.events: {len(events)}",
            f"  fail: {len(failed)}",
            f"  warn: {len(warned)}",
            f"  slowest: {slowest.get('repo')}:{slowest.get('phase')} {slowest.get('duration_ms', 0)}ms",
            "  § critique",
        ]
        if failed:
            lines.extend(
                f"    ✗ {e.get('repo')}:{e.get('phase')} exit={e.get('exit_code')} cmd={e.get('command_preview', e.get('summary', ''))}"
                for e in failed[:5]
            )
        elif warned:
            lines.extend(
                f"    ◐ {e.get('repo')}:{e.get('phase')} evidence gap or warning needs classification"
                for e in warned[:5]
            )
        else:
            lines.append("    ✓ recent evidence clean")
        lines.append("  § next")
        lines.append("    W! fix failing gate first" if failed else "    R! proceed to next verified slice")
        lines.append("∎")
        return "\n".join(lines)

    @mcp.tool
    def dev_observe_record(obs: Observation) -> dict[str, Any]:
        """Append a manual local development observation."""
        event = {
            "schema": "apocky.dev_observe.v1",
            "kind": "manual",
            "ts_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
            "repo": obs.repo,
            "phase": obs.phase,
            "status": obs.status,
            "summary": obs.summary,
            "critique": obs.critique,
            "next": obs.next,
        }
        _append_event(event)
        return {"status": "ok", "event": event}

    @mcp.tool
    def dev_observe_render_v3_analyze(req: RenderV3BenchRequest) -> dict[str, Any]:
        """Analyze Render V3 present telemetry/artifacts through the shared benchmark gate."""
        bench = _load_render_v3_bench()
        args = type(
            "Args",
            (),
            {
                "log": _repo_path(req.log_path),
                "image": _repo_path(req.image_path),
                "compare_image": _repo_path(req.compare_image_path),
                "compare_label": req.compare_label,
                "diff_image": _repo_path(req.diff_image_path),
                "diff_amplify": 4,
                "production_shape": req.production_shape,
                "rich_scene": req.rich_scene,
                "mixed_sim": req.mixed_sim,
                "input_telemetry": req.input_telemetry,
                "accepted_utilization": req.accepted_utilization,
            },
        )()
        report = bench.build_report(args)
        result: dict[str, Any] = {"status": "ok", "report": report}
        if req.include_csl:
            result["csl"] = bench.report_to_csl(report)
        return result
