-- =====================================================================
-- § T11-W4-SUPABASE-SIGNALING · 0006_signaling_seed.sql
-- Demo data : 1 open room (code 'DEMO01') · 3 peers · 4 messages.
-- All flagged with meta = '{"seed": true}' for easy cleanup.
--
-- Cleanup any time :
--   DELETE FROM public.multiplayer_rooms WHERE meta @> '{"seed": true}'::jsonb;
--   -- CASCADE removes peers / signals / snapshots
--
-- Run as service_role (RLS would otherwise reject these inserts since
-- the seeded host_player_id is a synthetic UUID, not auth.uid()).
-- =====================================================================

-- Stable IDs for the demo (so re-running the seed is idempotent)
WITH
demo_room AS (
    INSERT INTO public.multiplayer_rooms (
        id, code, host_player_id, expires_at, max_peers, is_open, meta
    ) VALUES (
        '00000000-0000-0000-0000-00000000DE01'::uuid,
        'DEMO01',
        '11111111-1111-1111-1111-111111111111',
        now() + INTERVAL '24 hours',
        8,
        true,
        '{"seed": true, "purpose": "verify + onboarding demo"}'::jsonb
    )
    ON CONFLICT (id) DO UPDATE
        SET expires_at = EXCLUDED.expires_at,
            is_open    = EXCLUDED.is_open,
            meta       = EXCLUDED.meta
    RETURNING id
)
SELECT 'demo_room created' AS step FROM demo_room;

-- ---------------------------------------------------------------------
-- 3 example peers
-- ---------------------------------------------------------------------
INSERT INTO public.room_peers (
    id, room_id, player_id, display_name, is_host
) VALUES
(
    '00000000-0000-0000-0000-0000000DEAA1'::uuid,
    '00000000-0000-0000-0000-00000000DE01'::uuid,
    '11111111-1111-1111-1111-111111111111',
    'host-alice',
    true
),
(
    '00000000-0000-0000-0000-0000000DEBB2'::uuid,
    '00000000-0000-0000-0000-00000000DE01'::uuid,
    '22222222-2222-2222-2222-222222222222',
    'guest-bob',
    false
),
(
    '00000000-0000-0000-0000-0000000DECC3'::uuid,
    '00000000-0000-0000-0000-00000000DE01'::uuid,
    '33333333-3333-3333-3333-333333333333',
    'guest-carol',
    false
)
ON CONFLICT (id) DO UPDATE
    SET display_name = EXCLUDED.display_name,
        last_seen_at = now();

-- ---------------------------------------------------------------------
-- 4 example signaling messages : offer · answer · 2x ice
-- ---------------------------------------------------------------------
INSERT INTO public.signaling_messages (
    room_id, from_peer, to_peer, kind, payload, delivered
) VALUES
(
    '00000000-0000-0000-0000-00000000DE01'::uuid,
    '11111111-1111-1111-1111-111111111111',
    '22222222-2222-2222-2222-222222222222',
    'offer',
    jsonb_build_object(
        'type', 'offer',
        'sdp',  'v=0\r\no=- 4611731400430051336 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n[demo-truncated]'
    ),
    true
),
(
    '00000000-0000-0000-0000-00000000DE01'::uuid,
    '22222222-2222-2222-2222-222222222222',
    '11111111-1111-1111-1111-111111111111',
    'answer',
    jsonb_build_object(
        'type', 'answer',
        'sdp',  'v=0\r\no=- 4611731400430051337 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n[demo-truncated]'
    ),
    true
),
(
    '00000000-0000-0000-0000-00000000DE01'::uuid,
    '11111111-1111-1111-1111-111111111111',
    '22222222-2222-2222-2222-222222222222',
    'ice',
    jsonb_build_object(
        'candidate',     'candidate:842163049 1 udp 1677729535 192.0.2.1 56789 typ srflx',
        'sdpMid',        '0',
        'sdpMLineIndex', 0
    ),
    false
),
(
    '00000000-0000-0000-0000-00000000DE01'::uuid,
    '22222222-2222-2222-2222-222222222222',
    '11111111-1111-1111-1111-111111111111',
    'ice',
    jsonb_build_object(
        'candidate',     'candidate:842163050 1 udp 1677729535 192.0.2.2 56790 typ srflx',
        'sdpMid',        '0',
        'sdpMLineIndex', 0
    ),
    false
)
ON CONFLICT DO NOTHING;
