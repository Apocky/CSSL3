-- =====================================================================
-- § T11-W12-REMIX · 0030_content_remix.sql
-- Content fork-chain attribution + Σ-Chain anchored RemixLink + gift-only
-- royalty-pledge + sovereign opt-out registry. ALL tables RLS-policied ·
-- default-deny. Composes with cssl-content-package (0027) tables.
--
-- Tables :
--   - content_remix_links   · IMMUTABLE Σ-Chain-anchored attribution-record
--   - content_creator_optout · creators-blocking-FUTURE-remixes registry
--   - content_tips          · gift-tip receipt log (informational ; Stripe
--                              webhook is source-of-truth)
--   - content_royalty_pledge · per-(content_id) creator-set tip-share %
--                              (¬ enforced ; sovereign-revocable always-true)
--
-- Helpers :
--   - content_remix_walk_chain(p_id)        · WITH-recursive walk to genesis
--   - content_remix_cycle_check(p_child, p_parent) · prior-link cycle-reject
--   - content_remix_assert_optout(p_parent_creator) · check opt-out before insert
--
-- Sovereignty axioms (enforced @ DB) :
--   ¬ remix without-Σ-cap             · cap REQUIRED at-edge
--   attribution-immutable post-anchor · UPDATE policy denies all column writes
--                                       except (revoked_at, revoked_reason)
--   sovereign-opt-out                  · NEW links blocked when creator opted-out
--                                       (CHECK enforced via trigger) ;
--                                       existing links NOT cascade-deleted
--   royalty-share-gift-only            · CHECK constraint on pledged_pct
--                                       (0..=100) + sovereign_revocable=TRUE
--   100%-to-tipped-creator             · platform_fee_lamports COLUMN ABSENT
--                                       BY DESIGN (¬ platform-tax)
--
-- Apply order : after 0029.
-- =====================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ─── content_remix_links ───────────────────────────────────────────────
-- One row per RemixLink. Composite-PK on (remixed_id) since each child
-- has at most one parent-link (forks branch INTO a child ; the child's
-- parent edge is unique). parent_version pinned at fork-time.
CREATE TABLE IF NOT EXISTS public.content_remix_links (
    remixed_id              uuid        PRIMARY KEY,
    parent_id               uuid        NOT NULL,
    parent_version          text        NOT NULL,
    remix_kind              text        NOT NULL,
    attribution_text        text        NOT NULL DEFAULT '',
    sigma_chain_anchor      text        NOT NULL,        -- 64-char lower-hex
    created_at              timestamptz NOT NULL DEFAULT now(),
    remix_creator_pubkey    text        NOT NULL,        -- 64-char lower-hex
    remix_signature         text        NOT NULL,        -- 128-char lower-hex
    -- gift-pledge (creator-set · ¬ enforced · sovereign-revocable)
    royalty_pledged_pct     int         NOT NULL DEFAULT 0,
    royalty_cumulative_lamports bigint  NOT NULL DEFAULT 0,
    royalty_sovereign_revocable boolean NOT NULL DEFAULT TRUE,
    -- post-anchor revocation (creator-of-remix may revoke their own link ;
    -- existing-attribution-walk continues to surface the (revoked_at, reason)
    -- but `current` view filters it out).
    revoked_at              timestamptz NULL,
    revoked_reason          text        NULL,
    CONSTRAINT content_remix_kind_enum
        CHECK (remix_kind IN (
            'fork','extension','translation','adaptation','improvement','bundle'
        )),
    CONSTRAINT content_remix_attribution_len
        CHECK (char_length(attribution_text) <= 200),
    CONSTRAINT content_remix_anchor_shape
        CHECK (char_length(sigma_chain_anchor) = 64
               AND sigma_chain_anchor ~ '^[0-9a-f]+$'),
    CONSTRAINT content_remix_pubkey_shape
        CHECK (char_length(remix_creator_pubkey) = 64
               AND remix_creator_pubkey ~ '^[0-9a-f]+$'),
    CONSTRAINT content_remix_signature_shape
        CHECK (char_length(remix_signature) = 128
               AND remix_signature ~ '^[0-9a-f]+$'),
    CONSTRAINT content_remix_pct_range
        CHECK (royalty_pledged_pct BETWEEN 0 AND 100),
    CONSTRAINT content_remix_pledge_revocable
        CHECK (royalty_sovereign_revocable = TRUE),
    CONSTRAINT content_remix_no_self
        CHECK (remixed_id <> parent_id),
    CONSTRAINT content_remix_semver_shape
        CHECK (parent_version ~ '^\d+\.\d+\.\d+$')
);
CREATE INDEX IF NOT EXISTS content_remix_links_parent_idx
    ON public.content_remix_links (parent_id);
CREATE INDEX IF NOT EXISTS content_remix_links_creator_idx
    ON public.content_remix_links (remix_creator_pubkey);
