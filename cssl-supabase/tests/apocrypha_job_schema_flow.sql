-- Transactional lifecycle test for 0046_apocrypha_jobs.sql.
-- Requires the local bootstrap (or equivalent Supabase auth schema). All rows
-- are rolled back. Run with psql -v ON_ERROR_STOP=1.

BEGIN;

DO $flow$
DECLARE
    v_user_id        uuid := '11111111-1111-4111-8111-111111111111';
    v_tenant_id      uuid;
    v_principal_id   uuid;
    v_node_id        uuid;
    v_node_token     text;
    v_token_version  integer;
    v_job            public.apocrypha_job;
    v_replay_job     public.apocrypha_job;
    v_attempt_id     uuid;
    v_attempt_no     integer;
    v_lease_epoch    bigint;
    v_lease_token    text;
    v_replay_token   text;
    v_lease_expiry   timestamptz;
    v_cancel_flag    boolean;
    v_chunk_id       bigint;
    v_revision       public.apocrypha_job_revision;
    v_revision_replay public.apocrypha_job_revision;
    v_count          integer;
    v_outbox_id      uuid;
    v_outbox         public.apocrypha_alert_outbox;
BEGIN
    INSERT INTO auth.users (id, email)
    VALUES (v_user_id, 'schema-flow@example.invalid');

    SELECT e.tenant_id, e.principal_id
    INTO v_tenant_id, v_principal_id
    FROM public.apocrypha_ensure_owner_principal(
        'apocky-test', 'Apocky schema test', v_user_id
    ) AS e;

    SELECT n.node_id, n.node_token, n.token_version
    INTO v_node_id, v_node_token, v_token_version
    FROM public.apocrypha_issue_worker_token(
        'a770-schema-test',
        'A770 schema test',
        ARRAY['apocky_owner_chat'],
        v_tenant_id,
        1::smallint,
        jsonb_build_object('model_alias', 'qwen35-35b-a3b-q4'),
        '{}'::jsonb
    ) AS n;

    v_job := public.apocrypha_enqueue_job(
        v_tenant_id,
        v_principal_id,
        'chat',
        'apocky_owner_chat',
        jsonb_build_object('prompt', 'Give a durable answer.'),
        public.apocrypha_sha256('request-1'),
        'apocky.com:owner:chat',
        'enqueue-key-0001',
        'qwen35-35b-a3b-q4',
        repeat('a', 64),
        'tools-v1',
        repeat('b', 64)
    );

    v_replay_job := public.apocrypha_enqueue_job(
        v_tenant_id,
        v_principal_id,
        'chat',
        'apocky_owner_chat',
        jsonb_build_object('prompt', 'Give a durable answer.'),
        public.apocrypha_sha256('request-1'),
        'apocky.com:owner:chat',
        'enqueue-key-0001',
        'qwen35-35b-a3b-q4',
        repeat('a', 64),
        'tools-v1',
        repeat('b', 64)
    );
    IF v_replay_job.id <> v_job.id THEN
        RAISE EXCEPTION 'enqueue idempotency returned a different job';
    END IF;

    BEGIN
        PERFORM public.apocrypha_enqueue_job(
            v_tenant_id, v_principal_id, 'chat', 'apocky_owner_chat',
            jsonb_build_object('prompt', 'Different request.'),
            public.apocrypha_sha256('request-different'),
            'apocky.com:owner:chat', 'enqueue-key-0001',
            'qwen35-35b-a3b-q4', repeat('a', 64), 'tools-v1', repeat('b', 64)
        );
        RAISE EXCEPTION 'mismatched idempotency replay was accepted';
    EXCEPTION WHEN unique_violation THEN
        NULL;
    END;

    SELECT c.attempt_id, c.attempt_no, c.lease_epoch, c.lease_token, c.lease_expires_at
    INTO v_attempt_id, v_attempt_no, v_lease_epoch, v_lease_token, v_lease_expiry
    FROM public.apocrypha_claim_job(
        v_node_id, v_node_token, 'claim-key-0001', 180
    ) AS c;

    IF v_attempt_id IS NULL OR v_lease_token IS NULL THEN
        RAISE EXCEPTION 'claim did not return a fence';
    END IF;

    SELECT c.lease_token INTO v_replay_token
    FROM public.apocrypha_claim_job(
        v_node_id, v_node_token, 'claim-key-0001', 180
    ) AS c;
    IF v_replay_token IS DISTINCT FROM v_lease_token THEN
        RAISE EXCEPTION 'claim replay did not return the exact lease token';
    END IF;

    SELECT r.lease_expires_at, r.cancel_requested
    INTO v_lease_expiry, v_cancel_flag
    FROM public.apocrypha_renew_lease(
        v_node_id, v_node_token, v_job.id, v_attempt_id,
        v_lease_epoch, v_lease_token, 180
    ) AS r;
    IF v_cancel_flag THEN
        RAISE EXCEPTION 'healthy lease incorrectly reported cancellation';
    END IF;

    v_chunk_id := public.apocrypha_append_chunk(
        v_node_id, v_node_token, v_job.id, v_attempt_id,
        v_lease_epoch, v_lease_token, 0, 'section', 'The durable answer.',
        jsonb_build_object('section', 1), 0, 'The durable answer.',
        jsonb_build_object('next_section', 2)
    );
    IF v_chunk_id IS NULL THEN
        RAISE EXCEPTION 'chunk append did not return an id';
    END IF;

    -- Same sequence and content is an idempotent replay.
    IF public.apocrypha_append_chunk(
        v_node_id, v_node_token, v_job.id, v_attempt_id,
        v_lease_epoch, v_lease_token, 0, 'section', 'The durable answer.',
        jsonb_build_object('section', 1), 0, 'The durable answer.',
        jsonb_build_object('next_section', 2)
    ) <> v_chunk_id THEN
        RAISE EXCEPTION 'chunk replay returned a different id';
    END IF;

    v_revision := public.apocrypha_complete_job(
        v_node_id, v_node_token, v_job.id, v_attempt_id,
        v_lease_epoch, v_lease_token, 'The durable answer.', 'primary',
        jsonb_build_object('source', 'schema-flow'),
        jsonb_build_object('output_tokens', 4)
    );

    v_revision_replay := public.apocrypha_complete_job(
        v_node_id, v_node_token, v_job.id, v_attempt_id,
        v_lease_epoch, v_lease_token, 'The durable answer.', 'primary',
        jsonb_build_object('source', 'schema-flow'),
        jsonb_build_object('output_tokens', 4)
    );
    IF v_revision_replay.id <> v_revision.id THEN
        RAISE EXCEPTION 'completion replay returned a different revision';
    END IF;

    BEGIN
        UPDATE public.apocrypha_job_revision
        SET content = 'mutated'
        WHERE id = v_revision.id;
        RAISE EXCEPTION 'immutable revision update was accepted';
    EXCEPTION WHEN SQLSTATE '55000' THEN
        NULL;
    END;

    -- A terminal failure must create alertable events and outbox rows.
    v_job := public.apocrypha_enqueue_job(
        v_tenant_id, v_principal_id, 'chat', 'apocky_owner_chat',
        jsonb_build_object('prompt', 'Exercise failure.'),
        public.apocrypha_sha256('request-2'),
        'apocky.com:owner:chat', 'enqueue-key-0002',
        'qwen35-35b-a3b-q4', repeat('a', 64), 'tools-v1', repeat('b', 64),
        0::smallint, 1::smallint
    );
    SELECT c.attempt_id, c.lease_epoch, c.lease_token
    INTO v_attempt_id, v_lease_epoch, v_lease_token
    FROM public.apocrypha_claim_job(
        v_node_id, v_node_token, 'claim-key-0002', 180
    ) AS c;
    v_job := public.apocrypha_fail_job(
        v_node_id, v_node_token, v_job.id, v_attempt_id,
        v_lease_epoch, v_lease_token, 'EXPECTED_TEST_FAILURE',
        'Schema flow deliberately exercised failure.', false, '{}'::jsonb
    );
    IF v_job.status <> 'failed' THEN
        RAISE EXCEPTION 'non-retryable failure was not terminal';
    END IF;

    SELECT count(*) INTO v_count
    FROM public.apocrypha_alert_outbox AS o
    WHERE o.job_id = v_job.id;
    IF v_count < 1 THEN
        RAISE EXCEPTION 'failed job did not enqueue an alert';
    END IF;

    SELECT a.outbox_id INTO v_outbox_id
    FROM public.apocrypha_claim_alerts('schema-flow-dispatcher', 20, 60) AS a
    WHERE a.job_id = v_job.id
    ORDER BY a.outbox_id
    LIMIT 1;
    IF v_outbox_id IS NULL THEN
        RAISE EXCEPTION 'alert dispatcher could not claim a failed-job alert';
    END IF;
    v_outbox := public.apocrypha_record_alert_delivery(
        v_outbox_id, 'schema-flow-dispatcher', true, false, 200,
        'Delivered by transactional schema test',
        jsonb_build_object('receipt', 'schema-flow')
    );
    IF v_outbox.status <> 'delivered' THEN
        RAISE EXCEPTION 'alert delivery receipt did not close the outbox row';
    END IF;

    -- Long-running cancellation becomes visible on lease renewal and is then
    -- acknowledged through the same fenced failure endpoint.
    v_job := public.apocrypha_enqueue_job(
        v_tenant_id, v_principal_id, 'chat', 'apocky_owner_chat',
        jsonb_build_object('prompt', 'Exercise cancellation.'),
        public.apocrypha_sha256('request-3'),
        'apocky.com:owner:chat', 'enqueue-key-0003',
        'qwen35-35b-a3b-q4', repeat('a', 64), 'tools-v1', repeat('b', 64)
    );
    SELECT c.attempt_id, c.lease_epoch, c.lease_token
    INTO v_attempt_id, v_lease_epoch, v_lease_token
    FROM public.apocrypha_claim_job(
        v_node_id, v_node_token, 'claim-key-0003', 180
    ) AS c;
    v_job := public.apocrypha_cancel_job(v_job.id, v_principal_id, 'Schema flow cancellation');
    SELECT r.cancel_requested INTO v_cancel_flag
    FROM public.apocrypha_renew_lease(
        v_node_id, v_node_token, v_job.id, v_attempt_id,
        v_lease_epoch, v_lease_token, 180
    ) AS r;
    IF NOT v_cancel_flag THEN
        RAISE EXCEPTION 'lease renewal did not report cancellation';
    END IF;
    v_job := public.apocrypha_fail_job(
        v_node_id, v_node_token, v_job.id, v_attempt_id,
        v_lease_epoch, v_lease_token, 'CANCEL_ACK', 'Worker stopped.', false, '{}'::jsonb
    );
    IF v_job.status <> 'cancelled' THEN
        RAISE EXCEPTION 'worker cancellation acknowledgement did not cancel job';
    END IF;
END
$flow$;

SET LOCAL request.jwt.claim.sub = '11111111-1111-4111-8111-111111111111';
SET LOCAL request.jwt.claim.role = 'authenticated';
SET LOCAL ROLE authenticated;

DO $owner_rls$
DECLARE
    v_count integer;
BEGIN
    SELECT count(*) INTO v_count FROM public.apocrypha_job;
    IF v_count <> 3 THEN
        RAISE EXCEPTION 'owner RLS expected 3 jobs, observed %', v_count;
    END IF;
END
$owner_rls$;

RESET ROLE;
SET LOCAL request.jwt.claim.sub = '22222222-2222-4222-8222-222222222222';
SET LOCAL ROLE authenticated;

DO $stranger_rls$
DECLARE
    v_count integer;
BEGIN
    SELECT count(*) INTO v_count FROM public.apocrypha_job;
    IF v_count <> 0 THEN
        RAISE EXCEPTION 'stranger RLS exposed % jobs', v_count;
    END IF;
END
$stranger_rls$;

RESET ROLE;
ROLLBACK;

SELECT 'APOCRYPHA_JOB_SCHEMA_FLOW_OK' AS result;
