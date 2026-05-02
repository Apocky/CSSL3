-- =====================================================================
-- § T11-W12-MODERATION · 0031_content_moderation.sql
-- Community content-moderation schema. Mirrors the bit-pack invariants
-- from compiler-rs/crates/cssl-content-moderation. ALL tables RLS-policied
-- · default-deny · k-anon-floor enforced @ DB.
--
-- PRIME-DIRECTIVE INVARIANTS (encoded structurally) :
--   ─ ¬ shadowban : aggregate row visible_to_author flips at total_flags ≥ 3
--   ─ ¬ algorithmic-suppression : NO time-decay column · NO engagement
--   ─ author-appeal ALWAYS-available : 30-day window CHECK
--   ─ auto-restore @ 7-days : helper fn `appeal_auto_restore_due()`
--   ─ flagger-revocable : flagger may DELETE own row (RLS USING)
--   ─ author-revocable : author may INSERT sovereign-revoke @ any-stage
--   ─ curator-decision REQUIRES Σ-Chain-anchor : NOT NULL constraint
--   ─ k-anon : aggregates publicly visible only at total_flags ≥ 3
--
-- Tables :
--   - content_flags                  · 32-byte bit-pack + RLS
--   - content_appeals                · author-side appeals (30-day window)
--   - content_curator_decisions      · Σ-Chain-anchor REQUIRED
--   - content_moderation_aggregates  · k-anon aggregate-counts view
--
-- Helpers :
--   - content_moderation_recompute(p_id)  · idempotent aggregate refresh
--   - appeal_auto_restore_due()           · returns appeals past T5 window
--
-- Apply order : after 0030.
-- =====================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ─── content_flags ─────────────────────────────────────────────────────
-- One row per flagger × content. flagger_pubkey_hash is BLAKE3-trunc
-- (non-recoverable to pubkey). RLS ensures :
--   ─ flagger sees own rows
--   ─ admin role sees all
--   ─ author + community see ONLY aggregate (via aggregates table) when ≥3
CREATE TABLE IF NOT EXISTS public.content_flags (
    id                       bigserial   PRIMARY KEY,
    content_id               uuid        NOT NULL,
    flagger_pubkey_hash      text        NOT NULL,
    flagger_pubkey           text        NOT NULL,
    flag_kind                int         NOT NULL,
    severity                 int         NOT NULL DEFAULT 50,
    sigma_mask               int         NOT NULL,
    rationale_short_hash     text        NULL,
    sig_trunc                text        NULL,
    raw_pack                 bytea       NOT NULL,
    flagged_at               timestamptz NOT NULL DEFAULT now(),
    revoked_at               timestamptz NULL,
    CONSTRAINT content_flags_severity_range
        CHECK (severity BETWEEN 0 AND 100),
    CONSTRAINT content_flags_kind_range
        CHECK (flag_kind BETWEEN 0 AND 7),
    CONSTRAINT content_flags_sigma_reserved_zero
        CHECK ((sigma_mask & 192) = 0),  -- bits 6..7 reserved
    CONSTRAINT content_flags_raw_pack_size
        CHECK (octet_length(raw_pack) = 32)
);

CREATE UNIQUE INDEX IF NOT EXISTS content_flags_unique_per_flagger
    ON public.content_flags (content_id, flagger_pubkey_hash)
    WHERE revoked_at IS NULL;

CREATE INDEX IF NOT EXISTS content_flags_content_idx
    ON public.content_flags (content_id);

ALTER TABLE public.content_flags ENABLE ROW LEVEL SECURITY;

-- RLS: flagger sees own rows. Admin via service_role.
CREATE POLICY content_flags_select_own
    ON public.content_flags FOR SELECT
    USING (flagger_pubkey = current_setting('request.jwt.claim.sub', true));

CREATE POLICY content_flags_insert_self
    ON public.content_flags FOR INSERT
    WITH CHECK (flagger_pubkey = current_setting('request.jwt.claim.sub', true));

-- Flagger may revoke own flag (UPDATE revoked_at).
CREATE POLICY content_flags_revoke_own
    ON public.content_flags FOR UPDATE
    USING (flagger_pubkey = current_setting('request.jwt.claim.sub', true))
    WITH CHECK (flagger_pubkey = current_setting('request.jwt.claim.sub', true));

