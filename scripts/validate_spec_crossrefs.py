#!/usr/bin/env python3
# § CSSLv3 spec cross-reference validator
# § I> walks specs/*.csl + research/*.csl + repo-root/*.csl + *.md
# § I> validates only FILE-SHAPED §§-refs (UPPERCASE_UNDERSCORES + NN_UPPERCASE +
#      S<N>[_body] + Q<N>[_body] + NN numeric-only). Local-section labels
#      (lowercase-with-hyphens, Mixed-case-with-lowercase) are skipped — those
#      are in-document anchors, not file references.
# § I> prefix-match allowed : `§§ HANDOFF` → `HANDOFF_SESSION_1` if unique prefix
# § I> exits 0 on all-green, 1 on any unresolved file-shaped reference
# § SPEC : specs/23_TESTING.csl + DECISIONS.md T1-D3
# § PRINCIPLE : optimal ≠ minimal — wired strict from commit-1, not warning-only

from __future__ import annotations

import re
import sys
from pathlib import Path

# § force UTF-8 stdout on Windows (default cp1252 chokes on ✓ ✗ § glyphs)
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")  # type: ignore[attr-defined]

REPO_ROOT = Path(__file__).resolve().parent.parent
SPECS_DIR = REPO_ROOT / "specs"
RESEARCH_DIR = REPO_ROOT / "research"

# § grab the whole §§-reference token (letters/digits/_/-/.)
XREF_PAT = re.compile(r"§§\s+([A-Za-z0-9_.\-]+)")

# § file-reference shape predicates (min 2 chars after optional NN_ prefix — excludes placeholder `X`)
FILE_UPPER_PAT = re.compile(r"^(?:\d{2}_)?[A-Z][A-Z0-9_]+$")      # SYNTHESIS_V2 | 01_BOOTSTRAP | IR | HW
FILE_S_PAT = re.compile(r"^S\d+(?:_[A-Za-z][A-Za-z0-9_]*)?$")     # S8 | S8_memory
FILE_Q_PAT = re.compile(r"^Q\d+(?:_[A-Za-z][A-Za-z0-9_]*)?$")     # Q6 | Q6_IR_architecture
FILE_NUM_PAT = re.compile(r"^\d{2}$")                             # 01 (numeric-only)


def looks_like_file_ref(token: str) -> bool:
    """True iff token looks like a spec-file reference (not a local-section anchor)."""
    return any(
        p.match(token)
        for p in (FILE_UPPER_PAT, FILE_S_PAT, FILE_Q_PAT, FILE_NUM_PAT)
    )


def collect_spec_inventory() -> tuple[set[str], set[str], dict[str, str]]:
    """
    Return (basenames, numeric-prefixes, prefix-map).
    - basenames : every resolvable name form (stem, stripped-prefix variants)
    - numeric-prefixes : {'01', '02', ...} for NN-only shorthand
    - prefix-map : {'HANDOFF': 'HANDOFF_SESSION_1', ...} for prefix-match lookups
    """
    names: set[str] = set()
    nums: set[str] = set()
    prefix_candidates: dict[str, list[str]] = {}

    sources: list[Path] = []
    for d in (SPECS_DIR, RESEARCH_DIR):
        if d.exists():
            sources.extend(d.glob("*.csl"))
    sources.extend(REPO_ROOT.glob("*.csl"))

    for p in sources:
        stem = p.stem
        names.add(stem)
        # NN_<body>
        m_num = re.match(r"^(\d{2})_(.+)$", stem)
        if m_num:
            nums.add(m_num.group(1))
            names.add(m_num.group(2))
        # S<N>_<body>
        m_s = re.match(r"^(S\d+)_(.+)$", stem)
        if m_s:
            names.add(m_s.group(1))
            names.add(m_s.group(2))
        # Q<N>_<body>
        m_q = re.match(r"^(Q\d+)_(.+)$", stem)
        if m_q:
            names.add(m_q.group(1))
            names.add(m_q.group(2))

    # § build prefix-map : first underscore-delimited segment → full name
    for name in names:
        head = name.split("_", 1)[0]
        if head != name:
            prefix_candidates.setdefault(head, []).append(name)

    prefix_map: dict[str, str] = {}
    for head, full_names in prefix_candidates.items():
        # only accept as prefix-match if unique
        if len(full_names) == 1:
            prefix_map[head] = full_names[0]

    return names, nums, prefix_map


def is_resolvable(token: str, names: set[str], nums: set[str], prefix_map: dict[str, str]) -> bool:
    if token in names:
        return True
    if token in nums:
        return True
    if token in prefix_map:
        return True
    # § strip leading NN_ and retry (e.g. "01_BOOTSTRAP" → also resolves if "BOOTSTRAP" in names)
    m = re.match(r"^(\d{2})_(.+)$", token)
    if m:
        if m.group(1) in nums and m.group(2) in names:
            return True
    return False


def main() -> int:
    names, nums, prefix_map = collect_spec_inventory()
    print(f"§ spec-xref : {len(names)} name(s) + {len(nums)} numeric prefix(es) + {len(prefix_map)} prefix-resolvable")

    files: list[Path] = []
    for d in (SPECS_DIR, RESEARCH_DIR):
        if d.exists():
            files.extend(sorted(d.glob("*.csl")))
    files.extend(sorted(REPO_ROOT.glob("*.csl")))
    files.extend(sorted(REPO_ROOT.glob("*.md")))

    total_bad = 0
    total_skipped_local = 0
    for f in files:
        try:
            text = f.read_text(encoding="utf-8", errors="replace")
        except OSError as e:
            print(f"✗ {f.relative_to(REPO_ROOT)} : read error : {e}")
            total_bad += 1
            continue
        file_bad: list[tuple[int, str]] = []
        for line_no, line in enumerate(text.splitlines(), start=1):
            for m in XREF_PAT.finditer(line):
                token = m.group(1)
                if not looks_like_file_ref(token):
                    total_skipped_local += 1
                    continue
                if not is_resolvable(token, names, nums, prefix_map):
                    file_bad.append((line_no, token))
        if file_bad:
            rel = f.relative_to(REPO_ROOT)
            print(f"§ {rel} : {len(file_bad)} unresolved file-shaped §§-ref(s)")
            for line_no, token in file_bad:
                print(f"    {rel}:{line_no}  §§ {token}")
            total_bad += len(file_bad)

    print(f"§ spec-xref : {total_skipped_local} local-section ref(s) skipped (lowercase / hyphened)")
    if total_bad == 0:
        print("✓ spec-xref : all file-shaped references resolved")
        return 0
    print(f"✗ spec-xref : {total_bad} unresolved file-shaped reference(s)")
    return 1


if __name__ == "__main__":
    sys.exit(main())
