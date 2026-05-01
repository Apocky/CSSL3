-- =====================================================================
-- § T11-W5b-SUPABASE-COCREATIVE · 0007_cocreative.sql
-- Cross-session learning state for the cssl-host-cocreative optimizer.
-- A bias-vector is the persistent θ-parameters of an online learner ;
-- feedback events are the supervised signal driving SGD-style updates ;
-- snapshots are point-in-time θ checkpoints for replay / rollback / debug.
--
-- Apply order : after 0001-0006.
-- =====================================================================

-- pgcrypto is loaded by 0001_initial.sql ; reassert defensively
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- =====================================================================
-- public.cocreative_bias_vectors · one persistent θ per player
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.cocreative_bias_vectors (
    id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id       text        NOT NULL,
    dim             integer     NOT NULL,
    theta           jsonb       NOT NULL,
    lr              real        NOT NULL DEFAULT 0.01,
    momentum_decay  real        NOT NULL DEFAULT 0.9,
    step_count      bigint      NOT NULL DEFAULT 0,
    last_loss       real,
    last_grad_l2    real,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT cocreative_bias_vectors_player_unique UNIQUE (player_id),
    CONSTRAINT cocreative_bias_vectors_dim_range
        CHECK (dim BETWEEN 1 AND 256),
    CONSTRAINT cocreative_bias_vectors_lr_range
        CHECK (lr > 0 AND lr <= 1),
    CONSTRAINT cocreative_bias_vectors_momentum_range
        CHECK (momentum_decay >= 0 AND momentum_decay < 1),
    CONSTRAINT cocreative_bias_vectors_step_count_nonneg
        CHECK (step_count >= 0),
    CONSTRAINT cocreative_bias_vectors_player_id_length
        CHECK (char_length(player_id) BETWEEN 1 AND 200),
    CONSTRAINT cocreative_bias_vectors_theta_is_array
        CHECK (jsonb_typeof(theta) = 'array'),
    CONSTRAINT cocreative_bias_vectors_theta_length_matches_dim
        CHECK (jsonb_array_length(theta) = dim)
);

CREATE INDEX IF NOT EXISTS cocreative_bias_vectors_updated_at_idx
    ON public.cocreative_bias_vectors (updated_at DESC);

COMMENT ON TABLE public.cocreative_bias_vectors IS
    'One bias-vector (θ) per player. UNIQUE(player_id) — cssl-host-cocreative reads-or-creates on session start. theta is JSONB array of f32, length=dim.';
COMMENT ON COLUMN public.cocreative_bias_vectors.theta IS
    'Online-learner parameters θ ∈ R^dim. Stored as JSONB array of numbers (f32 in host). Length-equals-dim is enforced by CHECK.';
COMMENT ON COLUMN public.cocreative_bias_vectors.lr IS
    'Learning rate η ∈ (0, 1]. Default 0.01. Player-tunable.';
COMMENT ON COLUMN public.cocreative_bias_vectors.momentum_decay IS
    'Momentum decay β ∈ [0, 1). Default 0.9. Player-tunable.';

-- =====================================================================
-- public.cocreative_feedback_events · supervised signal for the optimizer
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.cocreative_feedback_events (
    id              bigserial   PRIMARY KEY,
    player_id       text        NOT NULL,
    bias_id         uuid        REFERENCES public.cocreative_bias_vectors(id)
                                ON DELETE CASCADE,
    kind            text        NOT NULL,
    target_label    text        NOT NULL,
    scene_features  jsonb       NOT NULL,
    score           real,
    comment_text    text,
    recorded_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT cocreative_feedback_events_kind_check
        CHECK (kind IN ('thumbs_up','thumbs_down','scalar_score','comment')),
    CONSTRAINT cocreative_feedback_events_player_id_length
        CHECK (char_length(player_id) BETWEEN 1 AND 200),
    CONSTRAINT cocreative_feedback_events_target_label_length
        CHECK (char_length(target_label) BETWEEN 1 AND 200),
    CONSTRAINT cocreative_feedback_events_score_present_iff_scalar
        CHECK (
            (kind = 'scalar_score' AND score IS NOT NULL)
            OR (kind <> 'scalar_score' AND score IS NULL)
        ),
    CONSTRAINT cocreative_feedback_events_comment_present_iff_comment
        CHECK (
            (kind = 'comment' AND comment_text IS NOT NULL)
            OR (kind <> 'comment' AND comment_text IS NULL)
        ),
    CONSTRAINT cocreative_feedback_events_score_range
        CHECK (score IS NULL OR (score >= -1 AND score <= 1)),
    CONSTRAINT cocreative_feedback_events_comment_text_length
        CHECK (comment_text IS NULL OR char_length(comment_text) BETWEEN 1 AND 4000)
);