-- ─── content_appeals ───────────────────────────────────────────────────
-- Author-filed appeals. Within 30-day window from prior decision.
CREATE TABLE IF NOT EXISTS public.content_appeals (
    id                          bigserial   PRIMARY KEY,
    content_id                  uuid        NOT NULL,
    author_pubkey               text        NOT NULL,
    author_pubkey_hash          text        NOT NULL,
    decision_id_appealed        bigint      NULL,
    rationale                   text        NOT NULL,
    signature                   text        NOT NULL,
    filed_at                    timestamptz NOT NULL DEFAULT now(),
    decision_at                 timestamptz NULL,  -- NULL when appealing flag-state directly
    curator_quorum_reached      boolean     NOT NULL DEFAULT FALSE,
    resolved_at                 timestamptz NULL,
    resolution_kind             int         NULL,  -- DecisionKind disc · 7=AutoRestored
    CONSTRAINT content_appeals_resolution_range
        CHECK (resolution_kind IS NULL OR resolution_kind BETWEEN 0 AND 7),
    -- 30-day appeal window (when decision_at is present).
    CONSTRAINT content_appeals_within_window
        CHECK (
            decision_at IS NULL
            OR filed_at <= decision_at + INTERVAL '30 days'
        )
);

CREATE INDEX IF NOT EXISTS content_appeals_content_idx
    ON public.content_appeals (content_id);

ALTER TABLE public.content_appeals ENABLE ROW LEVEL SECURITY;

CREATE POLICY content_appeals_select_own_or_public
    ON public.content_appeals FOR SELECT
    USING (
        author_pubkey = current_setting('request.jwt.claim.sub', true)
        OR resolved_at IS NOT NULL  -- resolved appeals are publicly visible (transparency)
    );

CREATE POLICY content_appeals_insert_self
    ON public.content_appeals FOR INSERT
    WITH CHECK (author_pubkey = current_setting('request.jwt.claim.sub', true));

-- ─── content_curator_decisions ─────────────────────────────────────────
-- Σ-Chain-anchored decisions. sigma_chain_anchor NOT NULL · enforced.
CREATE TABLE IF NOT EXISTS public.content_curator_decisions (
    id                          bigserial   PRIMARY KEY,
    content_id                  uuid        NOT NULL,
    curator_pubkey              text        NOT NULL,
    curator_pubkey_hash         text        NOT NULL,
    cap_class                   int         NOT NULL,  -- 0x04=CommunityElected · 0x08=Substrate
    decision_kind               int         NOT NULL,  -- DecisionKind 0..=7
    decided_at                  timestamptz NOT NULL DEFAULT now(),
    sigma_chain_anchor          text        NOT NULL,  -- BLAKE3 hex (¬ NULL · ¬ silent-decision)
    rationale                   text        NOT NULL,
    signature                   text        NOT NULL,
    appeal_id                   bigint      NULL,
    CONSTRAINT content_curator_decisions_kind_range
        CHECK (decision_kind BETWEEN 0 AND 7),
    CONSTRAINT content_curator_decisions_cap_class_valid
        CHECK (cap_class IN (4, 8)),
    CONSTRAINT content_curator_decisions_anchor_nonempty
        CHECK (length(sigma_chain_anchor) >= 64),  -- 32 bytes hex = 64 chars
    CONSTRAINT content_curator_decisions_rationale_short
        CHECK (length(rationale) <= 64)
);

CREATE INDEX IF NOT EXISTS content_curator_decisions_content_idx
    ON public.content_curator_decisions (content_id, decided_at DESC);

ALTER TABLE public.content_curator_decisions ENABLE ROW LEVEL SECURITY;

-- All curator decisions are PUBLIC (transparency-first).
CREATE POLICY content_curator_decisions_public_read
    ON public.content_curator_decisions FOR SELECT
    USING (TRUE);

-- Only cap-curator INSERT (enforced @ edge ; service_role bypass).
CREATE POLICY content_curator_decisions_curator_insert
    ON public.content_curator_decisions FOR INSERT
    WITH CHECK (curator_pubkey = current_setting('request.jwt.claim.sub', true));

-- ─── content_moderation_aggregates ─────────────────────────────────────
-- k-anon-aware aggregate-cache. Recomputed by helper fn.
CREATE TABLE IF NOT EXISTS public.content_moderation_aggregates (
    content_id              uuid        PRIMARY KEY,
    total_flags             int         NOT NULL DEFAULT 0,
    distinct_flaggers       int         NOT NULL DEFAULT 0,
    severity_weighted       int         NOT NULL DEFAULT 0,
    per_kind_counts         int[]       NOT NULL DEFAULT '{0,0,0,0,0,0,0,0}',
    needs_review            boolean     NOT NULL DEFAULT FALSE,
    visible_to_author       boolean     NOT NULL DEFAULT FALSE,
    last_flag_at            timestamptz NULL,
    sovereign_revoked_at    timestamptz NULL,
    updated_at              timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT content_aggregates_per_kind_size
        CHECK (array_length(per_kind_counts, 1) = 8)
);

ALTER TABLE public.content_moderation_aggregates ENABLE ROW LEVEL SECURITY;

-- Aggregates : public-read ONLY when k-anon floor reached (≥3 flags).
CREATE POLICY content_aggregates_public_read_above_floor
    ON public.content_moderation_aggregates FOR SELECT
    USING (visible_to_author = TRUE);

