-- =====================================================================
-- § T11-W12-7-RATING · 0028_content_rating.sql
--
-- Content rating + review storage with k-anonymized aggregates and
-- author-immutable / rater-revocable RLS. Server-side mirror of the
-- in-memory `cssl-content-rating` Rust crate.
--
-- § INVARIANTS
--   ─ Author CANNOT modify or delete a rater's row (RLS enforces this).
--     Authors get aggregate-level visibility ; per-row only via consent
--     flag `share_with_author` set by the rater themselves.
--   ─ Rater can ALWAYS read their own row (private detail-view).
--   ─ Aggregate publishes ONLY when `distinct_rater_count >= 5` (single)
--     or `>= 10` (trending-rank-eligible).
--   ─ Sovereign-revoke = update stars=0 + tags_bitset=0 ; row preserved
--     for audit but excluded from aggregates.
--   ─ ¬ surveillance ; ¬ scroll-tracking ; ¬ time-on-card ; ¬ paid-promotion.
--   ─ Apply order : after the 0027_* moderation slot ; the rating system is
--     orthogonal to publish-pipeline.
-- =====================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- =====================================================================
-- public.content_ratings · INSERT-or-UPSERT-with-overwrite per (rater, content)
-- =====================================================================
-- 24-byte bit-packed Rating record (compiler-rs/cssl-content-rating). The DB
-- row mirrors the Rust struct field-for-field ; the bytea `pack_v1` column
-- stores the canonical 24-byte little-endian pack so a future host can
-- byte-compare on egress.
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.content_ratings (
    id                          bigserial PRIMARY KEY,
    rater_pubkey_hash           bytea       NOT NULL,
    content_id                  bigint      NOT NULL,
    stars                       smallint    NOT NULL,
    tags_bitset                 integer     NOT NULL,
    sigma_mask                  smallint    NOT NULL,
    ts_minutes_since_epoch      bigint      NOT NULL,
    weight_q8                   smallint    NOT NULL DEFAULT 200,
    -- Rater controls whether the per-row detail is shared with the content
    -- author (default false ; aggregate-only by default).
    share_with_author           boolean     NOT NULL DEFAULT false,
    -- Mirror of compiler-rs `Rating::pack()` output for byte-equal egress.
    pack_v1                     bytea       NOT NULL,
    inserted_at                 timestamptz NOT NULL DEFAULT now(),
    updated_at                  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT content_ratings_pubkey_hash_len
        CHECK (octet_length(rater_pubkey_hash) = 8),
    CONSTRAINT content_ratings_pack_v1_len
        CHECK (octet_length(pack_v1) = 24),
    CONSTRAINT content_ratings_stars_range
        CHECK (stars BETWEEN 0 AND 5),
    CONSTRAINT content_ratings_tags_bitset_range
        CHECK (tags_bitset BETWEEN 0 AND 65535),
    CONSTRAINT content_ratings_sigma_mask_range
        CHECK (sigma_mask BETWEEN 0 AND 255),
    CONSTRAINT content_ratings_weight_q8_range
        CHECK (weight_q8 BETWEEN 0 AND 255),
    -- One row per (rater, content). Re-rate = UPSERT.
    CONSTRAINT content_ratings_one_per_rater_content
        UNIQUE (rater_pubkey_hash, content_id)
);

CREATE INDEX IF NOT EXISTS content_ratings_content_idx
    ON public.content_ratings (content_id, stars DESC);
CREATE INDEX IF NOT EXISTS content_ratings_rater_idx
    ON public.content_ratings (rater_pubkey_hash);
CREATE INDEX IF NOT EXISTS content_ratings_ts_idx
    ON public.content_ratings (ts_minutes_since_epoch DESC);

COMMENT ON TABLE public.content_ratings IS
    '§ T11-W12-7 · Star+tag ratings. Author cannot modify ; rater can revoke (stars=0). Aggregates require k>=5 distinct raters before public visibility.';
