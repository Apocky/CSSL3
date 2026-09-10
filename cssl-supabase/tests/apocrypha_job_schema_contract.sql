-- Post-migration contract checks for 0046_apocrypha_jobs.sql.
-- Run with ON_ERROR_STOP after the migration. The script is read-only.

DO $contract$
DECLARE
    v_expected_tables text[] := ARRAY[
        'apocrypha_tenant',
        'apocrypha_principal',
        'apocrypha_worker_node',
        'apocrypha_job',
        'apocrypha_job_attempt',
        'apocrypha_job_chunk',
        'apocrypha_job_snapshot',
        'apocrypha_job_revision',
        'apocrypha_job_event',
        'apocrypha_entitlement_ledger',
        'apocrypha_alert_outbox',
        'apocrypha_alert_delivery'
    ];
    v_name text;
    v_proc regprocedure;
BEGIN
    FOREACH v_name IN ARRAY v_expected_tables LOOP
        IF to_regclass('public.' || v_name) IS NULL THEN
            RAISE EXCEPTION 'missing table: %', v_name;
        END IF;
        IF NOT EXISTS (
            SELECT 1
            FROM pg_class AS c
            JOIN pg_namespace AS n ON n.oid = c.relnamespace
            WHERE n.nspname = 'public' AND c.relname = v_name AND c.relrowsecurity
        ) THEN
            RAISE EXCEPTION 'RLS is not enabled: %', v_name;
        END IF;
    END LOOP;

    FOREACH v_proc IN ARRAY ARRAY[
        'public.apocrypha_ensure_owner_principal(text,text,uuid)'::regprocedure,
        'public.apocrypha_issue_worker_token(text,text,text[],uuid,smallint,jsonb,jsonb)'::regprocedure,
        'public.apocrypha_enqueue_job(uuid,uuid,text,text,jsonb,text,text,text,text,text,text,text,smallint,smallint,timestamptz,uuid,text)'::regprocedure,
        'public.apocrypha_claim_job(uuid,text,text,integer)'::regprocedure,
        'public.apocrypha_renew_lease(uuid,text,uuid,uuid,bigint,text,integer)'::regprocedure,
        'public.apocrypha_append_chunk(uuid,text,uuid,uuid,bigint,text,integer,text,text,jsonb,integer,text,jsonb)'::regprocedure,
        'public.apocrypha_complete_job(uuid,text,uuid,uuid,bigint,text,text,text,jsonb,jsonb)'::regprocedure,
        'public.apocrypha_fail_job(uuid,text,uuid,uuid,bigint,text,text,text,boolean,jsonb)'::regprocedure,
        'public.apocrypha_cancel_job(uuid,uuid,text)'::regprocedure,
        'public.apocrypha_reap(integer)'::regprocedure,
        'public.apocrypha_claim_alerts(text,integer,integer)'::regprocedure,
        'public.apocrypha_record_alert_delivery(uuid,text,boolean,boolean,integer,text,jsonb)'::regprocedure
    ] LOOP
        IF NOT EXISTS (
            SELECT 1 FROM pg_proc AS p
            WHERE p.oid = v_proc
              AND p.prosecdef
              AND EXISTS (
                  SELECT 1 FROM unnest(coalesce(p.proconfig, ARRAY[]::text[])) AS cfg
                  WHERE cfg LIKE 'search_path=pg_catalog, public, extensions%'
              )
        ) THEN
            RAISE EXCEPTION 'RPC lacks SECURITY DEFINER + fixed search_path: %', v_proc;
        END IF;
        IF EXISTS (
            SELECT 1
            FROM pg_proc AS p,
                 LATERAL aclexplode(coalesce(p.proacl, acldefault('f', p.proowner))) AS acl
            WHERE p.oid = v_proc
              AND acl.grantee = 0
              AND acl.privilege_type = 'EXECUTE'
        ) THEN
            RAISE EXCEPTION 'PUBLIC can execute privileged RPC: %', v_proc;
        END IF;
        IF has_function_privilege('authenticated', v_proc, 'EXECUTE') THEN
            RAISE EXCEPTION 'authenticated can execute privileged RPC: %', v_proc;
        END IF;
        IF NOT has_function_privilege('service_role', v_proc, 'EXECUTE') THEN
            RAISE EXCEPTION 'service_role cannot execute RPC: %', v_proc;
        END IF;
    END LOOP;

    IF EXISTS (
        SELECT 1
        FROM information_schema.role_table_grants
        WHERE grantee = 'authenticated'
          AND table_schema = 'public'
          AND table_name = ANY(v_expected_tables)
          AND privilege_type <> 'SELECT'
    ) THEN
        RAISE EXCEPTION 'authenticated received an Apocrypha table write privilege';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM pg_policies
        WHERE schemaname = 'public'
          AND tablename = ANY(v_expected_tables)
          AND roles @> ARRAY['authenticated']::name[]
          AND cmd <> 'SELECT'
    ) THEN
        RAISE EXCEPTION 'authenticated received an Apocrypha write policy';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger
        WHERE tgrelid = 'public.apocrypha_job_revision'::regclass
          AND tgname = 'apocrypha_job_revision_immutable'
          AND NOT tgisinternal
    ) THEN
        RAISE EXCEPTION 'revision immutability trigger is absent';
    END IF;

    IF public.apocrypha_event_is_alertable(
        'alert.delivery_failed', 'unexpected_fired', 'critical', 'alert.dispatch', true
    ) THEN
        RAISE EXCEPTION 'alert delivery can recursively enqueue another alert';
    END IF;
    IF NOT public.apocrypha_event_is_alertable(
        'job.failed', 'unexpected_fired', 'error', 'control_plane.fail', false
    ) THEN
        RAISE EXCEPTION 'failed jobs are not alert eligible';
    END IF;
    IF public.apocrypha_event_is_alertable(
        'job.running', 'expected_fired', 'info', 'worker.lease', false
    ) THEN
        RAISE EXCEPTION 'healthy lifecycle event was incorrectly marked alert eligible';
    END IF;
END
$contract$;

SELECT 'APOCRYPHA_JOB_SCHEMA_CONTRACT_OK' AS result;
