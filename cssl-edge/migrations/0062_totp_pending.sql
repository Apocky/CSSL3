-- 0062 · pending enrolment.
--
-- Setting up a new authenticator must not disturb the one that currently works. If enrolment
-- overwrote `secret` immediately, then starting setup and walking away -- closing the tab, losing
-- the phone mid-scan -- would leave an account whose only credential is a secret nobody holds.
--
-- So a new secret lands in pending_secret and only becomes THE secret when a code proves the
-- authenticator actually has it.
ALTER TABLE public.apocky_totp_factor
    ADD COLUMN IF NOT EXISTS pending_secret text,
    ADD COLUMN IF NOT EXISTS pending_created_at timestamptz;

ALTER TABLE public.apocky_totp_factor
    DROP CONSTRAINT IF EXISTS apocky_totp_pending_shape;
ALTER TABLE public.apocky_totp_factor
    ADD CONSTRAINT apocky_totp_pending_shape
    CHECK (pending_secret IS NULL OR pending_secret ~ '^[A-Z2-7]{16,64}$');

-- The row may now exist with no confirmed secret at all (first-time setup), so `secret` has to be
-- nullable. apocky_totp_begin already refuses when confirmed_at is null, which is what keeps an
-- unconfirmed row from being usable.
ALTER TABLE public.apocky_totp_factor ALTER COLUMN secret DROP NOT NULL;

CREATE OR REPLACE FUNCTION public.apocky_totp_start_enrolment(p_user_id uuid, p_secret text)
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path TO 'pg_catalog', 'public'
AS $function$
BEGIN
    IF p_secret !~ '^[A-Z2-7]{16,64}$' THEN
        RAISE EXCEPTION 'invalid authenticator secret' USING ERRCODE = '22023';
    END IF;
    INSERT INTO public.apocky_totp_factor (user_id, pending_secret, pending_created_at)
    VALUES (p_user_id, p_secret, now())
    ON CONFLICT (user_id) DO UPDATE
        SET pending_secret = excluded.pending_secret,
            pending_created_at = now();
END;
$function$;

CREATE OR REPLACE FUNCTION public.apocky_totp_pending(p_user_id uuid)
RETURNS TABLE(pending_secret text)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path TO 'pg_catalog', 'public'
AS $function$
BEGIN
    RETURN QUERY
    SELECT f.pending_secret FROM public.apocky_totp_factor AS f
    -- An abandoned enrolment expires rather than waiting around to be confirmed by whoever finds
    -- the screen later.
    WHERE f.user_id = p_user_id
      AND f.pending_secret IS NOT NULL
      AND f.pending_created_at > now() - interval '30 minutes';
END;
$function$;

CREATE OR REPLACE FUNCTION public.apocky_totp_confirm(p_user_id uuid, p_step bigint)
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path TO 'pg_catalog', 'public'
AS $function$
BEGIN
    UPDATE public.apocky_totp_factor
    SET secret = pending_secret,
        pending_secret = NULL,
        pending_created_at = NULL,
        confirmed_at = now(),
        -- The new secret has its own timeline; carrying the old watermark forward could refuse a
        -- perfectly good first code, and resetting it cannot replay anything because the code that
        -- was just used belongs to a secret that is now gone.
        last_used_step = p_step,
        failed_attempts = 0,
        locked_until = NULL
    WHERE user_id = p_user_id AND pending_secret IS NOT NULL;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'no enrolment in progress' USING ERRCODE = 'P4041';
    END IF;
END;
$function$;

REVOKE ALL ON FUNCTION public.apocky_totp_start_enrolment(uuid, text) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocky_totp_pending(uuid) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocky_totp_confirm(uuid, bigint) FROM PUBLIC, anon, authenticated;
