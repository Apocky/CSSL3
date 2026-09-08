-- §C apocrypha contributor transport atomic RPC boundary
--
-- 0053 owns the dedicated tables.  This migration adds one SECURITY DEFINER
-- function per controller operation so each read/write/idempotency decision
-- executes inside one PostgreSQL transaction.  The application adapter must
-- call these functions with the server-only service-role client; ordinary
-- PostgREST table calls remain an explicit non-transactional/fail-closed path.

-- ─── enrollment: node admission + request replay ─────────────────────

CREATE OR REPLACE FUNCTION public.apocrypha_contributor_enroll_atomic(
    p_request_id text,
    p_request_hash text,
    p_node jsonb,
    p_receipt jsonb
)
RETURNS jsonb
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_existing public.apocrypha_contributor_node%ROWTYPE;
    v_replay public.apocrypha_contributor_enrollment_replay%ROWTYPE;
    v_node_id text;
    v_revision bigint;
    v_has_existing boolean := false;
BEGIN
    IF p_request_id IS NULL OR p_request_id !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{7,127}$'
        OR p_request_hash IS NULL OR p_request_hash !~ '^[0-9a-f]{64}$'
        OR p_node IS NULL OR jsonb_typeof(p_node) <> 'object'
        OR p_receipt IS NULL OR jsonb_typeof(p_receipt) <> 'object' THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_SCHEMA_INVALID';
    END IF;

    -- Exact object shape prevents a future caller from smuggling authority or
    -- private material into the durable transport row.
    IF (SELECT count(*) FROM jsonb_object_keys(p_node)) <> 10
        OR EXISTS (
            SELECT 1 FROM jsonb_object_keys(p_node) AS k(key)
            WHERE k.key NOT IN (
                'node_id', 'node_key_id', 'node_public_key_spki_b64', 'platform',
                'capabilities', 'status', 'revision', 'enrolled_at',
                'revoked_at', 'revoke_reason'
            )
        )
        OR (SELECT count(*) FROM jsonb_object_keys(p_receipt)) <> 12
        OR EXISTS (
            SELECT 1 FROM jsonb_object_keys(p_receipt) AS k(key)
            WHERE k.key NOT IN (
                'schema_version', 'request_id', 'enrollment_id', 'node_id',
                'node_key_id', 'controller_key_id', 'status', 'revision',
                'request_hash', 'issued_at', 'expires_at', 'signature_b64'
            )
        ) THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_SCHEMA_INVALID';
    END IF;

    v_node_id := p_node->>'node_id';
    IF v_node_id IS NULL OR v_node_id !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'
        OR p_node->>'node_key_id' IS NULL
        OR p_node->>'node_key_id' !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$'
        OR p_node->>'node_public_key_spki_b64' IS NULL
        OR length(p_node->>'node_public_key_spki_b64') NOT BETWEEN 1 AND 512
        OR p_node->>'node_public_key_spki_b64' !~ '^[A-Za-z0-9+/]+={0,2}$'
        OR p_node->>'platform' NOT IN ('windows-x64', 'macos-arm64', 'linux-x64', 'android', 'ios')
        OR p_node->>'status' <> 'active'
        OR (p_node->'capabilities') <> '["vector_dot"]'::jsonb
        OR (p_node->>'revision') !~ '^[0-9]+$'
        OR (p_node->>'revision')::bigint NOT BETWEEN 1 AND 1000000000
        OR p_node->>'revoked_at' IS NOT NULL
        OR p_node->>'revoke_reason' IS NOT NULL
        OR p_node->>'enrolled_at' IS NULL
        OR p_node->>'enrolled_at' !~ '^[0-9]{4}-[0-9]{2}-[0-9]{2}T'
        OR (p_receipt->>'schema_version') <> 'apocrypha.contributor.enrollment-receipt.v1'
        OR (p_receipt->>'request_id') <> p_request_id
        OR (p_receipt->>'node_id') <> v_node_id
        OR (p_receipt->>'node_key_id') <> p_node->>'node_key_id'
        OR (p_receipt->>'request_hash') <> p_request_hash
        OR (p_receipt->>'status') <> 'active'
        OR (p_receipt->>'controller_key_id') IS NULL
        OR (p_receipt->>'controller_key_id') !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$'
        OR (p_receipt->>'enrollment_id') IS NULL
        OR (p_receipt->>'enrollment_id') !~ '^enr-[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'
        OR (p_receipt->>'issued_at') !~ '^[0-9]+$'
        OR (p_receipt->>'expires_at') !~ '^[0-9]+$'
        OR (p_receipt->>'issued_at')::bigint < 0
        OR (p_receipt->>'expires_at')::bigint <= (p_receipt->>'issued_at')::bigint
        OR (p_receipt->>'expires_at')::bigint - (p_receipt->>'issued_at')::bigint > 300000
        OR (p_receipt->>'revision') !~ '^[0-9]+$'
        OR (p_receipt->>'revision')::bigint NOT BETWEEN 1 AND 1000000000
        OR (p_receipt->>'signature_b64') IS NULL
        OR (p_receipt->>'signature_b64') !~ '^[A-Za-z0-9_-]{86}$'
        OR pg_column_size(p_node) > 65536
        OR pg_column_size(p_receipt) > 65536 THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_SCHEMA_INVALID';
    END IF;

    -- The replay row is the first lock.  A retry therefore returns the exact
    -- original signed receipt or fails closed on request-id/hash conflict.
    SELECT * INTO v_replay
    FROM public.apocrypha_contributor_enrollment_replay
    WHERE request_id = p_request_id
    FOR UPDATE;
    IF FOUND THEN
        IF v_replay.request_hash <> p_request_hash THEN
            RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_ENROLLMENT_REPLAY';
        END IF;
        RETURN v_replay.receipt;
    END IF;

    SELECT * INTO v_existing
    FROM public.apocrypha_contributor_node
    WHERE node_id = v_node_id
    FOR UPDATE;
    v_has_existing := FOUND;
    IF v_has_existing AND v_existing.status = 'revoked' THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_NODE_REVOKED';
    END IF;
    IF v_has_existing AND (
        v_existing.node_key_id <> p_node->>'node_key_id'
        OR v_existing.node_public_key_spki_b64 <> p_node->>'node_public_key_spki_b64'
    ) THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_NODE_KEY_MISMATCH';
    END IF;

    v_revision := coalesce(v_existing.revision, 0) + 1;
    IF (p_node->>'revision')::bigint <> v_revision
        OR (p_receipt->>'revision')::bigint <> v_revision THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_SCHEMA_INVALID';
    END IF;

    INSERT INTO public.apocrypha_contributor_node (
        node_id, node_key_id, node_public_key_spki_b64, platform, capabilities,
        status, revision, enrolled_at, revoked_at, revoke_reason
    ) VALUES (
        v_node_id,
        p_node->>'node_key_id',
        p_node->>'node_public_key_spki_b64',
        p_node->>'platform',
        ARRAY['vector_dot']::text[],
        'active',
        v_revision,
        (p_node->>'enrolled_at')::timestamptz,
        NULL,
        NULL
    )
    ON CONFLICT (node_id) DO UPDATE SET
        node_key_id = EXCLUDED.node_key_id,
        node_public_key_spki_b64 = EXCLUDED.node_public_key_spki_b64,
        platform = EXCLUDED.platform,
        capabilities = EXCLUDED.capabilities,
        status = EXCLUDED.status,
        revision = EXCLUDED.revision,
        revoked_at = NULL,
        revoke_reason = NULL;

    INSERT INTO public.apocrypha_contributor_enrollment_replay (
        request_id, request_hash, receipt
    ) VALUES (
        p_request_id, p_request_hash, p_receipt
    );

    RETURN p_receipt;
