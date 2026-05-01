-- =====================================================================
-- § T11-WAVE3-SUPABASE · 0001_initial.sql
-- Initial schema · 4 tables + indexes + helper functions + triggers
-- Apply via : supabase db push  OR  paste into Supabase SQL editor
-- =====================================================================

-- Required extensions
CREATE EXTENSION IF NOT EXISTS pgcrypto;   -- gen_random_uuid()
CREATE EXTENSION IF NOT EXISTS pg_trgm;    -- text search on seed_text

-- =====================================================================
-- public.assets · asset library metadata (Sketchfab / Polyhaven / Kenney / etc.)
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.assets (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    source       text NOT NULL,
    source_id    text NOT NULL,
    name         text NOT NULL,
    license      text NOT NULL,
    attribution  text,
    format       text NOT NULL,
    storage_url  text,
    upstream_url text NOT NULL,
    metadata     jsonb,
    bytes        bigint,
    created_at   timestamptz NOT NULL DEFAULT now(),
    indexed_at   timestamptz,
    CONSTRAINT assets_source_id_unique UNIQUE (source, source_id),
    CONSTRAINT assets_format_check CHECK (format IN ('glb', 'gltf', 'obj', 'fbx', 'usdz', 'ply', 'stl')),
    CONSTRAINT assets_bytes_positive CHECK (bytes IS NULL OR bytes >= 0)
);

CREATE INDEX IF NOT EXISTS assets_license_idx ON public.assets (license);
CREATE INDEX IF NOT EXISTS assets_source_idx  ON public.assets (source);
CREATE INDEX IF NOT EXISTS assets_format_idx  ON public.assets (format);
CREATE INDEX IF NOT EXISTS assets_name_trgm_idx ON public.assets USING gin (name gin_trgm_ops);

COMMENT ON TABLE public.assets IS
    'Cached asset library metadata. Service-role writes (cssl-edge crawlers); public reads.';
COMMENT ON COLUMN public.assets.license IS
    'SPDX-style identifier : CC0 / CC-BY-4.0 / public-domain / All-Rights-Reserved / etc.';
COMMENT ON COLUMN public.assets.metadata IS
    'Free-form JSON : tags, polycount, bbox, thumbnail_url, etc.';

-- =====================================================================
-- public.scenes · player-saved scene-graphs
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.scenes (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    name        text NOT NULL,
    description text,
    seed_text   text,
    scene_graph jsonb NOT NULL,
    is_public   boolean NOT NULL DEFAULT false,
    play_count  bigint NOT NULL DEFAULT 0,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT scenes_name_length CHECK (char_length(name) BETWEEN 1 AND 200),
    CONSTRAINT scenes_play_count_nonneg CHECK (play_count >= 0)
);

CREATE INDEX IF NOT EXISTS scenes_user_id_idx     ON public.scenes (user_id);
CREATE INDEX IF NOT EXISTS scenes_is_public_idx   ON public.scenes (is_public) WHERE is_public = true;
CREATE INDEX IF NOT EXISTS scenes_created_at_idx  ON public.scenes (created_at DESC);
CREATE INDEX IF NOT EXISTS scenes_seed_trgm_idx   ON public.scenes USING gin (seed_text gin_trgm_ops);

COMMENT ON TABLE public.scenes IS
    'Player-saved resolved scene-graphs. is_public=true → discoverable by other players.';

-- =====================================================================
-- public.history · text→scene mappings (OPT-IN training data corpus)
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.history (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid REFERENCES auth.users(id) ON DELETE SET NULL,  -- nullable: anonymous OK
    seed_text   text NOT NULL,
    scene_graph jsonb,
    success     boolean,
    user_rating int,
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT history_rating_range CHECK (user_rating IS NULL OR (user_rating BETWEEN 1 AND 5))
);

CREATE INDEX IF NOT EXISTS history_user_id_idx    ON public.history (user_id) WHERE user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS history_created_at_idx ON public.history (created_at DESC);
CREATE INDEX IF NOT EXISTS history_success_idx    ON public.history (success) WHERE success IS NOT NULL;
CREATE INDEX IF NOT EXISTS history_seed_trgm_idx  ON public.history USING gin (seed_text gin_trgm_ops);

