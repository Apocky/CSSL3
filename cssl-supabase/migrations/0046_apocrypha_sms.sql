-- =====================================================================
-- § APOCRYPHA DIRECT SMS · consent-bound encrypted ingress/outbox
-- =====================================================================
-- Provider webhooks are authenticated at the edge before these functions.
-- Persistence deliberately excludes raw phone numbers and plaintext bodies:
-- one server-keyed phone hash binds the configured subject; AES-GCM envelopes
-- hold message content. anon/authenticated receive no table or function grant.
--
-- The provider signature authenticates Twilio, not the human sender. SMS is
-- therefore response-only/no-effects, lives in a dedicated session, and must
-- re-check local consent plus the daily segment budget at dispatch time.
-- =====================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS public.apocrypha_sms_channels (
    phone_hash                  text PRIMARY KEY,
    consent_generation          bigint NOT NULL DEFAULT 0 CHECK (consent_generation >= 0),
    consent_state               text NOT NULL DEFAULT 'pending'
                                      CHECK (consent_state IN ('pending','carrier_started','active','revoked')),
    consent_disclosure_sha256   text,
    disclosure_presented_at     timestamptz,
    disclosure_presented_method text,
    consent_method              text,
    consented_at                timestamptz,
    revoked_at                  timestamptz,
    last_inbound_at             timestamptz,
    created_at                  timestamptz NOT NULL DEFAULT now(),
    updated_at                  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_sms_phone_hash_shape CHECK (phone_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT apocrypha_sms_consent_digest_shape CHECK (
        consent_disclosure_sha256 IS NULL
        OR consent_disclosure_sha256 ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT apocrypha_sms_disclosure_evidence_coherent CHECK (
        (
            disclosure_presented_at IS NULL
            AND disclosure_presented_method IS NULL
            AND consent_disclosure_sha256 IS NULL
        )
        OR (
            disclosure_presented_at IS NOT NULL
            AND disclosure_presented_method = 'sms:consent_required'
            AND consent_disclosure_sha256 IS NOT NULL
        )
    ),
    CONSTRAINT apocrypha_sms_consent_method_shape CHECK (
        consent_method IS NULL OR consent_method = 'sms:CONSENT_APOCRYPHA'
    ),
    CONSTRAINT apocrypha_sms_active_has_receipt CHECK (
        consent_state <> 'active'
        OR (
            consent_disclosure_sha256 IS NOT NULL
            AND disclosure_presented_at IS NOT NULL
            AND disclosure_presented_method IS NOT NULL
            AND consent_method IS NOT NULL
            AND consented_at IS NOT NULL
            AND revoked_at IS NULL
        )
    )
);

CREATE TABLE IF NOT EXISTS public.apocrypha_sms_messages (
    id                          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    provider                    text NOT NULL DEFAULT 'twilio' CHECK (provider = 'twilio'),
    provider_account_sid        text NOT NULL,
    provider_message_sid        text NOT NULL,
    provider_retry_token_hash   text,
    phone_hash                  text NOT NULL REFERENCES public.apocrypha_sms_channels(phone_hash),
    session_id                  uuid NOT NULL,
    request_id                  uuid NOT NULL UNIQUE,
    command_kind                text NOT NULL
                                      CHECK (command_kind IN ('message','stop','start','consent','help')),
    media_count                 integer NOT NULL DEFAULT 0 CHECK (media_count BETWEEN 0 AND 10),
    body_ciphertext             text NOT NULL,
    status                      text NOT NULL
                                      CHECK (status IN ('queued','processing','ready_to_send','sending','sent','delivered','undelivered','failed','uncertain','suppressed','budget_denied','rate_limited','consent_required','media_unsupported','command_processed')),
    reply_ciphertext            text,
    response_digest             text,
    outbound_segments           integer CHECK (outbound_segments BETWEEN 1 AND 10),
    outbound_message_sid        text UNIQUE,
    provider_status             text,
    error_code                  text,
    lease_owner                 text,
    lease_token                 uuid,
    leased_at                   timestamptz,
    dispatch_token              uuid,
    dispatch_consent_generation bigint,
    dispatched_at               timestamptz,
    completed_at                timestamptz,
    created_at                  timestamptz NOT NULL DEFAULT now(),
    updated_at                  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_sms_provider_identity_unique UNIQUE (
        provider,
        provider_account_sid,
        provider_message_sid
    ),
    CONSTRAINT apocrypha_sms_provider_account_sid_shape CHECK (
        provider_account_sid ~ '^AC[0-9A-Fa-f]{32}$'
    ),
    CONSTRAINT apocrypha_sms_provider_sid_shape CHECK (
        provider_message_sid ~ '^(SM|MM)[0-9A-Fa-f]{32}$'
    ),
    CONSTRAINT apocrypha_sms_retry_hash_shape CHECK (
        provider_retry_token_hash IS NULL
        OR provider_retry_token_hash ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT apocrypha_sms_ciphertext_shape CHECK (
        body_ciphertext ~ '^(v1[.][A-Za-z0-9_-]{16}[.][A-Za-z0-9_-]{2,}[.][A-Za-z0-9_-]{22}|v2[.][A-Za-z0-9_-]{1,32}[.][A-Za-z0-9_-]{16}[.][A-Za-z0-9_-]{2,}[.][A-Za-z0-9_-]{22})$'
        AND char_length(body_ciphertext) BETWEEN 32 AND 32768
    ),
    CONSTRAINT apocrypha_sms_reply_ciphertext_shape CHECK (
        reply_ciphertext IS NULL
        OR (
            reply_ciphertext ~ '^(v1[.][A-Za-z0-9_-]{16}[.][A-Za-z0-9_-]{2,}[.][A-Za-z0-9_-]{22}|v2[.][A-Za-z0-9_-]{1,32}[.][A-Za-z0-9_-]{16}[.][A-Za-z0-9_-]{2,}[.][A-Za-z0-9_-]{22})$'
            AND char_length(reply_ciphertext) BETWEEN 32 AND 16384
        )
    ),
    CONSTRAINT apocrypha_sms_response_digest_shape CHECK (
        response_digest IS NULL OR response_digest ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT apocrypha_sms_outbound_sid_shape CHECK (
        outbound_message_sid IS NULL
        OR outbound_message_sid ~ '^(SM|MM)[0-9A-Fa-f]{32}$'
    ),
    CONSTRAINT apocrypha_sms_error_code_shape CHECK (
        error_code IS NULL OR error_code ~ '^[a-z0-9_.:-]{1,96}$'
    ),
    CONSTRAINT apocrypha_sms_outbox_ready_has_reply CHECK (
        status NOT IN ('ready_to_send','sending','sent','delivered','undelivered','uncertain')
        OR (reply_ciphertext IS NOT NULL AND outbound_segments IS NOT NULL)
    ),
    CONSTRAINT apocrypha_sms_dispatch_has_timestamp CHECK (
        status NOT IN ('sending','sent','delivered','undelivered','uncertain')
        OR dispatched_at IS NOT NULL
    ),
    CONSTRAINT apocrypha_sms_provider_state_has_sid CHECK (
        status NOT IN ('sent','delivered','undelivered')
        OR outbound_message_sid IS NOT NULL
    ),
    CONSTRAINT apocrypha_sms_active_lease_coherent CHECK (
        status NOT IN ('processing','sending')
        OR (
            lease_owner IS NOT NULL
            AND leased_at IS NOT NULL
            AND CASE
                WHEN status = 'processing' THEN lease_token IS NOT NULL
                ELSE dispatch_token IS NOT NULL AND dispatch_consent_generation IS NOT NULL
            END
        )
    ),
    CONSTRAINT apocrypha_sms_dispatch_generation_shape CHECK (
        dispatch_consent_generation IS NULL OR dispatch_consent_generation >= 0
    )
);

CREATE INDEX IF NOT EXISTS apocrypha_sms_messages_queue_idx
    ON public.apocrypha_sms_messages (status, created_at);
CREATE INDEX IF NOT EXISTS apocrypha_sms_messages_phone_created_idx
    ON public.apocrypha_sms_messages (phone_hash, created_at DESC);
CREATE INDEX IF NOT EXISTS apocrypha_sms_messages_phone_dispatched_idx
    ON public.apocrypha_sms_messages (phone_hash, dispatched_at DESC)
    WHERE dispatched_at IS NOT NULL;

CREATE TABLE IF NOT EXISTS public.apocrypha_sms_delivery_events (
    id                  bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    outbound_message_sid text NOT NULL,
    provider_status     text NOT NULL,
    error_code          text,
    event_fingerprint   text NOT NULL UNIQUE,
    received_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_sms_delivery_sid_shape CHECK (
        outbound_message_sid ~ '^(SM|MM)[0-9A-Fa-f]{32}$'
    ),
    CONSTRAINT apocrypha_sms_delivery_status_shape CHECK (
        provider_status IN ('accepted','scheduled','queued','sending','sent','delivered','undelivered','failed','canceled','read')
    ),
    CONSTRAINT apocrypha_sms_delivery_fingerprint_shape CHECK (
        event_fingerprint ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT apocrypha_sms_delivery_error_code_shape CHECK (
        error_code IS NULL OR error_code ~ '^[0-9]{1,16}$'
    )
);

CREATE INDEX IF NOT EXISTS apocrypha_sms_delivery_events_sid_received_idx
    ON public.apocrypha_sms_delivery_events (outbound_message_sid, received_at DESC, id DESC);

ALTER TABLE public.apocrypha_sms_channels ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_sms_messages ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_sms_delivery_events ENABLE ROW LEVEL SECURITY;

REVOKE ALL ON TABLE public.apocrypha_sms_channels FROM PUBLIC, anon, authenticated;
REVOKE ALL ON TABLE public.apocrypha_sms_messages FROM PUBLIC, anon, authenticated;
REVOKE ALL ON TABLE public.apocrypha_sms_delivery_events FROM PUBLIC, anon, authenticated;
REVOKE ALL ON SEQUENCE public.apocrypha_sms_delivery_events_id_seq FROM PUBLIC, anon, authenticated;
GRANT SELECT, INSERT, UPDATE ON TABLE public.apocrypha_sms_channels TO service_role;
GRANT SELECT, INSERT, UPDATE ON TABLE public.apocrypha_sms_messages TO service_role;
GRANT SELECT, INSERT ON TABLE public.apocrypha_sms_delivery_events TO service_role;
GRANT USAGE, SELECT ON SEQUENCE public.apocrypha_sms_delivery_events_id_seq TO service_role;

CREATE OR REPLACE FUNCTION public.ingest_apocrypha_sms_message(
    p_provider_account_sid text,
    p_provider_message_sid text,
    p_provider_retry_token_hash text,
    p_phone_hash text,
    p_session_id uuid,
    p_request_id uuid,
    p_body_ciphertext text,
    p_command_kind text,
    p_media_count integer,
    p_consent_disclosure_sha256 text
)
RETURNS TABLE (
    message_id uuid,
    action text,
    message_status text,
    channel_consent_state text,
    duplicate boolean
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    v_existing public.apocrypha_sms_messages%ROWTYPE;
    v_consent text;
    v_presented_digest text;
    v_presented_at timestamptz;
    v_status text;
    v_action text;
    v_message_id uuid := gen_random_uuid();
    v_recent integer := 0;
BEGIN
    IF p_provider_account_sid IS NULL
       OR p_provider_account_sid !~ '^AC[0-9A-Fa-f]{32}$'
       OR p_provider_message_sid IS NULL
       OR p_provider_message_sid !~ '^(SM|MM)[0-9A-Fa-f]{32}$'
       OR p_phone_hash IS NULL
       OR p_phone_hash !~ '^[0-9a-f]{64}$'
       OR p_session_id IS NULL
       OR p_request_id IS NULL
       OR p_body_ciphertext IS NULL
       OR p_body_ciphertext !~ '^(v1[.][A-Za-z0-9_-]{16}[.][A-Za-z0-9_-]{2,}[.][A-Za-z0-9_-]{22}|v2[.][A-Za-z0-9_-]{1,32}[.][A-Za-z0-9_-]{16}[.][A-Za-z0-9_-]{2,}[.][A-Za-z0-9_-]{22})$'
       OR char_length(p_body_ciphertext) NOT BETWEEN 32 AND 32768
       OR p_command_kind IS NULL
       OR p_command_kind NOT IN ('message','stop','start','consent','help')
       OR p_media_count IS NULL
       OR p_media_count NOT BETWEEN 0 AND 10
       OR p_consent_disclosure_sha256 IS NULL
       OR p_consent_disclosure_sha256 !~ '^[0-9a-f]{64}$'
       OR (p_provider_retry_token_hash IS NOT NULL AND p_provider_retry_token_hash !~ '^[0-9a-f]{64}$')
    THEN
        RAISE EXCEPTION 'apocrypha_sms_ingress_invalid';
    END IF;

    INSERT INTO public.apocrypha_sms_channels (phone_hash)
    VALUES (p_phone_hash)
    ON CONFLICT (phone_hash) DO NOTHING;

    SELECT c.consent_state, c.consent_disclosure_sha256, c.disclosure_presented_at
      INTO v_consent, v_presented_digest, v_presented_at
      FROM public.apocrypha_sms_channels AS c
     WHERE c.phone_hash = p_phone_hash
     FOR UPDATE;

    SELECT m.*
      INTO v_existing
      FROM public.apocrypha_sms_messages AS m
     WHERE m.provider = 'twilio'
       AND m.provider_account_sid = p_provider_account_sid
       AND m.provider_message_sid = p_provider_message_sid;
    IF FOUND THEN
        RETURN QUERY SELECT
            v_existing.id,
            'duplicate'::text,
            v_existing.status,
            v_consent,
            true;
        RETURN;
    END IF;

    -- A disclosure revision invalidates the old active receipt. The next
    -- non-command message presents the new disclosure; a later matching
    -- CONSENT APOCRYPHA command can activate it.
    IF v_consent = 'active'
       AND v_presented_digest IS DISTINCT FROM p_consent_disclosure_sha256
    THEN
        v_consent := 'pending';
        UPDATE public.apocrypha_sms_channels
           SET consent_state = 'pending',
               consent_generation = consent_generation + 1,
               updated_at = now()
         WHERE phone_hash = p_phone_hash;
    END IF;

    IF p_command_kind = 'stop' THEN
        v_action := 'stop';
        v_status := 'command_processed';
        v_consent := 'revoked';
        UPDATE public.apocrypha_sms_channels
           SET consent_state = 'revoked',
               consent_generation = consent_generation + 1,
               revoked_at = now(),
               updated_at = now(),
               last_inbound_at = now()
         WHERE phone_hash = p_phone_hash;
        UPDATE public.apocrypha_sms_messages
           SET status = 'suppressed',
               error_code = 'consent_revoked',
               lease_owner = NULL,
               lease_token = NULL,
               leased_at = NULL,
               completed_at = now(),
               updated_at = now()
         WHERE phone_hash = p_phone_hash
           AND status IN ('queued','processing','ready_to_send');
    ELSIF p_command_kind = 'start' THEN
        v_action := 'start';
        v_status := 'command_processed';
        IF v_consent <> 'active' THEN
            v_consent := 'carrier_started';
            UPDATE public.apocrypha_sms_channels
               SET consent_state = 'carrier_started',
                   updated_at = now(),
                   last_inbound_at = now()
             WHERE phone_hash = p_phone_hash;
        END IF;
    ELSIF p_command_kind = 'consent' THEN
        IF v_presented_digest = p_consent_disclosure_sha256
           AND v_presented_at IS NOT NULL
        THEN
            v_action := 'consent';
            v_status := 'command_processed';
            v_consent := 'active';
            UPDATE public.apocrypha_sms_channels
               SET consent_state = 'active',
                   consent_disclosure_sha256 = p_consent_disclosure_sha256,
                   consent_method = 'sms:CONSENT_APOCRYPHA',
                   consented_at = now(),
                   revoked_at = NULL,
                   updated_at = now(),
                   last_inbound_at = now()
             WHERE phone_hash = p_phone_hash;
        ELSE
            -- A bare first-use consent keyword is not informed consent. Store
            -- the exact disclosure version being presented and require a
            -- subsequent explicit CONSENT APOCRYPHA against that same digest.
            v_action := 'consent_required';
            v_status := 'consent_required';
            UPDATE public.apocrypha_sms_channels
               SET consent_disclosure_sha256 = p_consent_disclosure_sha256,
                   disclosure_presented_at = now(),
                   disclosure_presented_method = 'sms:consent_required',
                   updated_at = now(),
                   last_inbound_at = now()
             WHERE phone_hash = p_phone_hash;
        END IF;
    ELSIF p_command_kind = 'help' THEN
        v_action := 'help';
        v_status := 'command_processed';
        UPDATE public.apocrypha_sms_channels
           SET last_inbound_at = now(), updated_at = now()
         WHERE phone_hash = p_phone_hash;
    ELSE
        SELECT count(*)::integer
          INTO v_recent
          FROM public.apocrypha_sms_messages AS m
         WHERE m.phone_hash = p_phone_hash
           AND m.command_kind = 'message'
           AND m.created_at >= now() - interval '60 seconds';
        IF v_recent >= 4 THEN
            v_action := 'rate_limited';
            v_status := 'rate_limited';
        ELSIF p_media_count > 0 THEN
            v_action := 'media_unsupported';
            v_status := 'media_unsupported';
        ELSIF v_consent <> 'active' THEN
            v_action := 'consent_required';
            v_status := 'consent_required';
            UPDATE public.apocrypha_sms_channels
               SET consent_disclosure_sha256 = p_consent_disclosure_sha256,
                   disclosure_presented_at = now(),
                   disclosure_presented_method = 'sms:consent_required'
             WHERE phone_hash = p_phone_hash;
        ELSE
            v_action := 'queued';
            v_status := 'queued';
        END IF;
        UPDATE public.apocrypha_sms_channels
           SET last_inbound_at = now(), updated_at = now()
         WHERE phone_hash = p_phone_hash;
    END IF;

    INSERT INTO public.apocrypha_sms_messages (
        id,
        provider_account_sid,
        provider_message_sid,
        provider_retry_token_hash,
        phone_hash,
        session_id,
        request_id,
        command_kind,
        media_count,
        body_ciphertext,
        status,
        completed_at
    ) VALUES (
        v_message_id,
        p_provider_account_sid,
        p_provider_message_sid,
        p_provider_retry_token_hash,
        p_phone_hash,
        p_session_id,
        p_request_id,
        p_command_kind,
        p_media_count,
        p_body_ciphertext,
        v_status,
        CASE WHEN v_status = 'queued' THEN NULL ELSE now() END
    );

    RETURN QUERY SELECT v_message_id, v_action, v_status, v_consent, false;
END;
$$;

CREATE OR REPLACE FUNCTION public.claim_apocrypha_sms_job(p_worker_id text)
RETURNS TABLE (
    message_id uuid,
    provider_message_sid text,
    phone_hash text,
    session_id uuid,
    request_id uuid,
    body_ciphertext text,
    lease_token uuid,
    reconcile_only boolean
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF p_worker_id IS NULL OR char_length(btrim(p_worker_id)) NOT BETWEEN 1 AND 128 THEN
        RAISE EXCEPTION 'apocrypha_sms_worker_invalid';
    END IF;
    RETURN QUERY
    WITH candidate AS (
        SELECT m.id, (m.status = 'processing') AS reconcile_only
          FROM public.apocrypha_sms_messages AS m
          JOIN public.apocrypha_sms_channels AS c ON c.phone_hash = m.phone_hash
         WHERE (
                   m.status = 'queued'
                   OR (
                       m.status = 'processing'
                       AND m.leased_at IS NOT NULL
                       AND m.leased_at <= now() - interval '5 minutes'
                   )
               )
           AND c.consent_state = 'active'
         ORDER BY m.created_at
         FOR UPDATE OF m SKIP LOCKED
         LIMIT 1
    )
    UPDATE public.apocrypha_sms_messages AS m
       SET status = 'processing',
           lease_owner = btrim(p_worker_id),
           lease_token = gen_random_uuid(),
           leased_at = now(),
           updated_at = now()
      FROM candidate
     WHERE m.id = candidate.id
    RETURNING
        m.id,
        m.provider_message_sid,
        m.phone_hash,
        m.session_id,
        m.request_id,
        m.body_ciphertext,
        m.lease_token,
        candidate.reconcile_only;
END;
$$;

CREATE OR REPLACE FUNCTION public.mark_apocrypha_sms_job_ready(
    p_message_id uuid,
    p_lease_token uuid,
    p_reply_ciphertext text,
    p_response_digest text,
    p_outbound_segments integer
)
RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    v_active boolean;
    v_phone_hash text;
BEGIN
    IF p_message_id IS NULL
       OR p_lease_token IS NULL
       OR p_reply_ciphertext IS NULL
       OR p_reply_ciphertext !~ '^(v1[.][A-Za-z0-9_-]{16}[.][A-Za-z0-9_-]{2,}[.][A-Za-z0-9_-]{22}|v2[.][A-Za-z0-9_-]{1,32}[.][A-Za-z0-9_-]{16}[.][A-Za-z0-9_-]{2,}[.][A-Za-z0-9_-]{22})$'
       OR char_length(p_reply_ciphertext) NOT BETWEEN 32 AND 16384
       OR p_response_digest IS NULL
       OR p_response_digest !~ '^[0-9a-f]{64}$'
       OR p_outbound_segments IS NULL
       OR p_outbound_segments NOT BETWEEN 1 AND 10
    THEN
        RAISE EXCEPTION 'apocrypha_sms_reply_invalid';
    END IF;

    SELECT m.phone_hash
      INTO v_phone_hash
      FROM public.apocrypha_sms_messages AS m
     WHERE m.id = p_message_id
       AND m.status = 'processing'
       AND m.lease_token = p_lease_token;
    IF NOT FOUND THEN RETURN false; END IF;

    SELECT c.consent_state = 'active'
      INTO v_active
      FROM public.apocrypha_sms_channels AS c
     WHERE c.phone_hash = v_phone_hash
     FOR UPDATE;
    IF NOT COALESCE(v_active, false) THEN
        UPDATE public.apocrypha_sms_messages
           SET status = 'suppressed',
                error_code = 'consent_not_active',
                lease_owner = NULL,
                lease_token = NULL,
                leased_at = NULL,
               completed_at = now(),
               updated_at = now()
         WHERE id = p_message_id
           AND status = 'processing'
           AND lease_token = p_lease_token;
        RETURN false;
    END IF;
    UPDATE public.apocrypha_sms_messages
       SET status = 'ready_to_send',
           reply_ciphertext = p_reply_ciphertext,
           response_digest = p_response_digest,
            outbound_segments = p_outbound_segments,
            lease_owner = NULL,
            lease_token = NULL,
            leased_at = NULL,
           updated_at = now()
     WHERE id = p_message_id
       AND status = 'processing'
       AND lease_token = p_lease_token;
    RETURN FOUND;
END;
$$;

CREATE OR REPLACE FUNCTION public.mark_apocrypha_sms_runtime_failed(
    p_message_id uuid,
    p_lease_token uuid,
    p_error_code text
)
RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF p_message_id IS NULL
       OR p_lease_token IS NULL
       OR p_error_code IS NULL
       OR p_error_code !~ '^[a-z0-9_.:-]{1,96}$'
    THEN
        RAISE EXCEPTION 'apocrypha_sms_error_code_invalid';
    END IF;
    UPDATE public.apocrypha_sms_messages
       SET status = 'failed',
            error_code = p_error_code,
            lease_owner = NULL,
            lease_token = NULL,
            leased_at = NULL,
           completed_at = now(),
           updated_at = now()
     WHERE id = p_message_id
       AND status = 'processing'
       AND lease_token = p_lease_token;
    RETURN FOUND;
END;
$$;

CREATE OR REPLACE FUNCTION public.claim_apocrypha_sms_send(
    p_worker_id text,
    p_daily_segment_budget integer
)
RETURNS TABLE (
    message_id uuid,
    provider_message_sid text,
    reply_ciphertext text,
    outbound_segments integer,
    dispatch_token uuid
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    v_job public.apocrypha_sms_messages%ROWTYPE;
    v_candidate_id uuid;
    v_candidate_phone_hash text;
    v_consent text;
    v_consent_generation bigint;
    v_used integer;
BEGIN
    IF p_worker_id IS NULL OR char_length(btrim(p_worker_id)) NOT BETWEEN 1 AND 128
       OR p_daily_segment_budget IS NULL
       OR p_daily_segment_budget NOT BETWEEN 1 AND 1000
    THEN
        RAISE EXCEPTION 'apocrypha_sms_send_claim_invalid';
    END IF;

    -- A process can die after claiming dispatch but before persisting a provider
    -- receipt. Never requeue or resend that ambiguous outcome: terminalize the
    -- abandoned lease while retaining its dispatch token for a late receipt
    -- from the original worker.
    WITH stale_channels AS MATERIALIZED (
        SELECT c.phone_hash
          FROM public.apocrypha_sms_channels AS c
         WHERE EXISTS (
                   SELECT 1
                     FROM public.apocrypha_sms_messages AS pending
                    WHERE pending.phone_hash = c.phone_hash
                      AND pending.status = 'sending'
                      AND pending.outbound_message_sid IS NULL
                      AND pending.leased_at IS NOT NULL
                      AND pending.leased_at <= now() - interval '5 minutes'
               )
         ORDER BY c.phone_hash
         FOR UPDATE OF c
    )
    UPDATE public.apocrypha_sms_messages AS m
       SET status = 'uncertain',
           error_code = 'send_worker_lost',
           lease_owner = NULL,
           leased_at = NULL,
           completed_at = now(),
           updated_at = now()
      FROM stale_channels AS c
     WHERE m.phone_hash = c.phone_hash
       AND m.status = 'sending'
       AND m.outbound_message_sid IS NULL
       AND m.leased_at IS NOT NULL
       AND m.leased_at <= now() - interval '5 minutes';

    -- Choose without a row lock, then lock the channel before the message.
    -- STOP takes the same channel-first order. That both avoids a lock-order
    -- deadlock and serializes budget claims for the bound SMS subject.
    SELECT m.id, m.phone_hash
      INTO v_candidate_id, v_candidate_phone_hash
      FROM public.apocrypha_sms_messages AS m
     WHERE m.status = 'ready_to_send'
     ORDER BY m.created_at
     LIMIT 1;
    IF NOT FOUND THEN RETURN; END IF;

    SELECT c.consent_state, c.consent_generation
      INTO v_consent, v_consent_generation
      FROM public.apocrypha_sms_channels AS c
     WHERE c.phone_hash = v_candidate_phone_hash
     FOR UPDATE;
    IF v_consent <> 'active' THEN
        UPDATE public.apocrypha_sms_messages
           SET status = 'suppressed',
               error_code = 'consent_not_active',
               lease_owner = NULL,
               leased_at = NULL,
               completed_at = now(),
               updated_at = now()
         WHERE id = v_candidate_id
           AND status = 'ready_to_send';
        RETURN;
    END IF;

    SELECT m.*
      INTO v_job
      FROM public.apocrypha_sms_messages AS m
     WHERE m.id = v_candidate_id
       AND m.status = 'ready_to_send'
     FOR UPDATE SKIP LOCKED;
    IF NOT FOUND THEN RETURN; END IF;

    IF v_job.outbound_segments IS NULL OR v_job.reply_ciphertext IS NULL THEN
        UPDATE public.apocrypha_sms_messages
           SET status = 'failed',
               error_code = 'outbox_invariant_invalid',
               completed_at = now(),
               updated_at = now()
         WHERE id = v_job.id;
        RETURN;
    END IF;

    SELECT COALESCE(sum(m.outbound_segments), 0)::integer
      INTO v_used
      FROM public.apocrypha_sms_messages AS m
     WHERE m.phone_hash = v_job.phone_hash
       AND m.dispatched_at >= date_trunc('day', now())
       AND m.dispatched_at < date_trunc('day', now()) + interval '1 day';
    IF v_used + v_job.outbound_segments > p_daily_segment_budget THEN
        UPDATE public.apocrypha_sms_messages
           SET status = 'budget_denied', error_code = 'daily_segment_budget', completed_at = now(), updated_at = now()
         WHERE id = v_job.id;
        RETURN;
    END IF;

    UPDATE public.apocrypha_sms_messages
       SET status = 'sending',
           lease_owner = btrim(p_worker_id),
           dispatch_token = gen_random_uuid(),
           dispatch_consent_generation = v_consent_generation,
           leased_at = now(),
           dispatched_at = now(),
           updated_at = now()
     WHERE id = v_job.id;
    RETURN QUERY
    SELECT m.id, m.provider_message_sid, m.reply_ciphertext, m.outbound_segments, m.dispatch_token
      FROM public.apocrypha_sms_messages AS m
     WHERE m.id = v_job.id;
END;
$$;

CREATE OR REPLACE FUNCTION public.authorize_apocrypha_sms_send(
    p_message_id uuid,
    p_dispatch_token uuid
)
RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    v_phone_hash text;
    v_dispatch_consent_generation bigint;
    v_consent text;
    v_consent_generation bigint;
BEGIN
    IF p_message_id IS NULL OR p_dispatch_token IS NULL THEN
        RAISE EXCEPTION 'apocrypha_sms_send_authorization_invalid';
    END IF;

    SELECT m.phone_hash, m.dispatch_consent_generation
      INTO v_phone_hash, v_dispatch_consent_generation
      FROM public.apocrypha_sms_messages AS m
     WHERE m.id = p_message_id
       AND m.status = 'sending'
       AND m.dispatch_token = p_dispatch_token;
    IF NOT FOUND THEN RETURN false; END IF;

    -- Preserve the channel-first lock order shared by STOP and send claiming.
    -- A STOP or disclosure invalidation that committed before this check bumps
    -- the generation and makes the captured dispatch authority stale.
    SELECT c.consent_state, c.consent_generation
      INTO v_consent, v_consent_generation
      FROM public.apocrypha_sms_channels AS c
     WHERE c.phone_hash = v_phone_hash
     FOR UPDATE;

    IF v_consent <> 'active'
       OR v_consent_generation IS DISTINCT FROM v_dispatch_consent_generation
    THEN
        UPDATE public.apocrypha_sms_messages
           SET status = 'suppressed',
               error_code = 'consent_generation_stale',
               lease_owner = NULL,
               leased_at = NULL,
               completed_at = now(),
               updated_at = now()
         WHERE id = p_message_id
           AND status = 'sending'
           AND dispatch_token = p_dispatch_token;
        RETURN false;
    END IF;

    RETURN EXISTS (
        SELECT 1
          FROM public.apocrypha_sms_messages AS m
         WHERE m.id = p_message_id
           AND m.status = 'sending'
           AND m.dispatch_token = p_dispatch_token
           AND m.dispatch_consent_generation = v_consent_generation
    );
END;
$$;

CREATE OR REPLACE FUNCTION public.project_apocrypha_sms_delivery(
    p_outbound_message_sid text
)
RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    v_provider_status text;
    v_error_code text;
BEGIN
    IF p_outbound_message_sid IS NULL
       OR p_outbound_message_sid !~ '^(SM|MM)[0-9A-Fa-f]{32}$'
    THEN
        RAISE EXCEPTION 'apocrypha_sms_delivery_sid_invalid';
    END IF;

    -- Project by semantic precedence rather than callback arrival order.
    -- A delivered receipt dominates a contradictory late failure; terminal
    -- failures dominate stale queued/sent callbacks. Every callback remains
    -- in the append-only event table regardless of this current-state view.
    SELECT e.provider_status, e.error_code
      INTO v_provider_status, v_error_code
      FROM public.apocrypha_sms_delivery_events AS e
     WHERE e.outbound_message_sid = p_outbound_message_sid
     ORDER BY CASE e.provider_status
                  WHEN 'delivered' THEN 50
                  WHEN 'undelivered' THEN 40
                  WHEN 'failed' THEN 40
                  WHEN 'canceled' THEN 40
                  WHEN 'read' THEN 35
                  WHEN 'sent' THEN 30
                  WHEN 'sending' THEN 20
                  WHEN 'queued' THEN 10
                  WHEN 'scheduled' THEN 9
                  WHEN 'accepted' THEN 8
                  ELSE 0
              END DESC,
              e.received_at DESC,
              e.id DESC
     LIMIT 1;
    IF NOT FOUND THEN RETURN false; END IF;

    UPDATE public.apocrypha_sms_messages
       SET provider_status = v_provider_status,
           status = CASE
               WHEN v_provider_status = 'delivered' THEN 'delivered'
               WHEN v_provider_status = 'undelivered' THEN 'undelivered'
               WHEN v_provider_status IN ('failed','canceled') THEN 'failed'
               WHEN v_provider_status IN ('sent','read')
                    AND status NOT IN ('delivered','undelivered','failed') THEN 'sent'
               ELSE status
           END,
           error_code = CASE
               WHEN v_error_code IS NOT NULL THEN 'twilio:' || v_error_code
               WHEN v_provider_status IN ('accepted','scheduled','queued','sending','sent','delivered','read') THEN NULL
               ELSE error_code
           END,
           updated_at = now()
     WHERE outbound_message_sid = p_outbound_message_sid;
    RETURN FOUND;
END;
$$;

CREATE OR REPLACE FUNCTION public.record_apocrypha_sms_sent(
    p_message_id uuid,
    p_dispatch_token uuid,
    p_outbound_message_sid text,
    p_provider_status text
)
RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF p_message_id IS NULL
       OR p_dispatch_token IS NULL
       OR p_outbound_message_sid IS NULL
       OR p_outbound_message_sid !~ '^(SM|MM)[0-9A-Fa-f]{32}$'
       OR p_provider_status IS NULL
       OR p_provider_status NOT IN ('accepted','scheduled','queued')
    THEN
        RAISE EXCEPTION 'apocrypha_sms_send_receipt_invalid';
    END IF;
    UPDATE public.apocrypha_sms_messages
       SET status = 'sent',
           outbound_message_sid = p_outbound_message_sid,
           provider_status = p_provider_status,
           error_code = NULL,
           lease_owner = NULL,
           leased_at = NULL,
           completed_at = now(),
           updated_at = now()
     WHERE id = p_message_id
       AND dispatch_token = p_dispatch_token
       AND (
           status = 'sending'
           OR (
               status = 'uncertain'
               AND error_code = 'send_worker_lost'
               AND outbound_message_sid IS NULL
           )
       );
    IF NOT FOUND THEN RETURN false; END IF;

    -- A signed status callback can win the race with the create-message HTTP
    -- response. Re-project any already-appended callback after SID binding.
    PERFORM public.project_apocrypha_sms_delivery(p_outbound_message_sid);
    RETURN true;
END;
$$;

CREATE OR REPLACE FUNCTION public.record_apocrypha_sms_send_failure(
    p_message_id uuid,
    p_dispatch_token uuid,
    p_outcome text,
    p_error_code text
)
RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    IF p_message_id IS NULL
       OR p_dispatch_token IS NULL
       OR p_outcome IS NULL
       OR p_outcome NOT IN ('failed','uncertain')
       OR p_error_code IS NULL
       OR p_error_code !~ '^[a-z0-9_.:-]{1,96}$'
    THEN
        RAISE EXCEPTION 'apocrypha_sms_send_failure_invalid';
    END IF;
    UPDATE public.apocrypha_sms_messages
       SET status = p_outcome,
           error_code = p_error_code,
           lease_owner = NULL,
           leased_at = NULL,
           completed_at = now(),
           updated_at = now()
     WHERE id = p_message_id
       AND status = 'sending'
       AND dispatch_token = p_dispatch_token;
    RETURN FOUND;
END;
$$;

CREATE OR REPLACE FUNCTION public.record_apocrypha_sms_delivery(
    p_outbound_message_sid text,
    p_provider_status text,
    p_error_code text
)
RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    v_fingerprint text;
    v_inserted bigint;
BEGIN
    IF p_outbound_message_sid IS NULL
       OR p_outbound_message_sid !~ '^(SM|MM)[0-9A-Fa-f]{32}$'
       OR p_provider_status IS NULL
       OR p_provider_status NOT IN ('accepted','scheduled','queued','sending','sent','delivered','undelivered','failed','canceled','read')
       OR (p_error_code IS NOT NULL AND p_error_code !~ '^[0-9]{1,16}$')
    THEN
        RAISE EXCEPTION 'apocrypha_sms_delivery_invalid';
    END IF;
    v_fingerprint := encode(digest(
        jsonb_build_array(p_outbound_message_sid, p_provider_status, p_error_code)::text,
        'sha256'
    ), 'hex');
    INSERT INTO public.apocrypha_sms_delivery_events (
        outbound_message_sid, provider_status, error_code, event_fingerprint
    ) VALUES (
        p_outbound_message_sid, p_provider_status, p_error_code, v_fingerprint
    ) ON CONFLICT (event_fingerprint) DO NOTHING
    RETURNING id INTO v_inserted;
    IF v_inserted IS NULL THEN RETURN false; END IF;

    RETURN public.project_apocrypha_sms_delivery(p_outbound_message_sid);
END;
$$;

REVOKE ALL ON FUNCTION public.ingest_apocrypha_sms_message(text,text,text,text,uuid,uuid,text,text,integer,text) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.claim_apocrypha_sms_job(text) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.mark_apocrypha_sms_job_ready(uuid,uuid,text,text,integer) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.mark_apocrypha_sms_runtime_failed(uuid,uuid,text) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.claim_apocrypha_sms_send(text,integer) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.authorize_apocrypha_sms_send(uuid,uuid) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.project_apocrypha_sms_delivery(text) FROM PUBLIC, anon, authenticated, service_role;
REVOKE ALL ON FUNCTION public.record_apocrypha_sms_sent(uuid,uuid,text,text) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.record_apocrypha_sms_send_failure(uuid,uuid,text,text) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.record_apocrypha_sms_delivery(text,text,text) FROM PUBLIC, anon, authenticated;

GRANT EXECUTE ON FUNCTION public.ingest_apocrypha_sms_message(text,text,text,text,uuid,uuid,text,text,integer,text) TO service_role;
GRANT EXECUTE ON FUNCTION public.claim_apocrypha_sms_job(text) TO service_role;
GRANT EXECUTE ON FUNCTION public.mark_apocrypha_sms_job_ready(uuid,uuid,text,text,integer) TO service_role;
GRANT EXECUTE ON FUNCTION public.mark_apocrypha_sms_runtime_failed(uuid,uuid,text) TO service_role;
GRANT EXECUTE ON FUNCTION public.claim_apocrypha_sms_send(text,integer) TO service_role;
GRANT EXECUTE ON FUNCTION public.authorize_apocrypha_sms_send(uuid,uuid) TO service_role;
GRANT EXECUTE ON FUNCTION public.record_apocrypha_sms_sent(uuid,uuid,text,text) TO service_role;
GRANT EXECUTE ON FUNCTION public.record_apocrypha_sms_send_failure(uuid,uuid,text,text) TO service_role;
GRANT EXECUTE ON FUNCTION public.record_apocrypha_sms_delivery(text,text,text) TO service_role;

COMMENT ON TABLE public.apocrypha_sms_channels IS
    'Hashed SMS binding and explicit local consent receipt; contains no raw phone number.';
COMMENT ON TABLE public.apocrypha_sms_messages IS
    'Encrypted inbound/outbound SMS outbox with provider idempotency and no effect authority.';
COMMENT ON TABLE public.apocrypha_sms_delivery_events IS
    'Append-only, semantically deduplicated provider delivery callbacks.';
