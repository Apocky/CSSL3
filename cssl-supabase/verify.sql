-- =====================================================================
-- § T11-WAVE3-SUPABASE · verify.sql
-- Post-migration assertions. Run after 0001 + 0002 + 0003 + seed.sql.
-- Each block raises an exception on failure (transaction rolls back).
-- Manual run :
--   psql "$SUPABASE_DB_URL" -f verify.sql
-- =====================================================================

DO $$
DECLARE
    v_count bigint;
BEGIN
    -- 1. Tables exist
    PERFORM 'public.assets'::regclass;
    PERFORM 'public.scenes'::regclass;
    PERFORM 'public.history'::regclass;
    PERFORM 'public.companion_logs'::regclass;

    -- 2. RLS enabled on every table
    SELECT count(*) INTO v_count
      FROM pg_tables
     WHERE schemaname = 'public'
       AND tablename IN ('assets','scenes','history','companion_logs')
       AND rowsecurity = true;
    IF v_count <> 4 THEN
        RAISE EXCEPTION 'RLS not enabled on all 4 tables (expected 4, got %)', v_count;
    END IF;

    -- 3. Seed data loaded
    SELECT count(*) INTO v_count FROM public.assets;
    IF v_count < 10 THEN
        RAISE EXCEPTION 'Expected >=10 seed asset rows, got %', v_count;
    END IF;

    -- 4. Storage buckets exist
    SELECT count(*) INTO v_count
      FROM storage.buckets
     WHERE id IN ('assets','screenshots','audio');
    IF v_count <> 3 THEN
        RAISE EXCEPTION 'Expected 3 storage buckets, got %', v_count;
    END IF;

    -- 5. assets bucket public, others private
    SELECT count(*) INTO v_count
      FROM storage.buckets
     WHERE id = 'assets' AND public = true;
    IF v_count <> 1 THEN
        RAISE EXCEPTION 'assets bucket should be public';
    END IF;

    SELECT count(*) INTO v_count
      FROM storage.buckets
     WHERE id IN ('screenshots','audio') AND public = false;
    IF v_count <> 2 THEN
        RAISE EXCEPTION 'screenshots+audio buckets should be private';
    END IF;

    -- 6. RPC functions exist
    PERFORM 'public.scene_record_play(uuid)'::regprocedure;
    PERFORM 'public.companion_log_append(text,text,jsonb,boolean,text)'::regprocedure;

    -- 7. Required indexes exist
    SELECT count(*) INTO v_count
      FROM pg_indexes
     WHERE schemaname = 'public'
       AND indexname IN (
            'assets_license_idx', 'assets_source_idx', 'assets_format_idx',
            'scenes_user_id_idx', 'scenes_is_public_idx',
            'history_user_id_idx', 'history_created_at_idx',
            'companion_logs_user_id_idx', 'companion_logs_created_at_idx'
       );
    IF v_count < 9 THEN
        RAISE EXCEPTION 'Expected >=9 indexes, got %', v_count;
    END IF;

    -- 8. RLS policies present (>= one per table per relevant verb)
    SELECT count(*) INTO v_count
      FROM pg_policies
     WHERE schemaname = 'public'
       AND tablename IN ('assets','scenes','history','companion_logs');
    IF v_count < 14 THEN
        RAISE EXCEPTION 'Expected >=14 RLS policies, got %', v_count;
    END IF;

    RAISE NOTICE 'verify.sql : all assertions passed';
END$$;