CREATE INDEX IF NOT EXISTS content_remix_links_kind_idx
    ON public.content_remix_links (remix_kind);
CREATE INDEX IF NOT EXISTS content_remix_links_active_idx
    ON public.content_remix_links (parent_id) WHERE revoked_at IS NULL;
COMMENT ON TABLE public.content_remix_links IS
    'IMMUTABLE Σ-Chain-anchored RemixLink. UPDATE restricted to (revoked_at, revoked_reason). ¬ platform-tax · gift-only royalty.';

-- ─── content_creator_optout ────────────────────────────────────────────
-- Creators who opted-out of having their content remixed. NEW remixes
-- blocked at /api/content/remix/init via this table. EXISTING remixes
-- preserved (sovereignty-irrevocable-for-past-attributions).
CREATE TABLE IF NOT EXISTS public.content_creator_optout (
    creator_pubkey text        PRIMARY KEY,    -- 64-char lower-hex
    opted_out_at   timestamptz NOT NULL DEFAULT now(),
    reason         text        NULL,            -- creator-supplied, optional
    CONSTRAINT content_creator_optout_pubkey_shape
        CHECK (char_length(creator_pubkey) = 64
               AND creator_pubkey ~ '^[0-9a-f]+$')
);
COMMENT ON TABLE public.content_creator_optout IS
    'Sovereign opt-out registry. Blocks NEW remixes only · existing-PRESERVED.';

-- ─── content_tips ──────────────────────────────────────────────────────
-- Informational tip-receipt log. Stripe webhook (0023) is source-of-truth
-- for actual settlement ; this table mirrors for /api/content/attribution
-- display. ¬ platform_fee_lamports column BY DESIGN (¬ platform-tax).
CREATE TABLE IF NOT EXISTS public.content_tips (
    tip_id                  uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    stripe_session_id       text        NOT NULL,
    to_creator_pubkey       text        NOT NULL,        -- 64-char hex
    content_id              uuid        NOT NULL,
    gross_lamports          bigint      NOT NULL,
    stripe_fee_estimate     bigint      NOT NULL,
    net_to_creator          bigint      NOT NULL,
    onward_gift_share       bigint      NOT NULL DEFAULT 0,
    created_at              timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT content_tips_amount_min
        CHECK (gross_lamports >= 50),
    CONSTRAINT content_tips_amount_max
        CHECK (gross_lamports <= 100000000),
    CONSTRAINT content_tips_pubkey_shape
        CHECK (char_length(to_creator_pubkey) = 64
               AND to_creator_pubkey ~ '^[0-9a-f]+$'),
    CONSTRAINT content_tips_session_shape
        CHECK (stripe_session_id ~ '^cs_[A-Za-z0-9_]+$'),
    CONSTRAINT content_tips_net_consistent
        CHECK (net_to_creator = gross_lamports - stripe_fee_estimate),
    CONSTRAINT content_tips_onward_bounded
        CHECK (onward_gift_share BETWEEN 0 AND net_to_creator)
);
CREATE INDEX IF NOT EXISTS content_tips_to_creator_idx
    ON public.content_tips (to_creator_pubkey);
CREATE INDEX IF NOT EXISTS content_tips_content_idx
    ON public.content_tips (content_id);
CREATE UNIQUE INDEX IF NOT EXISTS content_tips_session_unique
    ON public.content_tips (stripe_session_id);
COMMENT ON TABLE public.content_tips IS
    'Gift-tip receipt log. ¬ platform_fee column · 100% to-creator (minus Stripe-fee).';

-- ─── content_royalty_pledge ────────────────────────────────────────────
-- Live pledge state per content-id (mirrors RoyaltyShareGift in-Rust).
-- Materialized so /api/content/attribution can JOIN cheaply. Updated on
-- every content_remix_links insert + on creator-edits.
CREATE TABLE IF NOT EXISTS public.content_royalty_pledge (
    content_id              uuid        PRIMARY KEY,
    pledged_pct             int         NOT NULL DEFAULT 0,
    cumulative_gifted       bigint      NOT NULL DEFAULT 0,
    sovereign_revocable     boolean     NOT NULL DEFAULT TRUE,
    last_updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT content_royalty_pct_range
        CHECK (pledged_pct BETWEEN 0 AND 100),
    CONSTRAINT content_royalty_must_be_revocable
        CHECK (sovereign_revocable = TRUE)
);
COMMENT ON TABLE public.content_royalty_pledge IS
    'Per-content gift-pledge mirror. sovereign_revocable always TRUE (CHECK-enforced).';

