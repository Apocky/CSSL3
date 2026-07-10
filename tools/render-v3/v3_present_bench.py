#!/usr/bin/env python3
"""Render V3 telemetry + artifact benchmark analysis.

This is deliberately dependency-free. It parses LoA V3 present telemetry logs,
PPM capture artifacts, and optional image pairs into a durable JSON report with
explicit high-refresh gates and evidence gaps.
"""

from __future__ import annotations

import argparse
import ctypes
import json
import math
import os
import subprocess
import sys
import threading
import tempfile
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable


SCHEMA = "apocky.render_v3_bench.v1"
ROOT = Path(__file__).resolve().parents[2]
FRAME_PREFIX = "telemetry.present_frame "
CAPTURE_PREFIX = "telemetry.present_capture "
CPU_FIELDS = (
    "cpu_wait_us",
    "cpu_acquire_us",
    "cpu_upload_us",
    "cpu_record_us",
    "cpu_submit_us",
    "cpu_present_us",
)
MIXED_SIM_FIELDS = (
    "mixed_sim_tick_us",
    "synthetic_input_sample_us",
    "synthetic_input_jitter_us",
)
REAL_INPUT_FIELDS = (
    "raw_input_latency_us",
    "input_latency_us",
    "input_jitter_us",
)
UTILIZATION_FIELDS = (
    "process_cpu_pct",
    "working_set_mb",
    "private_bytes_mb",
    "gpu_busy_pct",
    "vram_used_mb",
)
BUDGETS_US = {
    "144hz": 6_944,
    "240hz": 4_166,
}

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
if hasattr(sys.stderr, "reconfigure"):
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def maybe_number(value: str) -> int | float | bool | str:
    if value == "true":
        return True
    if value == "false":
        return False
    try:
        if value.lower().startswith("0x"):
            return int(value, 16)
        return int(value)
    except ValueError:
        pass
    try:
        return float(value)
    except ValueError:
        return value


def parse_kv_payload(payload: str) -> dict[str, Any]:
    row: dict[str, Any] = {}
    for token in payload.strip().split():
        if "=" not in token:
            continue
        key, value = token.split("=", 1)
        row[key] = maybe_number(value)
    return row


def parse_telemetry_log(path: Path) -> dict[str, Any]:
    frames: list[dict[str, Any]] = []
    captures: list[dict[str, Any]] = []
    if not path.exists():
        raise FileNotFoundError(path)
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        if line.startswith(FRAME_PREFIX):
            frames.append(parse_kv_payload(line[len(FRAME_PREFIX) :]))
        elif line.startswith(CAPTURE_PREFIX):
            captures.append(parse_kv_payload(line[len(CAPTURE_PREFIX) :]))
    return {
        "path": str(path),
        "line_count": sum(1 for _ in path.open("r", encoding="utf-8", errors="replace")),
        "frames": frames,
        "captures": captures,
    }


def numeric_values(frames: Iterable[dict[str, Any]], field: str) -> list[int | float]:
    values: list[int | float] = []
    for frame in frames:
        value = frame.get(field)
        if isinstance(value, bool):
            continue
        if isinstance(value, (int, float)):
            values.append(value)
    return values


def cpu_total_values(frames: Iterable[dict[str, Any]]) -> list[int]:
    values: list[int] = []
    for frame in frames:
        total = 0
        ok = True
        for field in CPU_FIELDS:
            value = frame.get(field)
            if not isinstance(value, int):
                ok = False
                break
            total += value
        if ok:
            values.append(total)
    return values


def cpu_total_for_frame(frame: dict[str, Any]) -> int | None:
    total = 0
    for field in CPU_FIELDS:
        value = frame.get(field)
        if not isinstance(value, int):
            return None
        total += value
    return total


def percentile_nearest_rank(values: list[int | float], pct: float) -> int | float | None:
    if not values:
        return None
    ordered = sorted(values)
    rank = max(1, math.ceil((pct / 100.0) * len(ordered)))
    return ordered[min(rank - 1, len(ordered) - 1)]


def stats(values: list[int | float]) -> dict[str, Any]:
    if not values:
        return {"count": 0}
    return {
        "count": len(values),
        "min": min(values),
        "p50": percentile_nearest_rank(values, 50.0),
        "p95": percentile_nearest_rank(values, 95.0),
        "p99": percentile_nearest_rank(values, 99.0),
        "p99_9": percentile_nearest_rank(values, 99.9),
        "max": max(values),
    }


def ratio_pct(numerator: Any, denominator: Any) -> float | None:
    if not isinstance(numerator, (int, float)) or not isinstance(denominator, (int, float)):
        return None
    if denominator <= 0:
        return None
    return (float(numerator) / float(denominator)) * 100.0


def occupancy_from(wall: dict[str, Any], cpu: dict[str, Any], gpu: dict[str, Any]) -> dict[str, Any]:
    keys = ("p50", "p95", "p99", "p99_9")
    return {
        "cpu_frame_pct": {key: ratio_pct(cpu.get(key), wall.get(key)) for key in keys},
        "gpu_dispatch_frame_pct": {key: ratio_pct(gpu.get(key), wall.get(key)) for key in keys},
    }


def first_last(fields: list[dict[str, Any]], key: str) -> dict[str, Any]:
    values = [row.get(key) for row in fields if key in row]
    if not values:
        return {"first": None, "last": None, "unique": []}
    unique = []
    for value in values:
        if value not in unique:
            unique.append(value)
    return {"first": values[0], "last": values[-1], "unique": unique[:16]}


