-- =====================================================================
-- § T11-W4-SUPABASE-SIGNALING · verify-signaling.sql
-- Post-migration assertions for 0004 + 0005 + 0006.
-- Run after the original verify.sql (or standalone after the 4 signaling
-- migrations are in place).
--   psql "$SUPABASE_DB_URL" -f verify-signaling.sql
-- =====================================================================

DO $$
DECLARE
    v_count       bigint;
    v_room_count  bigint;
    v_peer_count  bigint;
    v_msg_count   bigint;
BEGIN
    -- 1. New tables exist
    PERFORM 'public.multiplayer_rooms'::regclass;
    PERFORM 'public.room_peers'::regclass;
    PERFORM 'public.signaling_messages'::regclass;
    PERFORM 'public.room_state_snapshots'::regclass;

    -- 2. RLS enabled on every new table
    SELECT count(*) INTO v_count
      FROM pg_tables
     WHERE schemaname = 'public'
       AND tablename IN (
           'multiplayer_rooms','room_peers',
           'signaling_messages','room_state_snapshots'
       )
       AND rowsecurity = true;
    IF v_count <> 4 THEN
        RAISE EXCEPTION 'verify-signaling : RLS not enabled on all 4 signaling tables (expected 4, got %)', v_count;
    END IF;

    -- 3. Helper functions exist
    PERFORM 'public.gen_room_code()'::regprocedure;
    PERFORM 'public.cleanup_expired_rooms()'::regprocedure;
    PERFORM 'public.presence_touch(uuid, text)'::regprocedure;
    PERFORM 'public.current_user_id()'::regprocedure;

    -- 4. Required indexes exist
    SELECT count(*) INTO v_count
      FROM pg_indexes
     WHERE schemaname = 'public'
       AND indexname IN (
           'multiplayer_rooms_expires_at_idx',
           'multiplayer_rooms_host_idx',
           'multiplayer_rooms_is_open_idx',
           'room_peers_room_id_idx',
           'room_peers_last_seen_at_idx',
           'room_peers_player_id_idx',
           'signaling_messages_room_to_delivered_idx',
           'signaling_messages_room_created_at_idx',
           'signaling_messages_from_peer_idx',
           'room_state_snapshots_room_seq_desc_idx'
       );
    IF v_count < 10 THEN
        RAISE EXCEPTION 'verify-signaling : expected >=10 signaling indexes, got %', v_count;
    END IF;

    -- 5. Unique constraints / unique-indexes
    --    multiplayer_rooms.code (UNIQUE), room_peers (room_id, player_id),
    --    room_state_snapshots (room_id, seq)
    SELECT count(*) INTO v_count
      FROM pg_constraint
     WHERE conrelid = 'public.multiplayer_rooms'::regclass
       AND contype = 'u'
       AND conname IN ('multiplayer_rooms_code_key');
    IF v_count <> 1 THEN
        -- some pg versions name the auto-generated constraint differently ;
        -- fall back to checking that an index enforces uniqueness on code
        SELECT count(*) INTO v_count
          FROM pg_indexes pi
          JOIN pg_class c   ON c.relname = pi.indexname
          JOIN pg_index ix  ON ix.indexrelid = c.oid
         WHERE pi.schemaname = 'public'
           AND pi.tablename = 'multiplayer_rooms'
           AND ix.indisunique = true
           AND pi.indexdef LIKE '%(code)%';
        IF v_count < 1 THEN
            RAISE EXCEPTION 'verify-signaling : multiplayer_rooms.code is not UNIQUE';
        END IF;
    END IF;

    SELECT count(*) INTO v_count
      FROM pg_constraint
     WHERE conrelid = 'public.room_peers'::regclass
       AND contype = 'u'
       AND conname = 'room_peers_room_player_unique';
    IF v_count <> 1 THEN
        RAISE EXCEPTION 'verify-signaling : room_peers UNIQUE(room_id, player_id) missing';
    END IF;

    SELECT count(*) INTO v_count
      FROM pg_constraint
     WHERE conrelid = 'public.room_state_snapshots'::regclass
       AND contype = 'u'
       AND conname = 'room_state_snapshots_seq_unique';
    IF v_count <> 1 THEN
        RAISE EXCEPTION 'verify-signaling : room_state_snapshots UNIQUE(room_id, seq) missing';
    END IF;

    -- 6. RLS policies present (>= 10 across the 4 tables)
    SELECT count(*) INTO v_count
      FROM pg_policies
     WHERE schemaname = 'public'
       AND tablename IN (
           'multiplayer_rooms','room_peers',
           'signaling_messages','room_state_snapshots'
       );
    IF v_count < 10 THEN
        RAISE EXCEPTION 'verify-signaling : expected >=10 signaling RLS policies, got %', v_count;
    END IF;

    -- 7. Seed data loaded (DEMO01 room and friends)
    SELECT count(*) INTO v_room_count
      FROM public.multiplayer_rooms WHERE code = 'DEMO01';
    IF v_room_count < 1 THEN
        RAISE EXCEPTION 'verify-signaling : DEMO01 seed room missing';
    END IF;

    SELECT count(*) INTO v_peer_count
      FROM public.room_peers
     WHERE room_id = (SELECT id FROM public.multiplayer_rooms WHERE code = 'DEMO01');
    IF v_peer_count < 3 THEN
        RAISE EXCEPTION 'verify-signaling : expected >=3 seed peers in DEMO01, got %', v_peer_count;
    END IF;

    SELECT count(*) INTO v_msg_count
      FROM public.signaling_messages
     WHERE room_id = (SELECT id FROM public.multiplayer_rooms WHERE code = 'DEMO01');
    IF v_msg_count < 4 THEN
        RAISE EXCEPTION 'verify-signaling : expected >=4 seed messages in DEMO01, got %', v_msg_count;
    END IF;

    -- 8. CHECK constraint on signaling_messages.kind
    SELECT count(*) INTO v_count
      FROM pg_constraint
     WHERE conrelid = 'public.signaling_messages'::regclass
       AND contype = 'c'
       AND conname = 'signaling_messages_kind_check';
    IF v_count <> 1 THEN
        RAISE EXCEPTION 'verify-signaling : signaling_messages_kind_check constraint missing';
    END IF;

    -- 9. gen_room_code() smoke-test : returns a 6-char string of valid alphabet
    DECLARE
        v_code text := public.gen_room_code();
    BEGIN
        IF char_length(v_code) <> 6 THEN
            RAISE EXCEPTION 'verify-signaling : gen_room_code returned wrong length (got %)', char_length(v_code);
        END IF;
        IF v_code !~ '^[A-Z2-9]+$' THEN
            RAISE EXCEPTION 'verify-signaling : gen_room_code returned non-alphabet chars : %', v_code;
        END IF;
    END;

    RAISE NOTICE 'verify-signaling.sql : all assertions passed (rooms=%, peers=%, msgs=%)',
        v_room_count, v_peer_count, v_msg_count;
END$$;
