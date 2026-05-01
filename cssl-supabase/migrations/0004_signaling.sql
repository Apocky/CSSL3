-- =====================================================================
-- § T11-W4-SUPABASE-SIGNALING · 0004_signaling.sql
-- Multiplayer signaling primitives : rooms · peers · ICE/SDP exchange ·
-- replicated state-snapshots. WebRTC media flows peer-to-peer ; only
-- discovery + signaling messages traverse Supabase.
--
-- Apply order : after 0001 + 0002 + 0003.
-- =====================================================================

-- pgcrypto already loaded by 0001_initial.sql ; reassert defensively
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- =====================================================================
-- public.multiplayer_rooms · session-level peer-discovery rooms
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.multiplayer_rooms (
    id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    code            text        UNIQUE NOT NULL,
    host_player_id  text        NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    expires_at      timestamptz NOT NULL DEFAULT (now() + INTERVAL '4 hours'),
    max_peers       integer     NOT NULL DEFAULT 8,
    is_open         boolean     NOT NULL DEFAULT true,
    meta            jsonb       NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT multiplayer_rooms_max_peers_range
        CHECK (max_peers BETWEEN 2 AND 32),
    CONSTRAINT multiplayer_rooms_code_format
        CHECK (char_length(code) BETWEEN 4 AND 12),
    CONSTRAINT multiplayer_rooms_host_player_id_length
        CHECK (char_length(host_player_id) BETWEEN 1 AND 200),
    CONSTRAINT multiplayer_rooms_expires_after_created
        CHECK (expires_at > created_at)
);

-- code is already UNIQUE (implicit btree index) -- explicit alias for cleanup queries
CREATE INDEX IF NOT EXISTS multiplayer_rooms_expires_at_idx
    ON public.multiplayer_rooms (expires_at);
CREATE INDEX IF NOT EXISTS multiplayer_rooms_host_idx
    ON public.multiplayer_rooms (host_player_id);
CREATE INDEX IF NOT EXISTS multiplayer_rooms_is_open_idx
    ON public.multiplayer_rooms (is_open) WHERE is_open = true;

COMMENT ON TABLE public.multiplayer_rooms IS
    'Peer-discovery rooms. host_player_id stores auth.uid()::text. Auto-expires via cleanup_expired_rooms().';
COMMENT ON COLUMN public.multiplayer_rooms.code IS
    'Short shareable join-code (typically 6 alphanumeric chars from gen_room_code()).';
COMMENT ON COLUMN public.multiplayer_rooms.is_open IS
    'When false, room is private (host-only SELECT). New peers cannot INSERT into a closed room.';

-- =====================================================================
-- public.room_peers · membership + presence tracking
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.room_peers (
    id            uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    room_id       uuid        NOT NULL
                              REFERENCES public.multiplayer_rooms(id)
                              ON DELETE CASCADE,
    player_id     text        NOT NULL,
    display_name  text,
    joined_at     timestamptz NOT NULL DEFAULT now(),
    last_seen_at  timestamptz NOT NULL DEFAULT now(),
    is_host       boolean     NOT NULL DEFAULT false,
    CONSTRAINT room_peers_room_player_unique UNIQUE (room_id, player_id),
    CONSTRAINT room_peers_player_id_length
        CHECK (char_length(player_id) BETWEEN 1 AND 200),
    CONSTRAINT room_peers_display_name_length
        CHECK (display_name IS NULL OR char_length(display_name) BETWEEN 1 AND 100)
);

CREATE INDEX IF NOT EXISTS room_peers_room_id_idx
    ON public.room_peers (room_id);
CREATE INDEX IF NOT EXISTS room_peers_last_seen_at_idx
    ON public.room_peers (last_seen_at);
CREATE INDEX IF NOT EXISTS room_peers_player_id_idx
    ON public.room_peers (player_id);

COMMENT ON TABLE public.room_peers IS
    'Per-room peer membership + presence. last_seen_at refreshed by presence_touch().';

-- =====================================================================
-- public.signaling_messages · WebRTC offer/answer/ICE exchange
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.signaling_messages (
    id          bigserial   PRIMARY KEY,
    room_id     uuid        NOT NULL
                            REFERENCES public.multiplayer_rooms(id)
                            ON DELETE CASCADE,
    from_peer   text        NOT NULL,
    to_peer     text        NOT NULL,
    kind        text        NOT NULL,
    payload     jsonb       NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    delivered   boolean     NOT NULL DEFAULT false,
    CONSTRAINT signaling_messages_kind_check
        CHECK (kind IN ('offer','answer','ice','hello','ping','pong','bye','custom')),
    CONSTRAINT signaling_messages_from_peer_length
        CHECK (char_length(from_peer) BETWEEN 1 AND 200),
    CONSTRAINT signaling_messages_to_peer_length
        CHECK (char_length(to_peer) BETWEEN 1 AND 200)
);

-- Primary fan-out path : "give me undelivered messages addressed to me"
CREATE INDEX IF NOT EXISTS signaling_messages_room_to_delivered_idx
    ON public.signaling_messages (room_id, to_peer, delivered);
CREATE INDEX IF NOT EXISTS signaling_messages_room_created_at_idx
    ON public.signaling_messages (room_id, created_at DESC);
