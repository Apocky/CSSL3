// cssl-edge/lib/mneme/types.ts
// MNEME — TypeScript shapes for memory profiles, messages, memories, and pipeline I/O.
// Spec : ../../specs/43_MNEME.csl + 44_MNEME_PIPELINES.csl + 45_MNEME_SCHEMA.csl
//
// Storage canonical = CSL. paraphrase + search_queries are denormalised views.

// ── Core enums ────────────────────────────────────────────────────────
export type MemoryType = 'fact' | 'event' | 'instruction' | 'task';
export type Role       = 'user' | 'assistant' | 'system' | 'tool';

export type ChannelName =
    | 'fts_csl'
    | 'fts_paraphrase'
    | 'fts_messages'
    | 'topic_exact'
    | 'vec_direct'
    | 'vec_hyde';

// ── Sigma mask (re-exposed from the cap layer; full spec in specs/27) ─
// Carried as a 32-byte bytea on the wire (19 packed + 13 zeros padding to align).
export type SigmaMaskBytes = Uint8Array;

// ── Profile ───────────────────────────────────────────────────────────
export interface Profile {
    profile_id:     string;
    sovereign_pk:   Uint8Array;        // Ed25519 PK (32 bytes)
    sigma_mask:     SigmaMaskBytes;
    created_at:     string;            // ISO timestamp
    memory_count:   number;
    message_count:  number;
    meta:           Record<string, unknown>;
}

// ── Raw message ───────────────────────────────────────────────────────
// Wire shape — server fills `id` + `ts` from inputs.
export interface MessageInput {
    role:    Role;
    content: string;
}

export interface Message {
    id:          string;               // sha256[:32] hex
    profile_id:  string;
    session_id:  string;
    role:        Role;
    content:     string;
    ts:          string;
    sigma_mask:  SigmaMaskBytes;
}

// ── Memory ────────────────────────────────────────────────────────────
export interface Memory {
    id:              string;           // uuid
    profile_id:      string;
    type:            MemoryType;
    csl:             string;
    paraphrase:      string;
    topic_key:       string | null;
    search_queries:  string[];
    source_msg_ids:  string[];
    superseded_by:   string | null;
    sigma_mask:      SigmaMaskBytes;
    created_at:      string;
    embedding:       Float32Array | null;
}

// Public-facing memory shape (without bytea blobs / vector).
export interface MemoryPublic {
    id:              string;
    profile_id:      string;
    type:            MemoryType;
    csl:             string;
    paraphrase:      string;
    topic_key:       string | null;
    search_queries:  string[];
    source_msg_ids:  string[];
    superseded_by:   string | null;
    created_at:      string;
}

// ── API request / response shapes ────────────────────────────────────

export interface IngestRequest {
    session_id: string;
    messages:   MessageInput[];
    sigma_mask_hex?: string;
}

export interface IngestResponse {
    ok:        true;
    stored:    number;
    deduped:   number;
    extracted: number;
    dropped:   number;
    served_by: string;
    ts:        string;
    profile_id: string;
    session_id: string;
}

export interface RememberRequest {
    csl:        string;
    paraphrase?: string;
    type?:      MemoryType;
    topic_key?: string;
    sigma_mask_hex?: string;
}

export interface RememberResponse {
    ok:        true;
    memory:    MemoryPublic;
    served_by: string;
    ts:        string;
}

export interface RecallRequest {
    query:    string;
    k?:       number;
    types?:   MemoryType[];
    audience_bits?: number;
    debug?:   boolean;
}

export interface RecallResponse {
    ok:         true;
    result_nl:  string;
    result_csl: string;
    citations:  string[];
    confidence: number;
    debug?:     RetrievalDebug;
    served_by:  string;
    ts:         string;
}

export interface RetrievalDebug {
    channel_scores: Partial<Record<ChannelName, Array<[string, number]>>>;
    rrf_top_k:      Array<[string, number]>;
    synth_prompt_tokens: number;
    latency_ms:     Record<string, number>;
}

export interface ListRequest {
    type?:   MemoryType;
    limit?:  number;
    cursor?: string;
}

export interface ListResponse {
    ok:          true;
    memories:    MemoryPublic[];
    next_cursor: string | null;
    served_by:   string;
    ts:          string;
}

export interface ForgetResponse {
    ok:        true;
    revoked:   boolean;
    cascade:   number;
    served_by: string;
    ts:        string;
}

export interface ExportResponse {
    ok:        true;
    profile:   Profile;
    memories:  MemoryPublic[];
    messages:  Array<Omit<Message, 'sigma_mask'>>;
    served_by: string;
    ts:        string;
}

// ── Pipeline-internal types (used by lib/mneme/pipeline-*.ts) ────────

export interface ExtractedCandidate {
    csl:           string;
    paraphrase:    string;
    span:          [number, number];
    pass:          'full' | 'detail';
}

export interface VerifiedCandidate extends ExtractedCandidate {
    verdict: 'pass' | 'corrected' | 'dropped';
    drop_reason?: string;
}

export interface ClassifiedCandidate extends VerifiedCandidate {
    type:           MemoryType;
    topic_key:      string | null;
    search_queries: string[];
}

export interface QueryAnalysis {
    topic_keys:      string[];
    fts_terms:       string[];
    hyde_csl:        string;
    hyde_paraphrase: string;
    is_temporal:     boolean;
}

export interface ChannelHit {
    memory_id: string;
    score:     number;
}

// ── Errors ────────────────────────────────────────────────────────────
export class MnemeError extends Error {
    constructor(public code: string, message: string, public httpStatus = 500) {
        super(message);
        this.name = 'MnemeError';
    }
}
