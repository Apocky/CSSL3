-- =====================================================================
-- § T11-W7-RD-D4-MIGRATIONS · 0017_runs.sql
-- Roguelike-loop run records + gift-economy share-feed.
-- Ref : GDDs/ROGUELIKE_LOOP.csl § RUN-LIFECYCLE + ECHO-ECONOMY + SHARE-RECEIPT.
-- § PRIME_DIRECTIVE : sovereignty preserved · ended runs immutable except
-- via service-role · share-feed = friend-visible only · player owns their
-- run-history · DELETE = service-role-only. Apply after 0016.
-- =====================================================================
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- § runs · per-player roguelike-loop attempt records
CREATE TABLE IF NOT EXISTS public.runs (
    run_id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id       uuid        NOT NULL,
    run_seed        bytea       NOT NULL,
    started_at      timestamptz NOT NULL DEFAULT now(),
    ended_at        timestamptz NULL,
    end_reason      text        NULL,
    biome_path      text[]      NOT NULL DEFAULT ARRAY[]::text[],
    floor_max       smallint    NOT NULL DEFAULT 0,
    echoes_earned   bigint      NOT NULL DEFAULT 0,
    hard_perma      boolean     NOT NULL DEFAULT false,
    share_receipt   jsonb       NULL,
    CONSTRAINT runs_seed_length
        CHECK (octet_length(run_seed) BETWEEN 1 AND 256),
    CONSTRAINT runs_floor_max_range
        CHECK (floor_max BETWEEN 0 AND 9999),
    CONSTRAINT runs_echoes_nonneg
        CHECK (echoes_earned >= 0),
    CONSTRAINT runs_end_reason_enum
        CHECK (end_reason IS NULL OR end_reason IN ('death','victory','retreat','timeout','abandoned','disconnected')),
    CONSTRAINT runs_ended_at_after_started
        CHECK (ended_at IS NULL OR ended_at >= started_at),
    CONSTRAINT runs_share_receipt_object
        CHECK (share_receipt IS NULL OR jsonb_typeof(share_receipt) = 'object')
);
CREATE INDEX IF NOT EXISTS runs_player_started_idx ON public.runs (player_id, started_at DESC);
CREATE INDEX IF NOT EXISTS runs_active_idx         ON public.runs (player_id) WHERE ended_at IS NULL;
CREATE INDEX IF NOT EXISTS runs_perma_idx          ON public.runs (player_id) WHERE hard_perma = true;
COMMENT ON TABLE  public.runs IS
    'Roguelike-loop run records. ended_at = NULL → active run. share_receipt populated by run_archive when player gifts run-summary to friend (gift-economy).';
COMMENT ON COLUMN public.runs.run_seed      IS 'Deterministic seed (≤256 bytes) for procgen replay.';
COMMENT ON COLUMN public.runs.biome_path    IS 'Ordered biome IDs traversed during run (e.g. {forest,catacomb,vault}).';
COMMENT ON COLUMN public.runs.echoes_earned IS 'Soft-currency earned this run · feeds player_progression.total_echoes.';
COMMENT ON COLUMN public.runs.hard_perma    IS 'TRUE iff run was hard-perma (ironman) · loss = no echoes carried over.';
COMMENT ON COLUMN public.runs.share_receipt IS 'Optional jsonb gift-receipt {friend_id, sent_at, accepted_at}. Cross-refs run_share_feed.';

-- § run_share_feed · gift-economy · run-summary share-feed (friend-visible)
CREATE TABLE IF NOT EXISTS public.run_share_feed (
    share_id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id                uuid        NOT NULL REFERENCES public.runs(run_id) ON DELETE CASCADE,
    player_id             uuid        NOT NULL,
    friend_id             uuid        NOT NULL,
    sent_at               timestamptz NOT NULL DEFAULT now(),
    accepted_at           timestamptz NULL,
    gift_credit_received  boolean     NOT NULL DEFAULT false,
    CONSTRAINT run_share_feed_self_check
        CHECK (player_id <> friend_id),
    CONSTRAINT run_share_feed_accepted_after_sent
        CHECK (accepted_at IS NULL OR accepted_at >= sent_at),
    UNIQUE (run_id, friend_id)
);
CREATE INDEX IF NOT EXISTS run_share_feed_player_idx ON public.run_share_feed (player_id, sent_at DESC);
CREATE INDEX IF NOT EXISTS run_share_feed_friend_idx ON public.run_share_feed (friend_id, sent_at DESC);
CREATE INDEX IF NOT EXISTS run_share_feed_pending_idx
    ON public.run_share_feed (friend_id) WHERE accepted_at IS NULL;
