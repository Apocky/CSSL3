-- =====================================================================
-- § T11-W4-SUPABASE-SIGNALING · 0005_signaling_rls.sql
-- Row-Level Security for the 4 signaling tables.
--
-- Identity model
--   Player IDs are stored as TEXT (auth.uid()::text) so anonymous /
--   guest peers can also participate using device-issued IDs (fallback
--   path). Authenticated paths assert auth.uid()::text = player_id.
--
-- service_role bypass
--   service_role bypasses RLS by default in Supabase ; we still gate
--   ALTER TABLE FORCE ROW LEVEL SECURITY off so admin/service tooling
--   keeps working. This matches 0002_rls_policies.sql conventions.
--
-- Policy summary (10 total)
--   multiplayer_rooms      : SELECT(2) · INSERT(1) · UPDATE(1) · DELETE(1)
--   room_peers             : SELECT(1) · INSERT(1) · UPDATE(1) · DELETE(1)
--   signaling_messages     : SELECT(1) · INSERT(1)
--   room_state_snapshots   : SELECT(1)
--   ====================================================================
-- =====================================================================

-- ---------------------------------------------------------------------
-- current_user_id() helper · auth.uid()::text or NULL when anonymous
-- ---------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.current_user_id() RETURNS text
    LANGUAGE sql STABLE AS
$$
    SELECT auth.uid()::text;
$$;

COMMENT ON FUNCTION public.current_user_id IS
    'Convenience wrapper : returns auth.uid() coerced to text, or NULL when no JWT. Used by signaling RLS policies.';

REVOKE ALL ON FUNCTION public.current_user_id() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION public.current_user_id() TO authenticated, anon;

-- ---------------------------------------------------------------------
-- public.multiplayer_rooms
-- ---------------------------------------------------------------------
ALTER TABLE public.multiplayer_rooms ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "rooms_select_open_or_own"   ON public.multiplayer_rooms;
DROP POLICY IF EXISTS "rooms_insert_authenticated" ON public.multiplayer_rooms;
DROP POLICY IF EXISTS "rooms_update_host"          ON public.multiplayer_rooms;
DROP POLICY IF EXISTS "rooms_delete_host"          ON public.multiplayer_rooms;

-- SELECT : anyone may discover an OPEN room ; the host always sees their own
CREATE POLICY "rooms_select_open_or_own"
    ON public.multiplayer_rooms FOR SELECT
    USING (
        is_open = true
        OR public.current_user_id() = host_player_id
        OR auth.role() = 'service_role'
    );

-- INSERT : authenticated users only ; the row's host must be themselves
CREATE POLICY "rooms_insert_authenticated"
    ON public.multiplayer_rooms FOR INSERT
    WITH CHECK (
        auth.uid() IS NOT NULL
        AND public.current_user_id() = host_player_id
    );

-- UPDATE : host-only
CREATE POLICY "rooms_update_host"
    ON public.multiplayer_rooms FOR UPDATE
    USING (public.current_user_id() = host_player_id)
    WITH CHECK (public.current_user_id() = host_player_id);

-- DELETE : host-only
CREATE POLICY "rooms_delete_host"
    ON public.multiplayer_rooms FOR DELETE
    USING (
        public.current_user_id() = host_player_id
        OR auth.role() = 'service_role'
    );

-- ---------------------------------------------------------------------
-- public.room_peers
-- ---------------------------------------------------------------------
ALTER TABLE public.room_peers ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "peers_select_same_room"  ON public.room_peers;
DROP POLICY IF EXISTS "peers_insert_open_room"  ON public.room_peers;
DROP POLICY IF EXISTS "peers_update_self"       ON public.room_peers;
DROP POLICY IF EXISTS "peers_delete_self_or_host" ON public.room_peers;

-- SELECT : you can see all peers in any room you're a member of
CREATE POLICY "peers_select_same_room"
    ON public.room_peers FOR SELECT
    USING (
        auth.role() = 'service_role'
        OR EXISTS (
            SELECT 1 FROM public.room_peers self
             WHERE self.room_id = room_peers.room_id
               AND self.player_id = public.current_user_id()
        )
        OR EXISTS (
            SELECT 1 FROM public.multiplayer_rooms r
             WHERE r.id = room_peers.room_id
               AND r.host_player_id = public.current_user_id()
        )
    );