def ppm_tokens(blob: bytes) -> tuple[list[bytes], int]:
    tokens: list[bytes] = []
    i = 0
    n = len(blob)
    while len(tokens) < 4 and i < n:
        while i < n and blob[i] in b" \t\r\n":
            i += 1
        if i < n and blob[i] == ord("#"):
            while i < n and blob[i] not in b"\r\n":
                i += 1
            continue
        start = i
        while i < n and blob[i] not in b" \t\r\n":
            i += 1
        if start != i:
            tokens.append(blob[start:i])
    if i < n and blob[i] in b" \t\r\n":
        if blob[i] == ord("\r") and i + 1 < n and blob[i + 1] == ord("\n"):
            i += 2
        else:
            i += 1
    return tokens, i


@dataclass(frozen=True)
class PpmImage:
    path: Path
    width: int
    height: int
    max_value: int
    data: bytes


def read_ppm(path: Path) -> PpmImage:
    blob = path.read_bytes()
    tokens, offset = ppm_tokens(blob)
    if len(tokens) != 4 or tokens[0] != b"P6":
        raise ValueError(f"{path} is not a binary P6 PPM")
    width = int(tokens[1])
    height = int(tokens[2])
    max_value = int(tokens[3])
    expected = width * height * 3
    data = blob[offset:]
    if len(data) != expected:
        raise ValueError(f"{path} has {len(data)} RGB bytes, expected {expected}")
    if max_value != 255:
        raise ValueError(f"{path} max value {max_value} is unsupported")
    return PpmImage(path=path, width=width, height=height, max_value=max_value, data=data)


def fnv1a64(data: bytes) -> int:
    h = 0xCBF29CE484222325
    for byte in data:
        h ^= byte
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def analyze_ppm(path: Path, rough_stride: int = 8) -> dict[str, Any]:
    img = read_ppm(path)
    count = img.width * img.height
    sums = [0, 0, 0]
    mins = [255, 255, 255]
    maxs = [0, 0, 0]
    unique = [set(), set(), set()]
    data = img.data
    for i in range(0, len(data), 3):
        for c in range(3):
            value = data[i + c]
            sums[c] += value
            mins[c] = min(mins[c], value)
            maxs[c] = max(maxs[c], value)
            unique[c].add(value)

    rough_x = [0, 0, 0]
    rough_y = [0, 0, 0]
    nx = 0
    ny = 0
    stride = max(1, rough_stride)
    for y in range(0, img.height, stride):
        for x in range(0, img.width, stride):
            idx = (y * img.width + x) * 3
            if x + stride < img.width:
                idx2 = (y * img.width + x + stride) * 3
                for c in range(3):
                    rough_x[c] += abs(data[idx + c] - data[idx2 + c])
                nx += 1
            if y + stride < img.height:
                idx2 = ((y + stride) * img.width + x) * 3
                for c in range(3):
                    rough_y[c] += abs(data[idx + c] - data[idx2 + c])
                ny += 1

    return {
        "path": str(path),
        "width": img.width,
        "height": img.height,
        "bytes": len(data),
        "checksum_fnv1a64": fnv1a64(data),
        "extrema": {"r": [mins[0], maxs[0]], "g": [mins[1], maxs[1]], "b": [mins[2], maxs[2]]},
        "mean_rgb": [sums[c] / count for c in range(3)],
        "unique_bins": {"r": len(unique[0]), "g": len(unique[1]), "b": len(unique[2])},
        "roughness_stride": stride,
        "roughness_dx_rgb": [rough_x[c] / nx if nx else 0.0 for c in range(3)],
        "roughness_dy_rgb": [rough_y[c] / ny if ny else 0.0 for c in range(3)],
    }


def compare_ppm(path_a: Path, path_b: Path, diff_path: Path | None = None, amplify: int = 4) -> dict[str, Any]:
    a = read_ppm(path_a)
    b = read_ppm(path_b)
    if (a.width, a.height) != (b.width, b.height):
        raise ValueError(f"image dimensions differ: {path_a} {a.width}x{a.height}, {path_b} {b.width}x{b.height}")
    changed = 0
    sums = [0, 0, 0]
    max_delta = [0, 0, 0]
    diff = bytearray(len(a.data)) if diff_path else None
    for i in range(0, len(a.data), 3):
        pixel_changed = False
        for c in range(3):
            delta = abs(a.data[i + c] - b.data[i + c])
            sums[c] += delta
            max_delta[c] = max(max_delta[c], delta)
            if delta:
                pixel_changed = True
            if diff is not None:
                diff[i + c] = min(255, delta * max(1, amplify))
        if pixel_changed:
            changed += 1
    pixels = a.width * a.height
    if diff_path and diff is not None:
        header = f"P6\n{a.width} {a.height}\n255\n".encode("ascii")
        diff_path.parent.mkdir(parents=True, exist_ok=True)
        diff_path.write_bytes(header + bytes(diff))
    return {
        "path_a": str(path_a),
        "path_b": str(path_b),
        "width": a.width,
        "height": a.height,
        "changed_pixels": changed,
        "changed_ppm": (changed / pixels) * 1_000_000,
        "mean_abs_delta_rgb": [sums[c] / pixels for c in range(3)],
        "max_delta_rgb": max_delta,
        "diff_path": str(diff_path) if diff_path else None,
    }