END;
$$;

-- ─── lease: node admission + idempotent dispatch ─────────────────────

CREATE OR REPLACE FUNCTION public.apocrypha_contributor_issue_lease_atomic(
    p_node_id text,
    p_idempotency_key text,
    p_request_hash text,
    p_dispatch jsonb
)
RETURNS jsonb
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_node public.apocrypha_contributor_node%ROWTYPE;
    v_replay public.apocrypha_contributor_lease_replay%ROWTYPE;
    v_dispatch public.apocrypha_contributor_lease_replay%ROWTYPE;
    v_lease jsonb;
BEGIN
    IF p_node_id IS NULL OR p_node_id !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'
        OR p_idempotency_key IS NULL OR p_idempotency_key !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{7,199}$'
        OR p_request_hash IS NULL OR p_request_hash !~ '^[0-9a-f]{64}$'
        OR p_dispatch IS NULL OR jsonb_typeof(p_dispatch) <> 'object'
        OR pg_column_size(p_dispatch) > 131072
        OR (SELECT count(*) FROM jsonb_object_keys(p_dispatch)) <> 7
        OR EXISTS (
            SELECT 1 FROM jsonb_object_keys(p_dispatch) AS k(key)
            WHERE k.key NOT IN (
                'schema_version', 'dispatch_id', 'request_id', 'idempotency_key',
                'node_id', 'lease', 'signature_b64'
            )
        )
        OR (p_dispatch->>'schema_version') <> 'apocrypha.contributor.lease-dispatch.v1'
        OR (p_dispatch->>'dispatch_id') !~ '^dispatch-[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'
        OR (p_dispatch->>'dispatch_id') <> 'dispatch-' || left(p_request_hash, 48)
        OR (p_dispatch->>'request_id') !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{7,127}$'
        OR (p_dispatch->>'idempotency_key') <> p_idempotency_key
        OR (p_dispatch->>'node_id') <> p_node_id
        OR (p_dispatch->>'signature_b64') !~ '^[A-Za-z0-9_-]{86}$'
        OR jsonb_typeof(p_dispatch->'lease') <> 'object' THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_LEASE_INVALID';
    END IF;

    v_lease := p_dispatch->'lease';
    IF (SELECT count(*) FROM jsonb_object_keys(v_lease)) <> 9
        OR EXISTS (
            SELECT 1 FROM jsonb_object_keys(v_lease) AS k(key)
            WHERE k.key NOT IN (
                'schema_version', 'key_id', 'lease_id', 'node_id',
                'issued_at', 'expires_at', 'attempt', 'task', 'signature_b64'
            )
        )
        OR (v_lease->>'schema_version') <> 'apocrypha.contributor.lease.v1'
        OR (v_lease->>'key_id') IS NULL
        OR (v_lease->>'key_id') !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$'
        OR (v_lease->>'lease_id') <> 'lease-' || left(p_request_hash, 56)
        OR (v_lease->>'node_id') <> p_node_id
        OR (v_lease->>'issued_at') !~ '^[0-9]+$'
        OR (v_lease->>'expires_at') !~ '^[0-9]+$'
        OR (v_lease->>'issued_at')::bigint < 0
        OR (v_lease->>'expires_at')::bigint <= (v_lease->>'issued_at')::bigint
        OR (v_lease->>'expires_at')::bigint - (v_lease->>'issued_at')::bigint > 300000
        OR (v_lease->>'attempt') !~ '^[0-9]+$'
        OR (v_lease->>'attempt')::bigint NOT BETWEEN 0 AND 1000000
        OR jsonb_typeof(v_lease->'task') <> 'object'
        OR (v_lease->>'signature_b64') !~ '^[A-Za-z0-9_-]{86}$' THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_LEASE_INVALID';
    END IF;

    SELECT * INTO v_node
    FROM public.apocrypha_contributor_node
    WHERE node_id = p_node_id
    FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_NODE_NOT_ENROLLED';
    END IF;
    IF v_node.status = 'revoked' THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_NODE_REVOKED';
    END IF;
    IF NOT (v_node.capabilities = ARRAY['vector_dot']::text[]) THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_LEASE_INVALID';
    END IF;

    SELECT * INTO v_replay
    FROM public.apocrypha_contributor_lease_replay
    WHERE node_id = p_node_id AND idempotency_key = p_idempotency_key
    FOR UPDATE;
    IF FOUND THEN
        IF v_replay.request_hash <> p_request_hash THEN
            RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_IDEMPOTENCY_CONFLICT';
        END IF;
        RETURN v_replay.dispatch;
    END IF;

    SELECT * INTO v_dispatch
    FROM public.apocrypha_contributor_lease_replay
    WHERE dispatch_id = p_dispatch->>'dispatch_id'
    FOR UPDATE;
    IF FOUND THEN
        IF v_dispatch.request_hash <> p_request_hash
            OR v_dispatch.node_id <> p_node_id
            OR v_dispatch.idempotency_key <> p_idempotency_key THEN
            RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_IDEMPOTENCY_CONFLICT';
        END IF;
        RETURN v_dispatch.dispatch;
    END IF;

    INSERT INTO public.apocrypha_contributor_lease_replay (
        node_id, idempotency_key, dispatch_id, request_hash, dispatch
    ) VALUES (
        p_node_id, p_idempotency_key, p_dispatch->>'dispatch_id', p_request_hash, p_dispatch
    );

    RETURN p_dispatch;
