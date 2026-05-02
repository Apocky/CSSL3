-- =====================================================================
-- § T11-W11-HOTFIX-INFRA · 0025_hotfix.sql
-- Live-update infrastructure schema. Stores per-channel manifest
-- versions, fleet-wide apply rollups, and per-user (Σ-mask-gated)
-- status rows. ALL tables RLS-policied · default-deny-everything.
--
-- Tables :
--   - hotfix_manifest_versions  · authoritative version table (admin-write only)
--   - hotfix_apply_status       · fleet-wide rollup counters (anonymous)
--   - hotfix_user_status        · per-user status (Σ-mask-gated · self-row only)
--   - hotfix_revocations        · audit-trail of revoke calls
--
-- Helpers :
--   - bump_hotfix_apply_status(p_channel, p_version, p_column)
--
-- All tables UUID-PK or natural-PK · all carry created_at/updated_at
-- timestamptz · all carry sigma_mask uuid for cross-table consent linkage.
--
-- Apply order : after 0024.
-- =====================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ─── hotfix_manifest_versions ─────────────────────────────────────────
-- Authoritative table : the manifest endpoint reads from this.
-- (channel, version) is the natural primary key.
CREATE TABLE IF NOT EXISTS public.hotfix_manifest_versions (
    channel             text        NOT NULL,
    version             text        NOT NULL,
    bundle_sha256       text        NOT NULL,
    cap_signer          text        NOT NULL,
    signature           text        NOT NULL,
    effective_from_ns   bigint      NOT NULL DEFAULT (EXTRACT(EPOCH FROM now())::bigint * 1000000000),
    size_bytes          bigint      NOT NULL,
    revoked_at          timestamptz NULL,
    revoked_reason      text        NULL,
    sigma_mask          uuid        NOT NULL DEFAULT gen_random_uuid(),
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (channel, version),
    CONSTRAINT hotfix_manifest_versions_channel_enum
        CHECK (channel IN (
            'loa.binary', 'cssl.bundle', 'kan.weights', 'balance.config',
            'recipe.book', 'nemesis.bestiary', 'security.patch',
            'storylet.content', 'render.pipeline'
        )),
    CONSTRAINT hotfix_manifest_versions_cap_enum
        CHECK (cap_signer IN ('cap-A','cap-B','cap-C','cap-D','cap-E')),
    CONSTRAINT hotfix_manifest_versions_version_shape
        CHECK (version ~ '^\d+\.\d+\.\d+$'),
    CONSTRAINT hotfix_manifest_versions_sha256_shape
        CHECK (length(bundle_sha256) = 64 AND bundle_sha256 ~ '^[0-9a-f]+$'),
    CONSTRAINT hotfix_manifest_versions_signature_shape
        CHECK (length(signature) = 128 AND signature ~ '^[0-9a-f]+$')
);
COMMENT ON TABLE public.hotfix_manifest_versions IS
    'Per-channel current-version table. Admin-writes only ; reads via /api/hotfix/manifest.';

CREATE INDEX IF NOT EXISTS hotfix_manifest_versions_active_idx
    ON public.hotfix_manifest_versions (channel, revoked_at)
    WHERE revoked_at IS NULL;

-- ─── hotfix_apply_status ──────────────────────────────────────────────
-- Fleet-wide rollup. INSERT-or-UPDATE keyed on (channel, version).
-- No PII : just tally counters.
CREATE TABLE IF NOT EXISTS public.hotfix_apply_status (
    channel              text        NOT NULL,
    version              text        NOT NULL,
    applied_count        bigint      NOT NULL DEFAULT 0,
    failed_count         bigint      NOT NULL DEFAULT 0,
    rolled_back_count    bigint      NOT NULL DEFAULT 0,
    sigma_mask           uuid        NOT NULL DEFAULT gen_random_uuid(),
    first_seen_at        timestamptz NOT NULL DEFAULT now(),
    last_updated_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (channel, version)
);
COMMENT ON TABLE public.hotfix_apply_status IS
    'Aggregate counters per (channel, version). No per-user data.';

-- ─── hotfix_user_status ───────────────────────────────────────────────
-- Per-user row, gated by Σ-mask consent. Only written when the client
-- supplies jwt_sub ; anonymous clients hit only the aggregate above.
CREATE TABLE IF NOT EXISTS public.hotfix_user_status (
    id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    jwt_sub     text        NOT NULL,
    channel     text        NOT NULL,
    version     text        NOT NULL,
    status      text        NOT NULL,
    ts_ns       bigint      NOT NULL,
    error_msg   text        NULL,
    sigma_mask  uuid        NOT NULL DEFAULT gen_random_uuid(),
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT hotfix_user_status_status_enum
        CHECK (status IN ('applied', 'failed', 'rolled_back')),
    CONSTRAINT hotfix_user_status_unique_recent
        UNIQUE (jwt_sub, channel, version, status)
);
COMMENT ON TABLE public.hotfix_user_status IS
    'Per-user status row (Σ-mask-gated). UNIQUE on (jwt_sub, channel, version, status).';

