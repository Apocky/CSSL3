-- Reduce the already-deployed owner-chat projection to caller authority and
-- prove that no role beyond the function owner and service_role can execute it.

ALTER FUNCTION public.apocrypha_project_owner_chat_revisions(
    uuid, uuid, uuid[], uuid[]
) SECURITY INVOKER;

REVOKE ALL ON FUNCTION public.apocrypha_project_owner_chat_revisions(
    uuid, uuid, uuid[], uuid[]
) FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.apocrypha_project_owner_chat_revisions(
    uuid, uuid, uuid[], uuid[]
) TO service_role;

DO $verification$
DECLARE
    projection_oid oid := to_regprocedure(
        'public.apocrypha_project_owner_chat_revisions(uuid,uuid,uuid[],uuid[])'
    );
BEGIN
    IF projection_oid IS NULL THEN
        RAISE EXCEPTION 'bounded owner chat revision projection is missing';
    END IF;
    IF EXISTS (
        SELECT 1
        FROM pg_catalog.pg_proc AS procedure
        WHERE procedure.oid = projection_oid
          AND procedure.prosecdef
    ) THEN
        RAISE EXCEPTION 'owner chat revision projection still has definer authority';
    END IF;
    IF EXISTS (
        SELECT 1
        FROM pg_catalog.pg_proc AS procedure
        CROSS JOIN LATERAL pg_catalog.aclexplode(procedure.proacl) AS grant_entry
        LEFT JOIN pg_catalog.pg_roles AS granted_role
          ON granted_role.oid = grant_entry.grantee
        WHERE procedure.oid = projection_oid
          AND grant_entry.privilege_type = 'EXECUTE'
          AND grant_entry.grantee <> procedure.proowner
          AND COALESCE(granted_role.rolname, 'PUBLIC') <> 'service_role'
    ) THEN
        RAISE EXCEPTION 'unexpected role retains owner chat projection execution';
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
