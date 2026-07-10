-- =====================================================================
-- § APOCRYPHA PUBLIC CHAT · row-level security
-- =====================================================================
-- A signed-in user may touch ONLY their own rows. The bridge connects with
-- SUPABASE_SERVICE_ROLE_KEY, which bypasses RLS, and is the only writer of
-- responses / chunks / mind-state / rate windows. Clients can create a queued
-- turn and read their own turns + streamed chunks — they cannot forge a
-- response, mark a turn done, or read anyone else's sub-mind.
-- =====================================================================

ALTER TABLE public.chat_session    ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.chat_turn       ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.chat_chunk      ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.user_mind_state ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.chat_rate       ENABLE ROW LEVEL SECURITY;

-- chat_session: the user fully owns their own conversations
DROP POLICY IF EXISTS chat_session_owner ON public.chat_session;
CREATE POLICY chat_session_owner ON public.chat_session
    FOR ALL TO authenticated
    USING (user_id = auth.uid())
    WITH CHECK (user_id = auth.uid());

-- chat_turn: read own; create own ONLY as a fresh queued turn (no client-set response/lease/status)
DROP POLICY IF EXISTS chat_turn_read_own ON public.chat_turn;
CREATE POLICY chat_turn_read_own ON public.chat_turn
    FOR SELECT TO authenticated
    USING (user_id = auth.uid());

DROP POLICY IF EXISTS chat_turn_insert_own ON public.chat_turn;
CREATE POLICY chat_turn_insert_own ON public.chat_turn
    FOR INSERT TO authenticated
    WITH CHECK (
        user_id = auth.uid()
        AND status = 'queued'
        AND response = ''
        AND leased_by IS NULL
    );
-- (no UPDATE/DELETE policy for authenticated ⇒ only the service-role bridge advances a turn)

-- chat_chunk: read own streamed output only (bridge writes via service-role)
DROP POLICY IF EXISTS chat_chunk_read_own ON public.chat_chunk;
CREATE POLICY chat_chunk_read_own ON public.chat_chunk
    FOR SELECT TO authenticated
    USING (user_id = auth.uid());

-- user_mind_state: a user may READ their own sub-mind ("what I remember about you"); only the bridge writes
DROP POLICY IF EXISTS user_mind_state_read_own ON public.user_mind_state;
CREATE POLICY user_mind_state_read_own ON public.user_mind_state
    FOR SELECT TO authenticated
    USING (user_id = auth.uid());

-- chat_rate: no authenticated policy ⇒ RLS denies all client access; the service-role bridge manages it.
