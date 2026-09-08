-- §C apocrypha contributor transport persistence
--
-- This schema is intentionally separate from every existing job, worker, chat,
-- and memory surface.  The application adapter requires a real database
-- transaction provider before it can be promoted; ordinary PostgREST calls
-- alone are not a transaction boundary.

CREATE TABLE IF NOT EXISTS public.apocrypha_contributor_node (
    node_id text PRIMARY KEY
        CHECK (node_id ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'),
    node_key_id text NOT NULL
        CHECK (node_key_id ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$'),
    node_public_key_spki_b64 text NOT NULL
        CHECK (
            length(node_public_key_spki_b64) BETWEEN 1 AND 512
            AND node_public_key_spki_b64 ~ '^[A-Za-z0-9+/]+={0,2}$'
        ),
    platform text NOT NULL
        CHECK (platform IN ('windows-x64', 'macos-arm64', 'linux-x64', 'android', 'ios')),
    capabilities text[] NOT NULL DEFAULT ARRAY['vector_dot']::text[]
        CHECK (capabilities = ARRAY['vector_dot']::text[]),
    status text NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'revoked')),
    revision bigint NOT NULL DEFAULT 1
        CHECK (revision BETWEEN 1 AND 1000000000),
    enrolled_at timestamptz NOT NULL,
    revoked_at timestamptz,
    revoke_reason text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (
        (status = 'active' AND revoked_at IS NULL AND revoke_reason IS NULL)
        OR (
            status = 'revoked'
            AND revoked_at IS NOT NULL
            AND revoke_reason IS NOT NULL
            AND length(revoke_reason) BETWEEN 1 AND 256
            AND revoke_reason !~ '[^ -~]'
        )
    )
);

CREATE TABLE IF NOT EXISTS public.apocrypha_contributor_enrollment_replay (
    request_id text PRIMARY KEY
        CHECK (request_id ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{7,127}$'),
    request_hash text NOT NULL
        CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    receipt jsonb NOT NULL
        CHECK (
            jsonb_typeof(receipt) = 'object'
            AND receipt->>'schema_version' = 'apocrypha.contributor.enrollment-receipt.v1'
            AND receipt ? 'signature_b64'
            AND pg_column_size(receipt) <= 65536
        ),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS public.apocrypha_contributor_revoke_replay (
    request_id text PRIMARY KEY
        CHECK (request_id ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{7,127}$'),
    request_hash text NOT NULL
        CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    receipt jsonb NOT NULL
        CHECK (
            jsonb_typeof(receipt) = 'object'
            AND receipt->>'schema_version' = 'apocrypha.contributor.revoke-receipt.v1'
            AND receipt ? 'signature_b64'
            AND pg_column_size(receipt) <= 65536
        ),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS public.apocrypha_contributor_lease_replay (
    node_id text NOT NULL
        REFERENCES public.apocrypha_contributor_node(node_id)
        ON UPDATE CASCADE
        ON DELETE RESTRICT,
    idempotency_key text NOT NULL
        CHECK (idempotency_key ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{7,199}$'),
    dispatch_id text PRIMARY KEY
        CHECK (dispatch_id ~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'),
    request_hash text NOT NULL
        CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    dispatch jsonb NOT NULL
        CHECK (
            jsonb_typeof(dispatch) = 'object'
            AND dispatch->>'schema_version' = 'apocrypha.contributor.lease-dispatch.v1'
            AND dispatch ? 'signature_b64'
            AND pg_column_size(dispatch) <= 131072
        ),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (node_id, idempotency_key)
);

CREATE TABLE IF NOT EXISTS public.apocrypha_contributor_result_replay (
    dispatch_id text PRIMARY KEY
        REFERENCES public.apocrypha_contributor_lease_replay(dispatch_id)
        ON UPDATE CASCADE
        ON DELETE RESTRICT,
    submission_hash text NOT NULL
        CHECK (submission_hash ~ '^[0-9a-f]{64}$'),
    receipt jsonb NOT NULL
        CHECK (
            jsonb_typeof(receipt) = 'object'
            AND receipt->>'schema_version' = 'apocrypha.contributor.result-receipt.v1'
            AND receipt ? 'signature_b64'
            AND pg_column_size(receipt) <= 65536
        ),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS apocrypha_contributor_node_status_idx
    ON public.apocrypha_contributor_node (status, updated_at);
CREATE INDEX IF NOT EXISTS apocrypha_contributor_lease_node_idx
    ON public.apocrypha_contributor_lease_replay (node_id, created_at);

CREATE OR REPLACE FUNCTION public.apocrypha_contributor_touch_updated_at()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS apocrypha_contributor_node_updated_at
    ON public.apocrypha_contributor_node;
CREATE TRIGGER apocrypha_contributor_node_updated_at
    BEFORE UPDATE ON public.apocrypha_contributor_node
    FOR EACH ROW
    EXECUTE FUNCTION public.apocrypha_contributor_touch_updated_at();

ALTER TABLE public.apocrypha_contributor_node ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_contributor_enrollment_replay ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_contributor_revoke_replay ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_contributor_lease_replay ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_contributor_result_replay ENABLE ROW LEVEL SECURITY;

REVOKE ALL ON TABLE
    public.apocrypha_contributor_node,
    public.apocrypha_contributor_enrollment_replay,
    public.apocrypha_contributor_revoke_replay,
    public.apocrypha_contributor_lease_replay,
    public.apocrypha_contributor_result_replay
FROM PUBLIC, anon, authenticated;

GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE
    public.apocrypha_contributor_node,
    public.apocrypha_contributor_enrollment_replay,
    public.apocrypha_contributor_revoke_replay,
    public.apocrypha_contributor_lease_replay,
    public.apocrypha_contributor_result_replay
TO service_role;

REVOKE ALL ON FUNCTION public.apocrypha_contributor_touch_updated_at() FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.apocrypha_contributor_touch_updated_at() TO service_role;

COMMENT ON TABLE public.apocrypha_contributor_node IS
    'Server-owned contributor node admission state; contains public keys only, never private keys or bearer tokens.';
COMMENT ON TABLE public.apocrypha_contributor_enrollment_replay IS
    'Bounded idempotency records for signed contributor enrollment receipts.';
COMMENT ON TABLE public.apocrypha_contributor_revoke_replay IS
    'Bounded idempotency records for operator-authorized contributor revocation receipts.';
COMMENT ON TABLE public.apocrypha_contributor_lease_replay IS
    'Bounded idempotency records for signed contributor lease dispatches.';
COMMENT ON TABLE public.apocrypha_contributor_result_replay IS
    'Bounded idempotency records for signed contributor result receipts.';