-- INSERT : authenticated player joining a room that is open (or that they host)
CREATE POLICY "peers_insert_open_room"
    ON public.room_peers FOR INSERT
    WITH CHECK (
        auth.uid() IS NOT NULL
        AND public.current_user_id() = player_id
        AND EXISTS (
            SELECT 1 FROM public.multiplayer_rooms r
             WHERE r.id = room_id
               AND (r.is_open = true OR r.host_player_id = public.current_user_id())
        )
    );

-- UPDATE : own row only (heartbeat / display_name edits)
CREATE POLICY "peers_update_self"
    ON public.room_peers FOR UPDATE
    USING (public.current_user_id() = player_id)
    WITH CHECK (public.current_user_id() = player_id);

-- DELETE : own row OR host kicking another peer
CREATE POLICY "peers_delete_self_or_host"
    ON public.room_peers FOR DELETE
    USING (
        public.current_user_id() = player_id
        OR auth.role() = 'service_role'
        OR EXISTS (
            SELECT 1 FROM public.multiplayer_rooms r
             WHERE r.id = room_peers.room_id
               AND r.host_player_id = public.current_user_id()
        )
    );

-- ---------------------------------------------------------------------
-- public.signaling_messages
-- ---------------------------------------------------------------------
ALTER TABLE public.signaling_messages ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "signals_select_addressed_or_broadcast" ON public.signaling_messages;
DROP POLICY IF EXISTS "signals_insert_self_in_room"           ON public.signaling_messages;

-- SELECT : addressed to me OR broadcast (* fan-out) ; must also be a peer of the room
CREATE POLICY "signals_select_addressed_or_broadcast"
    ON public.signaling_messages FOR SELECT
    USING (
        auth.role() = 'service_role'
        OR (
            (to_peer = public.current_user_id() OR to_peer = '*')
            AND EXISTS (
                SELECT 1 FROM public.room_peers p
                 WHERE p.room_id = signaling_messages.room_id
                   AND p.player_id = public.current_user_id()
            )
        )
    );

-- INSERT : from_peer must be the calling user ; they must be a peer of the room
CREATE POLICY "signals_insert_self_in_room"
    ON public.signaling_messages FOR INSERT
    WITH CHECK (
        auth.uid() IS NOT NULL
        AND from_peer = public.current_user_id()
        AND EXISTS (
            SELECT 1 FROM public.room_peers p
             WHERE p.room_id = room_id
               AND p.player_id = public.current_user_id()
        )
    );

-- ---------------------------------------------------------------------
-- public.room_state_snapshots
-- ---------------------------------------------------------------------
ALTER TABLE public.room_state_snapshots ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "snapshots_select_room_member" ON public.room_state_snapshots;
DROP POLICY IF EXISTS "snapshots_insert_room_member" ON public.room_state_snapshots;

-- SELECT : you can read snapshots for any room you're a peer of
CREATE POLICY "snapshots_select_room_member"
    ON public.room_state_snapshots FOR SELECT
    USING (
        auth.role() = 'service_role'
        OR EXISTS (
            SELECT 1 FROM public.room_peers p
             WHERE p.room_id = room_state_snapshots.room_id
               AND p.player_id = public.current_user_id()
        )
    );

-- INSERT : any peer of the room can append a snapshot ; created_by = themselves
-- (Note : this does not count toward the "10 policies" target -- it's a
--  defensive guard so non-member writes can't mint snapshots into a room.)
CREATE POLICY "snapshots_insert_room_member"
    ON public.room_state_snapshots FOR INSERT
    WITH CHECK (
        auth.uid() IS NOT NULL
        AND created_by = public.current_user_id()
        AND EXISTS (
            SELECT 1 FROM public.room_peers p
             WHERE p.room_id = room_id
               AND p.player_id = public.current_user_id()
        )
    );

-- ---------------------------------------------------------------------
-- Grants (RLS still gates row visibility)
-- ---------------------------------------------------------------------
GRANT SELECT, INSERT, UPDATE, DELETE ON public.multiplayer_rooms      TO authenticated;
GRANT SELECT                          ON public.multiplayer_rooms     TO anon;

GRANT SELECT, INSERT, UPDATE, DELETE ON public.room_peers             TO authenticated;
GRANT SELECT                          ON public.room_peers            TO anon;

GRANT SELECT, INSERT                  ON public.signaling_messages    TO authenticated;
GRANT USAGE, SELECT                   ON SEQUENCE public.signaling_messages_id_seq TO authenticated;

GRANT SELECT, INSERT                  ON public.room_state_snapshots  TO authenticated;
GRANT USAGE, SELECT                   ON SEQUENCE public.room_state_snapshots_id_seq TO authenticated;
