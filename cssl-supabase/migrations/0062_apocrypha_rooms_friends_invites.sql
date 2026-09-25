-- 0062 · rooms per account, invite-only lobbies, friends.
--
-- Owner decisions 2026-09-25: "each account to have a different chat and history", "individual
-- lobbies, not one public lobby", "build a friend and invite system".
--
--   room keys   'p:<auth uid>'  a member's private room with Apocrypha (created on first use)
--               'owner'         Apocky's private room (the legacy key, kept so its history stays)
--               'l:<uuid>'      a lobby: owned by one account, joined by invitation only
--               'lobby'         Apocky's first lobby (the legacy shared room, now invite-only)
--   membership  apocrypha_room_member decides who may read and write a room -- nothing else does
--   invites     a lobby owner or member mints a link; accepting it (signed in) joins the lobby and
--               makes inviter and invitee friends
--   friends     a friend can be added to any lobby you belong to without a new link
--   names       apocrypha_room_profile maps the author label on rows to a display name
-- All functions are service_role only; the site resolves the verified auth user first.

CREATE TABLE IF NOT EXISTS public.apocrypha_room (
    key         text        PRIMARY KEY,
    kind        text        NOT NULL,
    owner_user  uuid        NOT NULL,
    title       text        NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_room_kind_shape CHECK (kind IN ('private', 'lobby')),
    CONSTRAINT apocrypha_room_title_shape CHECK (char_length(btrim(title)) BETWEEN 1 AND 80)
);
ALTER TABLE public.apocrypha_room ENABLE ROW LEVEL SECURITY;

CREATE TABLE IF NOT EXISTS public.apocrypha_room_member (
    room_key  text        NOT NULL REFERENCES public.apocrypha_room(key) ON DELETE CASCADE,
    user_id   uuid        NOT NULL,
    role      text        NOT NULL DEFAULT 'member',
    added_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (room_key, user_id),
    CONSTRAINT apocrypha_room_member_role_shape CHECK (role IN ('owner', 'member'))
);
CREATE INDEX IF NOT EXISTS apocrypha_room_member_user ON public.apocrypha_room_member (user_id);
ALTER TABLE public.apocrypha_room_member ENABLE ROW LEVEL SECURITY;

CREATE TABLE IF NOT EXISTS public.apocrypha_room_profile (
    user_id      uuid        PRIMARY KEY,
    author       text        NOT NULL UNIQUE,
    display_name text        NOT NULL,
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_room_profile_name_shape CHECK (char_length(btrim(display_name)) BETWEEN 1 AND 40)
);
ALTER TABLE public.apocrypha_room_profile ENABLE ROW LEVEL SECURITY;

CREATE TABLE IF NOT EXISTS public.apocrypha_friend (
    user_a     uuid        NOT NULL,
    user_b     uuid        NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_a, user_b),
    CONSTRAINT apocrypha_friend_ordered CHECK (user_a < user_b)
);
ALTER TABLE public.apocrypha_friend ENABLE ROW LEVEL SECURITY;

CREATE TABLE IF NOT EXISTS public.apocrypha_room_invite (
    token_hash  text        PRIMARY KEY,
    room_key    text        NOT NULL REFERENCES public.apocrypha_room(key) ON DELETE CASCADE,
    created_by  uuid        NOT NULL,
    created_at  timestamptz NOT NULL DEFAULT now(),
    expires_at  timestamptz NOT NULL DEFAULT now() + interval '7 days',
    uses_left   integer     NOT NULL DEFAULT 10,
    CONSTRAINT apocrypha_room_invite_hash_shape CHECK (token_hash ~ '^[0-9a-f]{64}$')
);
ALTER TABLE public.apocrypha_room_invite ENABLE ROW LEVEL SECURITY;

-- The old turn table only knew two rooms.
ALTER TABLE public.apocrypha_room_turn DROP CONSTRAINT IF EXISTS apocrypha_room_turn_room_shape;

