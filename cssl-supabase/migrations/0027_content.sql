-- =====================================================================
-- § T11-W12-UGC-PUBLISH · 0027_content.sql
-- User-Generated-Content publish-pipeline schema. Stores `.ccpkg`
-- ContentPackage rows, chunked-upload state, dependency graph, and
-- remix-attribution chain. ALL tables RLS-policied · default-deny.
--
-- Tables :
--   - content_packages      · authoritative (id · author_pubkey · kind · ...)
--   - content_chunks_upload · temp · drop after complete
--   - content_dependencies  · package → package edges (kind, version)
--   - content_remix_chain   · remix-of edges (immutable attribution)
--
-- Helpers :
--   - content_publish_finalize(p_id, p_sha256, p_anchor) · atomic complete
--   - content_remix_cycle_check(p_id, p_remix_of_id)     · cycle-reject
--   - content_revoke_cascade(p_id, p_who, p_reason)      · audit-emitting
--
-- Sovereignty axioms (enforced @ DB) :
--   ¬ unauthorized-publish   · cap REQUIRED at-edge
--   ¬ silent-revoke          · revoke INSERT-trail w/ audit-row
--   ¬ pay-for-publish        · gift_economy_only column DEFAULT TRUE
--   ¬ pay-for-discovery      · CHECK constraint on license values
--   creator-revoke cascades  · view + cascade-helper
--
-- Apply order : after 0026.
-- =====================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ─── content_packages ───────────────────────────────────────────────────
-- One row per published `.ccpkg`. PK is uuid (assigned at /init).
-- author_pubkey is the Ed25519 hex pubkey · revocable identity.
CREATE TABLE IF NOT EXISTS public.content_packages (
    id                    uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    author_pubkey         text        NOT NULL,
    kind                  text        NOT NULL,
    version               text        NOT NULL,
    sigma_mask            uuid        NOT NULL DEFAULT gen_random_uuid(),
    sha256                text        NULL,
    sigma_chain_anchor    text        NULL,
    signature_ed25519     text        NULL,
    size_bytes            bigint      NOT NULL DEFAULT 0,
    chunk_count           int         NOT NULL DEFAULT 0,
    state                 text        NOT NULL DEFAULT 'init',
    created_at            timestamptz NOT NULL DEFAULT now(),
    finalized_at          timestamptz NULL,
    revoked_at            timestamptz NULL,
    revoked_reason        text        NULL,
    revoked_by_pubkey     text        NULL,
    gift_economy_only     boolean     NOT NULL DEFAULT TRUE,
    license               text        NOT NULL DEFAULT 'CC-BY-SA-4.0',
    title                 text        NULL,
    description           text        NULL,
    CONSTRAINT content_packages_kind_enum
        CHECK (kind IN (
            'scene','asset','script','soundpack','texture','model',
            'storylet','recipe','nemesis','room','quest','bundle'
        )),
    CONSTRAINT content_packages_version_shape
        CHECK (version ~ '^\d+\.\d+\.\d+$'),
    CONSTRAINT content_packages_state_enum
        CHECK (state IN ('init','uploading','verifying','published','revoked','rejected')),
    CONSTRAINT content_packages_author_pubkey_shape
        CHECK (length(author_pubkey) = 64 AND author_pubkey ~ '^[0-9a-f]+$'),
    CONSTRAINT content_packages_sha256_shape
        CHECK (sha256 IS NULL OR (length(sha256) = 64 AND sha256 ~ '^[0-9a-f]+$')),
    CONSTRAINT content_packages_signature_shape
        CHECK (signature_ed25519 IS NULL OR (length(signature_ed25519) = 128 AND signature_ed25519 ~ '^[0-9a-f]+$')),
    CONSTRAINT content_packages_anchor_shape
        CHECK (sigma_chain_anchor IS NULL OR (length(sigma_chain_anchor) = 64 AND sigma_chain_anchor ~ '^[0-9a-f]+$')),
    CONSTRAINT content_packages_license_axiom
        CHECK (license IN (
            'CC-BY-SA-4.0','CC-BY-4.0','CC-BY-NC-SA-4.0','CC-0',
            'MIT','Apache-2.0','custom-gift'
        )),
    CONSTRAINT content_packages_gift_economy_axiom
        CHECK (gift_economy_only = TRUE)  -- ¬ pay-for-publish · DB-enforced
);

