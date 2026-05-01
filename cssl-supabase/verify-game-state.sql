-- =====================================================================
-- § T11-W5c-SUPABASE-GAMESTATE · verify-game-state.sql
-- Post-migration assertions for 0010 + 0011 + 0012 + 0013.
-- Run after the original verify*.sql files, OR standalone once the
-- 4 game-state migrations are in place :
--   psql "$SUPABASE_DB_URL" -f verify-game-state.sql
-- =====================================================================

DO $$
DECLARE
    v_count           bigint;
    v_session_count   bigint;
    v_snapshot_count  bigint;
    v_audit_count     bigint;
    v_demo_session    uuid := '00000000-0000-0000-0000-00000000DE01'::uuid;
    v_latest_seq      bigint;
    v_summary_uses    bigint;
BEGIN
    -- 1. New tables exist
    PERFORM 'public.game_state_snapshots'::regclass;
    PERFORM 'public.game_session_index'::regclass;
    PERFORM 'public.sovereign_cap_audit'::regclass;
    PERFORM 'public.sovereign_cap_audit_summary'::regclass;

    -- 2. RLS enabled on every new BASE table (the view inherits)
    SELECT count(*) INTO v_count
      FROM pg_tables
     WHERE schemaname = 'public'
       AND tablename IN (
           'game_state_snapshots',
           'game_session_index',
           'sovereign_cap_audit'
       )
       AND rowsecurity = true;
    IF v_count <> 3 THEN
        RAISE EXCEPTION 'verify-game-state : RLS not enabled on all 3 game-state tables (expected 3, got %)', v_count;
    END IF;

    -- 3. Helper functions exist (with exact signatures)
    PERFORM 'public.record_snapshot(uuid, text, jsonb, text, text, jsonb)'::regprocedure;
    PERFORM 'public.latest_snapshot(uuid)'::regprocedure;
    PERFORM 'public.end_session(uuid)'::regprocedure;

    -- 4. Required indexes exist
    SELECT count(*) INTO v_count
      FROM pg_indexes
     WHERE schemaname = 'public'
       AND indexname IN (
           'game_session_index_player_started_idx',
           'game_session_index_active_partial_idx',
           'game_state_snapshots_player_created_idx',
           'game_state_snapshots_session_seq_desc_idx',
           'game_state_snapshots_omega_digest_idx',
           'sovereign_cap_audit_player_ts_desc_idx',
           'sovereign_cap_audit_action_kind_idx',
           'sovereign_cap_audit_session_ts_asc_idx'
       );
    IF v_count < 8 THEN
        RAISE EXCEPTION 'verify-game-state : expected >=8 game-state indexes, got %', v_count;
    END IF;

    -- 5. UNIQUE constraint on (session_id, seq)
    SELECT count(*) INTO v_count
      FROM pg_constraint
     WHERE conrelid = 'public.game_state_snapshots'::regclass
       AND contype = 'u'
       AND conname = 'game_state_snapshots_session_seq_unique';
    IF v_count <> 1 THEN
        RAISE EXCEPTION 'verify-game-state : game_state_snapshots UNIQUE(session_id, seq) missing';
    END IF;

    -- 6. CHECK constraints (digest format + ended-after-started)
    SELECT count(*) INTO v_count
      FROM pg_constraint
     WHERE conrelid = 'public.game_state_snapshots'::regclass
       AND contype = 'c'
       AND conname IN (
           'game_state_snapshots_seq_nonneg',
           'game_state_snapshots_omega_digest_format',
           'game_state_snapshots_companion_history_is_array'
       );
    IF v_count < 3 THEN
        RAISE EXCEPTION 'verify-game-state : expected 3 CHECK on game_state_snapshots, got %', v_count;
    END IF;

    SELECT count(*) INTO v_count
      FROM pg_constraint
     WHERE conrelid = 'public.game_session_index'::regclass
       AND contype = 'c'
       AND conname IN (
           'game_session_index_latest_seq_nonneg',
           'game_session_index_total_nonneg',
           'game_session_index_ended_after_started'
       );
    IF v_count < 3 THEN
        RAISE EXCEPTION 'verify-game-state : expected 3 CHECK on game_session_index, got %', v_count;
    END IF;

    -- 7. RLS policies present (8 across the 3 tables)
    SELECT count(*) INTO v_count
      FROM pg_policies
     WHERE schemaname = 'public'
       AND tablename IN (
           'game_state_snapshots',
           'game_session_index',
           'sovereign_cap_audit'
       );
    IF v_count < 8 THEN
        RAISE EXCEPTION 'verify-game-state : expected >=8 game-state RLS policies, got %', v_count;
    END IF;

    -- 8. TRANSPARENCY INVARIANT : sovereign_cap_audit must NOT have
    --    UPDATE or DELETE policies. Once written, immutable.
    SELECT count(*) INTO v_count
      FROM pg_policies
     WHERE schemaname = 'public'
       AND tablename  = 'sovereign_cap_audit'
       AND cmd IN ('UPDATE', 'DELETE');
    IF v_count <> 0 THEN
        RAISE EXCEPTION
            'verify-game-state : sovereign_cap_audit has % UPDATE/DELETE policies (must be 0 — transparency invariant)',
            v_count;
    END IF;

    -- 9. Seed data : demo session exists with 3 snapshots
    SELECT count(*) INTO v_session_count
      FROM public.game_session_index
     WHERE session_id = v_demo_session;
    IF v_session_count <> 1 THEN
        RAISE EXCEPTION 'verify-game-state : demo session row missing (session=%)', v_demo_session;
    END IF;

    SELECT count(*) INTO v_snapshot_count
      FROM public.game_state_snapshots
     WHERE session_id = v_demo_session;
    IF v_snapshot_count < 3 THEN
        RAISE EXCEPTION 'verify-game-state : expected >=3 demo snapshots, got %', v_snapshot_count;
    END IF;

    -- 10. latest_snapshot() smoke-test (returns highest seq)
    SELECT seq INTO v_latest_seq
      FROM public.latest_snapshot(v_demo_session);
    IF v_latest_seq IS NULL THEN
        RAISE EXCEPTION 'verify-game-state : latest_snapshot() returned no row for demo session';
    END IF;
    IF v_latest_seq < 2 THEN
        RAISE EXCEPTION
            'verify-game-state : latest_snapshot returned seq=% (expected >=2)', v_latest_seq;
    END IF;

    -- 11. Sovereign-cap audit seed entries
    SELECT count(*) INTO v_audit_count
      FROM public.sovereign_cap_audit
     WHERE session_id = v_demo_session;
    IF v_audit_count < 2 THEN
        RAISE EXCEPTION 'verify-game-state : expected >=2 sovereign_cap_audit seed rows, got %', v_audit_count;
    END IF;

    -- 12. Summary view returns aggregate per-action counts
    SELECT sum(uses) INTO v_summary_uses
      FROM public.sovereign_cap_audit_summary
     WHERE player_id = 'demo-player';
    IF v_summary_uses IS NULL OR v_summary_uses < 2 THEN
        RAISE EXCEPTION
            'verify-game-state : sovereign_cap_audit_summary aggregated uses=% (expected >=2)', v_summary_uses;
    END IF;

    RAISE NOTICE
        'verify-game-state.sql : all assertions passed (sessions=%, snapshots=%, audit=%, latest_seq=%)',
        v_session_count, v_snapshot_count, v_audit_count, v_latest_seq;
END$$;