COMMENT ON TABLE public.history IS
    'Privacy-respecting text→scene mapping corpus. user_id NULL = anonymous opt-in.';

-- =====================================================================
-- public.companion_logs · cap-gated AI-op audit-trail (PRIME-DIRECTIVE §11)
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.companion_logs (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id          uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
    sovereign_handle text NOT NULL,
    operation        text NOT NULL,
    params           jsonb,
    accepted         boolean NOT NULL,
    refusal_reason   text,
    created_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT companion_logs_handle_length CHECK (char_length(sovereign_handle) BETWEEN 1 AND 200),
    CONSTRAINT companion_logs_op_length CHECK (char_length(operation) BETWEEN 1 AND 100),
    CONSTRAINT companion_logs_refusal_consistent CHECK (
        (accepted = true AND refusal_reason IS NULL) OR
        (accepted = false AND refusal_reason IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS companion_logs_user_id_idx     ON public.companion_logs (user_id);
CREATE INDEX IF NOT EXISTS companion_logs_created_at_idx  ON public.companion_logs (created_at DESC);
CREATE INDEX IF NOT EXISTS companion_logs_handle_idx      ON public.companion_logs (sovereign_handle);
CREATE INDEX IF NOT EXISTS companion_logs_op_idx          ON public.companion_logs (operation);

COMMENT ON TABLE public.companion_logs IS
    'Audit-immutable record of AI-companion operations. INSERT-only for users; DELETE = service-role-only.';

-- =====================================================================
-- updated_at trigger (scenes table)
-- =====================================================================
CREATE OR REPLACE FUNCTION public.tg_set_updated_at() RETURNS trigger
    LANGUAGE plpgsql AS
$$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS scenes_updated_at ON public.scenes;
CREATE TRIGGER scenes_updated_at
    BEFORE UPDATE ON public.scenes
    FOR EACH ROW
    EXECUTE FUNCTION public.tg_set_updated_at();

-- =====================================================================
-- play_count increment helper (RPC-callable)
-- =====================================================================
CREATE OR REPLACE FUNCTION public.scene_record_play(p_scene_id uuid) RETURNS bigint
    LANGUAGE plpgsql SECURITY DEFINER AS
$$
DECLARE
    new_count bigint;
BEGIN
    UPDATE public.scenes
       SET play_count = play_count + 1
     WHERE id = p_scene_id
       AND (is_public = true OR user_id = auth.uid())
    RETURNING play_count INTO new_count;
    RETURN COALESCE(new_count, 0);
END;
$$;

COMMENT ON FUNCTION public.scene_record_play IS
    'Increments play_count for public scenes or own scenes. Returns new count or 0 if denied.';

-- =====================================================================
-- companion_logs.append (INSERT-only RPC for clients)
-- =====================================================================
CREATE OR REPLACE FUNCTION public.companion_log_append(
    p_sovereign_handle text,
    p_operation        text,
    p_params           jsonb,
    p_accepted         boolean,
    p_refusal_reason   text DEFAULT NULL
) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER AS
$$
DECLARE
    new_id uuid;
BEGIN
    IF auth.uid() IS NULL THEN
        RAISE EXCEPTION 'companion_log_append requires authenticated user';
    END IF;
    INSERT INTO public.companion_logs (
        user_id, sovereign_handle, operation, params, accepted, refusal_reason
    ) VALUES (
        auth.uid(), p_sovereign_handle, p_operation, p_params, p_accepted, p_refusal_reason
    ) RETURNING id INTO new_id;
    RETURN new_id;
END;
$$;

COMMENT ON FUNCTION public.companion_log_append IS
    'Append-only RPC for client-side companion-op audit logging. Auth required.';

-- =====================================================================
-- Function privileges
-- =====================================================================
REVOKE ALL ON FUNCTION public.scene_record_play(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION public.scene_record_play(uuid) TO authenticated, anon;

REVOKE ALL ON FUNCTION public.companion_log_append(text, text, jsonb, boolean, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION public.companion_log_append(text, text, jsonb, boolean, text) TO authenticated;