def write_bmp_from_ppm(path: Path, out_path: Path) -> dict[str, Any]:
    img = read_ppm(path)
    row_bytes = img.width * 3
    padded_row_bytes = (row_bytes + 3) & ~3
    image_bytes = padded_row_bytes * img.height
    file_size = 14 + 40 + image_bytes
    header = bytearray()
    header.extend(b"BM")
    header.extend(file_size.to_bytes(4, "little"))
    header.extend((0).to_bytes(4, "little"))
    header.extend((54).to_bytes(4, "little"))
    header.extend((40).to_bytes(4, "little"))
    header.extend(img.width.to_bytes(4, "little", signed=True))
    header.extend(img.height.to_bytes(4, "little", signed=True))
    header.extend((1).to_bytes(2, "little"))
    header.extend((24).to_bytes(2, "little"))
    header.extend((0).to_bytes(4, "little"))
    header.extend(image_bytes.to_bytes(4, "little"))
    header.extend((2835).to_bytes(4, "little"))
    header.extend((2835).to_bytes(4, "little"))
    header.extend((0).to_bytes(4, "little"))
    header.extend((0).to_bytes(4, "little"))

    padding = b"\x00" * (padded_row_bytes - row_bytes)
    body = bytearray()
    for y in range(img.height - 1, -1, -1):
        row_start = y * img.width * 3
        row = img.data[row_start : row_start + row_bytes]
        for i in range(0, len(row), 3):
            body.extend((row[i + 2], row[i + 1], row[i]))
        body.extend(padding)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_bytes(bytes(header) + bytes(body))
    return {
        "source": str(path),
        "out_bmp": str(out_path),
        "width": img.width,
        "height": img.height,
        "bytes": file_size,
    }


def frame_summary(frames: list[dict[str, Any]]) -> dict[str, Any]:
    wall = stats(numeric_values(frames, "wall_since_prev_us"))
    cpu = stats(cpu_total_values(frames))
    gpu = stats(numeric_values(frames, "previous_gpu_dispatch_us"))
    return {
        "frame_rows": len(frames),
        "wall_since_prev_us": wall,
        "cpu_total_us": cpu,
        "gpu_dispatch_us": gpu,
        "derived_occupancy": occupancy_from(wall, cpu, gpu),
        "cpu_segments_us": {field: stats(numeric_values(frames, field)) for field in CPU_FIELDS},
        "mixed_workload": {field: stats(numeric_values(frames, field)) for field in MIXED_SIM_FIELDS},
        "input_latency": {field: stats(numeric_values(frames, field)) for field in REAL_INPUT_FIELDS},
        "utilization": {field: stats(numeric_values(frames, field)) for field in UTILIZATION_FIELDS},
        "fields": {
            "use_case": first_last(frames, "use_case"),
            "realworld_gate": first_last(frames, "realworld_gate"),
            "present_mode": first_last(frames, "present_mode"),
            "frame_pace": first_last(frames, "frame_pace"),
            "cpu_substrate_tick": first_last(frames, "cpu_substrate_tick"),
            "width": first_last(frames, "width"),
            "height": first_last(frames, "height"),
            "stress_room": first_last(frames, "stress_room"),
            "world_crystals": first_last(frames, "world_crystals"),
            "active_crystals": first_last(frames, "active_crystals"),
            "observer_yaw_milli": first_last(frames, "observer_yaw_milli"),
        },
    }


def frame_outliers(frames: list[dict[str, Any]], *, limit: int = 8) -> dict[str, Any]:
    rows: list[dict[str, Any]] = []
    for idx, frame in enumerate(frames):
        wall = frame.get("wall_since_prev_us")
        if not isinstance(wall, (int, float)):
            continue
        rows.append(
            {
                "row": idx,
                "frame_count_before": frame.get("frame_count_before"),
                "wall_since_prev_us": wall,
                "cpu_total_us": cpu_total_for_frame(frame),
                "previous_gpu_dispatch_us": frame.get("previous_gpu_dispatch_us"),
                "cpu_wait_us": frame.get("cpu_wait_us"),
                "cpu_acquire_us": frame.get("cpu_acquire_us"),
                "cpu_upload_us": frame.get("cpu_upload_us"),
                "cpu_record_us": frame.get("cpu_record_us"),
                "cpu_submit_us": frame.get("cpu_submit_us"),
                "cpu_present_us": frame.get("cpu_present_us"),
                "process_cpu_pct": frame.get("process_cpu_pct"),
                "working_set_mb": frame.get("working_set_mb"),
            }
        )
    rows.sort(key=lambda r: float(r["wall_since_prev_us"]), reverse=True)
    p999 = stats([r["wall_since_prev_us"] for r in rows]).get("p99_9")
    p240_budget = BUDGETS_US["240hz"]
    return {
        "top_wall_frames": rows[:limit],
        "frames_over_240hz_budget": sum(
            1 for r in rows if isinstance(r["wall_since_prev_us"], (int, float)) and r["wall_since_prev_us"] > p240_budget
        ),
        "frames_over_p99_9": sum(
            1
            for r in rows
            if isinstance(p999, (int, float))
            and isinstance(r["wall_since_prev_us"], (int, float))
            and r["wall_since_prev_us"] > p999
        ),
    }


def gate_value(summary: dict[str, Any], field: str, pct: str) -> int | float | None:
    value = summary.get(field, {}).get(pct)
    return value if isinstance(value, (int, float)) else None


def budget_report(summary: dict[str, Any]) -> dict[str, Any]:
    wall_p99 = gate_value(summary, "wall_since_prev_us", "p99")
    wall_p99_9 = gate_value(summary, "wall_since_prev_us", "p99_9")
    out: dict[str, Any] = {}
    for label, budget in BUDGETS_US.items():
        p99_pass = wall_p99 is not None and wall_p99 <= budget
        p99_9_pass = wall_p99_9 is not None and wall_p99_9 <= budget
        out[label] = {
            "budget_us": budget,
            "wall_p99_us": wall_p99,
            "wall_p99_9_us": wall_p99_9,
            "p99_pass": p99_pass,
            "p99_9_pass": p99_9_pass,
            "pass": p99_pass and p99_9_pass,
        }
    return out


def field_first(summary: dict[str, Any], name: str) -> Any:
    return summary.get("fields", {}).get(name, {}).get("first")