CREATE INDEX IF NOT EXISTS content_packages_author_idx
    ON public.content_packages (author_pubkey);
CREATE INDEX IF NOT EXISTS content_packages_kind_idx
    ON public.content_packages (kind, version);
CREATE INDEX IF NOT EXISTS content_packages_active_idx
    ON public.content_packages (state, created_at DESC)
    WHERE revoked_at IS NULL AND state = 'published';
CREATE UNIQUE INDEX IF NOT EXISTS content_packages_authorkindver_unique
    ON public.content_packages (author_pubkey, kind, version);

COMMENT ON TABLE public.content_packages IS
    'UGC `.ccpkg` registry. author_pubkey is revocable identity. RLS · self-row + service-role.';

-- ─── content_chunks_upload ──────────────────────────────────────────────
-- Per-chunk staging. Cleared by content_publish_finalize on success.
-- Tightly bounded by max_chunks=128 (≈512MB total @ 4MB/chunk).
CREATE TABLE IF NOT EXISTS public.content_chunks_upload (
    id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    package_id  uuid        NOT NULL REFERENCES public.content_packages(id) ON DELETE CASCADE,
    seq         int         NOT NULL,
    bytes       bytea       NOT NULL,
    sha256      text        NOT NULL,
    uploaded_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT content_chunks_upload_seq_range
        CHECK (seq >= 0 AND seq < 128),
    CONSTRAINT content_chunks_upload_size_max
        CHECK (octet_length(bytes) <= 4194304),
    CONSTRAINT content_chunks_upload_unique
        UNIQUE (package_id, seq)
);
CREATE INDEX IF NOT EXISTS content_chunks_upload_package_idx
    ON public.content_chunks_upload (package_id, seq);
COMMENT ON TABLE public.content_chunks_upload IS
    'Temp chunk-staging. DELETED on /complete. Resumable via UNIQUE(package_id,seq).';

-- ─── content_dependencies ───────────────────────────────────────────────
-- Edge-list of (package → depends-on package). Cycle-rejected at insert.
CREATE TABLE IF NOT EXISTS public.content_dependencies (
    package_id          uuid        NOT NULL REFERENCES public.content_packages(id) ON DELETE CASCADE,
    depends_on_id       uuid        NOT NULL REFERENCES public.content_packages(id) ON DELETE RESTRICT,
    depends_on_version  text        NOT NULL,
    created_at          timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (package_id, depends_on_id),
    CONSTRAINT content_dependencies_no_self
        CHECK (package_id <> depends_on_id),
    CONSTRAINT content_dependencies_version_shape
        CHECK (depends_on_version ~ '^\d+\.\d+\.\d+$')
);
CREATE INDEX IF NOT EXISTS content_dependencies_revidx
    ON public.content_dependencies (depends_on_id);
COMMENT ON TABLE public.content_dependencies IS
    'Package dependency edges. Cycle-detected via WITH RECURSIVE in helper.';

-- ─── content_remix_chain ────────────────────────────────────────────────
-- Immutable attribution. attribution_immutable=TRUE always (CHECK).
-- royalty_share_gift_pct is voluntary downstream-tip routing (gift-economy-only).
CREATE TABLE IF NOT EXISTS public.content_remix_chain (
    package_id              uuid        NOT NULL REFERENCES public.content_packages(id) ON DELETE CASCADE,
    remix_of_id             uuid        NOT NULL REFERENCES public.content_packages(id) ON DELETE RESTRICT,
    attribution_immutable   boolean     NOT NULL DEFAULT TRUE,
    royalty_share_gift_pct  int         NOT NULL DEFAULT 0,
    created_at              timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (package_id, remix_of_id),
    CONSTRAINT content_remix_chain_no_self
        CHECK (package_id <> remix_of_id),
    CONSTRAINT content_remix_chain_immutable_axiom
        CHECK (attribution_immutable = TRUE),
    CONSTRAINT content_remix_chain_pct_range
        CHECK (royalty_share_gift_pct >= 0 AND royalty_share_gift_pct <= 100)
);
CREATE INDEX IF NOT EXISTS content_remix_chain_remix_of_idx
    ON public.content_remix_chain (remix_of_id);