END;
$$;

-- ─── result: lease binding + result replay ───────────────────────────

CREATE OR REPLACE FUNCTION public.apocrypha_contributor_accept_result_atomic(
    p_node_id text,
    p_dispatch_id text,
    p_submission_hash text,
    p_receipt jsonb
)
RETURNS jsonb
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_node public.apocrypha_contributor_node%ROWTYPE;
    v_lease public.apocrypha_contributor_lease_replay%ROWTYPE;
    v_replay public.apocrypha_contributor_result_replay%ROWTYPE;
    v_dispatch jsonb;
    v_lease_payload jsonb;
BEGIN
    IF p_node_id IS NULL OR p_node_id !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'
        OR p_dispatch_id IS NULL OR p_dispatch_id !~ '^dispatch-[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'
        OR p_submission_hash IS NULL OR p_submission_hash !~ '^[0-9a-f]{64}$'
        OR p_receipt IS NULL OR jsonb_typeof(p_receipt) <> 'object'
        OR pg_column_size(p_receipt) > 65536
        OR (SELECT count(*) FROM jsonb_object_keys(p_receipt)) <> 10
        OR EXISTS (
            SELECT 1 FROM jsonb_object_keys(p_receipt) AS k(key)
            WHERE k.key NOT IN (
                'schema_version', 'dispatch_id', 'request_id', 'idempotency_key',
                'node_id', 'lease_id', 'result_hash', 'status', 'accepted_at',
                'signature_b64'
            )
        )
        OR (p_receipt->>'schema_version') <> 'apocrypha.contributor.result-receipt.v1'
        OR (p_receipt->>'dispatch_id') <> p_dispatch_id
        OR (p_receipt->>'node_id') <> p_node_id
        OR (p_receipt->>'result_hash') !~ '^[0-9a-f]{64}$'
        OR (p_receipt->>'status') <> 'accepted'
        OR (p_receipt->>'accepted_at') !~ '^[0-9]+$'
        OR (p_receipt->>'accepted_at')::bigint < 0
        OR (p_receipt->>'signature_b64') !~ '^[A-Za-z0-9_-]{86}$' THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_RESULT_INVALID';
    END IF;

    -- Lock admission state before replay lookup, matching the controller's
    -- revoked-node rule: a revoked node cannot create or replay a result.
    SELECT * INTO v_node
    FROM public.apocrypha_contributor_node
    WHERE node_id = p_node_id
    FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_NODE_NOT_ENROLLED';
    END IF;
    IF v_node.status = 'revoked' THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_NODE_REVOKED';
    END IF;

    SELECT * INTO v_lease
    FROM public.apocrypha_contributor_lease_replay
    WHERE dispatch_id = p_dispatch_id
    FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_LEASE_UNKNOWN';
    END IF;
    v_dispatch := v_lease.dispatch;
    v_lease_payload := v_dispatch->'lease';
    IF (v_dispatch->>'node_id') <> p_node_id
        OR (p_receipt->>'request_id') <> v_dispatch->>'request_id'
        OR (p_receipt->>'idempotency_key') <> v_dispatch->>'idempotency_key'
        OR (p_receipt->>'lease_id') <> v_lease_payload->>'lease_id' THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_RESULT_INVALID';
    END IF;

    SELECT * INTO v_replay
    FROM public.apocrypha_contributor_result_replay
    WHERE dispatch_id = p_dispatch_id
    FOR UPDATE;
    IF FOUND THEN
        IF v_replay.submission_hash <> p_submission_hash THEN
            RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_RESULT_REPLAY';
        END IF;
        RETURN v_replay.receipt;
    END IF;

    INSERT INTO public.apocrypha_contributor_result_replay (
        dispatch_id, submission_hash, receipt
    ) VALUES (
        p_dispatch_id, p_submission_hash, p_receipt
    );

    RETURN p_receipt;