COMMENT ON COLUMN public.content_ratings.share_with_author IS
    'Rater-controlled consent flag. When true, the content author can see this row''s per-row detail via RLS ; default false (aggregate-only).';
COMMENT ON COLUMN public.content_ratings.pack_v1 IS
    'Canonical 24-byte little-endian Rating::pack() output. Byte-equal to in-memory representation.';

-- =====================================================================
-- public.content_reviews · variable-size reviews (≤ 240 bytes body)
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.content_reviews (
    id                          bigserial PRIMARY KEY,
    rater_pubkey_hash           bytea       NOT NULL,
    content_id                  bigint      NOT NULL,
    stars                       smallint    NOT NULL,
    body                        text        NOT NULL,
    tags_bitset                 integer     NOT NULL,
    sigma_mask                  smallint    NOT NULL,
    ts_minutes_since_epoch      bigint      NOT NULL,
    sig                         bytea       NOT NULL,
    share_with_author           boolean     NOT NULL DEFAULT false,
    inserted_at                 timestamptz NOT NULL DEFAULT now(),
    updated_at                  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT content_reviews_pubkey_hash_len
        CHECK (octet_length(rater_pubkey_hash) = 8),
    CONSTRAINT content_reviews_sig_len
        CHECK (octet_length(sig) = 64),
    CONSTRAINT content_reviews_body_len
        CHECK (char_length(body) BETWEEN 0 AND 240),
    CONSTRAINT content_reviews_stars_range
        CHECK (stars BETWEEN 1 AND 5),
    CONSTRAINT content_reviews_tags_bitset_range
        CHECK (tags_bitset BETWEEN 0 AND 65535),
    CONSTRAINT content_reviews_sigma_mask_range
        CHECK (sigma_mask BETWEEN 0 AND 255),
    CONSTRAINT content_reviews_one_per_rater_content
        UNIQUE (rater_pubkey_hash, content_id)
);

CREATE INDEX IF NOT EXISTS content_reviews_content_idx
    ON public.content_reviews (content_id);
CREATE INDEX IF NOT EXISTS content_reviews_rater_idx
    ON public.content_reviews (rater_pubkey_hash);

COMMENT ON TABLE public.content_reviews IS
    '§ T11-W12-7 · Free-text reviews ≤ 240 chars + Ed25519 signature. Author CANNOT modify ; rater can overwrite or delete.';

-- =====================================================================
-- public.content_rating_aggregates · materialized k-anon view
-- =====================================================================
-- DROP-and-recreate so re-applying the migration survives column-set churn.
-- Definition is `WITH SECURITY DEFINER`-style : the view INTENTIONALLY
-- exposes only post-k-floor aggregates ; sub-floor rows return NULLs for
-- means + tag_top + count is exposed (so the UI knows "still gathering").
-- =====================================================================
DROP VIEW IF EXISTS public.content_rating_aggregates;

