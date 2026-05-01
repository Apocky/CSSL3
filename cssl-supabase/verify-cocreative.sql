-- =====================================================================
-- § T11-W5b-SUPABASE-COCREATIVE · verify-cocreative.sql
-- Post-migration assertions for 0007 + 0008 + 0009.
-- Run after the original verify.sql + verify-signaling.sql, OR standalone
-- once the 3 cocreative migrations are in place.
--   psql "$SUPABASE_DB_URL" -f verify-cocreative.sql
-- =====================================================================

DO $$
DECLARE
    v_count             bigint;
    v_bias_count        bigint;
    v_feedback_count    bigint;
    v_snapshot_count    bigint;
    v_demo_dim          integer;
    v_demo_theta_len    integer;
    v_demo_bias_id      uuid;
    v_latest_seq        bigint;
BEGIN
    -- 1. New tables exist
    PERFORM 'public.cocreative_bias_vectors'::regclass;
    PERFORM 'public.cocreative_feedback_events'::regclass;
    PERFORM 'public.cocreative_optimizer_snapshots'::regclass;

    -- 2. RLS enabled on every new table
    SELECT count(*) INTO v_count
      FROM pg_tables
     WHERE schemaname = 'public'
       AND tablename IN (
           'cocreative_bias_vectors',
           'cocreative_feedback_events',
           'cocreative_optimizer_snapshots'
       )
       AND rowsecurity = true;
    IF v_count <> 3 THEN
        RAISE EXCEPTION 'verify-cocreative : RLS not enabled on all 3 cocreative tables (expected 3, got %)', v_count;
    END IF;

    -- 3. Helper functions exist (with exact signatures)
    PERFORM 'public.update_bias_with_step(uuid, jsonb, bigint, real, real)'::regprocedure;
    PERFORM 'public.latest_snapshot_for_player(text)'::regprocedure;

    -- 4. Required indexes exist
    SELECT count(*) INTO v_count
      FROM pg_indexes
     WHERE schemaname = 'public'
       AND indexname IN (
           'cocreative_bias_vectors_updated_at_idx',
           'cocreative_feedback_events_player_id_idx',
           'cocreative_feedback_events_bias_id_idx',
           'cocreative_feedback_events_recorded_at_desc_idx',
           'cocreative_optimizer_snapshots_bias_id_idx',
           'cocreative_optimizer_snapshots_created_at_desc_idx',
           'cocreative_optimizer_snapshots_bias_seq_desc_idx'
       );
    IF v_count < 7 THEN
        RAISE EXCEPTION 'verify-cocreative : expected >=7 cocreative indexes, got %', v_count;
    END IF;

    -- 5. Unique constraints
    SELECT count(*) INTO v_count
      FROM pg_constraint
     WHERE conrelid = 'public.cocreative_bias_vectors'::regclass
       AND contype = 'u'
       AND conname = 'cocreative_bias_vectors_player_unique';
    IF v_count <> 1 THEN
        RAISE EXCEPTION 'verify-cocreative : cocreative_bias_vectors UNIQUE(player_id) missing';
    END IF;

    SELECT count(*) INTO v_count
      FROM pg_constraint
     WHERE conrelid = 'public.cocreative_optimizer_snapshots'::regclass
       AND contype = 'u'
       AND conname = 'cocreative_optimizer_snapshots_seq_unique';
    IF v_count <> 1 THEN
        RAISE EXCEPTION 'verify-cocreative : cocreative_optimizer_snapshots UNIQUE(bias_id, seq) missing';
    END IF;

    -- 6. CHECK constraints (dim range + kind enum + score-iff-scalar guard)
    SELECT count(*) INTO v_count
      FROM pg_constraint
     WHERE conrelid = 'public.cocreative_bias_vectors'::regclass
       AND contype = 'c'
       AND conname IN (
           'cocreative_bias_vectors_dim_range',
           'cocreative_bias_vectors_theta_length_matches_dim',
           'cocreative_bias_vectors_lr_range',
           'cocreative_bias_vectors_momentum_range'
       );
    IF v_count < 4 THEN
        RAISE EXCEPTION 'verify-cocreative : expected 4 CHECK on bias_vectors, got %', v_count;
    END IF;

    SELECT count(*) INTO v_count
      FROM pg_constraint
     WHERE conrelid = 'public.cocreative_feedback_events'::regclass
       AND contype = 'c'
       AND conname IN (
           'cocreative_feedback_events_kind_check',
           'cocreative_feedback_events_score_present_iff_scalar',
           'cocreative_feedback_events_comment_present_iff_comment'
       );
    IF v_count < 3 THEN
        RAISE EXCEPTION 'verify-cocreative : expected 3 CHECK on feedback_events, got %', v_count;
    END IF;

    -- 7. FK CASCADE behavior asserted (feedback + snapshots → bias_vectors)
    SELECT count(*) INTO v_count
      FROM pg_constraint
     WHERE contype = 'f'
       AND conrelid IN (
           'public.cocreative_feedback_events'::regclass,
           'public.cocreative_optimizer_snapshots'::regclass
       )
       AND confrelid = 'public.cocreative_bias_vectors'::regclass
       AND confdeltype = 'c'; -- 'c' = CASCADE
    IF v_count < 2 THEN
        RAISE EXCEPTION 'verify-cocreative : expected 2 CASCADE FKs into bias_vectors, got %', v_count;
    END IF;

    -- 8. RLS policies present (8 across the 3 tables)
    SELECT count(*) INTO v_count
      FROM pg_policies
     WHERE schemaname = 'public'
       AND tablename IN (
           'cocreative_bias_vectors',
           'cocreative_feedback_events',
           'cocreative_optimizer_snapshots'
       );
    IF v_count < 8 THEN
        RAISE EXCEPTION 'verify-cocreative : expected >=8 cocreative RLS policies, got %', v_count;
    END IF;

    -- 9. Seed data loaded (demo-player + 4 feedback + 2 snapshots)
    SELECT count(*) INTO v_bias_count
      FROM public.cocreative_bias_vectors
     WHERE player_id = 'demo-player';
    IF v_bias_count < 1 THEN
        RAISE EXCEPTION 'verify-cocreative : demo-player seed bias missing';
    END IF;

    SELECT id, dim, jsonb_array_length(theta)
      INTO v_demo_bias_id, v_demo_dim, v_demo_theta_len
      FROM public.cocreative_bias_vectors
     WHERE player_id = 'demo-player';
    IF v_demo_dim <> 16 THEN
        RAISE EXCEPTION 'verify-cocreative : demo-player dim expected 16, got %', v_demo_dim;
    END IF;
    IF v_demo_theta_len <> v_demo_dim THEN
        RAISE EXCEPTION 'verify-cocreative : demo-player theta-length=% does not equal dim=%',
            v_demo_theta_len, v_demo_dim;
    END IF;

    SELECT count(*) INTO v_feedback_count
      FROM public.cocreative_feedback_events
     WHERE player_id = 'demo-player';
    IF v_feedback_count < 4 THEN
        RAISE EXCEPTION 'verify-cocreative : expected >=4 demo feedback events, got %', v_feedback_count;
    END IF;

    SELECT count(*) INTO v_snapshot_count
      FROM public.cocreative_optimizer_snapshots
     WHERE bias_id = v_demo_bias_id;
    IF v_snapshot_count < 2 THEN
        RAISE EXCEPTION 'verify-cocreative : expected >=2 demo snapshots, got %', v_snapshot_count;
    END IF;

    -- 10. latest_snapshot_for_player smoke-test (returns highest seq)
    SELECT seq INTO v_latest_seq
      FROM public.latest_snapshot_for_player('demo-player');
    IF v_latest_seq IS NULL THEN
        RAISE EXCEPTION 'verify-cocreative : latest_snapshot_for_player returned no row for demo-player';
    END IF;
    IF v_latest_seq < 1 THEN
        RAISE EXCEPTION
            'verify-cocreative : latest_snapshot_for_player returned seq=% (expected >=1)', v_latest_seq;
    END IF;

    RAISE NOTICE
        'verify-cocreative.sql : all assertions passed (bias=%, feedback=%, snapshots=%, latest_seq=%)',
        v_bias_count, v_feedback_count, v_snapshot_count, v_latest_seq;
END$$;