END;
$$;

-- ─── revoke: operator-authorized state transition + replay ───────────

CREATE OR REPLACE FUNCTION public.apocrypha_contributor_revoke_atomic(
    p_request_id text,
    p_node_id text,
    p_request_hash text,
    p_reason text,
    p_receipt jsonb
)
RETURNS jsonb
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_node public.apocrypha_contributor_node%ROWTYPE;
    v_replay public.apocrypha_contributor_revoke_replay%ROWTYPE;
    v_already_revoked boolean;
    v_expected_revision bigint;
BEGIN
    IF p_request_id IS NULL OR p_request_id !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{7,127}$'
        OR p_node_id IS NULL OR p_node_id !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$'
        OR p_request_hash IS NULL OR p_request_hash !~ '^[0-9a-f]{64}$'
        OR p_reason IS NULL OR length(p_reason) NOT BETWEEN 1 AND 256
        OR p_reason !~ '^[ -~]+$'
        OR p_receipt IS NULL OR jsonb_typeof(p_receipt) <> 'object'
        OR pg_column_size(p_receipt) > 65536
        OR (SELECT count(*) FROM jsonb_object_keys(p_receipt)) <> 9
        OR EXISTS (
            SELECT 1 FROM jsonb_object_keys(p_receipt) AS k(key)
            WHERE k.key NOT IN (
                'schema_version', 'request_id', 'node_id', 'controller_key_id',
                'status', 'revision', 'reason', 'revoked_at', 'signature_b64'
            )
        )
        OR (p_receipt->>'schema_version') <> 'apocrypha.contributor.revoke-receipt.v1'
        OR (p_receipt->>'request_id') <> p_request_id
        OR (p_receipt->>'node_id') <> p_node_id
        OR (p_receipt->>'controller_key_id') IS NULL
        OR (p_receipt->>'controller_key_id') !~ '^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$'
        OR (p_receipt->>'status') NOT IN ('revoked', 'already_revoked')
        OR (p_receipt->>'revision') !~ '^[0-9]+$'
        OR (p_receipt->>'revision')::bigint NOT BETWEEN 1 AND 1000000000
        OR (p_receipt->>'reason') <> p_reason
        OR (p_receipt->>'revoked_at') !~ '^[0-9]+$'
        OR (p_receipt->>'revoked_at')::bigint < 0
        OR (p_receipt->>'signature_b64') !~ '^[A-Za-z0-9_-]{86}$' THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_SCHEMA_INVALID';
    END IF;

    SELECT * INTO v_replay
    FROM public.apocrypha_contributor_revoke_replay
    WHERE request_id = p_request_id
    FOR UPDATE;
    IF FOUND THEN
        IF v_replay.request_hash <> p_request_hash THEN
            RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_IDEMPOTENCY_CONFLICT';
        END IF;
        RETURN v_replay.receipt;
    END IF;

    SELECT * INTO v_node
    FROM public.apocrypha_contributor_node
    WHERE node_id = p_node_id
    FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_NODE_NOT_ENROLLED';
    END IF;

    v_already_revoked := v_node.status = 'revoked';
    v_expected_revision := v_node.revision + CASE WHEN v_already_revoked THEN 0 ELSE 1 END;
    IF (p_receipt->>'revision')::bigint <> v_expected_revision
        OR ((v_already_revoked AND (p_receipt->>'status') <> 'already_revoked')
            OR (NOT v_already_revoked AND (p_receipt->>'status') <> 'revoked')) THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'TRANSPORT_SCHEMA_INVALID';
    END IF;

    IF NOT v_already_revoked THEN
        UPDATE public.apocrypha_contributor_node
        SET status = 'revoked',
            revision = v_expected_revision,
            revoked_at = to_timestamp((p_receipt->>'revoked_at')::double precision / 1000.0),
            revoke_reason = p_reason
        WHERE node_id = p_node_id;
    END IF;

    INSERT INTO public.apocrypha_contributor_revoke_replay (
        request_id, request_hash, receipt
    ) VALUES (
        p_request_id, p_request_hash, p_receipt
    );

    RETURN p_receipt;