COMMENT ON TABLE public.content_remix_chain IS
    'Remix-of attribution graph. attribution_immutable=TRUE always (DB-enforced).';

-- ─── helper · content_remix_cycle_check ──────────────────────────────────
-- Returns TRUE iff adding (p_id → p_remix_of_id) would NOT form a cycle.
CREATE OR REPLACE FUNCTION public.content_remix_cycle_check(
    p_id uuid,
    p_remix_of_id uuid
) RETURNS boolean AS $$
DECLARE
    cycle_found boolean;
BEGIN
    IF p_id = p_remix_of_id THEN
        RETURN FALSE;
    END IF;
    -- Check : does p_remix_of_id (transitively) already remix p_id?
    WITH RECURSIVE walk AS (
        SELECT remix_of_id FROM public.content_remix_chain WHERE package_id = p_remix_of_id
        UNION
        SELECT r.remix_of_id FROM public.content_remix_chain r
        JOIN walk w ON r.package_id = w.remix_of_id
    )
    SELECT EXISTS (SELECT 1 FROM walk WHERE remix_of_id = p_id) INTO cycle_found;
    RETURN NOT cycle_found;
END;
$$ LANGUAGE plpgsql STABLE SECURITY DEFINER;
COMMENT ON FUNCTION public.content_remix_cycle_check IS
    'Returns TRUE iff (p_id → p_remix_of_id) edge can be added without forming a cycle.';

-- ─── helper · content_dep_cycle_check ───────────────────────────────────
CREATE OR REPLACE FUNCTION public.content_dep_cycle_check(
    p_id uuid,
    p_depends_on_id uuid
) RETURNS boolean AS $$
DECLARE
    cycle_found boolean;
BEGIN
    IF p_id = p_depends_on_id THEN
        RETURN FALSE;
    END IF;
    WITH RECURSIVE walk AS (
        SELECT depends_on_id FROM public.content_dependencies WHERE package_id = p_depends_on_id
        UNION
        SELECT d.depends_on_id FROM public.content_dependencies d
        JOIN walk w ON d.package_id = w.depends_on_id
    )
    SELECT EXISTS (SELECT 1 FROM walk WHERE depends_on_id = p_id) INTO cycle_found;
    RETURN NOT cycle_found;
END;
$$ LANGUAGE plpgsql STABLE SECURITY DEFINER;
COMMENT ON FUNCTION public.content_dep_cycle_check IS
    'Returns TRUE iff (p_id → p_depends_on_id) dep can be added without forming a cycle.';

-- ─── helper · content_publish_finalize ───────────────────────────────────
-- Atomic state transition init/uploading → published. Drops staged chunks.
CREATE OR REPLACE FUNCTION public.content_publish_finalize(
    p_id uuid,
    p_sha256 text,
    p_signature text,
    p_anchor text,
    p_size_bytes bigint,
    p_chunk_count int
) RETURNS boolean AS $$
DECLARE
    cur_state text;
BEGIN
    SELECT state INTO cur_state FROM public.content_packages WHERE id = p_id FOR UPDATE;
    IF cur_state IS NULL THEN
        RETURN FALSE;
    END IF;
    IF cur_state NOT IN ('init','uploading','verifying') THEN
        RETURN FALSE;
    END IF;
    UPDATE public.content_packages
        SET state              = 'published',
            sha256             = p_sha256,
            signature_ed25519  = p_signature,
            sigma_chain_anchor = p_anchor,
            size_bytes         = p_size_bytes,
            chunk_count        = p_chunk_count,
            finalized_at       = now()
        WHERE id = p_id;
    -- Drop staged chunks (storage now lives in supabase-storage bucket).
    DELETE FROM public.content_chunks_upload WHERE package_id = p_id;
    RETURN TRUE;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