def active_culling_proof(summary: dict[str, Any], accepted: bool) -> dict[str, Any]:
    world = field_first(summary, "world_crystals")
    active = field_first(summary, "active_crystals")
    active_meets_full_threshold = isinstance(active, int) and active >= 1000
    bounded_culling_pass = (
        bool(accepted)
        and isinstance(world, int)
        and isinstance(active, int)
        and world >= 1000
        and 128 <= active <= world
    )
    return {
        "accepted": bool(accepted),
        "proof_kind": "deterministic_nearest_bounded_scene",
        "world_crystals": world,
        "active_crystals": active,
        "full_active_threshold": 1000,
        "bounded_cap_floor": 128,
        "active_meets_full_threshold": active_meets_full_threshold,
        "proof_pass": active_meets_full_threshold or bounded_culling_pass,
        "note": (
            "active scene is accepted as production-shaped only when the host proves "
            "1024+ world crystals are deterministically nearest-culled into the bounded V3 scene."
        ),
    }


def evidence_gaps(
    summary: dict[str, Any],
    *,
    image_present: bool,
    compare_present: bool,
    production_shape: bool,
    rich_scene: bool,
    mixed_sim: bool,
    input_telemetry: bool,
    accepted_utilization: bool,
    accepted_culling_proof: bool,
) -> list[str]:
    gaps: list[str] = []
    rows = int(summary.get("frame_rows") or 0)
    width = field_first(summary, "width")
    height = field_first(summary, "height")
    realworld_gate = field_first(summary, "realworld_gate")
    active = field_first(summary, "active_crystals")
    world = field_first(summary, "world_crystals")
    mixed_stats = summary.get("mixed_workload", {})
    input_stats = summary.get("input_latency", {})
    util_stats = summary.get("utilization", {})
    occupancy = summary.get("derived_occupancy", {})
    mixed_data_present = any(row.get("count", 0) > 0 for row in mixed_stats.values())
    real_input_present = any(row.get("count", 0) > 0 for row in input_stats.values())
    synthetic_input_present = any(
        mixed_stats.get(field, {}).get("count", 0) > 0
        for field in ("synthetic_input_sample_us", "synthetic_input_jitter_us")
    )
    process_util_present = (
        util_stats.get("process_cpu_pct", {}).get("count", 0) > 0
        and util_stats.get("working_set_mb", {}).get("count", 0) > 0
    )
    timestamp_gpu_present = occupancy.get("gpu_dispatch_frame_pct", {}).get("p50") is not None
    adapter_util_present = (
        util_stats.get("gpu_busy_pct", {}).get("count", 0) > 0
        or util_stats.get("vram_used_mb", {}).get("count", 0) > 0
    )
    if rows < 1000:
        gaps.append("frame_sample_lt_1000")
    if width != 2560 or height != 1440:
        gaps.append("not_2560x1440_target_resolution")
    if realworld_gate is not True:
        gaps.append("telemetry_realworld_gate_false")
    if not image_present:
        gaps.append("full_frame_artifact_absent")
    if not compare_present:
        gaps.append("state_diff_artifact_absent")
    if not rich_scene:
        gaps.append("rich_scene_workload_not_declared")
    if isinstance(world, int) and world < 1000:
        gaps.append("world_crystals_lt_1000")
    if not active_culling_proof(summary, accepted_culling_proof)["proof_pass"]:
        gaps.append("active_gpu_scene_lt_1000")
    if not (mixed_sim or mixed_data_present):
        gaps.append("mixed_gameplay_sim_absent")
    if not (input_telemetry or real_input_present):
        gaps.append("input_latency_jitter_absent")
    if synthetic_input_present and not real_input_present:
        gaps.append("synthetic_input_only")
    if not (accepted_utilization or (process_util_present and timestamp_gpu_present)):
        gaps.append("accepted_cpu_gpu_ram_vram_utilization_absent")
    if process_util_present and timestamp_gpu_present and not adapter_util_present:
        gaps.append("adapter_vram_power_clock_absent")
    if not production_shape:
        gaps.append("production_shape_flag_absent")
    return gaps


def verdict_from(report: dict[str, Any]) -> dict[str, Any]:
    budgets = report["budgets"]
    budget_pass = all(row["pass"] for row in budgets.values())
    gaps = report["evidence"]["missing"]
    production_pass = budget_pass and not gaps
    if not budget_pass:
        status = "fail_budget"
    elif gaps:
        status = "pass_benchmark_budget_fail_production_evidence"
    else:
        status = "pass_production_shape"
    return {
        "status": status,
        "budget_pass": budget_pass,
        "production_shape_pass": production_pass,
        "recommendation": recommendation_for(status, report),
    }


def recommendation_for(status: str, report: dict[str, Any]) -> list[str]:
    if status == "fail_budget":
        return [
            "stop feature expansion",
            "isolate p99 contributor by CPU segment and GPU dispatch",
            "reduce shader work or active set until p99/p99.9 pass before next visual slice",
        ]
    gaps = report.get("evidence", {}).get("missing", [])
    recs: list[str] = []
    if "mixed_gameplay_sim_absent" in gaps:
        recs.append("add mixed gameplay-sim plus input latency/jitter harness before more visual cost")
    elif "input_latency_jitter_absent" in gaps:
        recs.append("replace synthetic input probe with real raw-input latency/jitter telemetry")
    if "accepted_cpu_gpu_ram_vram_utilization_absent" in gaps:
        recs.append("add accepted adapter/process utilization path or proprietary timestamp/queue-depth substitute")
    if "active_gpu_scene_lt_1000" in gaps:
        recs.append("add scalable active-set tier or prove culling policy as production renderer invariant")
    if "full_frame_artifact_absent" in gaps or "state_diff_artifact_absent" in gaps:
        recs.append("capture full-frame artifact and state diff for every visual-composition change")
    if not recs:
        recs.append("proceed to next engine slice with this report pinned as baseline")
    return recs


