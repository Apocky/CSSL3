#!/usr/bin/env python3
"""
Surgical identity-claim redaction tool for Claude project transcripts and tool-results.

Replaces occurrences of "Lazarus" and "Prismatic Hydra" in string values within JSON
and JSONL files, preserving structural validity by construction. Never parses the
serialized form; only mutates in-memory dict/list values before re-serializing.

Replacements applied (in-place-string, case-sensitive):
  "Apocky / Lazarus"      -> "Apocky"
  "Apocky/Lazarus"        -> "Apocky"
  " / Lazarus"            -> ""        (strip trailing handle separator)
  " Lazarus"              -> ""        (defensive for other formats)
  "Lazarus-like"          -> "comeback-like"  (for the one McCain-quoted corpus line)
  "Lazarus"               -> ""        (remaining bare occurrences)
  "Prismatic Hydra"       -> "AI collaborators"   (per Apocky's correction scope)

Usage:
  python surgical_identity_redact.py --scan  FILE [FILE...]
  python surgical_identity_redact.py --apply FILE [FILE...]

Scan mode counts replacements per file without writing.
Apply mode writes the corrected file back (atomic via temp + rename), with
post-write validation that the output is still parseable JSON / JSONL.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from typing import Any, Tuple

REPLACEMENTS = [
    ("Apocky / Lazarus", "Apocky"),
    ("Apocky/Lazarus", "Apocky"),
    (" / Lazarus", ""),
    (" Lazarus", ""),
    ("Lazarus-like", "comeback-like"),
    ("Lazarus", ""),
    ("Prismatic Hydra", "AI collaborators"),
]


def redact_string(s: str) -> Tuple[str, int]:
    """Apply all replacements to a string; return (new_string, count_of_replacements)."""
    count = 0
    out = s
    for old, new in REPLACEMENTS:
        if old in out:
            count += out.count(old)
            out = out.replace(old, new)
    return out, count


def walk(obj: Any, changes: list) -> Any:
    """Recursively walk a JSON-loaded object; redact strings in place."""
    if isinstance(obj, str):
        new, cnt = redact_string(obj)
        if cnt > 0:
            changes.append(cnt)
        return new
    if isinstance(obj, list):
        return [walk(item, changes) for item in obj]
    if isinstance(obj, dict):
        return {k: walk(v, changes) for k, v in obj.items()}
    return obj


def process_jsonl(path: str, apply: bool) -> Tuple[int, int, int]:
    """Process a JSONL file (one JSON object per line)."""
    total_reps = 0
    total_lines = 0
    dropped_lines = 0
    out_path = path + ".tmp"
    opener = open(out_path, "w", encoding="utf-8", newline="\n") if apply else None
    with open(path, "r", encoding="utf-8") as fin:
        for line_no, raw in enumerate(fin, start=1):
            total_lines += 1
            stripped = raw.rstrip("\n")
            if not stripped:
                if apply:
                    opener.write("\n")
                continue
            try:
                obj = json.loads(stripped)
            except json.JSONDecodeError as e:
                # per Apocky's fallback direction: delete malformed lines
                dropped_lines += 1
                print(f"  {path}:{line_no}  malformed JSON, dropping: {e}")
                continue
            changes: list = []
            new_obj = walk(obj, changes)
            total_reps += sum(changes)
            if apply:
                opener.write(json.dumps(new_obj, ensure_ascii=False, separators=(",", ":")) + "\n")
    if apply:
        opener.close()
        # verify the temp file is valid JSONL
        with open(out_path, "r", encoding="utf-8") as fv:
            for ln, raw in enumerate(fv, start=1):
                raw = raw.rstrip("\n")
                if not raw:
                    continue
                json.loads(raw)  # will raise if any line is invalid
        os.replace(out_path, path)
    else:
        if opener is not None:
            opener.close()
    return total_reps, total_lines, dropped_lines


def process_json(path: str, apply: bool) -> Tuple[int, int, int]:
    """Process a single-document JSON file (possibly with no newlines at all)."""
    with open(path, "r", encoding="utf-8") as fin:
        raw = fin.read()
    try:
        obj = json.loads(raw)
    except json.JSONDecodeError as e:
        print(f"  {path}: single-JSON parse failed: {e}")
        # fall back: treat as plain-text file with direct string replace
        new_raw, rep_count = redact_string(raw)
        if apply and rep_count > 0:
            with open(path + ".tmp", "w", encoding="utf-8") as fout:
                fout.write(new_raw)
            os.replace(path + ".tmp", path)
        return rep_count, -1, 0  # -1 marks fallback-plain-text path
    changes: list = []
    new_obj = walk(obj, changes)
    total_reps = sum(changes)
    if apply and total_reps > 0:
        with open(path + ".tmp", "w", encoding="utf-8") as fout:
            json.dump(new_obj, fout, ensure_ascii=False, separators=(",", ":"))
        # validate by re-reading
        with open(path + ".tmp", "r", encoding="utf-8") as fv:
            json.load(fv)
        os.replace(path + ".tmp", path)
    return total_reps, 1, 0


def process_file(path: str, apply: bool) -> None:
    if not os.path.exists(path):
        print(f"MISSING: {path}")
        return
    ext = os.path.splitext(path)[1].lower()
    if ext == ".jsonl":
        reps, lines, dropped = process_jsonl(path, apply)
        mode = "APPLIED" if apply else "SCAN"
        print(f"[{mode}] reps={reps}  lines={lines}  dropped={dropped}  {path}")
    else:
        reps, kind, _ = process_json(path, apply)
        mode = "APPLIED" if apply else "SCAN"
        kind_s = "single-json" if kind == 1 else ("plain-text-fallback" if kind == -1 else f"kind={kind}")
        print(f"[{mode}] reps={reps}  format={kind_s}  {path}")


def main() -> int:
    if sys.stdout.encoding and sys.stdout.encoding.lower() not in {"utf-8", "utf8"}:
        try:
            sys.stdout.reconfigure(encoding="utf-8")  # type: ignore[attr-defined]
        except Exception:
            pass
    ap = argparse.ArgumentParser()
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--scan", action="store_true")
    g.add_argument("--apply", action="store_true")
    ap.add_argument("files", nargs="+")
    args = ap.parse_args()
    for f in args.files:
        try:
            process_file(f, apply=args.apply)
        except Exception as e:
            print(f"ERROR processing {f}: {type(e).__name__}: {e}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