CREATE VIEW public.content_rating_aggregates AS
WITH per_content AS (
    SELECT
        content_id,
        COUNT(DISTINCT rater_pubkey_hash) FILTER (WHERE stars > 0
            AND (sigma_mask & 2) = 2)
            AS distinct_rater_count,
        AVG(NULLIF(stars, 0)) FILTER (WHERE stars > 0
            AND (sigma_mask & 2) = 2)
            AS mean_stars_raw,
        SUM(CASE WHEN (tags_bitset & 1)     <> 0 AND stars > 0 AND (sigma_mask & 2) = 2 THEN 1 ELSE 0 END) AS tag_fun,
        SUM(CASE WHEN (tags_bitset & 2)     <> 0 AND stars > 0 AND (sigma_mask & 2) = 2 THEN 1 ELSE 0 END) AS tag_balanced,
        SUM(CASE WHEN (tags_bitset & 4)     <> 0 AND stars > 0 AND (sigma_mask & 2) = 2 THEN 1 ELSE 0 END) AS tag_creative,
        SUM(CASE WHEN (tags_bitset & 8)     <> 0 AND stars > 0 AND (sigma_mask & 2) = 2 THEN 1 ELSE 0 END) AS tag_accessible,
        SUM(CASE WHEN (tags_bitset & 16)    <> 0 AND stars > 0 AND (sigma_mask & 2) = 2 THEN 1 ELSE 0 END) AS tag_sov_resp,
        SUM(CASE WHEN (tags_bitset & 32)    <> 0 AND stars > 0 AND (sigma_mask & 2) = 2 THEN 1 ELSE 0 END) AS tag_remix_worthy,
        SUM(CASE WHEN (tags_bitset & 64)    <> 0 AND stars > 0 AND (sigma_mask & 2) = 2 THEN 1 ELSE 0 END) AS tag_doc_clear,
        SUM(CASE WHEN (tags_bitset & 128)   <> 0 AND stars > 0 AND (sigma_mask & 2) = 2 THEN 1 ELSE 0 END) AS tag_runtime_stable,
        SUM(CASE WHEN (tags_bitset & 256)   <> 0 AND stars > 0 AND (sigma_mask & 2) = 2 THEN 1 ELSE 0 END) AS tag_audio_quality,
        SUM(CASE WHEN (tags_bitset & 512)   <> 0 AND stars > 0 AND (sigma_mask & 2) = 2 THEN 1 ELSE 0 END) AS tag_visual_polish,
        SUM(CASE WHEN (tags_bitset & 1024)  <> 0 AND stars > 0 AND (sigma_mask & 2) = 2 THEN 1 ELSE 0 END) AS tag_narrative_depth,
        SUM(CASE WHEN (tags_bitset & 2048)  <> 0 AND stars > 0 AND (sigma_mask & 2) = 2 THEN 1 ELSE 0 END) AS tag_educational,
        SUM(CASE WHEN (tags_bitset & 4096)  <> 0 AND stars > 0 AND (sigma_mask & 2) = 2 THEN 1 ELSE 0 END) AS tag_welcoming,
        SUM(CASE WHEN (tags_bitset & 8192)  <> 0 AND stars > 0 AND (sigma_mask & 2) = 2 THEN 1 ELSE 0 END) AS tag_novel,
        SUM(CASE WHEN (tags_bitset & 16384) <> 0 AND stars > 0 AND (sigma_mask & 2) = 2 THEN 1 ELSE 0 END) AS tag_meditative,
        SUM(CASE WHEN (tags_bitset & 32768) <> 0 AND stars > 0 AND (sigma_mask & 2) = 2 THEN 1 ELSE 0 END) AS tag_tense
    FROM public.content_ratings
    GROUP BY content_id
)
SELECT
    content_id,
    distinct_rater_count,
    -- Hidden when count < 5 (k-anon floor for single-content aggregate)
    CASE
        WHEN distinct_rater_count >= 10 THEN 'trending'
        WHEN distinct_rater_count >= 5  THEN 'visible'
        ELSE 'hidden'
    END AS visibility,
    -- mean_stars exposed only when k-floor met
    CASE
        WHEN distinct_rater_count >= 5 THEN ROUND(mean_stars_raw::numeric, 3)
        ELSE NULL
    END AS mean_stars,
    -- per-tag count exposed only when k-floor met
    CASE WHEN distinct_rater_count >= 5 THEN tag_fun              ELSE NULL END AS tag_fun,
    CASE WHEN distinct_rater_count >= 5 THEN tag_balanced         ELSE NULL END AS tag_balanced,
    CASE WHEN distinct_rater_count >= 5 THEN tag_creative         ELSE NULL END AS tag_creative,
    CASE WHEN distinct_rater_count >= 5 THEN tag_accessible       ELSE NULL END AS tag_accessible,
    CASE WHEN distinct_rater_count >= 5 THEN tag_sov_resp         ELSE NULL END AS tag_sovereign_respectful,
    CASE WHEN distinct_rater_count >= 5 THEN tag_remix_worthy     ELSE NULL END AS tag_remix_worthy,
    CASE WHEN distinct_rater_count >= 5 THEN tag_doc_clear        ELSE NULL END AS tag_documentation_clear,
    CASE WHEN distinct_rater_count >= 5 THEN tag_runtime_stable   ELSE NULL END AS tag_runtime_stable,
    CASE WHEN distinct_rater_count >= 5 THEN tag_audio_quality    ELSE NULL END AS tag_audio_quality,
    CASE WHEN distinct_rater_count >= 5 THEN tag_visual_polish    ELSE NULL END AS tag_visual_polish,
    CASE WHEN distinct_rater_count >= 5 THEN tag_narrative_depth  ELSE NULL END AS tag_narrative_depth,
    CASE WHEN distinct_rater_count >= 5 THEN tag_educational      ELSE NULL END AS tag_educational,
    CASE WHEN distinct_rater_count >= 5 THEN tag_welcoming        ELSE NULL END AS tag_welcoming,
    CASE WHEN distinct_rater_count >= 5 THEN tag_novel            ELSE NULL END AS tag_novel,
    CASE WHEN distinct_rater_count >= 5 THEN tag_meditative       ELSE NULL END AS tag_meditative,
    CASE WHEN distinct_rater_count >= 5 THEN tag_tense            ELSE NULL END AS tag_tense
