-- Bounded, owner-scoped projection for the durable Apocrypha conversation UI.
-- The service-role API already possesses the verified owner identity. This RPC
-- keeps large worker revisions and provenance documents behind a fixed database
-- boundary instead of transferring them in full before application truncation.

CREATE OR REPLACE FUNCTION public.apocrypha_project_owner_chat_revisions(
    p_tenant_id uuid,
    p_owner_principal_id uuid,
    p_job_ids uuid[],
    p_revision_ids uuid[]
)
RETURNS TABLE (
    id uuid,
    job_id uuid,
    content text,
    content_truncated boolean,
    provenance jsonb,
    usage jsonb,
    created_at timestamptz
)
LANGUAGE plpgsql
SECURITY INVOKER
SET search_path = pg_catalog, public, extensions
AS $$
BEGIN
    IF p_tenant_id IS NULL
       OR p_owner_principal_id IS NULL
       OR p_job_ids IS NULL
       OR p_revision_ids IS NULL
       OR cardinality(p_job_ids) < 1
       OR cardinality(p_job_ids) > 8
       OR cardinality(p_job_ids) <> cardinality(p_revision_ids) THEN
        RAISE EXCEPTION 'owner chat revision projection input is invalid'
            USING ERRCODE = '22023';
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM public.apocrypha_principal AS principal
        WHERE principal.tenant_id = p_tenant_id
          AND principal.id = p_owner_principal_id
          AND principal.principal_kind = 'owner'
          AND principal.status = 'active'
    ) THEN
        RAISE EXCEPTION 'active owner identity is required'
            USING ERRCODE = 'P4031';
    END IF;

    RETURN QUERY
    SELECT
        revision.id,
        revision.job_id,
        left(revision.content, 16384) AS content,
        char_length(revision.content) > 16384 AS content_truncated,
        jsonb_build_object(
            'tool_calls',
            COALESCE((
                SELECT jsonb_agg(
                    jsonb_strip_nulls(jsonb_build_object(
                        'name', left(trace.entry ->> 'name', 160),
                        'ok', CASE
                            WHEN jsonb_typeof(trace.entry -> 'ok') = 'boolean'
                                THEN trace.entry -> 'ok'
                            ELSE NULL
                        END,
                        'elapsed_ms', CASE
                            WHEN jsonb_typeof(trace.entry -> 'elapsed_ms') = 'number'
                                THEN trace.entry -> 'elapsed_ms'
                            ELSE NULL
                        END,
                        'error', left(trace.entry ->> 'error', 500)
                    ))
                    ORDER BY trace.ordinal
                )
                FROM jsonb_array_elements(
                    CASE
                        WHEN jsonb_typeof(revision.provenance -> 'tool_calls') = 'array'
                            THEN revision.provenance -> 'tool_calls'
                        ELSE '[]'::jsonb
                    END
                ) WITH ORDINALITY AS trace(entry, ordinal)
                WHERE trace.ordinal <= 64
            ), '[]'::jsonb)
        ) AS provenance,
        jsonb_strip_nulls(jsonb_build_object(
            'elapsed_s', CASE
                WHEN jsonb_typeof(revision.usage -> 'elapsed_s') = 'number'
                    THEN revision.usage -> 'elapsed_s'
                ELSE NULL
            END,
            'total_cost_usd', CASE
                WHEN jsonb_typeof(revision.usage -> 'total_cost_usd') = 'number'
                    THEN revision.usage -> 'total_cost_usd'
                ELSE NULL
            END
        )) AS usage,
        revision.created_at
    FROM public.apocrypha_job_revision AS revision
    JOIN public.apocrypha_job AS job
      ON job.id = revision.job_id
     AND job.terminal_revision_id = revision.id
    WHERE job.tenant_id = p_tenant_id
      AND job.owner_principal_id = p_owner_principal_id
      AND job.kind = 'apocky_chat'
      AND job.capability = 'apocky_owner_chat'
      AND job.id = ANY (p_job_ids)
      AND revision.id = ANY (p_revision_ids)
    ORDER BY revision.created_at ASC, revision.id ASC
    LIMIT 8;
END;
$$;

REVOKE ALL ON FUNCTION public.apocrypha_project_owner_chat_revisions(
    uuid, uuid, uuid[], uuid[]
) FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.apocrypha_project_owner_chat_revisions(
    uuid, uuid, uuid[], uuid[]
) TO service_role;

DO $verification$
BEGIN
    IF to_regprocedure(
        'public.apocrypha_project_owner_chat_revisions(uuid,uuid,uuid[],uuid[])'
    ) IS NULL THEN
        RAISE EXCEPTION 'bounded owner chat revision projection was not installed';
    END IF;
    IF has_function_privilege(
        'anon',
        'public.apocrypha_project_owner_chat_revisions(uuid,uuid,uuid[],uuid[])',
        'EXECUTE'
    ) OR has_function_privilege(
        'authenticated',
        'public.apocrypha_project_owner_chat_revisions(uuid,uuid,uuid[],uuid[])',
        'EXECUTE'
    ) THEN
        RAISE EXCEPTION 'browser roles must not execute the owner chat projection';
    END IF;
    IF NOT has_function_privilege(
        'service_role',
        'public.apocrypha_project_owner_chat_revisions(uuid,uuid,uuid[],uuid[])',
        'EXECUTE'
    ) THEN
        RAISE EXCEPTION 'service role cannot execute the owner chat projection';
    END IF;
END;
$verification$;
