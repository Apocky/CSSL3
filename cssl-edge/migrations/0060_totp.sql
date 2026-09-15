-- 0060 · authenticator sign-in.
--
-- Email was the failure. Whether a code appears in the message is decided by a mail template, the
-- link it sends opens the system browser and strands the session there, and none of that is
-- reachable from this codebase. An authenticator removes email from the sign-in path entirely: the
-- code is computed on the device, offline, and nothing has to be delivered.
--
-- One row per user. The secret is the credential, so this table is service-role only and never
-- reachable from the anon key.

CREATE TABLE IF NOT EXISTS public.apocky_totp_factor (
    user_id         uuid PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE,
    secret          text NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    confirmed_at    timestamptz,
    -- The step of the last accepted code. A code stays valid for its whole 30s window, so without
    -- this a code observed once can be replayed inside that window.
    last_used_step  bigint,
    failed_attempts integer NOT NULL DEFAULT 0,
    locked_until    timestamptz,
    CONSTRAINT apocky_totp_secret_shape CHECK (secret ~ '^[A-Z2-7]{16,64}$'),
    CONSTRAINT apocky_totp_attempts_sane CHECK (failed_attempts >= 0)
);

ALTER TABLE public.apocky_totp_factor ENABLE ROW LEVEL SECURITY;
-- No policies, deliberately. RLS with zero policies denies everything to anon and authenticated;
-- only the service role (which bypasses RLS) may touch it. A TOTP secret readable by the client
-- is not a second factor, it is a public number.
REVOKE ALL ON TABLE public.apocky_totp_factor FROM PUBLIC, anon, authenticated;

-- ---------------------------------------------------------------- verification
--
-- The comparison happens in the application, not here: this returns the stored secret only to the
-- service role and records the outcome. Keeping the HMAC in one place (lib/auth-totp.ts, checked
-- against the RFC vectors) is better than a second implementation in plpgsql that could drift.

CREATE OR REPLACE FUNCTION public.apocky_totp_begin(p_user_id uuid)
RETURNS TABLE(secret text, last_used_step bigint, locked_until timestamptz)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path TO 'pg_catalog', 'public'
AS $function$
DECLARE
    v_row public.apocky_totp_factor;
BEGIN
    SELECT * INTO v_row FROM public.apocky_totp_factor AS f WHERE f.user_id = p_user_id;
    IF NOT FOUND OR v_row.confirmed_at IS NULL THEN
        RAISE EXCEPTION 'no confirmed authenticator for this account' USING ERRCODE = 'P4041';
    END IF;
    IF v_row.locked_until IS NOT NULL AND v_row.locked_until > now() THEN
        RAISE EXCEPTION 'authenticator temporarily locked' USING ERRCODE = 'P4291';
    END IF;
    RETURN QUERY SELECT v_row.secret, v_row.last_used_step, v_row.locked_until;
END;
$function$;

CREATE OR REPLACE FUNCTION public.apocky_totp_succeed(p_user_id uuid, p_step bigint)
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path TO 'pg_catalog', 'public'
AS $function$
BEGIN
    -- GREATEST, not assignment: a concurrent request must never be able to move the replay
    -- watermark BACKWARDS and re-open an already-spent code.
    UPDATE public.apocky_totp_factor
    SET last_used_step = GREATEST(coalesce(last_used_step, 0), p_step),
        failed_attempts = 0,
        locked_until = NULL
    WHERE user_id = p_user_id;
END;
$function$;

CREATE OR REPLACE FUNCTION public.apocky_totp_fail(p_user_id uuid)
RETURNS TABLE(failed_attempts integer, locked_until timestamptz)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path TO 'pg_catalog', 'public'
AS $function$
DECLARE
    v_row public.apocky_totp_factor;
BEGIN
    -- Six digits is a million possibilities; without a lockout, an unthrottled attacker walks it
    -- in minutes. Ten wrong codes buys a fifteen-minute pause.
    UPDATE public.apocky_totp_factor
    SET failed_attempts = failed_attempts + 1,
        locked_until = CASE WHEN failed_attempts + 1 >= 10 THEN now() + interval '15 minutes' ELSE locked_until END
    WHERE user_id = p_user_id
    RETURNING * INTO v_row;
    IF NOT FOUND THEN RETURN QUERY SELECT 0, NULL::timestamptz; RETURN; END IF;
    RETURN QUERY SELECT v_row.failed_attempts, v_row.locked_until;
END;
$function$;

REVOKE ALL ON FUNCTION public.apocky_totp_begin(uuid) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocky_totp_succeed(uuid, bigint) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocky_totp_fail(uuid) FROM PUBLIC, anon, authenticated;