CREATE INDEX IF NOT EXISTS cocreative_feedback_events_player_id_idx
    ON public.cocreative_feedback_events (player_id);
CREATE INDEX IF NOT EXISTS cocreative_feedback_events_bias_id_idx
    ON public.cocreative_feedback_events (bias_id);
CREATE INDEX IF NOT EXISTS cocreative_feedback_events_recorded_at_desc_idx
    ON public.cocreative_feedback_events (recorded_at DESC);

COMMENT ON TABLE public.cocreative_feedback_events IS
    'Supervised feedback events feeding the cocreative optimizer. score ∈ [-1, 1] when kind=''scalar_score''. comment_text only set when kind=''comment''.';
COMMENT ON COLUMN public.cocreative_feedback_events.kind IS
    'thumbs_up|thumbs_down|scalar_score|comment. Score / comment-text presence is enforced via CHECK.';
COMMENT ON COLUMN public.cocreative_feedback_events.scene_features IS
    'Snapshot of the scene-feature vector that was being judged. JSONB so the optimizer can replay events later.';

-- =====================================================================
-- public.cocreative_optimizer_snapshots · point-in-time θ checkpoints
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.cocreative_optimizer_snapshots (
    id           bigserial   PRIMARY KEY,
    bias_id      uuid        NOT NULL
                             REFERENCES public.cocreative_bias_vectors(id)
                             ON DELETE CASCADE,
    seq          bigint      NOT NULL,
    theta        jsonb       NOT NULL,
    step_count   bigint      NOT NULL,
    last_loss    real,
    created_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT cocreative_optimizer_snapshots_seq_unique UNIQUE (bias_id, seq),
    CONSTRAINT cocreative_optimizer_snapshots_seq_nonneg CHECK (seq >= 0),
    CONSTRAINT cocreative_optimizer_snapshots_step_count_nonneg
        CHECK (step_count >= 0),
    CONSTRAINT cocreative_optimizer_snapshots_theta_is_array
        CHECK (jsonb_typeof(theta) = 'array')
);

CREATE INDEX IF NOT EXISTS cocreative_optimizer_snapshots_bias_id_idx
    ON public.cocreative_optimizer_snapshots (bias_id);
CREATE INDEX IF NOT EXISTS cocreative_optimizer_snapshots_created_at_desc_idx
    ON public.cocreative_optimizer_snapshots (created_at DESC);
-- Combined "latest snapshot for bias" path
CREATE INDEX IF NOT EXISTS cocreative_optimizer_snapshots_bias_seq_desc_idx
    ON public.cocreative_optimizer_snapshots (bias_id, seq DESC);

COMMENT ON TABLE public.cocreative_optimizer_snapshots IS
    'Append-only θ checkpoints. seq is monotonically increasing per bias_id ; UNIQUE(bias_id, seq) prevents duplicates.';
COMMENT ON COLUMN public.cocreative_optimizer_snapshots.seq IS
    'Per-bias monotonically-increasing sequence number. Consumers fetch latest with ORDER BY seq DESC LIMIT 1.';

