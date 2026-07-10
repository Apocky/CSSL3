-- =====================================================================
-- § APOCRYPHA PUBLIC CHAT · per-user instanced sub-minds via outbound relay
-- =====================================================================
-- Flow (no inbound to the A770 — the bridge polls OUTBOUND, like Lazarus):
--   browser → /api/chat (authed, rate-limited) → chat_turn (status=queued)
--   local bridge (SUPABASE_SERVICE_ROLE_KEY) leases queued turn → loads the user's
--     instanced sub-mind (user_mind_state, forked from a clean base persona) → runs
--     the substrate turn → streams chat_chunk rows → marks chat_turn done.
--   browser tails chat_chunk for its turn via Supabase Realtime / SSE.
--
-- Isolation: every row is keyed by user_id (= auth.users). RLS (0044) lets a user
-- touch only their own rows; the bridge uses the service role and bypasses RLS.
-- The bridge MUST NEVER load Apocky's personal mind_state for a public session.
-- =====================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- one logical conversation for a user (their sub-mind is bound to their user_id, not the session)
CREATE TABLE IF NOT EXISTS public.chat_session (
    id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         uuid        NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    title           text        NOT NULL DEFAULT 'Conversation' CHECK (char_length(title) BETWEEN 1 AND 160),
    created_at      timestamptz NOT NULL DEFAULT now(),
    last_active_at  timestamptz NOT NULL DEFAULT now(),
    metadata        jsonb       NOT NULL DEFAULT '{}'::jsonb
);
CREATE INDEX IF NOT EXISTS chat_session_user_idx ON public.chat_session (user_id, last_active_at DESC);

-- the work queue: one user turn awaiting the bridge
CREATE TABLE IF NOT EXISTS public.chat_turn (
    id            uuid          PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       uuid          NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    session_id    uuid          NOT NULL REFERENCES public.chat_session(id) ON DELETE CASCADE,
    prompt        text          NOT NULL CHECK (char_length(prompt) BETWEEN 1 AND 8192),
    status        text          NOT NULL DEFAULT 'queued'
                                  CHECK (status IN ('queued','leased','streaming','done','failed','cancelled')),
    response      text          NOT NULL DEFAULT '',
    error         text,
    leased_by     text,         -- bridge runner id
    created_at    timestamptz   NOT NULL DEFAULT now(),
    leased_at     timestamptz,
    finished_at   timestamptz,
    metadata      jsonb         NOT NULL DEFAULT '{}'::jsonb
);
CREATE INDEX IF NOT EXISTS chat_turn_status_created_idx ON public.chat_turn (status, created_at);
CREATE INDEX IF NOT EXISTS chat_turn_user_idx ON public.chat_turn (user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS chat_turn_session_idx ON public.chat_turn (session_id, created_at);

-- append-only streamed response chunks (bridge writes via service-role ; browser tails via Realtime)
CREATE TABLE IF NOT EXISTS public.chat_chunk (
    id            bigint        GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    turn_id       uuid          NOT NULL REFERENCES public.chat_turn(id) ON DELETE CASCADE,
    user_id       uuid          NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    seq           integer       NOT NULL,
    delta         text          NOT NULL,
    created_at    timestamptz   NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS chat_chunk_turn_seq_idx ON public.chat_chunk (turn_id, seq);

-- per-user instanced sub-mind state: the forked substrate (serialized MindState). Bridge owns writes.
CREATE TABLE IF NOT EXISTS public.user_mind_state (
    user_id       uuid          PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE,
    state         jsonb         NOT NULL DEFAULT '{}'::jsonb,
    seed_version  text          NOT NULL DEFAULT 'base-v1',
    turns         integer       NOT NULL DEFAULT 0 CHECK (turns >= 0),
    created_at    timestamptz   NOT NULL DEFAULT now(),
    updated_at    timestamptz   NOT NULL DEFAULT now()
);

-- lightweight per-user rate-limit window (managed server-side / service-role only)
CREATE TABLE IF NOT EXISTS public.chat_rate (
    user_id       uuid          PRIMARY KEY REFERENCES auth.users(id) ON DELETE CASCADE,
    window_start  timestamptz   NOT NULL DEFAULT now(),
    count         integer       NOT NULL DEFAULT 0 CHECK (count >= 0)
);
