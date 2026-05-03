-- =====================================================================
-- § T11-WAVE-MNEME · 0040_mneme.sql
-- ════════════════════════════════════════════════════════════════════
-- MNEME agent memory layer · CSL-canonical storage · pgvector + FTS
--
-- Spec : ../specs/43_MNEME.csl + 44_MNEME_PIPELINES.csl + 45_MNEME_SCHEMA.csl
--
-- Tables (4) :
--   - mneme_profiles  : per-context memory namespace · sovereign-owned
--   - mneme_messages  : raw conversation messages · content-addressed IDs
--   - mneme_memories  : classified memories · CSL-canonical · sigma-mask-gated
--   - mneme_audit     : append-only state-change log
--
-- Triggers (2) :
--   - mneme_supersede_on_insert  : forward-pointer chain on topic_key collision
--   - mneme_bump_*_count          : denormalised counters on profile rows
--
-- Apply order : after 0039_seasons (slot 0040 is unclaimed).
-- =====================================================================

-- ─── extensions ─────────────────────────────────────────────────────────
CREATE EXTENSION IF NOT EXISTS pgcrypto;   -- gen_random_uuid (already present)
CREATE EXTENSION IF NOT EXISTS pg_trgm;    -- already present
CREATE EXTENSION IF NOT EXISTS vector;     -- pgvector >= 0.7.0 required
                                           -- (Supabase: enable via Database > Extensions)

-- =====================================================================
-- public.mneme_profiles
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.mneme_profiles (
    profile_id     text        PRIMARY KEY,
    sovereign_pk   bytea       NOT NULL,
    sigma_mask     bytea       NOT NULL,
    created_at     timestamptz NOT NULL DEFAULT now(),
    memory_count   integer     NOT NULL DEFAULT 0,
    message_count  integer     NOT NULL DEFAULT 0,
    meta           jsonb       NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT mneme_profile_id_shape
        CHECK (profile_id ~ '^[a-z0-9-]{1,64}$'),
    CONSTRAINT mneme_sovereign_pk_len
        CHECK (octet_length(sovereign_pk) = 32),
    CONSTRAINT mneme_sigma_mask_len
        CHECK (octet_length(sigma_mask) BETWEEN 19 AND 32)
);
COMMENT ON TABLE public.mneme_profiles IS
    'MNEME memory profile · one per agent context · sovereign-owned · spec §§43.';
COMMENT ON COLUMN public.mneme_profiles.sovereign_pk IS
    'Ed25519 public key of the profile owner (32 bytes raw).';
COMMENT ON COLUMN public.mneme_profiles.sigma_mask IS
    'Default SigmaMask inherited by new memories in this profile (19B packed, see specs/27).';

-- =====================================================================
-- public.mneme_messages
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.mneme_messages (
    id             text        PRIMARY KEY,
    profile_id     text        NOT NULL
                               REFERENCES public.mneme_profiles(profile_id) ON DELETE CASCADE,
    session_id     text        NOT NULL,
    role           text        NOT NULL
                               CHECK (role IN ('user','assistant','system','tool')),
    content        text        NOT NULL,
    ts             timestamptz NOT NULL DEFAULT now(),
    sigma_mask     bytea       NOT NULL,
    content_tsv    tsvector    GENERATED ALWAYS AS (to_tsvector('english', content)) STORED,
    CONSTRAINT mneme_msg_id_shape
        CHECK (id ~ '^[0-9a-f]{32}$'),
    CONSTRAINT mneme_msg_content_len
        CHECK (char_length(content) BETWEEN 1 AND 65536),
    CONSTRAINT mneme_msg_sigma_mask_len
        CHECK (octet_length(sigma_mask) BETWEEN 19 AND 32)
);
CREATE INDEX IF NOT EXISTS mneme_msg_profile_session_idx
    ON public.mneme_messages (profile_id, session_id, ts);
CREATE INDEX IF NOT EXISTS mneme_msg_content_tsv_idx
    ON public.mneme_messages USING gin (content_tsv);
CREATE INDEX IF NOT EXISTS mneme_msg_profile_ts_idx
    ON public.mneme_messages (profile_id, ts DESC);

COMMENT ON TABLE public.mneme_messages IS
    'Raw conversation messages · content-addressed IDs (sha256[:32]) · idempotent re-ingest · spec §§43.';