-- =====================================================================
-- update_bias_with_step() · atomic θ-update + bookkeeping
-- =====================================================================
CREATE OR REPLACE FUNCTION public.update_bias_with_step(
    p_bias_id      uuid,
    p_new_theta    jsonb,
    p_step_count   bigint,
    p_loss         real,
    p_grad         real
) RETURNS timestamptz
    LANGUAGE plpgsql AS
$$
DECLARE
    v_now        timestamptz := now();
    v_dim        integer;
    v_new_dim    integer;
BEGIN
    -- defensive : new theta length must equal stored dim
    SELECT dim INTO v_dim
      FROM public.cocreative_bias_vectors
     WHERE id = p_bias_id
     FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'update_bias_with_step : bias_id % not found', p_bias_id;
    END IF;

    IF jsonb_typeof(p_new_theta) <> 'array' THEN
        RAISE EXCEPTION 'update_bias_with_step : p_new_theta must be a JSON array';
    END IF;

    v_new_dim := jsonb_array_length(p_new_theta);
    IF v_new_dim <> v_dim THEN
        RAISE EXCEPTION
            'update_bias_with_step : theta-length mismatch (stored dim=%, new=%)',
            v_dim, v_new_dim;
    END IF;

    IF p_step_count < 0 THEN
        RAISE EXCEPTION 'update_bias_with_step : p_step_count must be >= 0';
    END IF;

    UPDATE public.cocreative_bias_vectors
       SET theta        = p_new_theta,
           step_count   = p_step_count,
           last_loss    = p_loss,
           last_grad_l2 = p_grad,
           updated_at   = v_now
     WHERE id = p_bias_id;

    RETURN v_now;
END;
$$;

COMMENT ON FUNCTION public.update_bias_with_step IS
    'Atomic update of a bias-vector after one optimizer step. Validates dim-match. Returns the timestamp written to updated_at.';

-- =====================================================================
-- latest_snapshot_for_player() · most-recent snapshot for a player
-- =====================================================================
CREATE OR REPLACE FUNCTION public.latest_snapshot_for_player(
    p_player_id text
) RETURNS SETOF public.cocreative_optimizer_snapshots
    LANGUAGE sql STABLE AS
$$
    SELECT s.*
      FROM public.cocreative_optimizer_snapshots s
      JOIN public.cocreative_bias_vectors b ON b.id = s.bias_id
     WHERE b.player_id = p_player_id
     ORDER BY s.seq DESC
     LIMIT 1;
$$;

COMMENT ON FUNCTION public.latest_snapshot_for_player IS
    'Returns the most-recent optimizer snapshot for a player (joins through cocreative_bias_vectors). SETOF to keep the row-shape stable ; LIMIT 1 in body.';

-- =====================================================================
-- updated_at trigger (matches 0001 convention) — keep updated_at fresh on
-- direct UPDATE (e.g. lr / momentum_decay tweaks) without going through
-- update_bias_with_step()
-- =====================================================================
CREATE OR REPLACE FUNCTION public.cocreative_bias_vectors_touch_updated_at()
    RETURNS trigger LANGUAGE plpgsql AS
$$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS cocreative_bias_vectors_updated_at_tg
    ON public.cocreative_bias_vectors;

CREATE TRIGGER cocreative_bias_vectors_updated_at_tg
    BEFORE UPDATE ON public.cocreative_bias_vectors
    FOR EACH ROW
    EXECUTE FUNCTION public.cocreative_bias_vectors_touch_updated_at();

-- =====================================================================
-- Function privileges (RLS still gates row visibility)
-- =====================================================================
REVOKE ALL ON FUNCTION public.update_bias_with_step(uuid, jsonb, bigint, real, real)
    FROM PUBLIC;
REVOKE ALL ON FUNCTION public.latest_snapshot_for_player(text)
    FROM PUBLIC;

GRANT EXECUTE ON FUNCTION public.update_bias_with_step(uuid, jsonb, bigint, real, real)
    TO authenticated, service_role;
GRANT EXECUTE ON FUNCTION public.latest_snapshot_for_player(text)
    TO authenticated, service_role;
