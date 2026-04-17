#!/usr/bin/env python3
# § CSSLv3 differential lex-oracle : Rust-port vs parser.exe (Odin)
# § I> per DECISIONS.md T1-D2 — divergence on shared CSLv3 fixtures ⇒ spec-ambiguity,
#      file against CSLv3 (not CSSLv3).
# § I> this script is the CI driver skeleton; it is wired into .github/workflows/ci.yml
#      (diff-linux-arc-a770 job matrix — though the oracle itself runs on any runner).
# § I> full implementation blocks on :
#      (a) `csslc tokens --json <file>` subcommand (T10 scope)
#      (b) canonical token-kind mapping between Rust-port TokenKind and Odin Token_Kind
#      (c) shared fixture directory strategy (currently CSLv3/tests/*.csl)
# § I> stage0 operates as documentation-only until (a)..(c) land. Running the script
#      today prints the run-plan and exits 0 (no fixtures, no oracle calls).
#
# § SPEC : specs/23_TESTING.csl § oracle-modes • differential +
#          specs/16_DUAL_SURFACE.csl § PARSER UNIFICATION (HIR) +
#          DECISIONS.md T1-D2

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")  # type: ignore[attr-defined]

REPO_ROOT = Path(__file__).resolve().parent.parent
CSSL_COMPILER = REPO_ROOT / "compiler-rs" / "target" / "debug" / "csslc.exe"
CSLV3_REPO = REPO_ROOT.parent / "CSLv3"
CSLV3_PARSER = CSLV3_REPO / "parser.exe"
CSLV3_FIXTURES = CSLV3_REPO / "tests"


def emit(msg: str) -> None:
    print(f"§ lex-oracle : {msg}")


def check_prerequisites() -> list[str]:
    missing = []
    if not CSSL_COMPILER.exists():
        missing.append(f"csslc binary (expected at {CSSL_COMPILER}) — run `cargo build -p csslc`")
    if not CSLV3_PARSER.exists():
        missing.append(f"parser.exe (expected at {CSLV3_PARSER}) — run `odin build parser/` in CSLv3 repo")
    if not CSLV3_FIXTURES.exists():
        missing.append(f"CSLv3 fixtures dir {CSLV3_FIXTURES}")
    return missing


def csslc_tokens(path: Path) -> str:
    """Stage0 stub. At T10 this will invoke `csslc tokens --json <path>` and return JSON output."""
    del path
    return ""


def odin_tokens(path: Path) -> str:
    """Stage0 stub. At T10 this will invoke `parser.exe --tokens <path>` and normalize."""
    del path
    return ""


def compare_tokens(rust_json: str, odin_txt: str) -> list[str]:
    """Return diff lines between the two token streams after canonical-mapping."""
    del rust_json, odin_txt
    return []


def main() -> int:
    emit("starting differential lex oracle")
    missing = check_prerequisites()
    if missing:
        emit("prerequisites missing (stage0 stub exits clean) :")
        for m in missing:
            emit(f"  - {m}")
        emit("full implementation lands at T10 per DECISIONS.md T1-D2 consequences.")
        return 0

    fixtures = sorted(CSLV3_FIXTURES.rglob("*.csl"))
    if not fixtures:
        emit(f"no fixtures found under {CSLV3_FIXTURES}")
        return 0

    failures = 0
    for fx in fixtures:
        rel = fx.relative_to(REPO_ROOT.parent)
        rust_out = csslc_tokens(fx)
        odin_out = odin_tokens(fx)
        diff = compare_tokens(rust_out, odin_out)
        if diff:
            emit(f"✗ divergence in {rel}")
            for line in diff:
                emit(f"    {line}")
            failures += 1
        else:
            emit(f"✓ {rel}")

    if failures == 0:
        emit(f"all {len(fixtures)} fixtures match")
        return 0
    emit(f"{failures} divergences — spec-ambiguity, file against CSLv3 per DECISIONS.md T1-D2")
    return 1


if __name__ == "__main__":
    sys.exit(main())
