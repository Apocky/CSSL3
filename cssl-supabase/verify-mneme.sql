-- =====================================================================
-- verify-mneme.sql · MNEME schema verification (smoke tests)
-- ════════════════════════════════════════════════════════════════════
-- Run AFTER applying 0040_mneme.sql + 0041_mneme_rls.sql.
-- All assertions are smoke-tests; production tests live in
-- cssl-edge/tests/api/mneme/*.test.ts and cssl-supabase/tests/mneme.sql.
-- =====================================================================

\echo '=== verify-mneme: extensions ==='
SELECT extname FROM pg_extension WHERE extname IN ('pgcrypto','pg_trgm','vector')
ORDER BY extname;

\echo ''
\echo '=== verify-mneme: tables exist ==='
SELECT tablename FROM pg_tables
 WHERE schemaname = 'public' AND tablename LIKE 'mneme_%'
 ORDER BY tablename;

\echo ''
\echo '=== verify-mneme: columns on mneme_memories ==='
SELECT column_name, data_type
  FROM information_schema.columns
 WHERE table_schema = 'public' AND table_name = 'mneme_memories'
 ORDER BY ordinal_position;

\echo ''
\echo '=== verify-mneme: indexes on mneme_memories ==='
SELECT indexname, indexdef
  FROM pg_indexes
 WHERE schemaname = 'public' AND tablename = 'mneme_memories'
 ORDER BY indexname;

\echo ''
\echo '=== verify-mneme: triggers ==='
SELECT trigger_name, event_object_table, action_timing, event_manipulation
  FROM information_schema.triggers
 WHERE trigger_schema = 'public' AND trigger_name LIKE 'mneme_%'
 ORDER BY trigger_name;

\echo ''
\echo '=== verify-mneme: RLS enabled ==='
SELECT tablename, rowsecurity
  FROM pg_tables
 WHERE schemaname = 'public' AND tablename LIKE 'mneme_%'
 ORDER BY tablename;

\echo ''
\echo '=== verify-mneme: policies ==='
SELECT tablename, policyname, cmd, roles
  FROM pg_policies
 WHERE schemaname = 'public' AND tablename LIKE 'mneme_%'
 ORDER BY tablename, policyname;

\echo ''
\echo '=== verify-mneme: insert + supersede smoke test (service role) ==='
BEGIN;
    INSERT INTO public.mneme_profiles(profile_id, sovereign_pk, sigma_mask)
    VALUES ('verify-test',
            decode('aa', 'hex') || decode(repeat('00', 31), 'hex'),
            decode(repeat('00', 32), 'hex'));

    -- First fact (active)
    INSERT INTO public.mneme_memories(profile_id, type, csl, paraphrase, topic_key,
                                      search_queries, sigma_mask)
    VALUES ('verify-test', 'fact',
            'user.pref.pkg-mgr ⊗ pnpm', 'User prefers pnpm.', 'user.pref.pkg-mgr',
            '["which package manager?","pnpm or npm?","what JS package tool?"]'::jsonb,
            decode(repeat('00', 32), 'hex'));

    -- Superseding fact
    INSERT INTO public.mneme_memories(profile_id, type, csl, paraphrase, topic_key,
                                      search_queries, sigma_mask)
    VALUES ('verify-test', 'fact',
            'user.pref.pkg-mgr ⊗ yarn', 'User now prefers yarn.', 'user.pref.pkg-mgr',
            '["which package manager?","yarn or pnpm?","new JS package tool?"]'::jsonb,
            decode(repeat('00', 32), 'hex'));

    \echo 'Expect: 1 row with superseded_by NOT NULL, 1 row active'
    SELECT id, csl, superseded_by IS NOT NULL AS superseded
      FROM public.mneme_memories
     WHERE profile_id = 'verify-test'
     ORDER BY created_at;

    \echo 'Expect: memory_count = 2'
    SELECT memory_count FROM public.mneme_profiles WHERE profile_id = 'verify-test';

ROLLBACK;

\echo ''
\echo '=== verify-mneme: type discipline (event must NOT have topic_key) ==='
BEGIN;
    INSERT INTO public.mneme_profiles(profile_id, sovereign_pk, sigma_mask)
    VALUES ('verify-test', decode(repeat('00', 32), 'hex'), decode(repeat('00', 32), 'hex'))
    ON CONFLICT DO NOTHING;

    \echo 'Expect: ERROR (topic_key on event violates check constraint)'
    SAVEPOINT s1;
    DO $$
    BEGIN
        BEGIN
            INSERT INTO public.mneme_memories(profile_id, type, csl, paraphrase, topic_key,
                                              search_queries, sigma_mask)
            VALUES ('verify-test', 'event',
                    'deploy ✓ 2026-04-30', 'Deployed.', 'deploy.test',
                    '[]'::jsonb,
                    decode(repeat('00', 32), 'hex'));
            RAISE NOTICE 'FAIL: insert with topic_key on event was allowed';
        EXCEPTION WHEN check_violation THEN
            RAISE NOTICE 'PASS: type-discipline CHECK rejected event-with-topic_key';
        END;
    END $$;
    ROLLBACK TO SAVEPOINT s1;
ROLLBACK;

\echo ''
\echo '=== verify-mneme: vector dim enforcement ==='
BEGIN;
    INSERT INTO public.mneme_profiles(profile_id, sovereign_pk, sigma_mask)
    VALUES ('verify-test', decode(repeat('00', 32), 'hex'), decode(repeat('00', 32), 'hex'))
    ON CONFLICT DO NOTHING;

    \echo 'Expect: ERROR or success — pgvector enforces dim 1024'
    DO $$
    DECLARE
        v vector;
    BEGIN
        BEGIN
            v := array_fill(0.0::real, ARRAY[512])::vector;
            INSERT INTO public.mneme_memories(profile_id, type, csl, paraphrase, topic_key,
                                              search_queries, sigma_mask, embedding)
            VALUES ('verify-test', 'fact',
                    'test.dim ⊗ wrong', 'Test.', 'test.dim',
                    '[]'::jsonb,
                    decode(repeat('00', 32), 'hex'),
                    v);
            RAISE NOTICE 'FAIL: insert with wrong-dim vector was allowed';
        EXCEPTION WHEN OTHERS THEN
            RAISE NOTICE 'PASS: pgvector rejected wrong-dim vector (% : %)', SQLSTATE, SQLERRM;
        END;
    END $$;
ROLLBACK;

\echo ''
\echo '=== verify-mneme: COMPLETE ==='