def build_report(args: argparse.Namespace) -> dict[str, Any]:
    log = parse_telemetry_log(args.log)
    frames = log["frames"]
    summary = frame_summary(frames)
    image_report = analyze_ppm(args.image) if args.image else None
    compare_report = None
    if args.image and args.compare_image:
        compare_report = compare_ppm(args.image, args.compare_image, args.diff_image, args.diff_amplify)
    report: dict[str, Any] = {
        "schema": SCHEMA,
        "generated_utc": utc_now(),
        "inputs": {
            "log": str(args.log),
            "image": str(args.image) if args.image else None,
            "compare_image": str(args.compare_image) if args.compare_image else None,
            "compare_label": args.compare_label,
        },
        "classification": "benchmark",
        "telemetry": {
            "log_line_count": log["line_count"],
            "present_capture_rows": len(log["captures"]),
            "summary": summary,
            "outliers": frame_outliers(frames),
        },
        "budgets": budget_report(summary),
        "image": image_report,
        "image_compare": compare_report,
        "evidence": {
            "production_shape_requested": bool(args.production_shape),
            "rich_scene_declared": bool(args.rich_scene),
            "mixed_sim_declared": bool(args.mixed_sim),
            "input_telemetry_declared": bool(args.input_telemetry),
            "accepted_utilization_declared": bool(args.accepted_utilization),
            "accepted_culling_proof_declared": bool(args.accepted_culling_proof),
            "active_culling_proof": active_culling_proof(summary, bool(args.accepted_culling_proof)),
            "missing": evidence_gaps(
                summary,
                image_present=image_report is not None,
                compare_present=compare_report is not None,
                production_shape=bool(args.production_shape),
                rich_scene=bool(args.rich_scene),
                mixed_sim=bool(args.mixed_sim),
                input_telemetry=bool(args.input_telemetry),
                accepted_utilization=bool(args.accepted_utilization),
                accepted_culling_proof=bool(args.accepted_culling_proof),
            ),
        },
    }
    report["verdict"] = verdict_from(report)
    return report


def fmt_us(value: Any) -> str:
    if isinstance(value, float):
        return f"{value:.3f}us"
    if isinstance(value, int):
        return f"{value}us"
    return "n/a"


def fmt_pct(value: Any) -> str:
    if isinstance(value, (int, float)):
        return f"{float(value):.2f}%"
    return "n/a"