COMMENT ON TABLE  public.run_share_feed IS
    'Gift-economy share-feed for completed runs. Player gifts run-summary → friend accepts → gift_credit_received true. SELECT visible to author + recipient.';

-- § helper · run_archive — atomic finalize + populate share_receipt scaffolding
CREATE OR REPLACE FUNCTION public.run_archive(p_run_id uuid)
RETURNS uuid LANGUAGE plpgsql SECURITY DEFINER AS $$
DECLARE
    v_player uuid;
    v_ended  timestamptz;
BEGIN
    IF p_run_id IS NULL THEN RETURN NULL; END IF;
    SELECT player_id, ended_at INTO v_player, v_ended FROM public.runs WHERE run_id = p_run_id;
    IF v_player IS NULL THEN RETURN NULL; END IF;
    IF v_ended IS NOT NULL THEN
        -- already archived · idempotent · return run_id
        RETURN p_run_id;
    END IF;
    UPDATE public.runs
       SET ended_at      = now(),
           end_reason    = COALESCE(end_reason, 'abandoned'),
           share_receipt = COALESCE(share_receipt, jsonb_build_object('archived_at', now()))
     WHERE run_id = p_run_id;
    RETURN p_run_id;
END;
$$;
COMMENT ON FUNCTION public.run_archive(uuid) IS
    'SECURITY DEFINER · finalize an active run (idempotent). Sets ended_at = now, default end_reason = abandoned. Audit-emit-compatible.';

-- § RLS
ALTER TABLE public.runs            ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.run_share_feed  ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS "runs_select_self"            ON public.runs;
DROP POLICY IF EXISTS "runs_insert_self"            ON public.runs;
DROP POLICY IF EXISTS "runs_update_self"            ON public.runs;
DROP POLICY IF EXISTS "run_share_feed_select_party" ON public.run_share_feed;
DROP POLICY IF EXISTS "run_share_feed_insert_self"  ON public.run_share_feed;
DROP POLICY IF EXISTS "run_share_feed_update_party" ON public.run_share_feed;

CREATE POLICY "runs_select_self"
    ON public.runs FOR SELECT
    USING (auth.uid() = player_id OR auth.role() = 'service_role');
CREATE POLICY "runs_insert_self"
    ON public.runs FOR INSERT
    WITH CHECK ((auth.uid() IS NOT NULL AND auth.uid() = player_id) OR auth.role() = 'service_role');
CREATE POLICY "runs_update_self"
    ON public.runs FOR UPDATE
    USING      (auth.uid() = player_id OR auth.role() = 'service_role')
    WITH CHECK (auth.uid() = player_id OR auth.role() = 'service_role');
-- N! NO DELETE policy on runs · service-role only

-- share_feed : friend OR self-author can SELECT · only author can INSERT · either party UPDATE accepted_at
CREATE POLICY "run_share_feed_select_party"
    ON public.run_share_feed FOR SELECT
    USING (auth.uid() = player_id OR auth.uid() = friend_id OR auth.role() = 'service_role');
CREATE POLICY "run_share_feed_insert_self"
    ON public.run_share_feed FOR INSERT
    WITH CHECK ((auth.uid() IS NOT NULL AND auth.uid() = player_id) OR auth.role() = 'service_role');
CREATE POLICY "run_share_feed_update_party"
    ON public.run_share_feed FOR UPDATE
    USING      (auth.uid() = player_id OR auth.uid() = friend_id OR auth.role() = 'service_role')
    WITH CHECK (auth.uid() = player_id OR auth.uid() = friend_id OR auth.role() = 'service_role');

-- § grants
GRANT SELECT, INSERT, UPDATE ON public.runs           TO authenticated;
GRANT SELECT, INSERT, UPDATE ON public.run_share_feed TO authenticated;
GRANT ALL                    ON public.runs           TO service_role;
GRANT ALL                    ON public.run_share_feed TO service_role;
GRANT EXECUTE ON FUNCTION public.run_archive(uuid) TO authenticated, service_role;
