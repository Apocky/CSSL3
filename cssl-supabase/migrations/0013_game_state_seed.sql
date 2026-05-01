-- =====================================================================
-- § T11-W5c-SUPABASE-GAMESTATE · 0013_game_state_seed.sql
-- Demo data for game-state + sovereign-cap audit.
--   · 1 demo session for 'demo-player' (UUID 0000…D EM01)
--   · 3 snapshots @ seq 0 / 1 / 2
--   · 2 sovereign_cap_audit example transparency entries
--
-- All flagged via stable UUIDs + 'seed=' prefixes so cleanup is trivial :
--   DELETE FROM public.game_session_index
--    WHERE session_id = '00000000-0000-0000-0000-00000000DE01'::uuid;
--   DELETE FROM public.game_state_snapshots
--    WHERE session_id = '00000000-0000-0000-0000-00000000DE01'::uuid;
--   DELETE FROM public.sovereign_cap_audit
--    WHERE session_id = '00000000-0000-0000-0000-00000000DE01'::uuid;
--
-- Run as service_role — RLS would otherwise reject these inserts
-- (synthetic 'demo-player' is not a real auth.uid()).
-- =====================================================================

-- ---------------------------------------------------------------------
-- Session row (idempotent : seq counters are derived from snapshot inserts)
-- ---------------------------------------------------------------------
INSERT INTO public.game_session_index (
    session_id, player_id, started_at, ended_at,
    latest_seq, total_snapshots, meta
) VALUES (
    '00000000-0000-0000-0000-00000000DE01'::uuid,
    'demo-player',
    now() - interval '15 minutes',
    NULL,                      -- still active
    2,                         -- matches the highest seeded snapshot seq below
    3,                         -- 3 seeded snapshots
    jsonb_build_object(
        'seed',           true,
        'engine_version', 'cssl/session-15-W5c',
        'scene_id',       'test_room',
        'note',           'seed=true · transparent demo session'
    )
)
ON CONFLICT (session_id) DO UPDATE
    SET player_id       = EXCLUDED.player_id,
        ended_at        = EXCLUDED.ended_at,
        latest_seq      = EXCLUDED.latest_seq,
        total_snapshots = EXCLUDED.total_snapshots,
        meta            = EXCLUDED.meta;

-- ---------------------------------------------------------------------
-- 3 snapshots (seq 0 / 1 / 2)
-- ω-field-digest values are sha256 of placeholder bytes (deterministic so
-- re-running the seed is idempotent under ON CONFLICT (session_id, seq)).
-- ---------------------------------------------------------------------
INSERT INTO public.game_state_snapshots (
    session_id, player_id, seq,
    scene_graph, omega_field_digest, omega_field_url, companion_history
) VALUES
(
    '00000000-0000-0000-0000-00000000DE01'::uuid,
    'demo-player',
    0,
    jsonb_build_object(
        'seed',     true,
        'kind',     'dm_scene_graph',
        'root',     'test_room',
        'entities', jsonb_build_array(),
        'note',     'seed=true · empty initial state'
    ),
    'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855', -- sha256("")
    NULL,
    '[]'::jsonb
),
(
    '00000000-0000-0000-0000-00000000DE01'::uuid,
    'demo-player',
    1,
    jsonb_build_object(
        'seed',     true,
        'kind',     'dm_scene_graph',
        'root',     'test_room',
        'entities', jsonb_build_array(
            jsonb_build_object(
                'id',    'companion-001',
                'kind',  'labyrinth-creature-companion',
                'pose',  jsonb_build_array(0.0, 1.0, 0.0)
            )
        ),
        'note',     'seed=true · after companion spawn'
    ),
    '6b86b273ff34fce19d6b804eff5a3f5747ada4eaa22f1d49c01e52ddb7875b4b', -- sha256("1")
    NULL,
    jsonb_build_array(
        jsonb_build_object(
            'ts',                now() - interval '14 minutes',
            'sovereign_handle',  'demo-player',
            'op',                'spawn',
            'params',            jsonb_build_object(
                'kind',  'labyrinth-creature-companion',
                'note',  'seed=true'
            )
        )
    )
),
(
    '00000000-0000-0000-0000-00000000DE01'::uuid,
    'demo-player',
    2,
    jsonb_build_object(
        'seed',     true,
        'kind',     'dm_scene_graph',
        'root',     'test_room',
        'entities', jsonb_build_array(
            jsonb_build_object(
                'id',    'companion-001',
                'kind',  'labyrinth-creature-companion',
                'pose',  jsonb_build_array(2.5, 1.0, -1.5)
            )
        ),
        'note',     'seed=true · after companion move'
    ),
    'd4735e3a265e16eee03f59718b9b5d03019c07d8b6c51f90da3a666eec13ab35', -- sha256("3")
    'storage://omega-fields/demo-player/00000000-0000-0000-0000-00000000DE01/seq-2.bin',
    jsonb_build_array(
        jsonb_build_object(
            'ts',                now() - interval '14 minutes',
            'sovereign_handle',  'demo-player',
            'op',                'spawn',
            'params',            jsonb_build_object(
                'kind',  'labyrinth-creature-companion',
                'note',  'seed=true'
            )
        ),
        jsonb_build_object(
            'ts',                now() - interval '5 minutes',
            'sovereign_handle',  'demo-player',
            'op',                'modify',
            'params',            jsonb_build_object(
                'target', 'companion-001',
                'pose',   jsonb_build_array(2.5, 1.0, -1.5),
                'note',   'seed=true'
            )
        )
    )
)
ON CONFLICT (session_id, seq) DO UPDATE
    SET scene_graph        = EXCLUDED.scene_graph,
        omega_field_digest = EXCLUDED.omega_field_digest,
        omega_field_url    = EXCLUDED.omega_field_url,
        companion_history  = EXCLUDED.companion_history;

-- ---------------------------------------------------------------------
-- 2 sovereign_cap_audit transparency entries
-- These are illustrative — bypass events that would have been written by
-- the host when the demo player invoked sovereign-cap operations.
-- ---------------------------------------------------------------------
INSERT INTO public.sovereign_cap_audit (
    session_id, player_id, ts,
    action_kind, cap_bypassed_kind, reason,
    target_audit_event_id, caller_origin
) VALUES
(
    '00000000-0000-0000-0000-00000000DE01'::uuid,
    'demo-player',
    now() - interval '14 minutes',
    'companion.spawn',
    'rate_limit',
    'seed=true · explicit player override on companion spawn rate-limit',
    'audit-jsonl-row-0001',
    'mcp:companion'
),
(
    '00000000-0000-0000-0000-00000000DE01'::uuid,
    'demo-player',
    now() - interval '5 minutes',
    'render.snapshot_png',
    'external_io',
    'seed=true · player asked for off-engine PNG export ; cap-bypass attested',
    'audit-jsonl-row-0002',
    'mcp:render.snapshot_png'
)
ON CONFLICT DO NOTHING;