-- ─── helpers ────────────────────────────────────────────────────────────────
CREATE OR REPLACE FUNCTION public.apocrypha_room_role(p_room_key text, p_user_id uuid)
RETURNS text LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, public AS $$
    SELECT m.role FROM public.apocrypha_room_member AS m WHERE m.room_key = p_room_key AND m.user_id = p_user_id;
$$;

-- A member's private room exists from their first visit; Apocky's is the legacy 'owner' room.
CREATE OR REPLACE FUNCTION public.apocrypha_room_ensure_private(p_user_id uuid, p_is_owner boolean)
RETURNS text LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $$
DECLARE
    v_key text := CASE WHEN p_is_owner THEN 'owner' ELSE 'p:' || p_user_id::text END;
BEGIN
    INSERT INTO public.apocrypha_room (key, kind, owner_user, title)
    VALUES (v_key, 'private', p_user_id, 'Private')
    ON CONFLICT (key) DO NOTHING;
    INSERT INTO public.apocrypha_room_member (room_key, user_id, role)
    VALUES (v_key, p_user_id, 'owner')
    ON CONFLICT (room_key, user_id) DO NOTHING;
    RETURN v_key;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_room_set_profile(p_user_id uuid, p_author text, p_display_name text)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $$
BEGIN
    INSERT INTO public.apocrypha_room_profile (user_id, author, display_name)
    VALUES (p_user_id, p_author, left(btrim(p_display_name), 40))
    ON CONFLICT (user_id) DO UPDATE SET display_name = left(btrim(EXCLUDED.display_name), 40), updated_at = now();
END;
$$;

-- Every room this account belongs to, private first, then lobbies by recent activity.
CREATE OR REPLACE FUNCTION public.apocrypha_room_list(p_user_id uuid)
RETURNS TABLE (key text, kind text, title text, role text, owner_user uuid, members integer, last_at timestamptz)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, public AS $$
    SELECT r.key, r.kind, r.title, m.role, r.owner_user,
           (SELECT count(*)::integer FROM public.apocrypha_room_member x WHERE x.room_key = r.key),
           coalesce((SELECT max(e.created_at) FROM public.apocrypha_room_events e WHERE e.room = r.key AND e.kind = 'utterance'), r.created_at)
    FROM public.apocrypha_room AS r
    JOIN public.apocrypha_room_member AS m ON m.room_key = r.key AND m.user_id = p_user_id
    ORDER BY (r.kind = 'private') DESC, 7 DESC;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_room_create_lobby(p_user_id uuid, p_title text)
RETURNS text LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $$
DECLARE
    v_key text := 'l:' || gen_random_uuid()::text;
    v_count integer;
BEGIN
    SELECT count(*) INTO v_count FROM public.apocrypha_room WHERE owner_user = p_user_id AND kind = 'lobby';
    IF v_count >= 20 THEN RAISE EXCEPTION 'you can own 20 lobbies' USING ERRCODE = 'P4290'; END IF;
    INSERT INTO public.apocrypha_room (key, kind, owner_user, title)
    VALUES (v_key, 'lobby', p_user_id, coalesce(nullif(btrim(p_title), ''), 'Lobby'));
    INSERT INTO public.apocrypha_room_member (room_key, user_id, role) VALUES (v_key, p_user_id, 'owner');
    RETURN v_key;
END;
$$;

-- Members of a room with display names (only callable after the site checked membership).
CREATE OR REPLACE FUNCTION public.apocrypha_room_members(p_room_key text)
RETURNS TABLE (user_id uuid, role text, author text, display_name text)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, public AS $$
    SELECT m.user_id, m.role, p.author, coalesce(p.display_name, 'member')
    FROM public.apocrypha_room_member AS m
    LEFT JOIN public.apocrypha_room_profile AS p ON p.user_id = m.user_id
    WHERE m.room_key = p_room_key
    ORDER BY (m.role = 'owner') DESC, m.added_at;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_room_names(p_authors text[])
RETURNS TABLE (author text, display_name text)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, public AS $$
    SELECT p.author, p.display_name FROM public.apocrypha_room_profile AS p WHERE p.author = ANY (p_authors);
