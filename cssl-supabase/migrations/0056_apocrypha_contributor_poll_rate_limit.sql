-- §C public node-authenticated polling abuse control
--
-- 0056 widens the already service-role-only rate-limit scope to include the
-- node-signed lease polling endpoint.  No table privilege or caller authority
-- is widened; this is a constraint/function replacement only.

ALTER TABLE public.apocrypha_contributor_rate_limit_bucket
    DROP CONSTRAINT IF EXISTS apocrypha_contributor_rate_limit_bucket_scope_check;

ALTER TABLE public.apocrypha_contributor_rate_limit_bucket
    ADD CONSTRAINT apocrypha_contributor_rate_limit_bucket_scope_check
    CHECK (scope ~ '^apocrypha\.contributor\.(enroll|lease|poll|result|revoke)$');

CREATE OR REPLACE FUNCTION public.apocrypha_contributor_rate_limit_consume(
    p_scope text,
    p_key_digest text,
    p_limit integer,
    p_window_seconds integer
)
RETURNS jsonb
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_now timestamptz := clock_timestamp();
    v_window_start timestamptz;
    v_expires_at timestamptz;
    v_count integer;
    v_allowed boolean := false;
    v_retry_after integer;
BEGIN
    IF p_scope IS NULL
        OR p_scope !~ '^apocrypha\.contributor\.(enroll|lease|poll|result|revoke)$'
        OR p_key_digest IS NULL
        OR p_key_digest !~ '^[0-9a-f]{64}$'
        OR p_limit IS NULL
        OR p_limit NOT BETWEEN 1 AND 1000
        OR p_window_seconds IS NULL
        OR p_window_seconds NOT BETWEEN 1 AND 3600 THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'RATE_LIMIT_SCHEMA_INVALID';
    END IF;

    v_window_start := to_timestamp(
        floor(extract(epoch FROM v_now) / p_window_seconds) * p_window_seconds
    );
    v_expires_at := v_window_start + make_interval(secs => p_window_seconds);

    WITH stale AS (
        SELECT scope, key_digest, window_start
        FROM public.apocrypha_contributor_rate_limit_bucket
        WHERE expires_at < v_now
        ORDER BY expires_at
        LIMIT 256
    )
    DELETE FROM public.apocrypha_contributor_rate_limit_bucket AS bucket
    USING stale
    WHERE bucket.scope = stale.scope
      AND bucket.key_digest = stale.key_digest
      AND bucket.window_start = stale.window_start;

    INSERT INTO public.apocrypha_contributor_rate_limit_bucket (
        scope, key_digest, window_start, expires_at, request_count, updated_at
    ) VALUES (
        p_scope, p_key_digest, v_window_start, v_expires_at, 1, v_now
    )
    ON CONFLICT (scope, key_digest, window_start) DO UPDATE SET
        request_count = public.apocrypha_contributor_rate_limit_bucket.request_count + 1,
        updated_at = v_now
    WHERE public.apocrypha_contributor_rate_limit_bucket.request_count < p_limit
    RETURNING request_count INTO v_count;

    v_allowed := FOUND;
    v_retry_after := greatest(
        1,
        least(3600, ceil(extract(epoch FROM (v_expires_at - v_now)))::integer)
    );

    IF v_allowed THEN
        RETURN jsonb_build_object('allowed', true);
    END IF;
    RETURN jsonb_build_object('allowed', false, 'retry_after_seconds', v_retry_after);
END;
$$;

REVOKE ALL ON FUNCTION public.apocrypha_contributor_rate_limit_consume(text, text, integer, integer)
    FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.apocrypha_contributor_rate_limit_consume(text, text, integer, integer)
    TO service_role;

COMMENT ON FUNCTION public.apocrypha_contributor_rate_limit_consume(text, text, integer, integer) IS
    'Atomic service-role-only contributor rate-limit consume for enroll, lease, poll, result, and revoke scopes.';