-- =====================================================================
-- public.mneme_memories
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.mneme_memories (
    id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    profile_id      text        NOT NULL
                                REFERENCES public.mneme_profiles(profile_id) ON DELETE CASCADE,
    type            text        NOT NULL
                                CHECK (type IN ('fact','event','instruction','task')),
    csl             text        NOT NULL,
    paraphrase      text        NOT NULL,
    topic_key       text,
    search_queries  jsonb       NOT NULL DEFAULT '[]'::jsonb,
    source_msg_ids  text[]      NOT NULL DEFAULT ARRAY[]::text[],
    superseded_by   uuid        REFERENCES public.mneme_memories(id) ON DELETE SET NULL,
    sigma_mask      bytea       NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    embedding       vector(1024),

    -- Generated tsvectors. CSL uses 'simple' tokenizer with operator chars normalised
    -- to spaces so morpheme paths like 'user.pref.pkg-mgr' tokenise into atomic parts.
    csl_tsv         tsvector    GENERATED ALWAYS AS (
                                  to_tsvector('simple',
                                    regexp_replace(csl, '[\.\+\-@\u2297]', ' ', 'g'))
                                ) STORED,
    paraphrase_tsv  tsvector    GENERATED ALWAYS AS (
                                  to_tsvector('english', paraphrase)
                                ) STORED,

    CONSTRAINT mneme_csl_len           CHECK (char_length(csl)        BETWEEN 1 AND 4096),
    CONSTRAINT mneme_paraphrase_len    CHECK (char_length(paraphrase) BETWEEN 1 AND 1024),
    CONSTRAINT mneme_topic_key_len     CHECK (topic_key IS NULL OR char_length(topic_key) BETWEEN 1 AND 256),
    CONSTRAINT mneme_topic_discipline  CHECK (
        (type IN ('fact','instruction')   AND topic_key IS NOT NULL) OR
        (type IN ('event','task')         AND topic_key IS NULL)
    ),
    CONSTRAINT mneme_queries_len       CHECK (jsonb_array_length(search_queries) BETWEEN 0 AND 5),
    CONSTRAINT mneme_supersede_self    CHECK (id <> superseded_by),
    CONSTRAINT mneme_sigma_mask_len    CHECK (octet_length(sigma_mask) BETWEEN 19 AND 32)
);

CREATE INDEX IF NOT EXISTS mneme_mem_profile_active_idx
    ON public.mneme_memories (profile_id, type, topic_key)
    WHERE superseded_by IS NULL;
CREATE INDEX IF NOT EXISTS mneme_mem_csl_tsv_idx
    ON public.mneme_memories USING gin (csl_tsv);
CREATE INDEX IF NOT EXISTS mneme_mem_paraphrase_tsv_idx
    ON public.mneme_memories USING gin (paraphrase_tsv);
CREATE INDEX IF NOT EXISTS mneme_mem_created_idx
    ON public.mneme_memories (profile_id, created_at DESC);
CREATE INDEX IF NOT EXISTS mneme_mem_supersede_chain_idx
    ON public.mneme_memories (superseded_by) WHERE superseded_by IS NOT NULL;

-- pgvector ivfflat · cosine distance · lists = 100 (good for ≤10K rows).
-- AS table grows, ALTER to lists = ⌈√N⌉ ; pgvector docs recommend re-creating index.
CREATE INDEX IF NOT EXISTS mneme_mem_embedding_idx
    ON public.mneme_memories USING ivfflat (embedding vector_cosine_ops)
    WITH (lists = 100);

COMMENT ON TABLE public.mneme_memories IS
    'MNEME classified memories. CSL-canonical storage. SigmaMask per row · §§43.';
COMMENT ON COLUMN public.mneme_memories.csl IS
    'Canonical CSLv3 form (validated). The source of truth for this memory.';
COMMENT ON COLUMN public.mneme_memories.paraphrase IS
    'English paraphrase. Denormalised view for synthesis. Never the source of truth.';
COMMENT ON COLUMN public.mneme_memories.topic_key IS
    'Head morpheme path (e.g. user.pref.pkg-mgr). Required for fact/instruction. Used for supersession.';
COMMENT ON COLUMN public.mneme_memories.embedding IS
    'voyage-3-large 1024d. Embedding text = csl + paraphrase + search_queries (joined newline).';

