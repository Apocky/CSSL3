-- =====================================================================
-- § T11-W5b-SUPABASE-COCREATIVE · 0009_cocreative_seed.sql
-- Demo data : 1 bias-vector for 'demo-player' (dim=16 zeroed θ),
-- 4 feedback events (2× thumbs_up · 1× thumbs_down · 1× scalar_score 0.7),
-- 2 optimizer snapshots (seq 0 and seq 1).
--
-- All flagged with comment seed=true via target_label/comment_text patterns
-- so cleanup is straightforward :
--   DELETE FROM public.cocreative_bias_vectors WHERE player_id = 'demo-player';
--   -- CASCADE removes seeded feedback events + snapshots
--
-- Run as service_role (RLS would otherwise reject these inserts since
-- the seeded player_id 'demo-player' is a synthetic literal, not auth.uid()).
-- =====================================================================

-- Stable IDs for the demo (so re-running the seed is idempotent)
INSERT INTO public.cocreative_bias_vectors (
    id, player_id, dim, theta, lr, momentum_decay,
    step_count, last_loss, last_grad_l2
) VALUES (
    '00000000-0000-0000-0000-00000000C0C1'::uuid,
    'demo-player',
    16,
    -- 16 zeros, JSONB array of f32-like values
    '[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]'::jsonb,
    0.01,
    0.9,
    0,
    NULL,
    NULL
)
ON CONFLICT (id) DO UPDATE
    SET dim            = EXCLUDED.dim,
        theta          = EXCLUDED.theta,
        lr             = EXCLUDED.lr,
        momentum_decay = EXCLUDED.momentum_decay,
        step_count     = EXCLUDED.step_count,
        last_loss      = EXCLUDED.last_loss,
        last_grad_l2   = EXCLUDED.last_grad_l2,
        updated_at     = now();

-- ---------------------------------------------------------------------
-- 4 example feedback events
-- ---------------------------------------------------------------------
INSERT INTO public.cocreative_feedback_events (
    player_id, bias_id, kind, target_label,
    scene_features, score, comment_text
) VALUES
(
    'demo-player',
    '00000000-0000-0000-0000-00000000C0C1'::uuid,
    'thumbs_up',
    'seed:misty_forest_dawn',
    jsonb_build_object(
        'hue',         0.42,
        'saturation',  0.65,
        'lightness',   0.55,
        'fog_density', 0.30,
        'note',        'seed=true'
    ),
    NULL,
    NULL
),
(
    'demo-player',
    '00000000-0000-0000-0000-00000000C0C1'::uuid,
    'thumbs_up',
    'seed:dragon_silhouette',
    jsonb_build_object(
        'hue',         0.92,
        'saturation',  0.80,
        'lightness',   0.30,
        'fog_density', 0.10,
        'note',        'seed=true'
    ),
    NULL,
    NULL
),
(
    'demo-player',
    '00000000-0000-0000-0000-00000000C0C1'::uuid,
    'thumbs_down',
    'seed:flat_grey_hallway',
    jsonb_build_object(
        'hue',         0.0,
        'saturation',  0.05,
        'lightness',   0.45,
        'fog_density', 0.0,
        'note',        'seed=true'
    ),
    NULL,
    NULL
),
(
    'demo-player',
    '00000000-0000-0000-0000-00000000C0C1'::uuid,
    'scalar_score',
    'seed:bioluminescent_grove',
    jsonb_build_object(
        'hue',         0.35,
        'saturation',  0.70,
        'lightness',   0.20,
        'fog_density', 0.55,
        'note',        'seed=true'
    ),
    0.7,
    NULL
)
ON CONFLICT DO NOTHING;

-- ---------------------------------------------------------------------
-- 2 example optimizer snapshots (seq 0 and seq 1)
-- ---------------------------------------------------------------------
INSERT INTO public.cocreative_optimizer_snapshots (
    bias_id, seq, theta, step_count, last_loss
) VALUES
(
    '00000000-0000-0000-0000-00000000C0C1'::uuid,
    0,
    '[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]'::jsonb,
    0,
    NULL
),
(
    '00000000-0000-0000-0000-00000000C0C1'::uuid,
    1,
    -- light gradient applied to a couple of dims (seed=true via shape)
    '[0.01,-0.005,0,0,0.003,0,0,0,0,0,0,0,0,0,0,0]'::jsonb,
    1,
    0.42
)
ON CONFLICT (bias_id, seq) DO UPDATE
    SET theta      = EXCLUDED.theta,
        step_count = EXCLUDED.step_count,
        last_loss  = EXCLUDED.last_loss;