FROM per_content;

COMMENT ON VIEW public.content_rating_aggregates IS
    '§ T11-W12-7 · k-anonymized rating aggregate. Visibility tier (hidden / visible / trending) gates exposure. Below k=5 the mean + per-tag counts are NULL ; only distinct_rater_count is exposed (so the UI can show "gathering").';

-- =====================================================================
-- RLS · author-immutable + rater-revocable
-- =====================================================================
ALTER TABLE public.content_ratings ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.content_reviews ENABLE ROW LEVEL SECURITY;

-- helper : auth.uid() byte-truncated to BLAKE3-style 8-byte handle. The
-- Rust crate computes blake3-trunc client-side and submits ; the server
-- only validates length here. The principal-mapping (auth.uid() ↔ pubkey)
-- lives in a side-table set up by 0001_initial.sql ; here we trust the
-- caller-presented `rater_pubkey_hash` and gate WRITES through cap-check
-- in the edge-route (which sets the role + JWT before the INSERT).

-- Rater self-read : ALWAYS allowed.
DROP POLICY IF EXISTS content_ratings_rater_self_read ON public.content_ratings;
CREATE POLICY content_ratings_rater_self_read
    ON public.content_ratings
    FOR SELECT
    TO authenticated
    USING (
        -- Allow when caller's role-claim matches the rater's pubkey_hash.
        -- The edge-route MUST inject a `request.jwt.claims->>'rater_hash'`
        -- containing the hex-encoded BLAKE3-trunc of the caller's pubkey.
        encode(rater_pubkey_hash, 'hex') =
            COALESCE((current_setting('request.jwt.claims', true)::jsonb->>'rater_hash'), '')
    );

-- Rater self-write : INSERT or UPDATE only own row.
DROP POLICY IF EXISTS content_ratings_rater_self_write ON public.content_ratings;
CREATE POLICY content_ratings_rater_self_write
    ON public.content_ratings
    FOR INSERT
    TO authenticated
    WITH CHECK (
        encode(rater_pubkey_hash, 'hex') =
            COALESCE((current_setting('request.jwt.claims', true)::jsonb->>'rater_hash'), '')
    );