CREATE INDEX IF NOT EXISTS signaling_messages_from_peer_idx
    ON public.signaling_messages (from_peer);

COMMENT ON TABLE public.signaling_messages IS
    'WebRTC signaling envelopes. to_peer = ''*'' for room-broadcast (hello/bye). delivered flag is advisory only ; clients dedupe by id.';
COMMENT ON COLUMN public.signaling_messages.kind IS
    'offer|answer|ice = SDP/ICE ; hello|bye = lifecycle ; ping|pong = liveness ; custom = app-defined.';

-- =====================================================================
-- public.room_state_snapshots · authoritative replicated game-state
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.room_state_snapshots (
    id          bigserial   PRIMARY KEY,
    room_id     uuid        NOT NULL
                            REFERENCES public.multiplayer_rooms(id)
                            ON DELETE CASCADE,
    seq         bigint      NOT NULL,
    created_by  text        NOT NULL,
    state       jsonb       NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT room_state_snapshots_seq_unique UNIQUE (room_id, seq),
    CONSTRAINT room_state_snapshots_seq_nonneg CHECK (seq >= 0),
    CONSTRAINT room_state_snapshots_created_by_length
        CHECK (char_length(created_by) BETWEEN 1 AND 200)
);

CREATE INDEX IF NOT EXISTS room_state_snapshots_room_seq_desc_idx
    ON public.room_state_snapshots (room_id, seq DESC);

COMMENT ON TABLE public.room_state_snapshots IS
    'Sequential snapshots of replicated game-state. seq is monotonically increasing per room ; consumers fetch latest by ORDER BY seq DESC LIMIT 1.';

-- =====================================================================
-- gen_room_code() · 6-char alphanumeric · retries up to 5 times for collision
-- =====================================================================
CREATE OR REPLACE FUNCTION public.gen_room_code() RETURNS text
    LANGUAGE plpgsql VOLATILE AS
$$
DECLARE
    v_alphabet  constant text := 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';  -- no I/O/0/1 (legibility)
    v_code      text;
    v_idx       integer;
    v_attempt   integer := 0;
    v_exists    boolean;
BEGIN
    LOOP
        v_attempt := v_attempt + 1;
        v_code := '';
        FOR v_idx IN 1..6 LOOP
            v_code := v_code || substr(
                v_alphabet,
                1 + floor(random() * char_length(v_alphabet))::integer,
                1
            );
        END LOOP;
        SELECT EXISTS (
            SELECT 1 FROM public.multiplayer_rooms WHERE code = v_code
        ) INTO v_exists;
        IF NOT v_exists THEN
            RETURN v_code;
        END IF;
        IF v_attempt >= 5 THEN
            RAISE EXCEPTION 'gen_room_code: 5 collisions in a row (table near saturation?)';
        END IF;
    END LOOP;
END;
$$;

COMMENT ON FUNCTION public.gen_room_code IS
    'Generates a unique 6-char room code from a legibility-friendly alphabet. Retries up to 5 times on collision.';

-- =====================================================================
-- cleanup_expired_rooms() · idempotent · CASCADE deletes peers/signals/snapshots
-- =====================================================================
CREATE OR REPLACE FUNCTION public.cleanup_expired_rooms() RETURNS bigint
    LANGUAGE plpgsql SECURITY DEFINER AS
$$
DECLARE
    v_deleted bigint;
BEGIN
    DELETE FROM public.multiplayer_rooms
     WHERE expires_at <= now();
    GET DIAGNOSTICS v_deleted = ROW_COUNT;
    RETURN v_deleted;
END;
$$;

COMMENT ON FUNCTION public.cleanup_expired_rooms IS
    'Deletes all expired rooms ; CASCADE removes peers, signaling_messages, snapshots. Returns count of rooms deleted. Suitable for pg_cron schedule.';

-- =====================================================================
-- presence_touch(p_room, p_player) · refresh last_seen_at heartbeat
-- =====================================================================
CREATE OR REPLACE FUNCTION public.presence_touch(
    p_room   uuid,
    p_player text
) RETURNS timestamptz
    LANGUAGE plpgsql SECURITY DEFINER AS
$$
DECLARE
    v_now timestamptz := now();
BEGIN
    UPDATE public.room_peers
       SET last_seen_at = v_now
     WHERE room_id = p_room
       AND player_id = p_player;
    IF NOT FOUND THEN
        RAISE NOTICE 'presence_touch : no peer (room=%, player=%)', p_room, p_player;
    END IF;
    RETURN v_now;
END;
$$;

COMMENT ON FUNCTION public.presence_touch IS
    'Refreshes last_seen_at for a peer ; idempotent ; returns the timestamp written.';

-- =====================================================================
-- Function privileges
-- =====================================================================
REVOKE ALL ON FUNCTION public.gen_room_code()                  FROM PUBLIC;
REVOKE ALL ON FUNCTION public.cleanup_expired_rooms()          FROM PUBLIC;
REVOKE ALL ON FUNCTION public.presence_touch(uuid, text)       FROM PUBLIC;

GRANT EXECUTE ON FUNCTION public.gen_room_code()               TO authenticated;
GRANT EXECUTE ON FUNCTION public.cleanup_expired_rooms()       TO service_role;
GRANT EXECUTE ON FUNCTION public.presence_touch(uuid, text)    TO authenticated;