END;
$$;

-- RPCs are the only permitted execution surface.  Table RLS/grants remain
-- defense-in-depth; no anon/authenticated caller can invoke these functions.
REVOKE EXECUTE ON FUNCTION public.apocrypha_contributor_enroll_atomic(text, text, jsonb, jsonb)
    FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_contributor_issue_lease_atomic(text, text, text, jsonb)
    FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_contributor_accept_result_atomic(text, text, text, jsonb)
    FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_contributor_revoke_atomic(text, text, text, text, jsonb)
    FROM PUBLIC, anon, authenticated;

GRANT EXECUTE ON FUNCTION public.apocrypha_contributor_enroll_atomic(text, text, jsonb, jsonb)
    TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_contributor_issue_lease_atomic(text, text, text, jsonb)
    TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_contributor_accept_result_atomic(text, text, text, jsonb)
    TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_contributor_revoke_atomic(text, text, text, text, jsonb)
    TO service_role;

COMMENT ON FUNCTION public.apocrypha_contributor_enroll_atomic(text, text, jsonb, jsonb) IS
    'Atomic contributor enrollment: locks request replay and node, enforces key/revision invariants, writes node plus signed receipt replay in one transaction.';
COMMENT ON FUNCTION public.apocrypha_contributor_issue_lease_atomic(text, text, text, jsonb) IS
    'Atomic contributor lease issue: locks active node and idempotency key, rejects conflicting dispatches, writes one signed lease replay.';
COMMENT ON FUNCTION public.apocrypha_contributor_accept_result_atomic(text, text, text, jsonb) IS
    'Atomic contributor result acceptance: locks active node and lease, binds receipt identity, and commits one result replay.';
COMMENT ON FUNCTION public.apocrypha_contributor_revoke_atomic(text, text, text, text, jsonb) IS
    'Atomic contributor revocation: locks request replay and node, applies monotonic revocation, and commits operator receipt replay.';