CREATE INDEX IF NOT EXISTS hotfix_user_status_jwt_sub_idx
    ON public.hotfix_user_status (jwt_sub);
CREATE INDEX IF NOT EXISTS hotfix_user_status_channel_idx
    ON public.hotfix_user_status (channel, version);

-- ─── hotfix_revocations ───────────────────────────────────────────────
-- Audit trail. INSERT-only (no UPDATE/DELETE policy).
CREATE TABLE IF NOT EXISTS public.hotfix_revocations (
    id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    channel     text        NOT NULL,
    version     text        NOT NULL,
    reason      text        NOT NULL,
    revoked_by  text        NOT NULL,
    sigma_mask  uuid        NOT NULL DEFAULT gen_random_uuid(),
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT hotfix_revocations_reason_length
        CHECK (char_length(reason) BETWEEN 4 AND 200)
);
COMMENT ON TABLE public.hotfix_revocations IS
    'Audit trail of /api/hotfix/revoke calls. INSERT-only.';

-- ─── helper · bump_hotfix_apply_status ────────────────────────────────
CREATE OR REPLACE FUNCTION public.bump_hotfix_apply_status(
    p_channel text,
    p_version text,
    p_column  text
) RETURNS void AS $$
BEGIN
    INSERT INTO public.hotfix_apply_status (channel, version)
    VALUES (p_channel, p_version)
    ON CONFLICT (channel, version) DO NOTHING;

    IF p_column = 'applied_count' THEN
        UPDATE public.hotfix_apply_status
            SET applied_count = applied_count + 1, last_updated_at = now()
            WHERE channel = p_channel AND version = p_version;
    ELSIF p_column = 'failed_count' THEN
        UPDATE public.hotfix_apply_status
            SET failed_count = failed_count + 1, last_updated_at = now()
            WHERE channel = p_channel AND version = p_version;
    ELSIF p_column = 'rolled_back_count' THEN
        UPDATE public.hotfix_apply_status
            SET rolled_back_count = rolled_back_count + 1, last_updated_at = now()
            WHERE channel = p_channel AND version = p_version;
    END IF;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
COMMENT ON FUNCTION public.bump_hotfix_apply_status IS
    'Increment one counter on hotfix_apply_status. Invoked from /api/hotfix/status.';

-- =====================================================================
-- § Row-Level Security
-- =====================================================================

-- ─── hotfix_manifest_versions · public-read · service-only-write ──────
ALTER TABLE public.hotfix_manifest_versions ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "hotfix_manifest_versions_select_anon" ON public.hotfix_manifest_versions;
CREATE POLICY "hotfix_manifest_versions_select_anon"
    ON public.hotfix_manifest_versions FOR SELECT
    USING (true);

DROP POLICY IF EXISTS "hotfix_manifest_versions_service_write" ON public.hotfix_manifest_versions;
CREATE POLICY "hotfix_manifest_versions_service_write"
    ON public.hotfix_manifest_versions FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- ─── hotfix_apply_status · service-only · counters opaque to clients ──
ALTER TABLE public.hotfix_apply_status ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "hotfix_apply_status_service_only" ON public.hotfix_apply_status;
CREATE POLICY "hotfix_apply_status_service_only"
    ON public.hotfix_apply_status FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- ─── hotfix_user_status · self-read · service-write ───────────────────
ALTER TABLE public.hotfix_user_status ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "hotfix_user_status_self_read" ON public.hotfix_user_status;
CREATE POLICY "hotfix_user_status_self_read"
    ON public.hotfix_user_status FOR SELECT
    USING (auth.uid()::text = jwt_sub OR auth.role() = 'service_role');

DROP POLICY IF EXISTS "hotfix_user_status_service_write" ON public.hotfix_user_status;
CREATE POLICY "hotfix_user_status_service_write"
    ON public.hotfix_user_status FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- ─── hotfix_revocations · service-only · INSERT-ONLY ──────────────────
ALTER TABLE public.hotfix_revocations ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "hotfix_revocations_service_only" ON public.hotfix_revocations;
CREATE POLICY "hotfix_revocations_service_only"
    ON public.hotfix_revocations FOR INSERT
    WITH CHECK (auth.role() = 'service_role');

DROP POLICY IF EXISTS "hotfix_revocations_service_select" ON public.hotfix_revocations;
CREATE POLICY "hotfix_revocations_service_select"
    ON public.hotfix_revocations FOR SELECT
    USING (auth.role() = 'service_role');

-- =====================================================================
-- § ATTESTATION
-- ¬ harm · sovereign-revocable · Σ-mask-gated · default-deny everywhere
-- ¬ DRM · ¬ rootkit · rollback-always-available · cap-key-restricted-writes
-- =====================================================================
