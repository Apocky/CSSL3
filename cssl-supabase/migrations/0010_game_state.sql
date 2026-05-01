-- =====================================================================
-- § T11-W5c-SUPABASE-GAMESTATE · 0010_game_state.sql
-- Cross-session game-state snapshots for the LoA / DM-engine.
--
-- A game-state snapshot is a serialized capture of :
--   1. the DM scene-graph (causal-seed DAG + entity bag at instant t)
--   2. a content-hash digest of the ω-field state (sha256-hex)
--   3. an optional pointer to the full ω-field tensor in storage
--      (the field is ~MB-scale per scene; we don't inline it here)
--   4. the companion-history append-log (sovereign-cap conversation context)
--
-- A game-session is a logical container : one sitting from boot → save/quit.
-- Snapshots are append-only with monotonic seq per session ; the session
-- index tracks the latest_seq + total_snapshots for fast lookup.
--
-- Apply order : after 0001-0009.
-- =====================================================================

-- pgcrypto is loaded by 0001_initial.sql ; reassert defensively
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- =====================================================================
-- public.game_session_index · one row per logical play-session
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.game_session_index (
    session_id        uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id         text        NOT NULL,
    started_at        timestamptz NOT NULL DEFAULT now(),
    ended_at          timestamptz,                          -- NULL = active
    latest_seq        bigint      NOT NULL DEFAULT 0,
    total_snapshots   bigint      NOT NULL DEFAULT 0,
    meta              jsonb       NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT game_session_index_player_id_length
        CHECK (char_length(player_id) BETWEEN 1 AND 200),
    CONSTRAINT game_session_index_latest_seq_nonneg
        CHECK (latest_seq >= 0),
    CONSTRAINT game_session_index_total_nonneg
        CHECK (total_snapshots >= 0),
    CONSTRAINT game_session_index_ended_after_started
        CHECK (ended_at IS NULL OR ended_at >= started_at)
);

-- Per-player history index (timeline)
CREATE INDEX IF NOT EXISTS game_session_index_player_started_idx
    ON public.game_session_index (player_id, started_at DESC);

-- Active-sessions partial index (NULL ended_at = currently in-progress)
CREATE INDEX IF NOT EXISTS game_session_index_active_partial_idx
    ON public.game_session_index (started_at DESC)
    WHERE ended_at IS NULL;

COMMENT ON TABLE public.game_session_index IS
    'One row per logical play-session. ended_at IS NULL while active. latest_seq + total_snapshots are maintained by record_snapshot().';
COMMENT ON COLUMN public.game_session_index.session_id IS
    'Stable session identifier. Generated client-side or via gen_random_uuid() default. Same session_id is referenced by every snapshot in the session.';
COMMENT ON COLUMN public.game_session_index.latest_seq IS
    'Highest seq written to game_state_snapshots for this session. Maintained by record_snapshot() ; readers can fetch latest with seq = latest_seq.';
COMMENT ON COLUMN public.game_session_index.meta IS
    'Free-form session metadata : engine_version, scene_id, difficulty_settings, etc.';

-- =====================================================================
-- public.game_state_snapshots · append-only DM-state captures
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.game_state_snapshots (
    id                  bigserial   PRIMARY KEY,
    session_id          uuid        NOT NULL DEFAULT gen_random_uuid(),
    player_id           text        NOT NULL,
    seq                 bigint      NOT NULL,
    scene_graph         jsonb       NOT NULL,
    omega_field_digest  text        NOT NULL,
    omega_field_url     text,
    companion_history   jsonb       NOT NULL DEFAULT '[]'::jsonb,
    created_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT game_state_snapshots_session_seq_unique
        UNIQUE (session_id, seq),
    CONSTRAINT game_state_snapshots_player_id_length
        CHECK (char_length(player_id) BETWEEN 1 AND 200),
    CONSTRAINT game_state_snapshots_seq_nonneg
        CHECK (seq >= 0),
    CONSTRAINT game_state_snapshots_omega_digest_format
        CHECK (omega_field_digest ~ '^[0-9a-f]{64}$'),
    CONSTRAINT game_state_snapshots_omega_url_length
        CHECK (omega_field_url IS NULL OR char_length(omega_field_url) BETWEEN 1 AND 2048),
    CONSTRAINT game_state_snapshots_companion_history_is_array
        CHECK (jsonb_typeof(companion_history) = 'array')
);

-- Per-player timeline (most-recent-first)
CREATE INDEX IF NOT EXISTS game_state_snapshots_player_created_idx
    ON public.game_state_snapshots (player_id, created_at DESC);

-- Per-session monotonic-seq (DESC for "latest" lookups)
CREATE INDEX IF NOT EXISTS game_state_snapshots_session_seq_desc_idx
    ON public.game_state_snapshots (session_id, seq DESC);

-- ω-field-digest dedup lookup (e.g. content-addressed cache)
CREATE INDEX IF NOT EXISTS game_state_snapshots_omega_digest_idx
    ON public.game_state_snapshots (omega_field_digest);

COMMENT ON TABLE public.game_state_snapshots IS
    'Append-only DM scene-graph + ω-field-digest captures. UNIQUE(session_id, seq) prevents replay duplicates. Use record_snapshot() to atomically write + bump session_index.';
COMMENT ON COLUMN public.game_state_snapshots.scene_graph IS
    'Serialized DM causal-seed DAG + entity bag at the captured moment. JSONB so the engine can rehydrate any cross-version (with migration).';
COMMENT ON COLUMN public.game_state_snapshots.omega_field_digest IS
    'sha256-hex of the ω-field tensor at capture-time. Always computed even when omega_field_url is NULL — supports content-addressed dedup + integrity check on rehydrate.';
COMMENT ON COLUMN public.game_state_snapshots.omega_field_url IS
    'Optional pointer to the full ω-field bytes in a storage bucket. NULL = field is regenerable from scene_graph (deterministic seed) ; non-NULL = explicitly archived.';
COMMENT ON COLUMN public.game_state_snapshots.companion_history IS
    'Append-log of companion (sovereign-cap) interactions within the session up to this snapshot. JSONB array ; each entry is {ts, sovereign_handle, op, params}.';

-- =====================================================================
-- record_snapshot() · atomic INSERT + session-index bookkeeping
-- =====================================================================
-- Inserts a new snapshot row AND updates the matching game_session_index
-- entry (creating it if missing). Returns the newly-assigned snapshot id.
--
-- Caller-visible behavior :
--   * Pass p_session = NULL or '00000000-...'::uuid to start a new session
--     (record_snapshot will gen_random_uuid() and seq=0).
--   * Pass an existing p_session to append : seq is monotonic = latest_seq+1.
--   * If p_player does not match the session_index.player_id we error
--     out (defensive — RLS would also block but this gives a clear msg).
-- =====================================================================
CREATE OR REPLACE FUNCTION public.record_snapshot(
    p_session  uuid,
    p_player   text,
    p_scene    jsonb,
    p_digest   text,
    p_url      text,
    p_history  jsonb
) RETURNS bigint
    LANGUAGE plpgsql AS
$$
DECLARE
    v_session_id  uuid := p_session;
    v_next_seq    bigint;
    v_existing    text;
    v_new_id      bigint;
BEGIN
    IF p_player IS NULL OR char_length(p_player) = 0 THEN
        RAISE EXCEPTION 'record_snapshot : p_player required';
    END IF;
    IF p_scene IS NULL OR jsonb_typeof(p_scene) <> 'object' THEN
        RAISE EXCEPTION 'record_snapshot : p_scene must be a JSON object';
    END IF;
    IF p_digest IS NULL OR p_digest !~ '^[0-9a-f]{64}$' THEN
        RAISE EXCEPTION 'record_snapshot : p_digest must be 64-char hex (sha256)';
    END IF;
    IF p_history IS NULL THEN
        p_history := '[]'::jsonb;
    END IF;
    IF jsonb_typeof(p_history) <> 'array' THEN
        RAISE EXCEPTION 'record_snapshot : p_history must be a JSON array';
    END IF;

    -- Generate session_id if caller did not pass one
    IF v_session_id IS NULL THEN
        v_session_id := gen_random_uuid();
    END IF;

    -- Upsert session_index row + lock for atomic seq increment
    INSERT INTO public.game_session_index (session_id, player_id)
    VALUES (v_session_id, p_player)
    ON CONFLICT (session_id) DO NOTHING;

    SELECT player_id INTO v_existing
      FROM public.game_session_index
     WHERE session_id = v_session_id
     FOR UPDATE;

    IF v_existing IS NULL THEN
        RAISE EXCEPTION 'record_snapshot : session_index row missing after upsert (session=%)', v_session_id;
    END IF;
    IF v_existing <> p_player THEN
        RAISE EXCEPTION 'record_snapshot : session % already owned by a different player', v_session_id;
    END IF;

    -- Pick next seq = latest_seq + 1
    SELECT latest_seq + 1 INTO v_next_seq
      FROM public.game_session_index
     WHERE session_id = v_session_id;

    -- Insert the snapshot
    INSERT INTO public.game_state_snapshots (
        session_id, player_id, seq,
        scene_graph, omega_field_digest, omega_field_url, companion_history
    ) VALUES (
        v_session_id, p_player, v_next_seq,
        p_scene, p_digest, p_url, p_history
    )
    RETURNING id INTO v_new_id;

    -- Bump session_index counters
    UPDATE public.game_session_index
       SET latest_seq      = v_next_seq,
           total_snapshots = total_snapshots + 1
     WHERE session_id = v_session_id;

    RETURN v_new_id;
END;
$$;

COMMENT ON FUNCTION public.record_snapshot IS
    'Atomic snapshot append : INSERT into game_state_snapshots + UPSERT/UPDATE game_session_index (latest_seq, total_snapshots). Pass NULL p_session to start a new session.';

-- =====================================================================
-- latest_snapshot() · most-recent snapshot for a session
-- =====================================================================
CREATE OR REPLACE FUNCTION public.latest_snapshot(
    p_session uuid
) RETURNS SETOF public.game_state_snapshots
    LANGUAGE sql STABLE AS
$$
    SELECT *
      FROM public.game_state_snapshots
     WHERE session_id = p_session
     ORDER BY seq DESC
     LIMIT 1;
$$;

COMMENT ON FUNCTION public.latest_snapshot IS
    'Returns the most-recent snapshot for a session (ORDER BY seq DESC LIMIT 1). SETOF for stable row-shape. Returns 0 rows if the session has no snapshots.';

-- =====================================================================
-- end_session() · mark a session as ended
-- =====================================================================
CREATE OR REPLACE FUNCTION public.end_session(
    p_session uuid
) RETURNS timestamptz
    LANGUAGE plpgsql AS
$$
DECLARE
    v_now timestamptz := now();
BEGIN
    UPDATE public.game_session_index
       SET ended_at = v_now
     WHERE session_id = p_session
       AND ended_at IS NULL;

    IF NOT FOUND THEN
        -- Either session does not exist or already ended ; idempotent no-op
        RETURN NULL;
    END IF;
    RETURN v_now;
END;
$$;

COMMENT ON FUNCTION public.end_session IS
    'Marks a session as ended (sets ended_at = now()). Idempotent : returns NULL if session does not exist or is already ended ; returns the timestamp on success.';

-- =====================================================================
-- Function privileges (RLS still gates row visibility)
-- =====================================================================
REVOKE ALL ON FUNCTION public.record_snapshot(uuid, text, jsonb, text, text, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION public.latest_snapshot(uuid)                                  FROM PUBLIC;
REVOKE ALL ON FUNCTION public.end_session(uuid)                                      FROM PUBLIC;

GRANT EXECUTE ON FUNCTION public.record_snapshot(uuid, text, jsonb, text, text, jsonb)
    TO authenticated, service_role;
GRANT EXECUTE ON FUNCTION public.latest_snapshot(uuid)
    TO authenticated, service_role;
GRANT EXECUTE ON FUNCTION public.end_session(uuid)
    TO authenticated, service_role;