def report_to_csl(report: dict[str, Any]) -> str:
    summary = report["telemetry"]["summary"]
    wall = summary["wall_since_prev_us"]
    cpu = summary["cpu_total_us"]
    gpu = summary["gpu_dispatch_us"]
    occupancy = summary.get("derived_occupancy", {})
    fields = summary["fields"]
    image = report.get("image")
    compare = report.get("image_compare")
    proof = report.get("evidence", {}).get("active_culling_proof", {})
    proof_status = "pass" if proof.get("proof_pass") else "fail"
    lines = [
        "§ render.v3.bench.report",
        f"  schema: {report['schema']}",
        f"  status: {report['verdict']['status']}",
        f"  frames: {summary['frame_rows']}",
        f"  mode: {fields['present_mode']['first']} pace={fields['frame_pace']['first']} cpu_substrate_tick={fields['cpu_substrate_tick']['first']}",
        f"  scene: room={fields['stress_room']['first']} world={fields['world_crystals']['first']} active={fields['active_crystals']['first']} yaw={fields['observer_yaw_milli']['first']}",
        f"  active_culling_proof: {proof_status} accepted={proof.get('accepted')} kind={proof.get('proof_kind')} threshold={proof.get('full_active_threshold')} bounded_floor={proof.get('bounded_cap_floor')}",
        "  § timing.us",
        f"    wall: p50={fmt_us(wall.get('p50'))} p95={fmt_us(wall.get('p95'))} p99={fmt_us(wall.get('p99'))} p99.9={fmt_us(wall.get('p99_9'))} max={fmt_us(wall.get('max'))}",
        f"    cpu:  p50={fmt_us(cpu.get('p50'))} p95={fmt_us(cpu.get('p95'))} p99={fmt_us(cpu.get('p99'))} p99.9={fmt_us(cpu.get('p99_9'))} max={fmt_us(cpu.get('max'))}",
        f"    gpu:  p50={fmt_us(gpu.get('p50'))} p95={fmt_us(gpu.get('p95'))} p99={fmt_us(gpu.get('p99'))} p99.9={fmt_us(gpu.get('p99_9'))} max={fmt_us(gpu.get('max'))}",
        "  § occupancy.derived",
        f"    gpu/frame: p50={fmt_pct(occupancy.get('gpu_dispatch_frame_pct', {}).get('p50'))} p95={fmt_pct(occupancy.get('gpu_dispatch_frame_pct', {}).get('p95'))} p99={fmt_pct(occupancy.get('gpu_dispatch_frame_pct', {}).get('p99'))}",
        f"    cpu/frame: p50={fmt_pct(occupancy.get('cpu_frame_pct', {}).get('p50'))} p95={fmt_pct(occupancy.get('cpu_frame_pct', {}).get('p95'))} p99={fmt_pct(occupancy.get('cpu_frame_pct', {}).get('p99'))}",
        "  § budgets",
    ]
    for name, row in report["budgets"].items():
        flag = "pass" if row["pass"] else "fail"
        lines.append(
            f"    {name}: {flag} budget={row['budget_us']}us p99={fmt_us(row['wall_p99_us'])} p99.9={fmt_us(row['wall_p99_9_us'])}"
        )
    outliers = report.get("telemetry", {}).get("outliers", {})
    top_wall = outliers.get("top_wall_frames", [])
    if top_wall:
        lines.append("  § outliers.wall")
        lines.append(
            f"    frames_over_240hz_budget={outliers.get('frames_over_240hz_budget')} frames_over_p99.9={outliers.get('frames_over_p99_9')}"
        )
        for row in top_wall[:3]:
            lines.append(
                "    "
                f"row={row.get('row')} frame={row.get('frame_count_before')} "
                f"wall={fmt_us(row.get('wall_since_prev_us'))} "
                f"cpu={fmt_us(row.get('cpu_total_us'))} "
                f"gpu={fmt_us(row.get('previous_gpu_dispatch_us'))} "
                f"present={fmt_us(row.get('cpu_present_us'))}"
            )
    mixed = summary.get("mixed_workload", {})
    if any(row.get("count", 0) > 0 for row in mixed.values()):
        lines.extend(
            [
                "  § mixed.synthetic",
                f"    sim: p50={fmt_us(mixed['mixed_sim_tick_us'].get('p50'))} p95={fmt_us(mixed['mixed_sim_tick_us'].get('p95'))} p99={fmt_us(mixed['mixed_sim_tick_us'].get('p99'))}",
                f"    input_sample: p50={fmt_us(mixed['synthetic_input_sample_us'].get('p50'))} p95={fmt_us(mixed['synthetic_input_sample_us'].get('p95'))} p99={fmt_us(mixed['synthetic_input_sample_us'].get('p99'))}",
                f"    synthetic_jitter: p50={fmt_us(mixed['synthetic_input_jitter_us'].get('p50'))} p95={fmt_us(mixed['synthetic_input_jitter_us'].get('p95'))} p99={fmt_us(mixed['synthetic_input_jitter_us'].get('p99'))}",
            ]
        )
    util = summary.get("utilization", {})
    if any(row.get("count", 0) > 0 for row in util.values()):
        lines.extend(
            [
                "  § utilization.process",
                f"    cpu_pct: p50={fmt_pct(util['process_cpu_pct'].get('p50'))} p95={fmt_pct(util['process_cpu_pct'].get('p95'))} p99={fmt_pct(util['process_cpu_pct'].get('p99'))}",
                f"    working_set_mb: p50={util['working_set_mb'].get('p50', 'n/a')} p95={util['working_set_mb'].get('p95', 'n/a')} p99={util['working_set_mb'].get('p99', 'n/a')}",
                f"    private_bytes_mb: p50={util['private_bytes_mb'].get('p50', 'n/a')} p95={util['private_bytes_mb'].get('p95', 'n/a')} p99={util['private_bytes_mb'].get('p99', 'n/a')}",
            ]
        )
    if image:
        mean = image["mean_rgb"]
        rough_x = image["roughness_dx_rgb"]
        rough_y = image["roughness_dy_rgb"]
        lines.extend(
            [
                "  § artifact",
                f"    image: {image['width']}x{image['height']} checksum={image['checksum_fnv1a64']}",
                f"    mean.rgb: {mean[0]:.3f},{mean[1]:.3f},{mean[2]:.3f}",
                f"    bins.rgb: {image['unique_bins']['r']},{image['unique_bins']['g']},{image['unique_bins']['b']}",
                f"    rough.dx8: {rough_x[0]:.3f},{rough_x[1]:.3f},{rough_x[2]:.3f}",
                f"    rough.dy8: {rough_y[0]:.3f},{rough_y[1]:.3f},{rough_y[2]:.3f}",
            ]
        )
    if compare:
        mad = compare["mean_abs_delta_rgb"]
        lines.extend(
            [
                "  § diff",
                f"    changed: {compare['changed_pixels']} ppm={compare['changed_ppm']:.3f}",
                f"    mean_abs_delta.rgb: {mad[0]:.3f},{mad[1]:.3f},{mad[2]:.3f}",
                f"    max_delta.rgb: {','.join(str(x) for x in compare['max_delta_rgb'])}",
            ]
        )
    lines.append("  § evidence.gaps")
    missing = report["evidence"]["missing"]
    if missing:
        lines.extend(f"    fail: {gap}" for gap in missing)
    else:
        lines.append("    pass: none")
    lines.append("  § next")
    lines.extend(f"    R! {rec}" for rec in report["verdict"]["recommendation"])
    lines.append("∎")
    return "\n".join(lines)


def write_report(path: Path, report: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2, sort_keys=True), encoding="utf-8")


def analyze_cmd(args: argparse.Namespace) -> int:
    report = build_report(args)
    if args.out_json:
        write_report(args.out_json, report)
    if args.print_json:
        print(json.dumps(report, indent=2, sort_keys=True))
    if args.print_csl:
        print(report_to_csl(report))
    if args.fail_on_budget and not report["verdict"]["budget_pass"]:
        return 2
    if args.require_production_shape and not report["verdict"]["production_shape_pass"]:
        return 3
    return 0


def write_ppm(path: Path, width: int, height: int, pixels: bytes) -> None:
    path.write_bytes(f"P6\n{width} {height}\n255\n".encode("ascii") + pixels)


def synthetic_log(path: Path, frames: int = 1200) -> None:
    lines = [
        "§ LoA-v13 starting · pure-CSSL native · log => logs/loa_runtime.log · pid=1",
        "§ LoA-host starting · winit + wgpu render",
    ]
    for i in range(frames + 1):
        wall = "unavailable" if i == 0 else str(1800 + (i % 11) * 9)
        gpu = "unavailable" if i < 3 else str(1500 + (i % 7) * 5)
        lines.append(
            "telemetry.present_frame "
            "use_case=present_loop_probe realworld_gate=false p99_us=unmeasured p99_9_us=unmeasured "
            f"frame_slot={i % 3} frame_count_before={i} image_index={i % 3} width=2560 height=1440 "
            "present_mode=IMMEDIATE cpu_wait_us=900 cpu_acquire_us=1 cpu_upload_us=5 "
            "cpu_record_us=20 cpu_submit_us=12 cpu_present_us=4 gpu_timestamps_available=true "
            f"previous_gpu_dispatch_us={gpu} wall_since_prev_us={wall} frame_pace=Poll(free-running) "
            "cpu_substrate_tick=false observer_yaw_milli=0 world_crystals=1024 active_crystals=128"
        )
    path.write_text("\n".join(lines), encoding="utf-8")


