-- 0061 · email -> user id, for the authenticator endpoint.
--
-- auth.users is not exposed through PostgREST, and it should not be. This is the narrowest
-- possible window onto it: one email in, one id out, nothing else, service role only. It
-- deliberately returns no rows rather than raising for an unknown address, so the caller cannot
-- learn from the error which addresses exist.
CREATE OR REPLACE FUNCTION public.apocky_totp_user_by_email(p_email text)
RETURNS TABLE(user_id uuid)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path TO 'pg_catalog', 'public', 'auth'
AS $function$
BEGIN
    IF p_email IS NULL OR length(p_email) = 0 OR length(p_email) > 320 THEN
        RETURN;
    END IF;
    RETURN QUERY
    SELECT u.id
    FROM auth.users AS u
    JOIN public.apocky_totp_factor AS f ON f.user_id = u.id
    WHERE lower(u.email) = lower(p_email)
      AND f.confirmed_at IS NOT NULL
    LIMIT 1;
END;
$function$;

REVOKE ALL ON FUNCTION public.apocky_totp_user_by_email(text) FROM PUBLIC, anon, authenticated;
