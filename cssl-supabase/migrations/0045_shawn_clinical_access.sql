-- =====================================================================
-- § SHAWN CLINICAL DOSSIER · fail-closed access boundary
-- =====================================================================
-- Contains authorization metadata only. Clinical content belongs in the
-- private storage object and is read only by the server-side service role.
-- No anon/authenticated table policy and no storage policy are created.
-- =====================================================================

CREATE TABLE IF NOT EXISTS public.shawn_clinical_allowlist (
    user_id     uuid PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE,
    email_snapshot text NOT NULL,
    expires_at  timestamptz NOT NULL,
    revoked_at  timestamptz,
    purpose     text NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT shawn_clinical_email_snapshot_normalized
        CHECK (
            email_snapshot = lower(btrim(email_snapshot))
            AND char_length(email_snapshot) BETWEEN 3 AND 320
            AND email_snapshot ~ '^[^[:space:]@]+@[^[:space:]@]+\.[^[:space:]@]+$'
        ),
    CONSTRAINT shawn_clinical_purpose_length
        CHECK (char_length(btrim(purpose)) BETWEEN 1 AND 500),
    CONSTRAINT shawn_clinical_expiry_after_grant
        CHECK (expires_at > created_at)
);

COMMENT ON TABLE public.shawn_clinical_allowlist IS
    'Authorization metadata for the private Shawn clinician dossier; never stores clinical content.';
COMMENT ON COLUMN public.shawn_clinical_allowlist.user_id IS
    'Immutable Supabase auth.users identifier used for authorization.';
COMMENT ON COLUMN public.shawn_clinical_allowlist.email_snapshot IS
    'Lowercase email snapshot for human audit only; never used as the grant key.';
COMMENT ON COLUMN public.shawn_clinical_allowlist.purpose IS
    'Human-readable reason access was granted; not returned by the dossier route.';
COMMENT ON COLUMN public.shawn_clinical_allowlist.expires_at IS
    'Required grant expiry; indefinite clinical access is intentionally unsupported.';

ALTER TABLE public.shawn_clinical_allowlist ENABLE ROW LEVEL SECURITY;

-- Defense in depth: clients have no privileges and RLS has no client policy.
REVOKE ALL ON TABLE public.shawn_clinical_allowlist FROM anon, authenticated;
GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE public.shawn_clinical_allowlist TO service_role;

DROP TRIGGER IF EXISTS shawn_clinical_allowlist_updated_at ON public.shawn_clinical_allowlist;
CREATE TRIGGER shawn_clinical_allowlist_updated_at
    BEFORE UPDATE ON public.shawn_clinical_allowlist
    FOR EACH ROW
    EXECUTE FUNCTION public.tg_set_updated_at();

-- Default bucket for SHAWN_CLINICAL_BUCKET. Object name is deployment config.
INSERT INTO storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
VALUES (
    'shawn-clinical',
    'shawn-clinical',
    false,
    2097152,
    ARRAY['application/json']
)
ON CONFLICT (id) DO UPDATE
    SET public = false,
        file_size_limit = EXCLUDED.file_size_limit,
        allowed_mime_types = EXCLUDED.allowed_mime_types;

-- Intentionally no storage.objects policy: anon and authenticated users have
-- no path to this bucket. The server's service_role bypasses storage RLS.