def synthetic_ppm_pair(a: Path, b: Path) -> None:
    width = 16
    height = 16
    pa = bytearray()
    pb = bytearray()
    for y in range(height):
        for x in range(width):
            pa.extend((x * 8, y * 8, (x + y) * 4))
            pb.extend((min(255, x * 8 + 5), y * 8, min(255, (x + y) * 4 + (20 if x > 7 else 0))))
    write_ppm(a, width, height, bytes(pa))
    write_ppm(b, width, height, bytes(pb))


def whitespace_pixel_ppm(path: Path) -> None:
    pixels = bytes((10, 20, 30, 40, 50, 60))
    write_ppm(path, 2, 1, pixels)


def self_test_cmd(_: argparse.Namespace) -> int:
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        log = root / "synthetic.log"
        image_a = root / "a.ppm"
        image_b = root / "b.ppm"
        diff = root / "diff.ppm"
        synthetic_log(log)
        synthetic_ppm_pair(image_a, image_b)
        whitespace = root / "whitespace.ppm"
        whitespace_pixel_ppm(whitespace)
        assert read_ppm(whitespace).data[0] == 10
        ns = argparse.Namespace(
            log=log,
            image=image_a,
            compare_image=image_b,
            compare_label="synthetic",
            diff_image=diff,
            diff_amplify=4,
            out_json=None,
            print_json=False,
            print_csl=False,
            production_shape=False,
            rich_scene=False,
            mixed_sim=False,
            input_telemetry=False,
            accepted_utilization=False,
            accepted_culling_proof=False,
        )
        report = build_report(ns)
        assert report["telemetry"]["summary"]["frame_rows"] == 1201
        assert report["budgets"]["144hz"]["pass"]
        assert report["budgets"]["240hz"]["pass"]
        assert report["image"]["width"] == 16
        assert report["image_compare"]["changed_pixels"] > 0
        assert "active_gpu_scene_lt_1000" in report["evidence"]["missing"]
        ns_proof = argparse.Namespace(**{**vars(ns), "accepted_culling_proof": True})
        proof_report = build_report(ns_proof)
        assert proof_report["evidence"]["active_culling_proof"]["proof_pass"]
        assert "active_gpu_scene_lt_1000" not in proof_report["evidence"]["missing"]
        assert diff.exists()
        bmp = root / "a.bmp"
        bmp_report = write_bmp_from_ppm(image_a, bmp)
        assert bmp.exists()
        assert bmp_report["width"] == 16
        assert report["verdict"]["status"] == "pass_benchmark_budget_fail_production_evidence"
    print(json.dumps({"schema": SCHEMA, "self_test": "pass"}, sort_keys=True))
    return 0


def run_cmd(args: argparse.Namespace) -> int:
    exe = args.exe or (ROOT / "compiler-rs" / "target" / "debug" / "loa-runtime.exe")
    if not exe.exists():
        raise FileNotFoundError(exe)
    log = args.log or (ROOT / "compiler-rs" / "target" / f"v3_present_run_{int(time.time())}.log")
    env = os.environ.copy()
    env.update(
        {
            "LOA_RENDER_V3": "1",
            "LOA_FRAME_PACE": "poll",
            "LOA_V3_PRESENT_TELEMETRY": "1",
            "LOA_QUICK_QUIT": str(args.frames),
        }
    )
    if args.mixed_bench:
        env["LOA_V3_MIXED_BENCH"] = "1"
    if args.stress_room:
        env["LOA_V3_STRESS_ROOM"] = args.stress_room
    if args.window_mode:
        env["CSSL_LOA_WINDOW"] = args.window_mode
    if args.yaw_milli is not None:
        env["LOA_V3_OBSERVER_YAW_MILLI"] = str(args.yaw_milli)
    if args.capture_frames:
        env["LOA_V3_PRESENT_CAPTURE"] = str(args.capture_frames)
    if args.capture_path:
        env["LOA_V3_PRESENT_CAPTURE_PATH"] = str(args.capture_path)
    log.parent.mkdir(parents=True, exist_ok=True)
    started = time.perf_counter()
    stop_input = threading.Event()
    injector: threading.Thread | None = None
    proc = subprocess.Popen(
        [str(exe)],
        cwd=exe.parent,
        env=env,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if args.send_input:
        injector = threading.Thread(
            target=post_window_input_loop,
            args=(proc.pid, stop_input, args.input_interval_ms),
            daemon=True,
        )
        injector.start()
    stdout, stderr = proc.communicate()
    stop_input.set()
    if injector is not None:
        injector.join(timeout=1.0)
    elapsed_ms = int((time.perf_counter() - started) * 1000)
    log.write_text((stdout or "") + (stderr or ""), encoding="utf-8", errors="replace")
    print(json.dumps({"exit_code": proc.returncode, "elapsed_ms": elapsed_ms, "log": str(log)}, sort_keys=True))
    if proc.returncode != 0:
        return proc.returncode
    if args.analyze:
        analyze_args = argparse.Namespace(
            log=log,
            image=args.capture_path,
            compare_image=None,
            compare_label=None,
            diff_image=None,
            diff_amplify=4,
            out_json=args.out_json,
            print_json=args.print_json,
            print_csl=args.print_csl,
            fail_on_budget=args.fail_on_budget,
            require_production_shape=args.require_production_shape,
            production_shape=args.production_shape,
            rich_scene=args.rich_scene,
            mixed_sim=args.mixed_sim,
            input_telemetry=args.input_telemetry,
            accepted_utilization=args.accepted_utilization,
            accepted_culling_proof=args.accepted_culling_proof,
        )
        return analyze_cmd(analyze_args)
    return 0


def convert_image_cmd(args: argparse.Namespace) -> int:
    result = write_bmp_from_ppm(args.image, args.out_bmp)
    print(json.dumps(result, sort_keys=True))
    return 0


def post_window_input_loop(pid: int, stop: threading.Event, interval_ms: int) -> None:
    if sys.platform != "win32":
        return
    hwnd = wait_for_window(pid, timeout_s=5.0)
    if not hwnd:
        return
    user32 = ctypes.windll.user32
    wm_keydown = 0x0100
    wm_keyup = 0x0101
    vk_w = 0x57
    keyeventf_keyup = 0x0002
    keydown_lparam = 0x0011_0001
    keyup_lparam = 0xC011_0001
    interval = max(5, interval_ms) / 1000.0
    while not stop.is_set():
        user32.SetForegroundWindow(hwnd)
        user32.PostMessageW(hwnd, wm_keydown, vk_w, keydown_lparam)
        user32.keybd_event(vk_w, 0, 0, 0)
        time.sleep(0.002)
        user32.keybd_event(vk_w, 0, keyeventf_keyup, 0)
        user32.PostMessageW(hwnd, wm_keyup, vk_w, keyup_lparam)
        time.sleep(interval)


def wait_for_window(pid: int, timeout_s: float) -> int | None:
    deadline = time.perf_counter() + timeout_s
    while time.perf_counter() < deadline:
        hwnd = find_window_for_pid(pid)
        if hwnd:
            return hwnd
        time.sleep(0.025)
    return None


def find_window_for_pid(pid: int) -> int | None:
    user32 = ctypes.windll.user32
    found: list[int] = []

    enum_proc = ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)

    @enum_proc
    def callback(hwnd: int, _: int) -> bool:
        if not user32.IsWindowVisible(hwnd):
            return True
        window_pid = ctypes.c_ulong()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(window_pid))
        if int(window_pid.value) == pid:
            found.append(int(hwnd))
            return False
        return True

    user32.EnumWindows(callback, 0)
    return found[0] if found else None