-- =====================================================================
-- public.mneme_audit
-- =====================================================================
CREATE TABLE IF NOT EXISTS public.mneme_audit (
    id          bigserial   PRIMARY KEY,
    profile_id  text        NOT NULL
                            REFERENCES public.mneme_profiles(profile_id) ON DELETE CASCADE,
    ts          timestamptz NOT NULL DEFAULT now(),
    kind        text        NOT NULL
                            CHECK (kind IN ('ingest','remember','recall','forget',
                                           'supersede','export','vacuum',
                                           'csl_invalid','verify_drop')),
    memory_id   uuid        REFERENCES public.mneme_memories(id) ON DELETE SET NULL,
    caller_pk   bytea,
    details     jsonb       NOT NULL DEFAULT '{}'::jsonb
);
CREATE INDEX IF NOT EXISTS mneme_audit_profile_ts_idx
    ON public.mneme_audit (profile_id, ts DESC);
CREATE INDEX IF NOT EXISTS mneme_audit_kind_ts_idx
    ON public.mneme_audit (kind, ts DESC);

COMMENT ON TABLE public.mneme_audit IS
    'Append-only audit log · every memory state-change emits a row · §§43.';

-- =====================================================================
-- Triggers
-- =====================================================================

-- Supersession chain : on insert of fact|instruction with topic_key,
-- mark any existing active row with the same (profile,type,topic_key) as
-- superseded by this new row.
CREATE OR REPLACE FUNCTION public.mneme_supersede_on_insert()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.topic_key IS NOT NULL AND NEW.superseded_by IS NULL THEN
        UPDATE public.mneme_memories
           SET superseded_by = NEW.id
         WHERE profile_id    = NEW.profile_id
           AND type          = NEW.type
           AND topic_key     = NEW.topic_key
           AND id            <> NEW.id
           AND superseded_by IS NULL;
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS mneme_supersede_trigger ON public.mneme_memories;
CREATE TRIGGER mneme_supersede_trigger
AFTER INSERT ON public.mneme_memories
FOR EACH ROW EXECUTE FUNCTION public.mneme_supersede_on_insert();

-- Memory count maintenance.
CREATE OR REPLACE FUNCTION public.mneme_bump_memory_count()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        UPDATE public.mneme_profiles
           SET memory_count = memory_count + 1
         WHERE profile_id = NEW.profile_id;
    ELSIF TG_OP = 'DELETE' THEN
        UPDATE public.mneme_profiles
           SET memory_count = GREATEST(0, memory_count - 1)
         WHERE profile_id = OLD.profile_id;
    END IF;
    RETURN COALESCE(NEW, OLD);
END $$;

DROP TRIGGER IF EXISTS mneme_memory_count_trigger ON public.mneme_memories;
CREATE TRIGGER mneme_memory_count_trigger
AFTER INSERT OR DELETE ON public.mneme_memories
FOR EACH ROW EXECUTE FUNCTION public.mneme_bump_memory_count();

-- Message count maintenance.
CREATE OR REPLACE FUNCTION public.mneme_bump_message_count()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        UPDATE public.mneme_profiles
           SET message_count = message_count + 1
         WHERE profile_id = NEW.profile_id;
    ELSIF TG_OP = 'DELETE' THEN
        UPDATE public.mneme_profiles
           SET message_count = GREATEST(0, message_count - 1)
         WHERE profile_id = OLD.profile_id;
    END IF;
    RETURN COALESCE(NEW, OLD);
END $$;

DROP TRIGGER IF EXISTS mneme_message_count_trigger ON public.mneme_messages;
CREATE TRIGGER mneme_message_count_trigger
AFTER INSERT OR DELETE ON public.mneme_messages
FOR EACH ROW EXECUTE FUNCTION public.mneme_bump_message_count();

-- =====================================================================
-- Helper : extract revoked_at u32 (LE) from packed SigmaMask bytes 11..14
-- See specs/27_SIGMA_MASK_RUNTIME.csl § Σ-MASK-BIT-LAYOUT.
-- =====================================================================
CREATE OR REPLACE FUNCTION public.mneme_mask_revoked_at(mask bytea)
RETURNS bigint
LANGUAGE sql IMMUTABLE
AS $$
    SELECT (get_byte(mask, 11)::bigint)
         | (get_byte(mask, 12)::bigint << 8)
         | (get_byte(mask, 13)::bigint << 16)
         | (get_byte(mask, 14)::bigint << 24)
$$;

COMMENT ON FUNCTION public.mneme_mask_revoked_at(bytea) IS
    'Extract revoked_at unix-seconds (u32 LE) from packed SigmaMask · 0 = active · spec §§27.';

-- =====================================================================
-- Done. Apply 0041_mneme_rls.sql next for row-level security policies.
-- =====================================================================