$$;

-- ─── invites + friends ──────────────────────────────────────────────────────
CREATE OR REPLACE FUNCTION public.apocrypha_room_invite_create(p_user_id uuid, p_room_key text, p_token_hash text)
RETURNS timestamptz LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $$
DECLARE v_kind text; v_expires timestamptz;
BEGIN
    SELECT kind INTO v_kind FROM public.apocrypha_room WHERE key = p_room_key;
    IF v_kind IS DISTINCT FROM 'lobby' THEN RAISE EXCEPTION 'only lobbies take invitations' USING ERRCODE = '22023'; END IF;
    IF public.apocrypha_room_role(p_room_key, p_user_id) IS NULL THEN RAISE EXCEPTION 'not a member' USING ERRCODE = 'P4031'; END IF;
    INSERT INTO public.apocrypha_room_invite (token_hash, room_key, created_by) VALUES (p_token_hash, p_room_key, p_user_id)
    RETURNING expires_at INTO v_expires;
    RETURN v_expires;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_befriend(p_x uuid, p_y uuid)
RETURNS void LANGUAGE sql SECURITY DEFINER SET search_path = pg_catalog, public AS $$
    INSERT INTO public.apocrypha_friend (user_a, user_b)
    SELECT least(p_x, p_y), greatest(p_x, p_y) WHERE p_x <> p_y
    ON CONFLICT DO NOTHING;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_room_invite_accept(p_user_id uuid, p_token_hash text)
RETURNS TABLE (room_key text, title text)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $$
DECLARE v_invite public.apocrypha_room_invite; v_title text;
BEGIN
    SELECT * INTO v_invite FROM public.apocrypha_room_invite WHERE token_hash = p_token_hash FOR UPDATE;
    IF NOT FOUND OR v_invite.expires_at < now() OR v_invite.uses_left <= 0 THEN
        RAISE EXCEPTION 'this invitation has expired or was used up' USING ERRCODE = 'P4040';
    END IF;
    IF public.apocrypha_room_role(v_invite.room_key, p_user_id) IS NULL THEN
        INSERT INTO public.apocrypha_room_member (room_key, user_id, role) VALUES (v_invite.room_key, p_user_id, 'member');
        UPDATE public.apocrypha_room_invite SET uses_left = uses_left - 1 WHERE token_hash = p_token_hash;
    END IF;
    PERFORM public.apocrypha_befriend(v_invite.created_by, p_user_id);
    SELECT r.title INTO v_title FROM public.apocrypha_room r WHERE r.key = v_invite.room_key;
    RETURN QUERY SELECT v_invite.room_key, v_title;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_friends(p_user_id uuid)
RETURNS TABLE (user_id uuid, display_name text, since timestamptz)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, public AS $$
    SELECT CASE WHEN f.user_a = p_user_id THEN f.user_b ELSE f.user_a END AS uid,
           coalesce(p.display_name, 'member'), f.created_at
    FROM public.apocrypha_friend f
    LEFT JOIN public.apocrypha_room_profile p ON p.user_id = CASE WHEN f.user_a = p_user_id THEN f.user_b ELSE f.user_a END
    WHERE f.user_a = p_user_id OR f.user_b = p_user_id
    ORDER BY 2;
$$;

-- Add a friend to a lobby you belong to.
CREATE OR REPLACE FUNCTION public.apocrypha_room_add_friend(p_user_id uuid, p_room_key text, p_friend uuid)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $$
BEGIN
    IF (SELECT kind FROM public.apocrypha_room WHERE key = p_room_key) IS DISTINCT FROM 'lobby' THEN
        RAISE EXCEPTION 'only lobbies take members' USING ERRCODE = '22023';
    END IF;
    IF public.apocrypha_room_role(p_room_key, p_user_id) IS NULL THEN RAISE EXCEPTION 'not a member' USING ERRCODE = 'P4031'; END IF;
    IF NOT EXISTS (SELECT 1 FROM public.apocrypha_friend WHERE user_a = least(p_user_id, p_friend) AND user_b = greatest(p_user_id, p_friend)) THEN
        RAISE EXCEPTION 'not a friend' USING ERRCODE = 'P4031';
    END IF;
    INSERT INTO public.apocrypha_room_member (room_key, user_id, role) VALUES (p_room_key, p_friend, 'member')
    ON CONFLICT DO NOTHING;