def add_evidence_flags(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--production-shape", action="store_true", help="declare run as production-shaped evidence")
    parser.add_argument("--rich-scene", action="store_true", help="declare workload includes target rich-scene semantics")
    parser.add_argument("--mixed-sim", action="store_true", help="declare gameplay/input/sim load is present")
    parser.add_argument("--input-telemetry", action="store_true", help="declare input latency/jitter telemetry is present")
    parser.add_argument("--accepted-utilization", action="store_true", help="declare accepted CPU/GPU/RAM/VRAM utilization evidence is present")
    parser.add_argument(
        "--accepted-culling-proof",
        action="store_true",
        help="declare deterministic 1024-world-to-bounded-active-scene culling proof is present",
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="LoA Render V3 present benchmark analyzer")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_analyze = sub.add_parser("analyze", help="analyze existing telemetry log and optional PPM artifacts")
    p_analyze.add_argument("--log", type=Path, required=True)
    p_analyze.add_argument("--image", type=Path)
    p_analyze.add_argument("--compare-image", type=Path)
    p_analyze.add_argument("--compare-label")
    p_analyze.add_argument("--diff-image", type=Path)
    p_analyze.add_argument("--diff-amplify", type=int, default=4)
    p_analyze.add_argument("--out-json", type=Path)
    p_analyze.add_argument("--print-json", action="store_true")
    p_analyze.add_argument("--print-csl", action="store_true")
    p_analyze.add_argument("--fail-on-budget", action="store_true")
    p_analyze.add_argument("--require-production-shape", action="store_true")
    add_evidence_flags(p_analyze)
    p_analyze.set_defaults(func=analyze_cmd)

    p_run = sub.add_parser("run", help="run loa-runtime with V3 telemetry, then optionally analyze")
    p_run.add_argument("--exe", type=Path)
    p_run.add_argument("--frames", type=int, default=1000)
    p_run.add_argument("--yaw-milli", type=int)
    p_run.add_argument(
        "--stress-room",
        choices=["shells", "printer", "material", "hair", "portal", "arena"],
        help="select LOA_V3_STRESS_ROOM for operator stress-lab runs",
    )
    p_run.add_argument("--mixed-bench", action="store_true")
    p_run.add_argument("--window-mode", choices=["windowed", "borderless", "exclusive"])
    p_run.add_argument("--send-input", action="store_true", help="post OS window key events during the run")
    p_run.add_argument("--input-interval-ms", type=int, default=16)
    p_run.add_argument("--capture-frames", type=int, default=0)
    p_run.add_argument("--capture-path", type=Path)
    p_run.add_argument("--log", type=Path)
    p_run.add_argument("--analyze", action="store_true")
    p_run.add_argument("--out-json", type=Path)
    p_run.add_argument("--print-json", action="store_true")
    p_run.add_argument("--print-csl", action="store_true")
    p_run.add_argument("--fail-on-budget", action="store_true")
    p_run.add_argument("--require-production-shape", action="store_true")
    add_evidence_flags(p_run)
    p_run.set_defaults(func=run_cmd)

    p_self = sub.add_parser("self-test", help="validate parser, image analysis, diff, and gates")
    p_self.set_defaults(func=self_test_cmd)

    p_convert = sub.add_parser("convert-image", help="convert binary PPM capture artifact to BMP preview")
    p_convert.add_argument("--image", type=Path, required=True)
    p_convert.add_argument("--out-bmp", type=Path, required=True)
    p_convert.set_defaults(func=convert_image_cmd)

    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
