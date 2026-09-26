-- 0065: apocrypha_ensure_member_principal failed on every call with 42702
-- (column reference "tenant_id" is ambiguous): its ON CONFLICT (tenant_id, auth_user_id) names a
-- column that is also this function's OUT column. Every member path runs through it -- attachment
-- upload was the visible break (Camera/Photos, owner report 2026-09-26). Same body, with
-- #variable_conflict use_column so bare names in SQL mean the table column.
CREATE OR REPLACE FUNCTION public.apocrypha_ensure_member_principal(p_verified_auth_user_id uuid)
 RETURNS TABLE(tenant_id uuid, principal_id uuid)
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'pg_catalog', 'public', 'extensions'
AS $function$
#variable_conflict use_column
DECLARE
    v_tenant       public.apocrypha_tenant;
    v_principal    public.apocrypha_principal;
BEGIN
    IF p_verified_auth_user_id IS NULL THEN
        RAISE EXCEPTION 'verified member auth user is required' USING ERRCODE = '23502';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM auth.users AS auth_user
        WHERE auth_user.id = p_verified_auth_user_id
    ) THEN
        RAISE EXCEPTION 'verified member auth user does not exist' USING ERRCODE = '23503';
    END IF;

    PERFORM pg_advisory_xact_lock(
        hashtextextended('apocky-member-principal:' || p_verified_auth_user_id::text, 0)
    );

    INSERT INTO public.apocrypha_tenant (slug, display_name)
    VALUES ('apocky-members', 'Apocky members')
    ON CONFLICT (slug) DO NOTHING;

    SELECT tenant.* INTO v_tenant
    FROM public.apocrypha_tenant AS tenant
    WHERE tenant.slug = 'apocky-members'
    FOR SHARE;

    IF NOT FOUND OR v_tenant.status <> 'active' THEN
        RAISE EXCEPTION 'Apocky member tenant is not active' USING ERRCODE = '55000';
    END IF;

    INSERT INTO public.apocrypha_principal (
        tenant_id, auth_user_id, principal_kind, display_name
    ) VALUES (
        v_tenant.id, p_verified_auth_user_id, 'member', 'Apocky member'
    )
    ON CONFLICT (tenant_id, auth_user_id) WHERE auth_user_id IS NOT NULL
    DO NOTHING;

    SELECT principal.* INTO v_principal
    FROM public.apocrypha_principal AS principal
    WHERE principal.tenant_id = v_tenant.id
      AND principal.auth_user_id = p_verified_auth_user_id
    FOR UPDATE;

    IF NOT FOUND
       OR v_principal.principal_kind <> 'member'
       OR v_principal.status <> 'active' THEN
        RAISE EXCEPTION 'active Apocky member principal is unavailable' USING ERRCODE = '42501';
    END IF;

    RETURN QUERY SELECT v_tenant.id, v_principal.id;
END;
$function$;