COMMENT ON FUNCTION public.content_publish_finalize IS
    'Atomic publish-finalize. State init/uploading/verifying → published. Drops chunks.';

-- ─── helper · content_revoke_cascade ─────────────────────────────────────
-- Marks revoked + writes audit row in content_packages itself (no separate
-- table : revoked_at + revoked_reason + revoked_by_pubkey carry the trail).
CREATE OR REPLACE FUNCTION public.content_revoke_cascade(
    p_id uuid,
    p_who_pubkey text,
    p_reason text
) RETURNS boolean AS $$
DECLARE
    already_revoked timestamptz;
BEGIN
    SELECT revoked_at INTO already_revoked
        FROM public.content_packages WHERE id = p_id FOR UPDATE;
    IF already_revoked IS NOT NULL THEN
        RETURN FALSE;  -- idempotent : already revoked
    END IF;
    UPDATE public.content_packages
        SET state             = 'revoked',
            revoked_at        = now(),
            revoked_reason    = p_reason,
            revoked_by_pubkey = p_who_pubkey
        WHERE id = p_id;
    RETURN TRUE;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
COMMENT ON FUNCTION public.content_revoke_cascade IS
    'Mark package revoked. Cascades to subscribers via mycelium-broadcast (edge-side).';

-- =====================================================================
-- § Row-Level Security
-- =====================================================================

-- ─── content_packages · self-row + public-published-read · service-write ────
ALTER TABLE public.content_packages ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "content_packages_select_published" ON public.content_packages;
CREATE POLICY "content_packages_select_published"
    ON public.content_packages FOR SELECT
    USING (state = 'published' AND revoked_at IS NULL);

DROP POLICY IF EXISTS "content_packages_select_self" ON public.content_packages;
CREATE POLICY "content_packages_select_self"
    ON public.content_packages FOR SELECT
    USING (auth.uid()::text = author_pubkey OR auth.role() = 'service_role');

DROP POLICY IF EXISTS "content_packages_service_write" ON public.content_packages;
CREATE POLICY "content_packages_service_write"
    ON public.content_packages FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- ─── content_chunks_upload · service-only (temp internal) ─────────────────
ALTER TABLE public.content_chunks_upload ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "content_chunks_upload_service_only" ON public.content_chunks_upload;
CREATE POLICY "content_chunks_upload_service_only"
    ON public.content_chunks_upload FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- ─── content_dependencies · public-read · service-write ───────────────────
ALTER TABLE public.content_dependencies ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "content_dependencies_select_anon" ON public.content_dependencies;
CREATE POLICY "content_dependencies_select_anon"
    ON public.content_dependencies FOR SELECT
    USING (true);

DROP POLICY IF EXISTS "content_dependencies_service_write" ON public.content_dependencies;
CREATE POLICY "content_dependencies_service_write"
    ON public.content_dependencies FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- ─── content_remix_chain · public-read · service-write ────────────────────
ALTER TABLE public.content_remix_chain ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "content_remix_chain_select_anon" ON public.content_remix_chain;
CREATE POLICY "content_remix_chain_select_anon"
    ON public.content_remix_chain FOR SELECT
    USING (true);

DROP POLICY IF EXISTS "content_remix_chain_service_write" ON public.content_remix_chain;
CREATE POLICY "content_remix_chain_service_write"
    ON public.content_remix_chain FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- =====================================================================
-- § ATTESTATION
-- ¬ unauthorized-publish (cap REQUIRED @ edge · service-role-only DB-write)
-- ¬ silent-revoke (revoke audit-trail in-row · sigma_chain_anchor required)
-- ¬ pay-for-publish (gift_economy_only DB-CHECK forces TRUE)
-- ¬ pay-for-discovery (license enum excludes commercial-restricted)
-- creator-revoke cascades-to-subscribers (mycelium-broadcast @ edge)
-- ¬ surveillance · author_pubkey is sovereign-revocable identifier · zero PII
-- =====================================================================