END;
$$;

-- Leave a lobby (an owner deletes it instead).
CREATE OR REPLACE FUNCTION public.apocrypha_room_leave(p_user_id uuid, p_room_key text)
RETURNS void LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $$
DECLARE v_room public.apocrypha_room;
BEGIN
    SELECT * INTO v_room FROM public.apocrypha_room WHERE key = p_room_key;
    IF NOT FOUND OR v_room.kind <> 'lobby' THEN RAISE EXCEPTION 'not a lobby' USING ERRCODE = '22023'; END IF;
    IF v_room.owner_user = p_user_id THEN
        IF p_room_key = 'lobby' THEN RAISE EXCEPTION 'the first lobby keeps its history' USING ERRCODE = '22023'; END IF;
        DELETE FROM public.apocrypha_room WHERE key = p_room_key;
    ELSE
        DELETE FROM public.apocrypha_room_member WHERE room_key = p_room_key AND user_id = p_user_id;
    END IF;
END;
$$;

-- ─── say: membership replaces the two-room rule ─────────────────────────────
CREATE OR REPLACE FUNCTION public.apocrypha_room_say_as(
    p_room text, p_user_id uuid, p_author text, p_body text,
    p_tenant_id uuid, p_principal_id uuid, p_capability text, p_request jsonb,
    p_engine_lane text, p_flagship_allowed boolean,
    p_model_alias text, p_profile_hash text, p_tool_registry_version text, p_memory_manifest_hash text
)
RETURNS TABLE (event_id bigint, created_at timestamptz, job_id uuid, job_status text, engine_lane text)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, extensions AS $$
DECLARE
    v_lane    text := coalesce(p_engine_lane, 'local');
    v_event   public.apocrypha_room_events;
    v_request jsonb;
    v_job     public.apocrypha_job;
    v_recent  integer;
BEGIN
    IF public.apocrypha_room_role(p_room, p_user_id) IS NULL THEN
        RAISE EXCEPTION 'you are not in that room' USING ERRCODE = 'P4031';
    END IF;
    IF v_lane NOT IN ('local', 'flagship') THEN RAISE EXCEPTION 'engine lane is invalid' USING ERRCODE = '22023'; END IF;
    IF v_lane = 'flagship' AND NOT coalesce(p_flagship_allowed, false) THEN
        RAISE EXCEPTION 'the flagship lane needs an active Apocrypha Premium plan' USING ERRCODE = 'P4020';
    END IF;
    IF p_capability NOT IN ('apocky_owner_chat', 'apocky_member_chat') THEN
        RAISE EXCEPTION 'room turns run on the owner or member chat capability' USING ERRCODE = '22023';
    END IF;
    IF p_body IS NULL OR char_length(btrim(p_body)) = 0 OR char_length(p_body) > 4000 THEN
        RAISE EXCEPTION 'message must be 1-4000 characters' USING ERRCODE = '22023';
    END IF;
    IF p_capability = 'apocky_member_chat' THEN
        SELECT count(*) INTO v_recent FROM public.apocrypha_room_turn AS t
        JOIN public.apocrypha_job AS j ON j.id = t.job_id
        WHERE j.tenant_id = p_tenant_id AND j.owner_principal_id = p_principal_id
          AND t.created_at >= now() - interval '1 hour';
        IF v_recent >= 60 THEN RAISE EXCEPTION 'room quota: 60 messages an hour' USING ERRCODE = 'P4290'; END IF;
    END IF;

    INSERT INTO public.apocrypha_room_events (room, author, kind, body, meta)
    VALUES (p_room, p_author, 'utterance', p_body, jsonb_build_object('engine_lane', v_lane))
    RETURNING * INTO v_event;
    v_request := coalesce(p_request, '{}'::jsonb) || jsonb_build_object('room', p_room, 'room_event_id', v_event.id, 'engine_lane', v_lane);
    SELECT * INTO v_job FROM public.apocrypha_enqueue_job(
        p_tenant_id, p_principal_id, 'apocky_chat', p_capability,
        v_request, public.apocrypha_sha256(v_request::text),
        'room:' || p_room, 'event:' || v_event.id::text,
        p_model_alias, p_profile_hash, p_tool_registry_version, p_memory_manifest_hash,
        (CASE WHEN p_capability = 'apocky_owner_chat' THEN 20 ELSE 0 END)::smallint,
        3::smallint, now(), NULL, 'primary');
    UPDATE public.apocrypha_job AS j
    SET metadata = j.metadata || jsonb_build_object('engine_lane', v_lane, 'room', p_room, 'room_event_id', v_event.id)
    WHERE j.id = v_job.id;
    INSERT INTO public.apocrypha_room_turn (job_id, room, event_id, author, engine_lane)
    VALUES (v_job.id, p_room, v_event.id, p_author, v_lane);
    RETURN QUERY SELECT v_event.id, v_event.created_at, v_job.id, v_job.status, v_lane;