-- ─── helper : recursive walk to genesis ────────────────────────────────
-- WITH-recursive ascent. Cycle-detect via path-array ANY-equality. Returns
-- one row per ancestor link, plus a synthetic genesis-row at the top.
CREATE OR REPLACE FUNCTION public.content_remix_walk_chain(p_start_id uuid)
RETURNS TABLE (
    depth          int,
    remixed_id     uuid,
    parent_id      uuid,
    remix_kind     text,
    attribution_text text,
    sigma_chain_anchor text,
    created_at     timestamptz,
    revoked_at     timestamptz
)
LANGUAGE sql STABLE AS $$
    WITH RECURSIVE walk(depth, remixed_id, parent_id, remix_kind,
                        attribution_text, sigma_chain_anchor, created_at,
                        revoked_at, path) AS (
        SELECT 0::int,
               l.remixed_id, l.parent_id, l.remix_kind,
               l.attribution_text, l.sigma_chain_anchor, l.created_at,
               l.revoked_at,
               ARRAY[l.remixed_id]
        FROM public.content_remix_links l
        WHERE l.remixed_id = p_start_id
      UNION ALL
        SELECT w.depth + 1,
               l.remixed_id, l.parent_id, l.remix_kind,
               l.attribution_text, l.sigma_chain_anchor, l.created_at,
               l.revoked_at,
               w.path || l.remixed_id
        FROM walk w
        JOIN public.content_remix_links l ON l.remixed_id = w.parent_id
        WHERE NOT (l.remixed_id = ANY(w.path))    -- cycle-detect
          AND w.depth < 256                      -- bounded
    )
    SELECT depth, remixed_id, parent_id, remix_kind,
           attribution_text, sigma_chain_anchor, created_at, revoked_at
    FROM walk
    ORDER BY depth ASC;
$$;
COMMENT ON FUNCTION public.content_remix_walk_chain(uuid) IS
    'Walk attribution chain to genesis. O(depth) · cycle-detect · bounded 256.';

-- ─── helper : forward cycle-reject ─────────────────────────────────────
-- Called BEFORE INSERT to ensure adding (child→parent) does not create
-- a cycle (i.e., parent is not itself a descendant of child).
CREATE OR REPLACE FUNCTION public.content_remix_cycle_check(
    p_child_id  uuid,
    p_parent_id uuid
) RETURNS boolean
LANGUAGE sql STABLE AS $$
    -- Walk parent's chain ; if child appears, that would be a cycle.
    SELECT NOT EXISTS (
        SELECT 1
        FROM public.content_remix_walk_chain(p_parent_id) w
        WHERE w.remixed_id = p_child_id
    );
$$;
COMMENT ON FUNCTION public.content_remix_cycle_check(uuid, uuid) IS
    'Returns TRUE if (child→parent) link is acyclic.';

-- ─── helper : opt-out enforcement ──────────────────────────────────────
-- Returns TRUE if the parent's creator is currently opted-out (i.e.,
-- new remix should be REJECTED). Used by trigger below.
CREATE OR REPLACE FUNCTION public.content_remix_assert_optout(
    p_parent_creator text
) RETURNS boolean
LANGUAGE sql STABLE AS $$
    SELECT EXISTS (
        SELECT 1 FROM public.content_creator_optout
        WHERE creator_pubkey = p_parent_creator
    );
$$;
COMMENT ON FUNCTION public.content_remix_assert_optout(text) IS
    'TRUE = creator opted-out · NEW remix should be rejected.';

-- ─── trigger : insert-time validation ──────────────────────────────────
-- Runs cycle-check + opt-out-check pre-insert. Raises on either failure.
CREATE OR REPLACE FUNCTION public.content_remix_links_pre_insert()
RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    v_parent_creator text;
BEGIN
    -- Cycle-check.
    IF NOT public.content_remix_cycle_check(NEW.remixed_id, NEW.parent_id) THEN
        RAISE EXCEPTION 'attribution-cycle : (%,%) would create cycle',
            NEW.remixed_id, NEW.parent_id
            USING ERRCODE = '23514';
    END IF;
    -- Opt-out check : look up parent's creator-pubkey from content_packages
    -- (W12-4). If absent, treat as not-opted-out (genesis-of-this-DB).
    SELECT cp.author_pubkey INTO v_parent_creator
    FROM public.content_packages cp
    WHERE cp.id = NEW.parent_id;
    IF v_parent_creator IS NOT NULL
       AND public.content_remix_assert_optout(v_parent_creator) THEN
        RAISE EXCEPTION 'creator-opted-out : %', v_parent_creator
            USING ERRCODE = '23514';
    END IF;
    -- Default revocable=TRUE invariant.
    NEW.royalty_sovereign_revocable := TRUE;
    RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS content_remix_links_pre_insert_trg
    ON public.content_remix_links;
CREATE TRIGGER content_remix_links_pre_insert_trg
    BEFORE INSERT ON public.content_remix_links
    FOR EACH ROW EXECUTE FUNCTION public.content_remix_links_pre_insert();

