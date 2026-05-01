-- =====================================================================
-- § T11-W5c-SUPABASE-GAMESTATE · 0011_sovereign_audit.sql
-- Sovereign-cap audit table for transparency on AI-companion / sovereign-
-- capability bypass events.
--
-- Every time a sovereign-cap is asserted (by any caller : MCP companion
-- tools, MCP render tools, the csslc CLI, etc.) the host is required to
-- write an audit row here. This table is the queryable in-database mirror
-- of the audit-JSONL log carried alongside every CSSL session.
--
-- The table is INSERT-ONLY by design (see 0012_game_state_rls.sql). Once
-- a row is written it cannot be UPDATE'd or DELETE'd by any non-service-
-- role principal. Transparency requires immutability ; the user is the
-- only principal who can SELECT their own rows, and they cannot rewrite
-- them.
--
-- Apply order : after 0010.
-- =====================================================================

-- pgcrypto is loaded by 0001_initial.sql ; reassert defensively
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- =====================================================================
-- public.sovereign_cap_audit · INSERT-ONLY transparency log
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.sovereign_cap_audit (
    id                     bigserial   PRIMARY KEY,
    session_id             uuid        NOT NULL,
    player_id              text        NOT NULL,
    ts                     timestamptz NOT NULL DEFAULT now(),
    action_kind            text        NOT NULL,
    cap_bypassed_kind      text        NOT NULL,
    reason                 text        NOT NULL,
    target_audit_event_id  text,
    caller_origin          text        NOT NULL,
    CONSTRAINT sovereign_cap_audit_player_id_length
        CHECK (char_length(player_id) BETWEEN 1 AND 200),
    CONSTRAINT sovereign_cap_audit_action_kind_length
        CHECK (char_length(action_kind) BETWEEN 1 AND 100),
    CONSTRAINT sovereign_cap_audit_cap_bypassed_kind_length
        CHECK (char_length(cap_bypassed_kind) BETWEEN 1 AND 100),
    CONSTRAINT sovereign_cap_audit_reason_length
        CHECK (char_length(reason) BETWEEN 1 AND 4000),
    CONSTRAINT sovereign_cap_audit_target_event_length
        CHECK (target_audit_event_id IS NULL
               OR char_length(target_audit_event_id) BETWEEN 1 AND 200),
    CONSTRAINT sovereign_cap_audit_caller_origin_length
        CHECK (char_length(caller_origin) BETWEEN 1 AND 200)
);

-- Per-player timeline (most-recent-first)
CREATE INDEX IF NOT EXISTS sovereign_cap_audit_player_ts_desc_idx
    ON public.sovereign_cap_audit (player_id, ts DESC);

-- Action-kind histogram (e.g. "spawn", "imbue", "render-png")
CREATE INDEX IF NOT EXISTS sovereign_cap_audit_action_kind_idx
    ON public.sovereign_cap_audit (action_kind);

-- Per-session forward-timeline (replay order)
CREATE INDEX IF NOT EXISTS sovereign_cap_audit_session_ts_asc_idx
    ON public.sovereign_cap_audit (session_id, ts ASC);

COMMENT ON TABLE public.sovereign_cap_audit IS
    'INSERT-ONLY transparency log for sovereign-capability bypass events. Once written, immutable to all non-service-role principals (RLS in 0012). Mirrors the audit-JSONL log carried with each CSSL session.';
COMMENT ON COLUMN public.sovereign_cap_audit.session_id IS
    'Logical session identifier. Cross-references public.game_session_index.session_id when the action occurred mid-game ; not enforced as FK because audit events MUST be writable even when session_index has no matching row (e.g. CLI uses csslc with no session).';
COMMENT ON COLUMN public.sovereign_cap_audit.action_kind IS
    'High-level action label (e.g. ''spawn'', ''imbue'', ''snapshot_png'', ''mutate_world'', ''force-skip-paywall''). Free-form to avoid premature lock-in ; enumerated in client-side typings.';
COMMENT ON COLUMN public.sovereign_cap_audit.cap_bypassed_kind IS
    'The capability that was bypassed (e.g. ''content_filter'', ''rate_limit'', ''external_io'', ''harm_check''). Mandatory.';
COMMENT ON COLUMN public.sovereign_cap_audit.reason IS
    'Human-readable rationale supplied by the principal who used the cap. Required ; must be specific enough that the player can later understand WHY the cap was asserted on their behalf.';
COMMENT ON COLUMN public.sovereign_cap_audit.target_audit_event_id IS
    'Optional pointer to the corresponding audit-JSONL row (typically the JSONL-line uuid). Bridges in-DB rows to file-system audit logs.';
COMMENT ON COLUMN public.sovereign_cap_audit.caller_origin IS
    'Origin of the call (e.g. ''mcp:companion'', ''mcp:render.snapshot_png'', ''cli:csslc'', ''ide:lsp''). Provides traceback for diagnostics.';

-- =====================================================================
-- public.sovereign_cap_audit_summary · aggregate VIEW for UI dashboards
-- =====================================================================
-- Player-facing transparency UI : "you used cap-bypass X times across these
-- N actions". RLS on the underlying table propagates through the view, so
-- a player can only see their own counts.
--
-- DROP-and-recreate so re-applying the migration does not get stuck on a
-- changed column-set.
-- =====================================================================
DROP VIEW IF EXISTS public.sovereign_cap_audit_summary;

CREATE VIEW public.sovereign_cap_audit_summary AS
    SELECT
        player_id,
        action_kind,
        count(*)  AS uses,
        min(ts)   AS first_use,
        max(ts)   AS last_use
      FROM public.sovereign_cap_audit
     GROUP BY player_id, action_kind;

COMMENT ON VIEW public.sovereign_cap_audit_summary IS
    'Aggregated per-player / per-action sovereign-cap usage counts. Used by transparency UI panels. RLS on the underlying table propagates : a player sees only their own row aggregates.';

-- =====================================================================
-- Grants (RLS still gates row visibility ; UPDATE/DELETE are NOT granted
-- to authenticated — the table is INSERT-ONLY for non-service-role.)
-- =====================================================================
GRANT SELECT, INSERT       ON public.sovereign_cap_audit         TO authenticated;
GRANT SELECT               ON public.sovereign_cap_audit_summary TO authenticated;
GRANT USAGE, SELECT        ON SEQUENCE public.sovereign_cap_audit_id_seq
                                                                  TO authenticated;