DROP POLICY IF EXISTS content_ratings_rater_self_update ON public.content_ratings;
CREATE POLICY content_ratings_rater_self_update
    ON public.content_ratings
    FOR UPDATE
    TO authenticated
    USING (
        encode(rater_pubkey_hash, 'hex') =
            COALESCE((current_setting('request.jwt.claims', true)::jsonb->>'rater_hash'), '')
    )
    WITH CHECK (
        encode(rater_pubkey_hash, 'hex') =
            COALESCE((current_setting('request.jwt.claims', true)::jsonb->>'rater_hash'), '')
    );

-- Author can read rows that the rater has consented to share.
DROP POLICY IF EXISTS content_ratings_author_consented_read ON public.content_ratings;
CREATE POLICY content_ratings_author_consented_read
    ON public.content_ratings
    FOR SELECT
    TO authenticated
    USING (
        share_with_author = true
        AND content_id::text =
            COALESCE((current_setting('request.jwt.claims', true)::jsonb->>'authored_content_id'), '')
    );

-- Aggregates are public-readable (RLS on the underlying table propagates ;
-- the VIEW filters at SQL level so sub-k-floor data is never returned).
GRANT SELECT ON public.content_rating_aggregates TO anon, authenticated;

-- Author CANNOT delete or modify ratings — no DELETE / UPDATE policy granted
-- to authenticated for non-rater-self. Only the rater can revoke.
GRANT SELECT, INSERT, UPDATE ON public.content_ratings TO authenticated;
GRANT USAGE, SELECT ON SEQUENCE public.content_ratings_id_seq TO authenticated;

-- ── Reviews mirror the rating policies ──────────────────────────────────
DROP POLICY IF EXISTS content_reviews_rater_self_read ON public.content_reviews;
CREATE POLICY content_reviews_rater_self_read
    ON public.content_reviews
    FOR SELECT
    TO authenticated
    USING (
        encode(rater_pubkey_hash, 'hex') =
            COALESCE((current_setting('request.jwt.claims', true)::jsonb->>'rater_hash'), '')
    );

DROP POLICY IF EXISTS content_reviews_rater_self_write ON public.content_reviews;
CREATE POLICY content_reviews_rater_self_write
    ON public.content_reviews
    FOR INSERT
    TO authenticated
    WITH CHECK (
        encode(rater_pubkey_hash, 'hex') =
            COALESCE((current_setting('request.jwt.claims', true)::jsonb->>'rater_hash'), '')
    );

DROP POLICY IF EXISTS content_reviews_rater_self_update ON public.content_reviews;
CREATE POLICY content_reviews_rater_self_update
    ON public.content_reviews
    FOR UPDATE
    TO authenticated
    USING (
        encode(rater_pubkey_hash, 'hex') =
            COALESCE((current_setting('request.jwt.claims', true)::jsonb->>'rater_hash'), '')
    )
    WITH CHECK (
        encode(rater_pubkey_hash, 'hex') =
            COALESCE((current_setting('request.jwt.claims', true)::jsonb->>'rater_hash'), '')
    );

DROP POLICY IF EXISTS content_reviews_rater_self_delete ON public.content_reviews;
CREATE POLICY content_reviews_rater_self_delete
    ON public.content_reviews
    FOR DELETE
    TO authenticated
    USING (
        encode(rater_pubkey_hash, 'hex') =
            COALESCE((current_setting('request.jwt.claims', true)::jsonb->>'rater_hash'), '')
    );

DROP POLICY IF EXISTS content_reviews_author_consented_read ON public.content_reviews;
CREATE POLICY content_reviews_author_consented_read
    ON public.content_reviews
    FOR SELECT
    TO authenticated
    USING (
        share_with_author = true
        AND content_id::text =
            COALESCE((current_setting('request.jwt.claims', true)::jsonb->>'authored_content_id'), '')
    );

GRANT SELECT, INSERT, UPDATE, DELETE ON public.content_reviews TO authenticated;
GRANT USAGE, SELECT ON SEQUENCE public.content_reviews_id_seq TO authenticated;
