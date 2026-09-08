-- Exact owner-chat summaries and a matching conversation-history access path.

CREATE INDEX IF NOT EXISTS idx_apocrypha_job_owner_chat_conversation_created
ON public.apocrypha_job (
    tenant_id,
    owner_principal_id,
    ((request ->> 'conversation_id')),
    created_at DESC,
    id DESC
)
WHERE kind = 'apocky_chat'
  AND capability = 'apocky_owner_chat';

CREATE OR REPLACE FUNCTION public.apocrypha_list_owner_chat_conversations(
    p_tenant_id uuid,
    p_owner_principal_id uuid,
    p_limit integer DEFAULT 257
)
RETURNS TABLE (
    conversation_id uuid,
    title text,
    last_active_iso timestamptz,
    message_count bigint
)
LANGUAGE plpgsql
SECURITY INVOKER
SET search_path = pg_catalog, public, extensions
AS $$
BEGIN
    IF p_tenant_id IS NULL
       OR p_owner_principal_id IS NULL
       OR p_limit < 1
       OR p_limit > 257 THEN
        RAISE EXCEPTION 'owner chat list input is invalid'
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
    WITH scoped AS (
        SELECT
            CASE
                WHEN (job.request ->> 'conversation_id') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
                    THEN (job.request ->> 'conversation_id')::uuid
                ELSE job.id
            END AS conversation_id,
            NULLIF(btrim(job.request ->> 'prompt'), '') AS prompt,
            job.status,
            job.terminal_revision_id,
            job.created_at,
            job.updated_at,
            job.id
        FROM public.apocrypha_job AS job
        WHERE job.tenant_id = p_tenant_id
          AND job.owner_principal_id = p_owner_principal_id
          AND job.kind = 'apocky_chat'
          AND job.capability = 'apocky_owner_chat'
    ), grouped AS (
        SELECT
            scoped.conversation_id,
            COALESCE(
                (array_agg(
                    left(regexp_replace(scoped.prompt, '\s+', ' ', 'g'), 80)
                    ORDER BY scoped.created_at ASC, scoped.id ASC
                ) FILTER (WHERE scoped.prompt IS NOT NULL))[1],
                'New conversation'
            )::text AS title,
            max(scoped.updated_at) AS last_active_iso,
            sum(
                (CASE WHEN scoped.prompt IS NOT NULL THEN 1 ELSE 0 END)
                + (CASE WHEN scoped.status = 'succeeded' AND scoped.terminal_revision_id IS NOT NULL THEN 1 ELSE 0 END)
            )::bigint AS message_count
        FROM scoped
        GROUP BY scoped.conversation_id
    )
    SELECT
        grouped.conversation_id,
        grouped.title,
        grouped.last_active_iso,
        grouped.message_count
    FROM grouped
    ORDER BY grouped.last_active_iso DESC, grouped.conversation_id ASC
    LIMIT p_limit;
END;
$$;

REVOKE ALL ON FUNCTION public.apocrypha_list_owner_chat_conversations(
    uuid, uuid, integer
) FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.apocrypha_list_owner_chat_conversations(
    uuid, uuid, integer
) TO service_role;

DO $verification$
DECLARE
    list_oid oid := to_regprocedure(
        'public.apocrypha_list_owner_chat_conversations(uuid,uuid,integer)'
    );
BEGIN
    IF list_oid IS NULL THEN
        RAISE EXCEPTION 'owner chat summary function was not installed';
    END IF;
    IF EXISTS (
        SELECT 1
        FROM pg_catalog.pg_proc AS procedure
        WHERE procedure.oid = list_oid
          AND procedure.prosecdef
    ) THEN
        RAISE EXCEPTION 'owner chat summary function has definer authority';
    END IF;
    IF EXISTS (
        SELECT 1
        FROM pg_catalog.pg_proc AS procedure
        CROSS JOIN LATERAL pg_catalog.aclexplode(procedure.proacl) AS grant_entry
        LEFT JOIN pg_catalog.pg_roles AS granted_role
          ON granted_role.oid = grant_entry.grantee
        WHERE procedure.oid = list_oid
          AND grant_entry.privilege_type = 'EXECUTE'
          AND grant_entry.grantee <> procedure.proowner
          AND COALESCE(granted_role.rolname, 'PUBLIC') <> 'service_role'
    ) THEN
        RAISE EXCEPTION 'unexpected role retains owner chat summary execution';
    END IF;
    IF NOT has_function_privilege(
        'service_role',
        'public.apocrypha_list_owner_chat_conversations(uuid,uuid,integer)',
        'EXECUTE'
    ) THEN
        RAISE EXCEPTION 'service role cannot execute the owner chat summary function';
    END IF;
END;
$verification$;
