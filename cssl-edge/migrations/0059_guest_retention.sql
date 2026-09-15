-- 0059 · guest chat retention.
--
-- A signed-out visitor's question is stored server-side: it is the job payload the worker reads,
-- and the reply is a job revision. That is unavoidable for the queue to work, but keeping a
-- stranger's messages indefinitely is not, and a privacy policy that promised deletion without
-- this function would have been aspirational rather than true.
--
-- Scoped to the GUEST TENANT by construction. The delete cannot reach member or owner rows because
-- it only ever selects jobs whose owning principal is a guest in 'apocky-guests'; there is no
-- parameter that could widen it.

CREATE OR REPLACE FUNCTION public.apocrypha_purge_guest_chat(p_older_than interval DEFAULT interval '30 days')
RETURNS TABLE(jobs_deleted integer, principals_deleted integer)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path TO 'pg_catalog', 'public', 'extensions'
AS $function$
DECLARE
    v_tenant uuid;
    v_jobs   integer := 0;
    v_people integer := 0;
BEGIN
    IF p_older_than IS NULL OR p_older_than < interval '1 day' THEN
        RAISE EXCEPTION 'guest retention window must be at least one day' USING ERRCODE = '22023';
    END IF;

    SELECT tenant.id INTO v_tenant
    FROM public.apocrypha_tenant AS tenant
    WHERE tenant.slug = 'apocky-guests';

    IF v_tenant IS NULL THEN
        RETURN QUERY SELECT 0, 0;
        RETURN;
    END IF;

    -- Only finished work is removed. An in-flight job is left alone so a purge running mid-answer
    -- cannot delete the thing a visitor is currently waiting on.
    WITH removed AS (
        DELETE FROM public.apocrypha_job AS job
        WHERE job.tenant_id = v_tenant
          AND job.created_at < now() - p_older_than
          AND job.status NOT IN ('queued', 'leased', 'running', 'cancel_requested')
        RETURNING 1
    )
    SELECT count(*) INTO v_jobs FROM removed;

    -- A guest principal with no work left is just a hash with nothing attached to it.
    WITH orphaned AS (
        DELETE FROM public.apocrypha_principal AS principal
        WHERE principal.tenant_id = v_tenant
          AND principal.principal_kind = 'guest'
          AND principal.created_at < now() - p_older_than
          AND NOT EXISTS (
              SELECT 1 FROM public.apocrypha_job AS job
              WHERE job.owner_principal_id = principal.id
          )
        RETURNING 1
    )
    SELECT count(*) INTO v_people FROM orphaned;

    RETURN QUERY SELECT v_jobs, v_people;
END;
$function$;

REVOKE ALL ON FUNCTION public.apocrypha_purge_guest_chat(interval) FROM PUBLIC, anon, authenticated;

SELECT cron.schedule(
    'apocrypha-guest-chat-purge',
    '20 4 * * *',
    $$SELECT public.apocrypha_purge_guest_chat(interval '30 days')$$
);
