#!/usr/bin/env python3
"""
mneme_tools.py · MNEME persistent-memory bridge for the Apocky-Grok-Harness.

Wires three @mcp.tool entries that proxy to the live MNEME deployment at
    https://www.apocky.com/api/mneme/{profile}/*

Routes used :
  GET  /smoke      · health probe (no body)
  POST /recall     · {query, k} → {ok,result_nl,result_csl,citations,confidence,ts}
  POST /remember   · {csl, paraphrase?, topic_key?, type="instruction"}
                     · csl MUST be a valid CSL morpheme path
                       (period-separated tokens, no spaces / special chars)
                       e.g. `build.cssl.toolchain.msvc-fix`

Avoided (currently 502 in prod) :
  POST /ingest    · LLM-extraction pipeline
  GET  /list      · Supabase route

Author : Claude (Opus 4.7) · 2026-05-04 · for Apocky
"""

from __future__ import annotations

import logging
import os
from typing import Any

try:
    import httpx
except ImportError:  # pragma: no cover
    httpx = None  # type: ignore

from fastmcp import FastMCP

logger = logging.getLogger("apocky-mneme-tools")

# Override via env if you ever need to point at a staging deployment.
MNEME_BASE = os.environ.get("MNEME_BASE", "https://www.apocky.com/api/mneme")
MNEME_TIMEOUT_S = float(os.environ.get("MNEME_TIMEOUT_S", "10.0"))


def _profile_url(profile: str, path: str) -> str:
    return f"{MNEME_BASE.rstrip('/')}/{profile}/{path.lstrip('/')}"


def _post_json(url: str, body: dict) -> tuple[int, dict | str]:
    """POST JSON; return (http_status, parsed_body_or_text)."""
    if httpx is None:
        return (0, "httpx not installed in harness env")
    try:
        with httpx.Client(timeout=MNEME_TIMEOUT_S) as c:
            r = c.post(url, json=body, headers={"Content-Type": "application/json"})
        try:
            return (r.status_code, r.json())
        except Exception:
            return (r.status_code, r.text[:500])
    except Exception as e:  # network / timeout
        return (0, f"transport error: {type(e).__name__}: {e}")


def _get(url: str) -> tuple[int, dict | str]:
    if httpx is None:
        return (0, "httpx not installed in harness env")
    try:
        with httpx.Client(timeout=MNEME_TIMEOUT_S) as c:
            r = c.get(url)
        try:
            return (r.status_code, r.json())
        except Exception:
            return (r.status_code, r.text[:500])
    except Exception as e:
        return (0, f"transport error: {type(e).__name__}: {e}")


def register_mneme_tools(mcp: FastMCP) -> None:
    """Register the three MNEME bridge tools on the given FastMCP instance."""

    @mcp.tool
    def mneme_recall(query: str, k: int = 5, profile: str = "apocky") -> dict:
        """Search MNEME persistent memory for relevant memories.

        MNEME is Apocky's substrate-native persistent-memory layer (live at
        https://www.apocky.com/api/mneme/{profile}/recall). Use this tool
        BEFORE answering questions about prior decisions, build flags, repo
        layout, working-paths, or anything that might already be remembered.

        Args:
            query: Natural-language query (≤512 chars).
            k: Number of citations to return (1..20, default 5).
            profile: MNEME profile id (default "apocky").

        Returns: {ok, result_nl, result_csl, citations, confidence, served_by, ts}
                 from MNEME, or {ok: False, status, error} on failure.
        """
        logger.info(f"mneme_recall · profile={profile} · k={k} · q={query[:60]!r}")
        if len(query) > 512:
            query = query[:512]
        k = max(1, min(20, int(k)))
        url = _profile_url(profile, "recall")
        status, body = _post_json(url, {"query": query, "k": k})
        if status == 200 and isinstance(body, dict):
            return body
        return {
            "ok": False,
            "status": status,
            "error": body if not isinstance(body, dict) else body.get("error", body),
            "url": url,
        }

    @mcp.tool
    def mneme_remember(
        csl: str,
        paraphrase: str = "",
        topic_key: str = "",
        profile: str = "apocky",
    ) -> dict:
        """Persist a memory directly to MNEME via /remember (no LLM extraction).

        IMPORTANT — `csl` syntax: must be a valid CSL morpheme path. Tokens are
        separated by periods (`.`), use ASCII letters/digits/hyphens only, no
        spaces. Example: `build.cssl.toolchain.msvc-fix`. The `·` separator
        used in prose is NOT accepted by /remember.

        Args:
            csl:        CSL morpheme path · REQUIRED · domain.subdomain.verb form.
            paraphrase: optional natural-language paraphrase (≤1024 chars).
            topic_key:  optional topic key for supersession (≤256 chars).
                        if empty, MNEME defaults to using `csl` as the key.
            profile:    MNEME profile id (default "apocky").

        Returns: {ok, memory: MemoryPublic, ts} from MNEME on success,
                 or {ok: False, status, error, url} on failure.
        """
        logger.info(f"mneme_remember · profile={profile} · csl={csl!r}")
        if not csl or not isinstance(csl, str):
            return {"ok": False, "error": "csl is required and must be a string"}
        if len(paraphrase) > 1024:
            paraphrase = paraphrase[:1024]
        if len(topic_key) > 256:
            topic_key = topic_key[:256]
        body: dict[str, Any] = {"csl": csl, "type": "instruction"}
        if paraphrase:
            body["paraphrase"] = paraphrase
        if topic_key:
            body["topic_key"] = topic_key
        url = _profile_url(profile, "remember")
        status, resp = _post_json(url, body)
        if status == 200 and isinstance(resp, dict):
            return resp
        return {
            "ok": False,
            "status": status,
            "error": resp if not isinstance(resp, dict) else resp.get("error", resp),
            "url": url,
            "sent": body,
        }

    @mcp.tool
    def mneme_health(profile: str = "apocky") -> dict:
        """Check MNEME health for a profile.

        Probes /smoke (GET), /recall (POST stub), and /remember (POST stub) and
        reports which routes are reachable. Useful when debugging whether a
        recall/remember failure is a wiring bug vs. an upstream outage.

        Returns: {ok, smoke_ok, recall_ok, remember_ok, smoke, recall, remember}
        """
        logger.info(f"mneme_health · profile={profile}")
        smoke_status, smoke_body = _get(_profile_url(profile, "smoke"))
        recall_status, recall_body = _post_json(
            _profile_url(profile, "recall"),
            {"query": "__health_probe__", "k": 1},
        )
        remember_status, remember_body = _post_json(
            _profile_url(profile, "remember"),
            {
                "csl": "diagnostic.health.probe",
                "paraphrase": "mneme_health probe — safe to ignore",
                "topic_key": "diagnostic.health.probe",
                "type": "instruction",
            },
        )
        smoke_ok = smoke_status == 200
        recall_ok = recall_status == 200
        remember_ok = remember_status == 200
        return {
            "ok": smoke_ok and recall_ok,  # remember currently flaky in prod
            "smoke_ok": smoke_ok,
            "recall_ok": recall_ok,
            "remember_ok": remember_ok,
            "smoke": {"status": smoke_status, "body": smoke_body},
            "recall": {"status": recall_status, "body": recall_body},
            "remember": {"status": remember_status, "body": remember_body},
        }

    logger.info("MNEME bridge tools registered: mneme_recall, mneme_remember, mneme_health")