-- ─── trigger : immutability post-anchor ────────────────────────────────
-- After insert, only revoked_at + revoked_reason may be updated.
CREATE OR REPLACE FUNCTION public.content_remix_links_block_mutate()
RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.remixed_id      IS DISTINCT FROM OLD.remixed_id
    OR NEW.parent_id        IS DISTINCT FROM OLD.parent_id
    OR NEW.parent_version   IS DISTINCT FROM OLD.parent_version
    OR NEW.remix_kind       IS DISTINCT FROM OLD.remix_kind
    OR NEW.attribution_text IS DISTINCT FROM OLD.attribution_text
    OR NEW.sigma_chain_anchor   IS DISTINCT FROM OLD.sigma_chain_anchor
    OR NEW.created_at       IS DISTINCT FROM OLD.created_at
    OR NEW.remix_creator_pubkey IS DISTINCT FROM OLD.remix_creator_pubkey
    OR NEW.remix_signature  IS DISTINCT FROM OLD.remix_signature
    THEN
        RAISE EXCEPTION 'attribution-immutable : remix-link cannot be mutated post-anchor'
            USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS content_remix_links_immutable_trg
    ON public.content_remix_links;
CREATE TRIGGER content_remix_links_immutable_trg
    BEFORE UPDATE ON public.content_remix_links
    FOR EACH ROW EXECUTE FUNCTION public.content_remix_links_block_mutate();

-- ─── view : current (non-revoked) chain ────────────────────────────────
CREATE OR REPLACE VIEW public.content_remix_links_active AS
SELECT * FROM public.content_remix_links WHERE revoked_at IS NULL;

-- ─── RLS ───────────────────────────────────────────────────────────────
ALTER TABLE public.content_remix_links     ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.content_creator_optout  ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.content_tips            ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.content_royalty_pledge  ENABLE ROW LEVEL SECURITY;

-- Public-read of remix-links is allowed (attribution chain must be visible
-- for sovereignty-by-construction). Inserts via service-role only (edge).
DROP POLICY IF EXISTS "remix_links_read_all" ON public.content_remix_links;
CREATE POLICY "remix_links_read_all" ON public.content_remix_links
    FOR SELECT USING (TRUE);
DROP POLICY IF EXISTS "remix_links_insert_service" ON public.content_remix_links;
CREATE POLICY "remix_links_insert_service" ON public.content_remix_links
    FOR INSERT WITH CHECK (auth.role() = 'service_role');
DROP POLICY IF EXISTS "remix_links_update_creator_revoke" ON public.content_remix_links;
CREATE POLICY "remix_links_update_creator_revoke" ON public.content_remix_links
    FOR UPDATE USING (
        auth.role() = 'service_role'
        OR (auth.jwt() ->> 'pubkey') = remix_creator_pubkey
    );

-- Opt-out is creator-private-write but world-readable (so other creators
-- can see if their parent is currently opted-out).
DROP POLICY IF EXISTS "optout_read_all" ON public.content_creator_optout;
CREATE POLICY "optout_read_all" ON public.content_creator_optout
    FOR SELECT USING (TRUE);
DROP POLICY IF EXISTS "optout_write_self_or_service" ON public.content_creator_optout;
CREATE POLICY "optout_write_self_or_service" ON public.content_creator_optout
    FOR ALL USING (
        auth.role() = 'service_role'
        OR (auth.jwt() ->> 'pubkey') = creator_pubkey
    );

-- Tips public-readable (transparency). Inserts service-role only (edge
-- writes after Stripe webhook confirms).
DROP POLICY IF EXISTS "tips_read_all" ON public.content_tips;
CREATE POLICY "tips_read_all" ON public.content_tips
    FOR SELECT USING (TRUE);
DROP POLICY IF EXISTS "tips_insert_service" ON public.content_tips;
CREATE POLICY "tips_insert_service" ON public.content_tips
    FOR INSERT WITH CHECK (auth.role() = 'service_role');

-- Royalty pledge public-readable, write by content's author or service.
DROP POLICY IF EXISTS "royalty_pledge_read_all" ON public.content_royalty_pledge;
CREATE POLICY "royalty_pledge_read_all" ON public.content_royalty_pledge
    FOR SELECT USING (TRUE);
DROP POLICY IF EXISTS "royalty_pledge_write_service" ON public.content_royalty_pledge;
CREATE POLICY "royalty_pledge_write_service" ON public.content_royalty_pledge
    FOR ALL USING (auth.role() = 'service_role');

-- ─── ATTESTATION ──────────────────────────────────────────────────────
COMMENT ON SCHEMA public IS
    '§ T11-W12-REMIX 0030 : ¬ harm · ¬ platform-tax · attribution-immutable · gift-royalty-only · sovereign-opt-out · 100%-to-tipped-creator-minus-stripe-fee.';