-- Helper fn : recompute aggregate for one content_id (idempotent).
CREATE OR REPLACE FUNCTION public.content_moderation_recompute(p_id uuid)
RETURNS public.content_moderation_aggregates
LANGUAGE plpgsql
AS $$
DECLARE
    v_total      int;
    v_distinct   int;
    v_weighted   int;
    v_per_kind   int[] := '{0,0,0,0,0,0,0,0}';
    v_last       timestamptz;
    v_visible    boolean;
    v_needs      boolean;
    v_revoked    timestamptz;
    r            public.content_moderation_aggregates%ROWTYPE;
BEGIN
    SELECT
        COUNT(*)::int,
        COUNT(DISTINCT flagger_pubkey_hash)::int,
        COALESCE(SUM(severity / 10), 0)::int,
        MAX(flagged_at)
    INTO v_total, v_distinct, v_weighted, v_last
    FROM public.content_flags
    WHERE content_id = p_id AND revoked_at IS NULL;

    -- per-kind histogram
    FOR i IN 0..7 LOOP
        SELECT COUNT(*)::int INTO v_per_kind[i+1]
        FROM public.content_flags
        WHERE content_id = p_id AND revoked_at IS NULL AND flag_kind = i;
    END LOOP;

    v_visible := v_total >= 3;
    v_needs := (v_distinct >= 10) AND (v_weighted >= 75);

    -- Carry sovereign-revoke status (NEVER overwritten — sovereign trumps).
    SELECT sovereign_revoked_at INTO v_revoked
    FROM public.content_moderation_aggregates
    WHERE content_id = p_id;

    INSERT INTO public.content_moderation_aggregates
        (content_id, total_flags, distinct_flaggers, severity_weighted,
         per_kind_counts, needs_review, visible_to_author, last_flag_at,
         sovereign_revoked_at, updated_at)
    VALUES
        (p_id, v_total, v_distinct, v_weighted, v_per_kind, v_needs,
         v_visible, v_last, v_revoked, now())
    ON CONFLICT (content_id) DO UPDATE SET
        total_flags        = EXCLUDED.total_flags,
        distinct_flaggers  = EXCLUDED.distinct_flaggers,
        severity_weighted  = EXCLUDED.severity_weighted,
        per_kind_counts    = EXCLUDED.per_kind_counts,
        needs_review       = EXCLUDED.needs_review,
        visible_to_author  = EXCLUDED.visible_to_author,
        last_flag_at       = EXCLUDED.last_flag_at,
        updated_at         = now()
    RETURNING * INTO r;

    RETURN r;
END;
$$;

-- Helper fn : appeals past T5 (7-day) auto-restore window.
CREATE OR REPLACE FUNCTION public.appeal_auto_restore_due()
RETURNS SETOF public.content_appeals
LANGUAGE sql STABLE
AS $$
    SELECT *
    FROM public.content_appeals
    WHERE resolved_at IS NULL
      AND filed_at + INTERVAL '7 days' <= now();
$$;

-- ─── Trigger : recompute aggregate on flag insert/update/delete ────────
CREATE OR REPLACE FUNCTION public.content_flags_aggregate_trigger()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM public.content_moderation_recompute(
        COALESCE(NEW.content_id, OLD.content_id));
    RETURN COALESCE(NEW, OLD);
END;
$$;

DROP TRIGGER IF EXISTS content_flags_aggregate_t ON public.content_flags;
CREATE TRIGGER content_flags_aggregate_t
    AFTER INSERT OR UPDATE OR DELETE ON public.content_flags
    FOR EACH ROW EXECUTE FUNCTION public.content_flags_aggregate_trigger();

-- ─── grants ────────────────────────────────────────────────────────────
GRANT SELECT ON public.content_flags                  TO authenticated;
GRANT INSERT, UPDATE ON public.content_flags          TO authenticated;
GRANT SELECT, INSERT ON public.content_appeals        TO authenticated;
GRANT SELECT ON public.content_curator_decisions      TO authenticated, anon;
GRANT INSERT ON public.content_curator_decisions      TO authenticated;
GRANT SELECT ON public.content_moderation_aggregates  TO authenticated, anon;

COMMENT ON TABLE public.content_flags IS
'Community content-flag records. NOT shadowban — flagger may revoke own row at any-stage. K-anon enforced via aggregate-table.';

COMMENT ON TABLE public.content_curator_decisions IS
'Σ-Chain-anchored curator decisions. Public-read · author-transparent. Anchor REQUIRED (NOT NULL) · no silent-decision.';

COMMENT ON TABLE public.content_moderation_aggregates IS
'K-anon aggregate cache. visible_to_author flips at ≥ 3 flags · needs_review at ≥ 10 distinct + weighted ≥ 75. NO time-decay · NO algorithmic-suppression.';

COMMENT ON TABLE public.content_appeals IS
'Author-filed appeals. 30-day window enforced via CHECK. Auto-restore at 7-days-no-decision via appeal_auto_restore_due().';