END;
$$;

-- ─── legacy rooms: Apocky keeps both histories ──────────────────────────────
DO $legacy$
DECLARE v_owner uuid;
BEGIN
    SELECT p.auth_user_id INTO v_owner
    FROM public.apocrypha_principal p JOIN public.apocrypha_tenant t ON t.id = p.tenant_id
    WHERE t.slug = 'apocky-owner' AND p.auth_user_id IS NOT NULL
    ORDER BY p.created_at LIMIT 1;
    IF v_owner IS NULL THEN RAISE EXCEPTION 'owner principal not found; cannot adopt legacy rooms'; END IF;
    PERFORM public.apocrypha_room_ensure_private(v_owner, true);
    INSERT INTO public.apocrypha_room (key, kind, owner_user, title) VALUES ('lobby', 'lobby', v_owner, 'Apocky''s lobby')
    ON CONFLICT (key) DO NOTHING;
    INSERT INTO public.apocrypha_room_member (room_key, user_id, role) VALUES ('lobby', v_owner, 'owner')
    ON CONFLICT DO NOTHING;
    INSERT INTO public.apocrypha_room_profile (user_id, author, display_name) VALUES (v_owner, 'apocky', 'Apocky')
    ON CONFLICT (user_id) DO NOTHING;
END;
$legacy$;

-- ─── grants ─────────────────────────────────────────────────────────────────
DO $grants$
DECLARE f text;
BEGIN
    FOREACH f IN ARRAY ARRAY[
        'apocrypha_room_role(text, uuid)', 'apocrypha_room_ensure_private(uuid, boolean)',
        'apocrypha_room_set_profile(uuid, text, text)', 'apocrypha_room_list(uuid)',
        'apocrypha_room_create_lobby(uuid, text)', 'apocrypha_room_members(text)', 'apocrypha_room_names(text[])',
        'apocrypha_room_invite_create(uuid, text, text)', 'apocrypha_befriend(uuid, uuid)',
        'apocrypha_room_invite_accept(uuid, text)', 'apocrypha_friends(uuid)',
        'apocrypha_room_add_friend(uuid, text, uuid)', 'apocrypha_room_leave(uuid, text)',
        'apocrypha_room_say_as(text, uuid, text, text, uuid, uuid, text, jsonb, text, boolean, text, text, text, text)'
    ] LOOP
        EXECUTE format('REVOKE ALL ON FUNCTION public.%s FROM PUBLIC, anon, authenticated', f);
        EXECUTE format('GRANT EXECUTE ON FUNCTION public.%s TO service_role', f);
    END LOOP;
END;
$grants$;
