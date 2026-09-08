-- =====================================================================
-- § APOCRYPHA DURABLE JOB CONTROL PLANE
-- =====================================================================
-- Additive successor to 0043/0044. The older chat relay remains intact as
-- a rollback surface while clients move to this durable, multi-tenant job
-- protocol.
--
-- Trust boundaries:
--   * browsers may read only jobs owned by their authenticated principal;
--   * application servers enqueue/cancel with service_role;
--   * outbound workers authenticate with high-entropy tokens stored only as
--     SHA-256 hashes and mutate jobs only through fenced RPCs;
--   * attempts, chunks, snapshots, revisions, lifecycle events, entitlement
--     entries, and alert deliveries preserve their historical records;
--   * alert delivery never emits another alert event, preventing recursion.
--
-- Worker flow:
--   enqueue -> claim (FOR UPDATE SKIP LOCKED) -> renew/append -> complete|fail
--   stale lease -> reap -> requeue|fail|cancel
-- =====================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ─── Tenant and principal identity ────────────────────────────────────

CREATE TABLE public.apocrypha_tenant (
    id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    slug            text        NOT NULL UNIQUE,
    display_name    text        NOT NULL,
    status          text        NOT NULL DEFAULT 'active',
    metadata        jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_tenant_slug_shape
        CHECK (slug = lower(slug) AND slug ~ '^[a-z0-9][a-z0-9-]{1,62}$'),
    CONSTRAINT apocrypha_tenant_display_name_length
        CHECK (char_length(btrim(display_name)) BETWEEN 1 AND 120),
    CONSTRAINT apocrypha_tenant_status_enum
        CHECK (status IN ('active', 'suspended', 'retired')),
    CONSTRAINT apocrypha_tenant_metadata_size
        CHECK (octet_length(metadata::text) <= 65536)
);

CREATE TABLE public.apocrypha_principal (
    id                      uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id               uuid        NOT NULL REFERENCES public.apocrypha_tenant(id) ON DELETE CASCADE,
    auth_user_id            uuid        REFERENCES auth.users(id) ON DELETE CASCADE,
    principal_kind          text        NOT NULL,
    external_subject_hash   text,
    display_name            text        NOT NULL,
    status                  text        NOT NULL DEFAULT 'active',
    metadata                jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at              timestamptz NOT NULL DEFAULT now(),
    updated_at              timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_principal_tenant_id_id_unique UNIQUE (tenant_id, id),
    CONSTRAINT apocrypha_principal_kind_enum
        CHECK (principal_kind IN ('owner', 'member', 'service', 'guest')),
    CONSTRAINT apocrypha_principal_status_enum
        CHECK (status IN ('active', 'suspended', 'revoked')),
    CONSTRAINT apocrypha_principal_display_name_length
        CHECK (char_length(btrim(display_name)) BETWEEN 1 AND 120),
    CONSTRAINT apocrypha_principal_subject_hash_shape
        CHECK (
            external_subject_hash IS NULL
            OR external_subject_hash ~ '^[0-9a-f]{64}$'
        ),
    CONSTRAINT apocrypha_principal_identity_present
        CHECK (
            auth_user_id IS NOT NULL
            OR external_subject_hash IS NOT NULL
            OR principal_kind = 'service'
        ),
    CONSTRAINT apocrypha_principal_metadata_size
        CHECK (octet_length(metadata::text) <= 65536)
);

CREATE UNIQUE INDEX apocrypha_principal_auth_user_unique
    ON public.apocrypha_principal (tenant_id, auth_user_id)
    WHERE auth_user_id IS NOT NULL;
CREATE UNIQUE INDEX apocrypha_principal_external_subject_unique
    ON public.apocrypha_principal (tenant_id, external_subject_hash)
    WHERE external_subject_hash IS NOT NULL;
CREATE UNIQUE INDEX apocrypha_principal_one_owner_per_tenant
    ON public.apocrypha_principal (tenant_id)
    WHERE principal_kind = 'owner';
CREATE INDEX apocrypha_principal_user_lookup
    ON public.apocrypha_principal (auth_user_id, tenant_id)
    WHERE auth_user_id IS NOT NULL;

-- ─── Outbound worker identities ───────────────────────────────────────

CREATE TABLE public.apocrypha_worker_node (
    id                      uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id               uuid        REFERENCES public.apocrypha_tenant(id) ON DELETE CASCADE,
    node_key                text        NOT NULL UNIQUE,
    display_name            text        NOT NULL,
    token_hash              text        NOT NULL,
    token_last_four         text        NOT NULL,
    token_version           integer     NOT NULL DEFAULT 1,
    status                  text        NOT NULL DEFAULT 'active',
    allowed_capabilities    text[]      NOT NULL,
    max_concurrency         smallint    NOT NULL DEFAULT 1,
    model_profiles          jsonb       NOT NULL DEFAULT '{}'::jsonb,
    metadata                jsonb       NOT NULL DEFAULT '{}'::jsonb,
    token_issued_at         timestamptz NOT NULL DEFAULT now(),
    last_seen_at            timestamptz,
    revoked_at              timestamptz,
    revoke_reason           text,
    created_at              timestamptz NOT NULL DEFAULT now(),
    updated_at              timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_worker_node_key_shape
        CHECK (node_key = lower(node_key) AND node_key ~ '^[a-z0-9][a-z0-9._-]{1,62}$'),
    CONSTRAINT apocrypha_worker_node_display_name_length
        CHECK (char_length(btrim(display_name)) BETWEEN 1 AND 120),
    CONSTRAINT apocrypha_worker_node_token_hash_shape
        CHECK (token_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT apocrypha_worker_node_token_last_four_shape
        CHECK (token_last_four ~ '^[0-9a-f]{4}$'),
    CONSTRAINT apocrypha_worker_node_token_version_positive
        CHECK (token_version > 0),
    CONSTRAINT apocrypha_worker_node_status_enum
        CHECK (status IN ('active', 'draining', 'revoked')),
    CONSTRAINT apocrypha_worker_node_capabilities_nonempty
        CHECK (cardinality(allowed_capabilities) > 0),
    CONSTRAINT apocrypha_worker_node_concurrency_range
        CHECK (max_concurrency BETWEEN 1 AND 16),
    CONSTRAINT apocrypha_worker_node_revoke_consistency
        CHECK (
            (status = 'revoked' AND revoked_at IS NOT NULL)
            OR (status <> 'revoked' AND revoked_at IS NULL)
        ),
    CONSTRAINT apocrypha_worker_node_metadata_size
        CHECK (octet_length(metadata::text) <= 65536),
    CONSTRAINT apocrypha_worker_node_profiles_size
        CHECK (octet_length(model_profiles::text) <= 262144)
);

CREATE INDEX apocrypha_worker_node_active_lookup
    ON public.apocrypha_worker_node (status, last_seen_at DESC)
    WHERE status IN ('active', 'draining');

-- ─── Durable jobs and attempts ────────────────────────────────────────

CREATE TABLE public.apocrypha_job (
    id                      uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id               uuid        NOT NULL REFERENCES public.apocrypha_tenant(id) ON DELETE CASCADE,
    owner_principal_id      uuid        NOT NULL,
    parent_job_id           uuid,
    kind                    text        NOT NULL,
    capability              text        NOT NULL,
    job_role                text        NOT NULL DEFAULT 'primary',
    status                  text        NOT NULL DEFAULT 'queued',
    request                 jsonb       NOT NULL,
    request_hash            text        NOT NULL,
    idempotency_scope       text        NOT NULL,
    idempotency_key         text        NOT NULL,
    priority                smallint    NOT NULL DEFAULT 0,
    max_attempts            smallint    NOT NULL DEFAULT 3,
    attempt_count           integer     NOT NULL DEFAULT 0,
    lease_epoch             bigint      NOT NULL DEFAULT 0,
    current_attempt_id      uuid,
    terminal_revision_id    uuid,
    model_alias             text        NOT NULL,
    profile_hash            text        NOT NULL,
    tool_registry_version   text        NOT NULL,
    memory_manifest_hash    text        NOT NULL,
    available_at            timestamptz NOT NULL DEFAULT now(),
    cancel_requested_at     timestamptz,
    completed_at            timestamptz,
    error_code              text,
    error_detail            text,
    metadata                jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at              timestamptz NOT NULL DEFAULT now(),
    updated_at              timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_job_tenant_id_id_unique UNIQUE (tenant_id, id),
    CONSTRAINT apocrypha_job_owner_fk
        FOREIGN KEY (tenant_id, owner_principal_id)
        REFERENCES public.apocrypha_principal(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT apocrypha_job_parent_fk
        FOREIGN KEY (tenant_id, parent_job_id)
        REFERENCES public.apocrypha_job(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT apocrypha_job_idempotency_unique
        UNIQUE (tenant_id, owner_principal_id, kind, idempotency_scope, idempotency_key),
    CONSTRAINT apocrypha_job_kind_shape
        CHECK (kind ~ '^[a-z][a-z0-9._:-]{1,62}$'),
    CONSTRAINT apocrypha_job_capability_shape
        CHECK (capability ~ '^[a-z][a-z0-9._:-]{1,95}$'),
    CONSTRAINT apocrypha_job_role_enum
        CHECK (job_role IN ('primary', 'corroboration', 'synthesis', 'tool')),
    CONSTRAINT apocrypha_job_status_enum
        CHECK (status IN (
            'queued', 'leased', 'running', 'cancel_requested',
            'succeeded', 'failed', 'cancelled'
        )),
    CONSTRAINT apocrypha_job_request_hash_shape
        CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT apocrypha_job_idempotency_scope_length
        CHECK (char_length(idempotency_scope) BETWEEN 1 AND 160),
    CONSTRAINT apocrypha_job_idempotency_key_length
        CHECK (char_length(idempotency_key) BETWEEN 1 AND 200),
    CONSTRAINT apocrypha_job_priority_range
        CHECK (priority BETWEEN -100 AND 100),
    CONSTRAINT apocrypha_job_max_attempts_range
        CHECK (max_attempts BETWEEN 1 AND 10),
    CONSTRAINT apocrypha_job_attempt_count_range
        CHECK (attempt_count BETWEEN 0 AND max_attempts),
    CONSTRAINT apocrypha_job_lease_epoch_nonnegative
        CHECK (lease_epoch >= 0),
    CONSTRAINT apocrypha_job_request_size
        CHECK (octet_length(request::text) <= 1048576),
    CONSTRAINT apocrypha_job_metadata_size
        CHECK (octet_length(metadata::text) <= 262144),
    CONSTRAINT apocrypha_job_model_alias_length
        CHECK (char_length(model_alias) BETWEEN 1 AND 160),
    CONSTRAINT apocrypha_job_profile_hash_shape
        CHECK (profile_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT apocrypha_job_memory_manifest_hash_shape
        CHECK (memory_manifest_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT apocrypha_job_tool_registry_version_length
        CHECK (char_length(tool_registry_version) BETWEEN 1 AND 160),
    CONSTRAINT apocrypha_job_error_code_length
        CHECK (error_code IS NULL OR char_length(error_code) BETWEEN 1 AND 120),
    CONSTRAINT apocrypha_job_error_detail_length
        CHECK (error_detail IS NULL OR char_length(error_detail) <= 4096),
    CONSTRAINT apocrypha_job_cancel_timestamp_consistency
        CHECK (
            (status = 'cancel_requested' AND cancel_requested_at IS NOT NULL)
            OR status <> 'cancel_requested'
        ),
    CONSTRAINT apocrypha_job_completion_timestamp_consistency
        CHECK (
            (status IN ('succeeded', 'failed', 'cancelled') AND completed_at IS NOT NULL)
            OR (status NOT IN ('succeeded', 'failed', 'cancelled') AND completed_at IS NULL)
        )
);

CREATE INDEX apocrypha_job_queue_claim
    ON public.apocrypha_job (priority DESC, available_at, created_at)
    WHERE status = 'queued';
CREATE INDEX apocrypha_job_owner_status_created
    ON public.apocrypha_job (owner_principal_id, status, created_at DESC);
CREATE INDEX apocrypha_job_tenant_status_created
    ON public.apocrypha_job (tenant_id, status, created_at DESC);
CREATE INDEX apocrypha_job_parent_created
    ON public.apocrypha_job (parent_job_id, created_at)
    WHERE parent_job_id IS NOT NULL;

CREATE TABLE public.apocrypha_job_attempt (
    id                  uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id              uuid        NOT NULL REFERENCES public.apocrypha_job(id) ON DELETE CASCADE,
    worker_node_id      uuid        NOT NULL REFERENCES public.apocrypha_worker_node(id),
    claim_key           text        NOT NULL,
    attempt_no          integer     NOT NULL,
    lease_epoch         bigint      NOT NULL,
    lease_token_hash    text        NOT NULL,
    lease_token_ciphertext bytea    NOT NULL,
    status              text        NOT NULL DEFAULT 'leased',
    leased_at           timestamptz NOT NULL DEFAULT now(),
    lease_expires_at    timestamptz NOT NULL,
    started_at          timestamptz,
    last_heartbeat_at   timestamptz NOT NULL DEFAULT now(),
    finished_at         timestamptz,
    failure_code        text,
    failure_detail      text,
    metrics             jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_job_attempt_job_id_id_unique UNIQUE (job_id, id),
    CONSTRAINT apocrypha_job_attempt_number_unique UNIQUE (job_id, attempt_no),
    CONSTRAINT apocrypha_job_attempt_epoch_unique UNIQUE (job_id, lease_epoch),
    CONSTRAINT apocrypha_job_attempt_claim_key_unique UNIQUE (worker_node_id, claim_key),
    CONSTRAINT apocrypha_job_attempt_claim_key_length
        CHECK (char_length(claim_key) BETWEEN 8 AND 200),
    CONSTRAINT apocrypha_job_attempt_number_positive CHECK (attempt_no > 0),
    CONSTRAINT apocrypha_job_attempt_epoch_positive CHECK (lease_epoch > 0),
    CONSTRAINT apocrypha_job_attempt_lease_token_hash_shape
        CHECK (lease_token_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT apocrypha_job_attempt_lease_token_ciphertext_size
        CHECK (octet_length(lease_token_ciphertext) BETWEEN 32 AND 2048),
    CONSTRAINT apocrypha_job_attempt_status_enum
        CHECK (status IN ('leased', 'running', 'succeeded', 'failed', 'abandoned', 'cancelled')),
    CONSTRAINT apocrypha_job_attempt_lease_window
        CHECK (lease_expires_at > leased_at),
    CONSTRAINT apocrypha_job_attempt_finish_consistency
        CHECK (
            (status IN ('succeeded', 'failed', 'abandoned', 'cancelled') AND finished_at IS NOT NULL)
            OR (status IN ('leased', 'running') AND finished_at IS NULL)
        ),
    CONSTRAINT apocrypha_job_attempt_failure_code_length
        CHECK (failure_code IS NULL OR char_length(failure_code) BETWEEN 1 AND 120),
    CONSTRAINT apocrypha_job_attempt_failure_detail_length
        CHECK (failure_detail IS NULL OR char_length(failure_detail) <= 4096),
    CONSTRAINT apocrypha_job_attempt_metrics_size
        CHECK (octet_length(metrics::text) <= 262144)
);

CREATE INDEX apocrypha_job_attempt_worker_active
    ON public.apocrypha_job_attempt (worker_node_id, lease_expires_at)
    WHERE status IN ('leased', 'running');
CREATE INDEX apocrypha_job_attempt_expired
    ON public.apocrypha_job_attempt (lease_expires_at)
    WHERE status IN ('leased', 'running');

ALTER TABLE public.apocrypha_job
    ADD CONSTRAINT apocrypha_job_current_attempt_fk
    FOREIGN KEY (id, current_attempt_id)
    REFERENCES public.apocrypha_job_attempt(job_id, id);

-- ─── Streaming, resumable snapshots, immutable output revisions ──────

CREATE TABLE public.apocrypha_job_chunk (
    id              bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    job_id          uuid        NOT NULL REFERENCES public.apocrypha_job(id) ON DELETE CASCADE,
    attempt_id      uuid        NOT NULL,
    seq             integer     NOT NULL,
    chunk_kind      text        NOT NULL,
    delta           text        NOT NULL,
    delta_hash      text        NOT NULL,
    metadata        jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_job_chunk_attempt_fk
        FOREIGN KEY (job_id, attempt_id)
        REFERENCES public.apocrypha_job_attempt(job_id, id) ON DELETE CASCADE,
    CONSTRAINT apocrypha_job_chunk_sequence_unique UNIQUE (attempt_id, seq),
    CONSTRAINT apocrypha_job_chunk_sequence_nonnegative CHECK (seq >= 0),
    CONSTRAINT apocrypha_job_chunk_kind_enum
        CHECK (chunk_kind IN ('token', 'progress', 'section', 'tool', 'status')),
    CONSTRAINT apocrypha_job_chunk_delta_size CHECK (octet_length(delta) <= 65536),
    CONSTRAINT apocrypha_job_chunk_delta_hash_shape CHECK (delta_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT apocrypha_job_chunk_metadata_size CHECK (octet_length(metadata::text) <= 65536)
);

CREATE INDEX apocrypha_job_chunk_job_sequence
    ON public.apocrypha_job_chunk (job_id, id);

CREATE TABLE public.apocrypha_job_snapshot (
    id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id          uuid        NOT NULL REFERENCES public.apocrypha_job(id) ON DELETE CASCADE,
    attempt_id      uuid        NOT NULL,
    snapshot_no     integer     NOT NULL,
    through_seq     integer     NOT NULL,
    body            text        NOT NULL,
    body_hash       text        NOT NULL,
    state           jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_job_snapshot_attempt_fk
        FOREIGN KEY (job_id, attempt_id)
        REFERENCES public.apocrypha_job_attempt(job_id, id) ON DELETE CASCADE,
    CONSTRAINT apocrypha_job_snapshot_number_unique UNIQUE (attempt_id, snapshot_no),
    CONSTRAINT apocrypha_job_snapshot_number_nonnegative CHECK (snapshot_no >= 0),
    CONSTRAINT apocrypha_job_snapshot_sequence_nonnegative CHECK (through_seq >= 0),
    CONSTRAINT apocrypha_job_snapshot_body_size CHECK (octet_length(body) <= 4194304),
    CONSTRAINT apocrypha_job_snapshot_body_hash_shape CHECK (body_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT apocrypha_job_snapshot_state_size CHECK (octet_length(state::text) <= 262144)
);

CREATE INDEX apocrypha_job_snapshot_job_latest
    ON public.apocrypha_job_snapshot (job_id, snapshot_no DESC);

CREATE TABLE public.apocrypha_job_revision (
    id                      uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id                  uuid        NOT NULL REFERENCES public.apocrypha_job(id) ON DELETE CASCADE,
    attempt_id              uuid        NOT NULL,
    revision_no             integer     NOT NULL,
    revision_role           text        NOT NULL,
    content                 text        NOT NULL,
    content_hash            text        NOT NULL,
    model_alias             text        NOT NULL,
    profile_hash            text        NOT NULL,
    tool_registry_version   text        NOT NULL,
    memory_manifest_hash    text        NOT NULL,
    provenance              jsonb       NOT NULL DEFAULT '{}'::jsonb,
    usage                   jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at              timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_job_revision_job_id_id_unique UNIQUE (job_id, id),
    CONSTRAINT apocrypha_job_revision_attempt_fk
        FOREIGN KEY (job_id, attempt_id)
        REFERENCES public.apocrypha_job_attempt(job_id, id),
    CONSTRAINT apocrypha_job_revision_number_unique UNIQUE (job_id, revision_no),
    CONSTRAINT apocrypha_job_revision_number_positive CHECK (revision_no > 0),
    CONSTRAINT apocrypha_job_revision_role_enum
        CHECK (revision_role IN ('primary', 'corroboration', 'synthesis')),
    CONSTRAINT apocrypha_job_revision_content_nonempty
        CHECK (char_length(content) > 0 AND octet_length(content) <= 8388608),
    CONSTRAINT apocrypha_job_revision_content_hash_shape CHECK (content_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT apocrypha_job_revision_profile_hash_shape CHECK (profile_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT apocrypha_job_revision_memory_manifest_hash_shape
        CHECK (memory_manifest_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT apocrypha_job_revision_provenance_size CHECK (octet_length(provenance::text) <= 524288),
    CONSTRAINT apocrypha_job_revision_usage_size CHECK (octet_length(usage::text) <= 65536)
);

CREATE INDEX apocrypha_job_revision_job_created
    ON public.apocrypha_job_revision (job_id, revision_no DESC);

ALTER TABLE public.apocrypha_job
    ADD CONSTRAINT apocrypha_job_terminal_revision_fk
    FOREIGN KEY (id, terminal_revision_id)
    REFERENCES public.apocrypha_job_revision(job_id, id);

-- ─── Chronological events, accounting, and non-recursive alerts ──────

CREATE TABLE public.apocrypha_job_event (
    id                  bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    job_id              uuid        NOT NULL REFERENCES public.apocrypha_job(id) ON DELETE CASCADE,
    attempt_id          uuid,
    ordinal             integer     NOT NULL,
    event_type          text        NOT NULL,
    outcome             text        NOT NULL,
    severity            text        NOT NULL,
    source              text        NOT NULL,
    flagged             boolean     NOT NULL DEFAULT false,
    alert_eligible      boolean     NOT NULL DEFAULT false,
    causal_event_id     bigint      REFERENCES public.apocrypha_job_event(id),
    metadata            jsonb       NOT NULL DEFAULT '{}'::jsonb,
    occurred_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_job_event_attempt_fk
        FOREIGN KEY (job_id, attempt_id)
        REFERENCES public.apocrypha_job_attempt(job_id, id),
    CONSTRAINT apocrypha_job_event_ordinal_unique UNIQUE (job_id, ordinal),
    CONSTRAINT apocrypha_job_event_ordinal_positive CHECK (ordinal > 0),
    CONSTRAINT apocrypha_job_event_type_shape
        CHECK (event_type ~ '^[a-z][a-z0-9._:-]{1,95}$'),
    CONSTRAINT apocrypha_job_event_outcome_enum
        CHECK (outcome IN (
            'expected_fired', 'expected_missed',
            'unexpected_fired', 'unexpected_absent', 'informational'
        )),
    CONSTRAINT apocrypha_job_event_severity_enum
        CHECK (severity IN ('debug', 'info', 'warning', 'error', 'critical')),
    CONSTRAINT apocrypha_job_event_source_shape
        CHECK (source ~ '^[a-z][a-z0-9._:-]{1,95}$'),
    CONSTRAINT apocrypha_job_event_metadata_size
        CHECK (octet_length(metadata::text) <= 262144)
);

CREATE INDEX apocrypha_job_event_job_ordinal
    ON public.apocrypha_job_event (job_id, ordinal);
CREATE INDEX apocrypha_job_event_alertable
    ON public.apocrypha_job_event (occurred_at)
    WHERE alert_eligible;

CREATE TABLE public.apocrypha_entitlement_ledger (
    id                          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id                   uuid        NOT NULL REFERENCES public.apocrypha_tenant(id) ON DELETE CASCADE,
    principal_id                uuid        NOT NULL,
    job_id                      uuid        REFERENCES public.apocrypha_job(id) ON DELETE SET NULL,
    account_key                 text        NOT NULL,
    unit                        text        NOT NULL,
    entry_kind                  text        NOT NULL,
    delta                       bigint      NOT NULL,
    balance_after               bigint      NOT NULL,
    provider_cost_microunits    bigint      NOT NULL DEFAULT 0,
    reference_key               text        NOT NULL,
    metadata                    jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at                  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_entitlement_principal_fk
        FOREIGN KEY (tenant_id, principal_id)
        REFERENCES public.apocrypha_principal(tenant_id, id),
    CONSTRAINT apocrypha_entitlement_reference_unique
        UNIQUE (tenant_id, principal_id, account_key, unit, reference_key),
    CONSTRAINT apocrypha_entitlement_account_key_shape
        CHECK (account_key ~ '^[a-z][a-z0-9._:-]{1,95}$'),
    CONSTRAINT apocrypha_entitlement_unit_shape
        CHECK (unit ~ '^[a-z][a-z0-9._:-]{1,63}$'),
    CONSTRAINT apocrypha_entitlement_entry_kind_enum
        CHECK (entry_kind IN ('grant', 'reserve', 'commit', 'release', 'debit', 'credit', 'refund', 'expire')),
    CONSTRAINT apocrypha_entitlement_delta_nonzero CHECK (delta <> 0),
    CONSTRAINT apocrypha_entitlement_provider_cost_nonnegative CHECK (provider_cost_microunits >= 0),
    CONSTRAINT apocrypha_entitlement_reference_key_length
        CHECK (char_length(reference_key) BETWEEN 1 AND 200),
    CONSTRAINT apocrypha_entitlement_metadata_size CHECK (octet_length(metadata::text) <= 131072)
);

CREATE INDEX apocrypha_entitlement_account_history
    ON public.apocrypha_entitlement_ledger
    (tenant_id, principal_id, account_key, unit, created_at, id);
CREATE INDEX apocrypha_entitlement_job_lookup
    ON public.apocrypha_entitlement_ledger (job_id)
    WHERE job_id IS NOT NULL;

CREATE TABLE public.apocrypha_alert_outbox (
    id                  uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           uuid        NOT NULL REFERENCES public.apocrypha_tenant(id) ON DELETE CASCADE,
    job_id              uuid        NOT NULL REFERENCES public.apocrypha_job(id) ON DELETE CASCADE,
    event_id            bigint      NOT NULL UNIQUE REFERENCES public.apocrypha_job_event(id) ON DELETE CASCADE,
    channel             text        NOT NULL DEFAULT 'ops',
    severity            text        NOT NULL,
    dedupe_key          text        NOT NULL UNIQUE,
    payload             jsonb       NOT NULL,
    status              text        NOT NULL DEFAULT 'pending',
    attempt_count       integer     NOT NULL DEFAULT 0,
    available_at        timestamptz NOT NULL DEFAULT now(),
    locked_by           text,
    locked_until        timestamptz,
    delivered_at        timestamptz,
    last_error          text,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_alert_outbox_channel_shape
        CHECK (channel ~ '^[a-z][a-z0-9._:-]{1,63}$'),
    CONSTRAINT apocrypha_alert_outbox_severity_enum
        CHECK (severity IN ('warning', 'error', 'critical')),
    CONSTRAINT apocrypha_alert_outbox_dedupe_key_length
        CHECK (char_length(dedupe_key) BETWEEN 1 AND 240),
    CONSTRAINT apocrypha_alert_outbox_payload_size
        CHECK (octet_length(payload::text) <= 131072),
    CONSTRAINT apocrypha_alert_outbox_status_enum
        CHECK (status IN ('pending', 'delivering', 'delivered', 'dead_letter')),
    CONSTRAINT apocrypha_alert_outbox_attempt_count_nonnegative CHECK (attempt_count >= 0),
    CONSTRAINT apocrypha_alert_outbox_lock_consistency
        CHECK (
            (status = 'delivering' AND locked_by IS NOT NULL AND locked_until IS NOT NULL)
            OR (status <> 'delivering')
        ),
    CONSTRAINT apocrypha_alert_outbox_delivery_consistency
        CHECK (
            (status = 'delivered' AND delivered_at IS NOT NULL)
            OR status <> 'delivered'
        ),
    CONSTRAINT apocrypha_alert_outbox_last_error_length
        CHECK (last_error IS NULL OR char_length(last_error) <= 4096)
);

CREATE INDEX apocrypha_alert_outbox_dispatch
    ON public.apocrypha_alert_outbox (available_at, created_at)
    WHERE status = 'pending';
CREATE INDEX apocrypha_alert_outbox_stale_lock
    ON public.apocrypha_alert_outbox (locked_until)
    WHERE status = 'delivering';

CREATE TABLE public.apocrypha_alert_delivery (
    id                  uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    outbox_id           uuid        NOT NULL REFERENCES public.apocrypha_alert_outbox(id) ON DELETE CASCADE,
    attempt_no          integer     NOT NULL,
    dispatcher          text        NOT NULL,
    outcome             text        NOT NULL,
    response_code       integer,
    response_detail     text,
    receipt             jsonb       NOT NULL DEFAULT '{}'::jsonb,
    attempted_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_alert_delivery_attempt_unique UNIQUE (outbox_id, attempt_no),
    CONSTRAINT apocrypha_alert_delivery_attempt_positive CHECK (attempt_no > 0),
    CONSTRAINT apocrypha_alert_delivery_dispatcher_length
        CHECK (char_length(dispatcher) BETWEEN 1 AND 120),
    CONSTRAINT apocrypha_alert_delivery_outcome_enum
        CHECK (outcome IN ('delivered', 'retry', 'dead_letter')),
    CONSTRAINT apocrypha_alert_delivery_response_code_range
        CHECK (response_code IS NULL OR response_code BETWEEN 100 AND 599),
    CONSTRAINT apocrypha_alert_delivery_response_detail_length
        CHECK (response_detail IS NULL OR char_length(response_detail) <= 4096),
    CONSTRAINT apocrypha_alert_delivery_receipt_size CHECK (octet_length(receipt::text) <= 131072)
);

CREATE INDEX apocrypha_alert_delivery_outbox_history
    ON public.apocrypha_alert_delivery (outbox_id, attempt_no);

COMMENT ON TABLE public.apocrypha_job IS
    'Durable Apocrypha work item. Request identity, model/tool/memory configuration, state, and terminal revision pointer.';
COMMENT ON TABLE public.apocrypha_job_attempt IS
    'One fenced lease per execution attempt. Stale lease epochs and tokens cannot mutate current work.';
COMMENT ON TABLE public.apocrypha_job_revision IS
    'Immutable result revision. Primary completion is terminal even when optional corroboration is unavailable.';
COMMENT ON TABLE public.apocrypha_job_event IS
    'Ordered lifecycle evidence including expected/missed/unexpected event outcomes.';
COMMENT ON TABLE public.apocrypha_alert_outbox IS
    'Retryable operational alerts. Alert-delivery failures stay in delivery history and never recursively enqueue alerts.';

-- ─── Integrity guards and shared helpers ─────────────────────────────

CREATE OR REPLACE FUNCTION public.apocrypha_touch_updated_at()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog, public, extensions
AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_reject_historical_update()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog, public, extensions
AS $$
BEGIN
    RAISE EXCEPTION '% rows are immutable; append a new row instead', TG_TABLE_NAME
        USING ERRCODE = '55000';
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_guard_job_update()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_allowed boolean := false;
BEGIN
    IF NEW.tenant_id IS DISTINCT FROM OLD.tenant_id
       OR NEW.owner_principal_id IS DISTINCT FROM OLD.owner_principal_id
       OR NEW.parent_job_id IS DISTINCT FROM OLD.parent_job_id
       OR NEW.kind IS DISTINCT FROM OLD.kind
       OR NEW.capability IS DISTINCT FROM OLD.capability
       OR NEW.job_role IS DISTINCT FROM OLD.job_role
       OR NEW.request IS DISTINCT FROM OLD.request
       OR NEW.request_hash IS DISTINCT FROM OLD.request_hash
       OR NEW.idempotency_scope IS DISTINCT FROM OLD.idempotency_scope
       OR NEW.idempotency_key IS DISTINCT FROM OLD.idempotency_key
       OR NEW.model_alias IS DISTINCT FROM OLD.model_alias
       OR NEW.profile_hash IS DISTINCT FROM OLD.profile_hash
       OR NEW.tool_registry_version IS DISTINCT FROM OLD.tool_registry_version
       OR NEW.memory_manifest_hash IS DISTINCT FROM OLD.memory_manifest_hash THEN
        RAISE EXCEPTION 'job identity and admitted execution configuration are immutable'
            USING ERRCODE = '55000';
    END IF;

    IF NEW.attempt_count < OLD.attempt_count OR NEW.lease_epoch < OLD.lease_epoch THEN
        RAISE EXCEPTION 'job attempt_count and lease_epoch cannot decrease'
            USING ERRCODE = '55000';
    END IF;

    IF NEW.status = OLD.status THEN
        v_allowed := true;
    ELSIF OLD.status = 'queued' AND NEW.status IN ('leased', 'cancelled') THEN
        v_allowed := true;
    ELSIF OLD.status = 'leased' AND NEW.status IN (
        'running', 'queued', 'cancel_requested', 'succeeded', 'failed', 'cancelled'
    ) THEN
        v_allowed := true;
    ELSIF OLD.status = 'running' AND NEW.status IN (
        'queued', 'cancel_requested', 'succeeded', 'failed', 'cancelled'
    ) THEN
        v_allowed := true;
    ELSIF OLD.status = 'cancel_requested' AND NEW.status IN ('cancelled', 'failed') THEN
        v_allowed := true;
    END IF;

    IF NOT v_allowed THEN
        RAISE EXCEPTION 'invalid Apocrypha job transition: % -> %', OLD.status, NEW.status
            USING ERRCODE = '23514';
    END IF;

    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_guard_attempt_update()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_allowed boolean := false;
BEGIN
    IF NEW.job_id IS DISTINCT FROM OLD.job_id
       OR NEW.worker_node_id IS DISTINCT FROM OLD.worker_node_id
       OR NEW.claim_key IS DISTINCT FROM OLD.claim_key
       OR NEW.attempt_no IS DISTINCT FROM OLD.attempt_no
       OR NEW.lease_epoch IS DISTINCT FROM OLD.lease_epoch
       OR NEW.lease_token_hash IS DISTINCT FROM OLD.lease_token_hash
       OR NEW.lease_token_ciphertext IS DISTINCT FROM OLD.lease_token_ciphertext
       OR NEW.leased_at IS DISTINCT FROM OLD.leased_at THEN
        RAISE EXCEPTION 'attempt identity and fence are immutable'
            USING ERRCODE = '55000';
    END IF;

    IF NEW.status = OLD.status THEN
        v_allowed := true;
    ELSIF OLD.status = 'leased' AND NEW.status IN (
        'running', 'succeeded', 'failed', 'abandoned', 'cancelled'
    ) THEN
        v_allowed := true;
    ELSIF OLD.status = 'running' AND NEW.status IN (
        'succeeded', 'failed', 'abandoned', 'cancelled'
    ) THEN
        v_allowed := true;
    END IF;

    IF NOT v_allowed THEN
        RAISE EXCEPTION 'invalid Apocrypha attempt transition: % -> %', OLD.status, NEW.status
            USING ERRCODE = '23514';
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER apocrypha_tenant_touch
    BEFORE UPDATE ON public.apocrypha_tenant
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_touch_updated_at();
CREATE TRIGGER apocrypha_principal_touch
    BEFORE UPDATE ON public.apocrypha_principal
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_touch_updated_at();
CREATE TRIGGER apocrypha_worker_node_touch
    BEFORE UPDATE ON public.apocrypha_worker_node
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_touch_updated_at();
CREATE TRIGGER apocrypha_job_guard
    BEFORE UPDATE ON public.apocrypha_job
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_guard_job_update();
CREATE TRIGGER apocrypha_job_attempt_guard
    BEFORE UPDATE ON public.apocrypha_job_attempt
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_guard_attempt_update();
CREATE TRIGGER apocrypha_alert_outbox_touch
    BEFORE UPDATE ON public.apocrypha_alert_outbox
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_touch_updated_at();

CREATE TRIGGER apocrypha_job_chunk_immutable
    BEFORE UPDATE ON public.apocrypha_job_chunk
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_reject_historical_update();
CREATE TRIGGER apocrypha_job_snapshot_immutable
    BEFORE UPDATE ON public.apocrypha_job_snapshot
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_reject_historical_update();
CREATE TRIGGER apocrypha_job_revision_immutable
    BEFORE UPDATE ON public.apocrypha_job_revision
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_reject_historical_update();
CREATE TRIGGER apocrypha_job_event_immutable
    BEFORE UPDATE ON public.apocrypha_job_event
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_reject_historical_update();
CREATE TRIGGER apocrypha_entitlement_ledger_immutable
    BEFORE UPDATE ON public.apocrypha_entitlement_ledger
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_reject_historical_update();
CREATE TRIGGER apocrypha_alert_delivery_immutable
    BEFORE UPDATE ON public.apocrypha_alert_delivery
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_reject_historical_update();

CREATE OR REPLACE FUNCTION public.apocrypha_sha256(p_value text)
RETURNS text
LANGUAGE sql
IMMUTABLE
STRICT
SET search_path = pg_catalog, public, extensions
AS $$
    SELECT encode(digest(convert_to(p_value, 'UTF8'), 'sha256'), 'hex');
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_event_is_alertable(
    p_event_type text,
    p_outcome text,
    p_severity text,
    p_source text,
    p_flagged boolean DEFAULT false
)
RETURNS boolean
LANGUAGE sql
IMMUTABLE
SET search_path = pg_catalog, public, extensions
AS $$
    SELECT CASE
        WHEN p_source LIKE 'alert.%' OR p_event_type LIKE 'alert.%' THEN false
        WHEN coalesce(p_flagged, false) THEN true
        WHEN p_severity IN ('error', 'critical') THEN true
        WHEN p_outcome IN ('expected_missed', 'unexpected_fired') THEN true
        ELSE false
    END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_record_job_event(
    p_job_id uuid,
    p_attempt_id uuid,
    p_event_type text,
    p_outcome text,
    p_severity text,
    p_source text,
    p_flagged boolean DEFAULT false,
    p_metadata jsonb DEFAULT '{}'::jsonb,
    p_causal_event_id bigint DEFAULT NULL
)
RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_job          public.apocrypha_job;
    v_event_id     bigint;
    v_ordinal      integer;
    v_alertable    boolean;
    v_alert_level  text;
    v_safe_meta    jsonb;
BEGIN
    SELECT * INTO v_job
    FROM public.apocrypha_job AS j
    WHERE j.id = p_job_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'Apocrypha job not found' USING ERRCODE = 'P0002';
    END IF;

    SELECT coalesce(max(e.ordinal), 0) + 1 INTO v_ordinal
    FROM public.apocrypha_job_event AS e
    WHERE e.job_id = p_job_id;

    v_alertable := public.apocrypha_event_is_alertable(
        p_event_type, p_outcome, p_severity, p_source, p_flagged
    );

    INSERT INTO public.apocrypha_job_event (
        job_id, attempt_id, ordinal, event_type, outcome, severity,
        source, flagged, alert_eligible, causal_event_id, metadata
    ) VALUES (
        p_job_id, p_attempt_id, v_ordinal, p_event_type, p_outcome, p_severity,
        p_source, coalesce(p_flagged, false), v_alertable, p_causal_event_id,
        coalesce(p_metadata, '{}'::jsonb)
    ) RETURNING id INTO v_event_id;

    IF v_alertable THEN
        v_alert_level := CASE
            WHEN p_severity IN ('error', 'critical') THEN p_severity
            ELSE 'warning'
        END;
        v_safe_meta := coalesce(p_metadata, '{}'::jsonb)
            - ARRAY[
                'authorization', 'token', 'node_token', 'lease_token',
                'request', 'prompt', 'content', 'secret'
            ];

        INSERT INTO public.apocrypha_alert_outbox (
            tenant_id, job_id, event_id, channel, severity, dedupe_key, payload
        ) VALUES (
            v_job.tenant_id,
            p_job_id,
            v_event_id,
            'ops',
            v_alert_level,
            p_job_id::text || ':' || p_event_type || ':' || v_ordinal::text,
            jsonb_build_object(
                'job_id', p_job_id,
                'attempt_id', p_attempt_id,
                'event_id', v_event_id,
                'event_type', p_event_type,
                'outcome', p_outcome,
                'severity', v_alert_level,
                'metadata', v_safe_meta
            )
        );
    END IF;

    RETURN v_event_id;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_require_worker(
    p_node_id uuid,
    p_node_token text,
    p_require_active boolean DEFAULT false
)
RETURNS public.apocrypha_worker_node
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_node public.apocrypha_worker_node;
BEGIN
    SELECT * INTO v_node
    FROM public.apocrypha_worker_node AS n
    WHERE n.id = p_node_id
    FOR UPDATE;

    IF NOT FOUND
       OR p_node_token IS NULL
       OR char_length(p_node_token) < 36
       OR public.apocrypha_sha256(p_node_token) <> v_node.token_hash THEN
        RAISE EXCEPTION 'worker authentication failed' USING ERRCODE = '28000';
    END IF;

    IF v_node.status = 'revoked' OR (p_require_active AND v_node.status <> 'active') THEN
        RAISE EXCEPTION 'worker is not admitted for this operation' USING ERRCODE = '28000';
    END IF;

    RETURN v_node;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_require_lease(
    p_node_id uuid,
    p_node_token text,
    p_job_id uuid,
    p_attempt_id uuid,
    p_lease_epoch bigint,
    p_lease_token text,
    p_allow_expired boolean DEFAULT false
)
RETURNS public.apocrypha_job_attempt
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_node      public.apocrypha_worker_node;
    v_attempt   public.apocrypha_job_attempt;
    v_job       public.apocrypha_job;
BEGIN
    v_node := public.apocrypha_require_worker(p_node_id, p_node_token, false);

    SELECT a.* INTO v_attempt
    FROM public.apocrypha_job_attempt AS a
    JOIN public.apocrypha_job AS j ON j.id = a.job_id
    WHERE a.id = p_attempt_id AND a.job_id = p_job_id
    FOR UPDATE OF a, j;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'stale or invalid worker lease fence' USING ERRCODE = '40001';
    END IF;

    SELECT * INTO v_job
    FROM public.apocrypha_job AS j
    WHERE j.id = p_job_id;

    IF v_attempt.worker_node_id <> p_node_id
       OR v_attempt.lease_epoch <> p_lease_epoch
       OR v_job.lease_epoch <> p_lease_epoch
       OR v_job.current_attempt_id IS DISTINCT FROM p_attempt_id
       OR p_lease_token IS NULL
       OR public.apocrypha_sha256(p_lease_token) <> v_attempt.lease_token_hash THEN
        RAISE EXCEPTION 'stale or invalid worker lease fence' USING ERRCODE = '40001';
    END IF;

    IF v_attempt.status NOT IN ('leased', 'running') THEN
        RAISE EXCEPTION 'attempt is no longer active' USING ERRCODE = '55000';
    END IF;

    IF NOT p_allow_expired AND v_attempt.lease_expires_at <= now() THEN
        RAISE EXCEPTION 'worker lease expired' USING ERRCODE = '57014';
    END IF;

    RETURN v_attempt;
END;
$$;

-- ─── Tenant/principal and worker administration ──────────────────────

CREATE OR REPLACE FUNCTION public.apocrypha_ensure_owner_principal(
    p_tenant_slug text,
    p_tenant_display_name text,
    p_auth_user_id uuid
)
RETURNS TABLE (tenant_id uuid, principal_id uuid)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_slug       text := lower(btrim(p_tenant_slug));
    v_tenant     public.apocrypha_tenant;
    v_principal  public.apocrypha_principal;
BEGIN
    IF p_auth_user_id IS NULL THEN
        RAISE EXCEPTION 'owner auth user is required' USING ERRCODE = '23502';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM auth.users AS u WHERE u.id = p_auth_user_id) THEN
        RAISE EXCEPTION 'owner auth user does not exist' USING ERRCODE = '23503';
    END IF;

    PERFORM pg_advisory_xact_lock(hashtextextended(v_slug, 0));

    INSERT INTO public.apocrypha_tenant (slug, display_name)
    VALUES (v_slug, btrim(p_tenant_display_name))
    ON CONFLICT (slug) DO NOTHING;

    SELECT * INTO v_tenant
    FROM public.apocrypha_tenant AS t
    WHERE t.slug = v_slug
    FOR UPDATE;

    IF v_tenant.status <> 'active' THEN
        RAISE EXCEPTION 'tenant is not active' USING ERRCODE = '55000';
    END IF;

    SELECT * INTO v_principal
    FROM public.apocrypha_principal AS p
    WHERE p.tenant_id = v_tenant.id AND p.principal_kind = 'owner'
    FOR UPDATE;

    IF FOUND THEN
        IF v_principal.auth_user_id IS DISTINCT FROM p_auth_user_id THEN
            RAISE EXCEPTION 'tenant already has a different owner; ownership cannot be reassigned implicitly'
                USING ERRCODE = '42501';
        END IF;
        IF v_principal.status <> 'active' THEN
            RAISE EXCEPTION 'owner principal is not active' USING ERRCODE = '55000';
        END IF;
    ELSE
        INSERT INTO public.apocrypha_principal (
            tenant_id, auth_user_id, principal_kind, display_name
        ) VALUES (
            v_tenant.id, p_auth_user_id, 'owner', btrim(p_tenant_display_name) || ' owner'
        ) RETURNING * INTO v_principal;
    END IF;

    RETURN QUERY SELECT v_tenant.id, v_principal.id;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_issue_worker_token(
    p_node_key text,
    p_display_name text,
    p_allowed_capabilities text[],
    p_tenant_id uuid DEFAULT NULL,
    p_max_concurrency smallint DEFAULT 1,
    p_model_profiles jsonb DEFAULT '{}'::jsonb,
    p_metadata jsonb DEFAULT '{}'::jsonb
)
RETURNS TABLE (node_id uuid, node_token text, token_version integer)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_node   public.apocrypha_worker_node;
    v_token  text;
BEGIN
    IF p_allowed_capabilities IS NULL OR cardinality(p_allowed_capabilities) = 0 THEN
        RAISE EXCEPTION 'at least one worker capability is required' USING ERRCODE = '23514';
    END IF;
    IF EXISTS (
        SELECT 1 FROM unnest(p_allowed_capabilities) AS c(value)
        WHERE c.value <> '*' AND c.value !~ '^[a-z][a-z0-9._:-]{1,95}$'
    ) THEN
        RAISE EXCEPTION 'invalid worker capability' USING ERRCODE = '23514';
    END IF;

    v_token := 'apn_' || encode(gen_random_bytes(32), 'hex');

    INSERT INTO public.apocrypha_worker_node (
        tenant_id, node_key, display_name, token_hash, token_last_four,
        allowed_capabilities, max_concurrency, model_profiles, metadata
    ) VALUES (
        p_tenant_id,
        lower(btrim(p_node_key)),
        btrim(p_display_name),
        public.apocrypha_sha256(v_token),
        right(v_token, 4),
        p_allowed_capabilities,
        p_max_concurrency,
        coalesce(p_model_profiles, '{}'::jsonb),
        coalesce(p_metadata, '{}'::jsonb)
    ) RETURNING * INTO v_node;

    RETURN QUERY SELECT v_node.id, v_token, v_node.token_version;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_rotate_worker_token(p_node_id uuid)
RETURNS TABLE (node_id uuid, node_token text, token_version integer)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_node   public.apocrypha_worker_node;
    v_token  text;
BEGIN
    v_token := 'apn_' || encode(gen_random_bytes(32), 'hex');

    UPDATE public.apocrypha_worker_node AS worker_node
    SET token_hash = public.apocrypha_sha256(v_token),
        token_last_four = right(v_token, 4),
        token_version = worker_node.token_version + 1,
        token_issued_at = now()
    WHERE id = p_node_id AND status <> 'revoked'
    RETURNING * INTO v_node;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'active or draining worker node not found' USING ERRCODE = 'P0002';
    END IF;

    RETURN QUERY SELECT v_node.id, v_token, v_node.token_version;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_revoke_worker_node(
    p_node_id uuid,
    p_reason text
)
RETURNS public.apocrypha_worker_node
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_node public.apocrypha_worker_node;
BEGIN
    IF char_length(btrim(coalesce(p_reason, ''))) NOT BETWEEN 1 AND 500 THEN
        RAISE EXCEPTION 'revoke reason must contain 1-500 characters' USING ERRCODE = '23514';
    END IF;

    UPDATE public.apocrypha_worker_node AS worker_node
    SET status = 'revoked',
        revoked_at = coalesce(revoked_at, now()),
        revoke_reason = btrim(p_reason),
        token_hash = public.apocrypha_sha256(
            'revoked:' || worker_node.id::text || ':' || worker_node.token_version::text || ':' || clock_timestamp()::text
        ),
        token_last_four = '0000',
        token_version = worker_node.token_version + 1
    WHERE id = p_node_id
    RETURNING * INTO v_node;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'worker node not found' USING ERRCODE = 'P0002';
    END IF;

    RETURN v_node;
END;
$$;

-- ─── Job lifecycle RPCs ──────────────────────────────────────────────

CREATE OR REPLACE FUNCTION public.apocrypha_enqueue_job(
    p_tenant_id uuid,
    p_owner_principal_id uuid,
    p_kind text,
    p_capability text,
    p_request jsonb,
    p_request_hash text,
    p_idempotency_scope text,
    p_idempotency_key text,
    p_model_alias text,
    p_profile_hash text,
    p_tool_registry_version text,
    p_memory_manifest_hash text,
    p_priority smallint DEFAULT 0,
    p_max_attempts smallint DEFAULT 3,
    p_available_at timestamptz DEFAULT now(),
    p_parent_job_id uuid DEFAULT NULL,
    p_job_role text DEFAULT 'primary'
)
RETURNS public.apocrypha_job
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_job         public.apocrypha_job;
    v_principal   public.apocrypha_principal;
    v_inserted    boolean := false;
BEGIN
    SELECT p.* INTO v_principal
    FROM public.apocrypha_principal AS p
    JOIN public.apocrypha_tenant AS t ON t.id = p.tenant_id
    WHERE p.id = p_owner_principal_id
      AND p.tenant_id = p_tenant_id
      AND p.status = 'active'
      AND t.status = 'active'
    FOR UPDATE OF p;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'active tenant/principal scope not found' USING ERRCODE = '42501';
    END IF;

    IF p_parent_job_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM public.apocrypha_job AS parent
        WHERE parent.id = p_parent_job_id
          AND parent.tenant_id = p_tenant_id
          AND parent.owner_principal_id = p_owner_principal_id
    ) THEN
        RAISE EXCEPTION 'parent job must belong to the same tenant and principal'
            USING ERRCODE = '23503';
    END IF;

    INSERT INTO public.apocrypha_job (
        tenant_id, owner_principal_id, parent_job_id, kind, capability,
        job_role, request, request_hash, idempotency_scope, idempotency_key,
        priority, max_attempts, model_alias, profile_hash,
        tool_registry_version, memory_manifest_hash, available_at
    ) VALUES (
        p_tenant_id, p_owner_principal_id, p_parent_job_id,
        lower(btrim(p_kind)), lower(btrim(p_capability)), p_job_role,
        p_request, lower(p_request_hash), p_idempotency_scope, p_idempotency_key,
        p_priority, p_max_attempts, p_model_alias, lower(p_profile_hash),
        p_tool_registry_version, lower(p_memory_manifest_hash),
        greatest(coalesce(p_available_at, now()), now())
    )
    ON CONFLICT (tenant_id, owner_principal_id, kind, idempotency_scope, idempotency_key)
    DO NOTHING
    RETURNING * INTO v_job;

    IF FOUND THEN
        v_inserted := true;
    ELSE
        SELECT * INTO v_job
        FROM public.apocrypha_job AS j
        WHERE j.tenant_id = p_tenant_id
          AND j.owner_principal_id = p_owner_principal_id
          AND j.kind = lower(btrim(p_kind))
          AND j.idempotency_scope = p_idempotency_scope
          AND j.idempotency_key = p_idempotency_key
        FOR UPDATE;

        IF v_job.request_hash <> lower(p_request_hash) THEN
            RAISE EXCEPTION 'idempotency key is already bound to a different request hash'
                USING ERRCODE = '23505';
        END IF;
    END IF;

    IF v_inserted THEN
        PERFORM public.apocrypha_record_job_event(
            v_job.id, NULL, 'job.enqueued', 'expected_fired', 'info',
            'control_plane.enqueue', false,
            jsonb_build_object(
                'kind', v_job.kind,
                'capability', v_job.capability,
                'job_role', v_job.job_role,
                'parent_job_id', v_job.parent_job_id
            )
        );
    END IF;

    RETURN v_job;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_claim_job(
    p_node_id uuid,
    p_node_token text,
    p_claim_key text,
    p_lease_seconds integer DEFAULT 180
)
RETURNS TABLE (
    job_id uuid,
    attempt_id uuid,
    attempt_no integer,
    lease_epoch bigint,
    lease_token text,
    lease_expires_at timestamptz,
    tenant_id uuid,
    owner_principal_id uuid,
    kind text,
    capability text,
    request jsonb,
    model_alias text,
    profile_hash text,
    tool_registry_version text,
    memory_manifest_hash text
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_node          public.apocrypha_worker_node;
    v_job           public.apocrypha_job;
    v_attempt       public.apocrypha_job_attempt;
    v_lease_token   text;
    v_lease_seconds integer;
    v_active_count  integer;
BEGIN
    v_node := public.apocrypha_require_worker(p_node_id, p_node_token, true);
    v_lease_seconds := least(900, greatest(30, coalesce(p_lease_seconds, 180)));

    IF char_length(btrim(coalesce(p_claim_key, ''))) NOT BETWEEN 8 AND 200 THEN
        RAISE EXCEPTION 'claim idempotency key must contain 8-200 characters'
            USING ERRCODE = '23514';
    END IF;

    -- Replay an ambiguously acknowledged claim before enforcing concurrency.
    -- The lease token is encrypted with the already-validated high-entropy node
    -- token; plaintext is never stored. Reusing the key can never lease a new
    -- job while the original attempt record exists.
    SELECT a.* INTO v_attempt
    FROM public.apocrypha_job_attempt AS a
    WHERE a.worker_node_id = p_node_id
      AND a.claim_key = btrim(p_claim_key);

    IF FOUND THEN
        SELECT * INTO v_job
        FROM public.apocrypha_job AS j
        WHERE j.id = v_attempt.job_id;

        IF NOT FOUND THEN
            RAISE EXCEPTION 'claim idempotency record lost its job'
                USING ERRCODE = '55000';
        END IF;

        BEGIN
            v_lease_token := pgp_sym_decrypt(v_attempt.lease_token_ciphertext, p_node_token);
        EXCEPTION WHEN OTHERS THEN
            RAISE EXCEPTION 'claim replay integrity check failed' USING ERRCODE = '28000';
        END;

        UPDATE public.apocrypha_worker_node SET last_seen_at = now() WHERE id = p_node_id;

        RETURN QUERY SELECT
            v_job.id,
            v_attempt.id,
            v_attempt.attempt_no,
            v_attempt.lease_epoch,
            v_lease_token,
            v_attempt.lease_expires_at,
            v_job.tenant_id,
            v_job.owner_principal_id,
            v_job.kind,
            v_job.capability,
            v_job.request,
            v_job.model_alias,
            v_job.profile_hash,
            v_job.tool_registry_version,
            v_job.memory_manifest_hash;
        RETURN;
    END IF;

    SELECT count(*) INTO v_active_count
    FROM public.apocrypha_job_attempt AS a
    WHERE a.worker_node_id = p_node_id
      AND a.status IN ('leased', 'running')
      AND a.lease_expires_at > now();

    UPDATE public.apocrypha_worker_node
    SET last_seen_at = now()
    WHERE id = p_node_id;

    IF v_active_count >= v_node.max_concurrency THEN
        RETURN;
    END IF;

    SELECT j.* INTO v_job
    FROM public.apocrypha_job AS j
    WHERE j.status = 'queued'
      AND j.available_at <= now()
      AND j.attempt_count < j.max_attempts
      AND (v_node.tenant_id IS NULL OR v_node.tenant_id = j.tenant_id)
      AND (
          '*' = ANY(v_node.allowed_capabilities)
          OR j.capability = ANY(v_node.allowed_capabilities)
      )
      AND NOT EXISTS (
          SELECT 1
          FROM public.apocrypha_job AS earlier
          WHERE earlier.owner_principal_id = j.owner_principal_id
            AND earlier.status = 'queued'
            AND earlier.available_at <= now()
            AND earlier.attempt_count < earlier.max_attempts
            AND (
                earlier.priority > j.priority
                OR (
                    earlier.priority = j.priority
                    AND (earlier.created_at, earlier.id) < (j.created_at, j.id)
                )
            )
      )
    ORDER BY j.priority DESC, j.available_at, j.created_at, j.id
    FOR UPDATE OF j SKIP LOCKED
    LIMIT 1;

    IF NOT FOUND THEN
        RETURN;
    END IF;

    v_lease_token := 'apl_' || encode(gen_random_bytes(32), 'hex');

    UPDATE public.apocrypha_job AS j
    SET status = 'leased',
        lease_epoch = j.lease_epoch + 1,
        attempt_count = j.attempt_count + 1,
        error_code = NULL,
        error_detail = NULL
    WHERE j.id = v_job.id
    RETURNING * INTO v_job;

    INSERT INTO public.apocrypha_job_attempt (
        job_id, worker_node_id, claim_key, attempt_no, lease_epoch,
        lease_token_hash, lease_token_ciphertext, status, leased_at,
        lease_expires_at, last_heartbeat_at
    ) VALUES (
        v_job.id,
        p_node_id,
        btrim(p_claim_key),
        v_job.attempt_count,
        v_job.lease_epoch,
        public.apocrypha_sha256(v_lease_token),
        pgp_sym_encrypt(
            v_lease_token,
            p_node_token,
            'cipher-algo=aes256,compress-algo=0'
        ),
        'leased',
        now(),
        now() + make_interval(secs => v_lease_seconds),
        now()
    ) RETURNING * INTO v_attempt;

    UPDATE public.apocrypha_job
    SET current_attempt_id = v_attempt.id
    WHERE id = v_job.id
    RETURNING * INTO v_job;

    PERFORM public.apocrypha_record_job_event(
        v_job.id, v_attempt.id, 'job.claimed', 'expected_fired', 'info',
        'control_plane.claim', false,
        jsonb_build_object(
            'worker_node_id', p_node_id,
            'attempt_no', v_attempt.attempt_no,
            'lease_epoch', v_attempt.lease_epoch,
            'lease_expires_at', v_attempt.lease_expires_at
        )
    );

    RETURN QUERY SELECT
        v_job.id,
        v_attempt.id,
        v_attempt.attempt_no,
        v_attempt.lease_epoch,
        v_lease_token,
        v_attempt.lease_expires_at,
        v_job.tenant_id,
        v_job.owner_principal_id,
        v_job.kind,
        v_job.capability,
        v_job.request,
        v_job.model_alias,
        v_job.profile_hash,
        v_job.tool_registry_version,
        v_job.memory_manifest_hash;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_renew_lease(
    p_node_id uuid,
    p_node_token text,
    p_job_id uuid,
    p_attempt_id uuid,
    p_lease_epoch bigint,
    p_lease_token text,
    p_lease_seconds integer DEFAULT 180
)
RETURNS TABLE (lease_expires_at timestamptz, cancel_requested boolean)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_attempt        public.apocrypha_job_attempt;
    v_job_status     text;
    v_new_expiry     timestamptz;
    v_lease_seconds  integer;
BEGIN
    v_attempt := public.apocrypha_require_lease(
        p_node_id, p_node_token, p_job_id, p_attempt_id,
        p_lease_epoch, p_lease_token, false
    );

    SELECT status INTO v_job_status
    FROM public.apocrypha_job
    WHERE id = p_job_id;

    UPDATE public.apocrypha_worker_node SET last_seen_at = now() WHERE id = p_node_id;

    -- Do not extend cancellation indefinitely. Return the cancellation flag so
    -- a long-running worker can stop and acknowledge it promptly.
    IF v_job_status = 'cancel_requested' THEN
        RETURN QUERY SELECT v_attempt.lease_expires_at, true;
        RETURN;
    END IF;
    IF v_job_status NOT IN ('leased', 'running') THEN
        RAISE EXCEPTION 'job is no longer executable' USING ERRCODE = '55000';
    END IF;

    v_lease_seconds := least(900, greatest(30, coalesce(p_lease_seconds, 180)));
    v_new_expiry := now() + make_interval(secs => v_lease_seconds);

    UPDATE public.apocrypha_job_attempt
    SET status = 'running',
        started_at = coalesce(started_at, now()),
        last_heartbeat_at = now(),
        lease_expires_at = v_new_expiry
    WHERE id = p_attempt_id;

    UPDATE public.apocrypha_job
    SET status = 'running'
    WHERE id = p_job_id AND status = 'leased';

    RETURN QUERY SELECT v_new_expiry, false;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_append_chunk(
    p_node_id uuid,
    p_node_token text,
    p_job_id uuid,
    p_attempt_id uuid,
    p_lease_epoch bigint,
    p_lease_token text,
    p_seq integer,
    p_chunk_kind text,
    p_delta text,
    p_metadata jsonb DEFAULT '{}'::jsonb,
    p_snapshot_no integer DEFAULT NULL,
    p_snapshot_body text DEFAULT NULL,
    p_snapshot_state jsonb DEFAULT NULL
)
RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_attempt          public.apocrypha_job_attempt;
    v_job_status       text;
    v_chunk_id         bigint;
    v_existing_kind    text;
    v_existing_hash    text;
    v_delta_hash       text;
    v_snapshot_hash    text;
    v_existing_snap    text;
    v_first_chunk      boolean;
BEGIN
    IF p_seq < 0 THEN
        RAISE EXCEPTION 'chunk sequence must be nonnegative' USING ERRCODE = '23514';
    END IF;
    IF (p_snapshot_no IS NULL) <> (p_snapshot_body IS NULL) THEN
        RAISE EXCEPTION 'snapshot number and body must be supplied together'
            USING ERRCODE = '23514';
    END IF;

    v_attempt := public.apocrypha_require_lease(
        p_node_id, p_node_token, p_job_id, p_attempt_id,
        p_lease_epoch, p_lease_token, false
    );

    SELECT status INTO v_job_status
    FROM public.apocrypha_job
    WHERE id = p_job_id;
    IF v_job_status = 'cancel_requested' THEN
        RAISE EXCEPTION 'job cancellation requested' USING ERRCODE = '57014';
    END IF;
    IF v_job_status NOT IN ('leased', 'running') THEN
        RAISE EXCEPTION 'job is no longer executable' USING ERRCODE = '55000';
    END IF;

    v_delta_hash := public.apocrypha_sha256(coalesce(p_delta, ''));
    v_first_chunk := NOT EXISTS (
        SELECT 1 FROM public.apocrypha_job_chunk AS c WHERE c.attempt_id = p_attempt_id
    );

    SELECT c.id, c.chunk_kind, c.delta_hash
    INTO v_chunk_id, v_existing_kind, v_existing_hash
    FROM public.apocrypha_job_chunk AS c
    WHERE c.attempt_id = p_attempt_id AND c.seq = p_seq;

    IF FOUND THEN
        IF v_existing_kind <> p_chunk_kind OR v_existing_hash <> v_delta_hash THEN
            RAISE EXCEPTION 'chunk sequence is already bound to different content'
                USING ERRCODE = '23505';
        END IF;
    ELSE
        INSERT INTO public.apocrypha_job_chunk (
            job_id, attempt_id, seq, chunk_kind, delta, delta_hash, metadata
        ) VALUES (
            p_job_id, p_attempt_id, p_seq, p_chunk_kind,
            coalesce(p_delta, ''), v_delta_hash, coalesce(p_metadata, '{}'::jsonb)
        ) RETURNING id INTO v_chunk_id;
    END IF;

    IF p_snapshot_no IS NOT NULL THEN
        v_snapshot_hash := public.apocrypha_sha256(p_snapshot_body);

        SELECT s.body_hash INTO v_existing_snap
        FROM public.apocrypha_job_snapshot AS s
        WHERE s.attempt_id = p_attempt_id AND s.snapshot_no = p_snapshot_no;

        IF FOUND THEN
            IF v_existing_snap <> v_snapshot_hash THEN
                RAISE EXCEPTION 'snapshot number is already bound to different content'
                    USING ERRCODE = '23505';
            END IF;
        ELSE
            INSERT INTO public.apocrypha_job_snapshot (
                job_id, attempt_id, snapshot_no, through_seq, body,
                body_hash, state
            ) VALUES (
                p_job_id, p_attempt_id, p_snapshot_no, p_seq, p_snapshot_body,
                v_snapshot_hash, coalesce(p_snapshot_state, '{}'::jsonb)
            );
        END IF;
    END IF;

    UPDATE public.apocrypha_job_attempt
    SET status = 'running',
        started_at = coalesce(started_at, now()),
        last_heartbeat_at = now()
    WHERE id = p_attempt_id;

    UPDATE public.apocrypha_job
    SET status = 'running'
    WHERE id = p_job_id AND status = 'leased';

    UPDATE public.apocrypha_worker_node SET last_seen_at = now() WHERE id = p_node_id;

    IF v_first_chunk THEN
        PERFORM public.apocrypha_record_job_event(
            p_job_id, p_attempt_id, 'job.streaming_started',
            'expected_fired', 'info', 'worker.chunk', false,
            jsonb_build_object('first_seq', p_seq, 'chunk_kind', p_chunk_kind)
        );
    ELSIF p_snapshot_no IS NOT NULL THEN
        PERFORM public.apocrypha_record_job_event(
            p_job_id, p_attempt_id, 'job.snapshot_saved',
            'expected_fired', 'info', 'worker.chunk', false,
            jsonb_build_object('snapshot_no', p_snapshot_no, 'through_seq', p_seq)
        );
    END IF;

    RETURN v_chunk_id;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_complete_job(
    p_node_id uuid,
    p_node_token text,
    p_job_id uuid,
    p_attempt_id uuid,
    p_lease_epoch bigint,
    p_lease_token text,
    p_content text,
    p_revision_role text DEFAULT 'primary',
    p_provenance jsonb DEFAULT '{}'::jsonb,
    p_usage jsonb DEFAULT '{}'::jsonb
)
RETURNS public.apocrypha_job_revision
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_node         public.apocrypha_worker_node;
    v_attempt      public.apocrypha_job_attempt;
    v_job          public.apocrypha_job;
    v_revision     public.apocrypha_job_revision;
    v_content_hash text;
    v_revision_no  integer;
BEGIN
    v_node := public.apocrypha_require_worker(p_node_id, p_node_token, false);
    v_content_hash := public.apocrypha_sha256(p_content);

    -- Network-safe idempotency: a worker that lost the completion response may
    -- repeat the exact completion and receive the already-committed revision.
    SELECT r.* INTO v_revision
    FROM public.apocrypha_job_revision AS r
    JOIN public.apocrypha_job_attempt AS a ON a.id = r.attempt_id
    JOIN public.apocrypha_job AS j ON j.id = r.job_id
    WHERE r.job_id = p_job_id
      AND r.attempt_id = p_attempt_id
      AND r.content_hash = v_content_hash
      AND r.revision_role = p_revision_role
      AND a.worker_node_id = p_node_id
      AND j.terminal_revision_id = r.id;

    IF FOUND THEN
        RETURN v_revision;
    END IF;

    v_attempt := public.apocrypha_require_lease(
        p_node_id, p_node_token, p_job_id, p_attempt_id,
        p_lease_epoch, p_lease_token, false
    );

    SELECT * INTO v_job
    FROM public.apocrypha_job AS j
    WHERE j.id = p_job_id
    FOR UPDATE;

    IF v_job.status = 'cancel_requested' THEN
        RAISE EXCEPTION 'job cancellation requested' USING ERRCODE = '57014';
    END IF;
    IF v_job.status NOT IN ('leased', 'running') THEN
        RAISE EXCEPTION 'job is no longer executable' USING ERRCODE = '55000';
    END IF;

    SELECT coalesce(max(r.revision_no), 0) + 1 INTO v_revision_no
    FROM public.apocrypha_job_revision AS r
    WHERE r.job_id = p_job_id;

    INSERT INTO public.apocrypha_job_revision (
        job_id, attempt_id, revision_no, revision_role, content, content_hash,
        model_alias, profile_hash, tool_registry_version, memory_manifest_hash,
        provenance, usage
    ) VALUES (
        p_job_id, p_attempt_id, v_revision_no, p_revision_role,
        p_content, v_content_hash,
        v_job.model_alias, v_job.profile_hash, v_job.tool_registry_version,
        v_job.memory_manifest_hash,
        coalesce(p_provenance, '{}'::jsonb), coalesce(p_usage, '{}'::jsonb)
    ) RETURNING * INTO v_revision;

    UPDATE public.apocrypha_job_attempt
    SET status = 'succeeded',
        started_at = coalesce(started_at, now()),
        last_heartbeat_at = now(),
        finished_at = now(),
        metrics = metrics || jsonb_build_object('usage', coalesce(p_usage, '{}'::jsonb))
    WHERE id = p_attempt_id;

    UPDATE public.apocrypha_job
    SET status = 'succeeded',
        terminal_revision_id = v_revision.id,
        completed_at = now(),
        error_code = NULL,
        error_detail = NULL
    WHERE id = p_job_id;

    UPDATE public.apocrypha_worker_node SET last_seen_at = now() WHERE id = p_node_id;

    PERFORM public.apocrypha_record_job_event(
        p_job_id, p_attempt_id, 'job.completed', 'expected_fired', 'info',
        'worker.complete', false,
        jsonb_build_object(
            'revision_id', v_revision.id,
            'revision_no', v_revision.revision_no,
            'revision_role', v_revision.revision_role,
            'content_hash', v_revision.content_hash
        )
    );

    RETURN v_revision;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_fail_job(
    p_node_id uuid,
    p_node_token text,
    p_job_id uuid,
    p_attempt_id uuid,
    p_lease_epoch bigint,
    p_lease_token text,
    p_error_code text,
    p_error_detail text,
    p_retryable boolean DEFAULT true,
    p_metrics jsonb DEFAULT '{}'::jsonb
)
RETURNS public.apocrypha_job
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_node         public.apocrypha_worker_node;
    v_attempt      public.apocrypha_job_attempt;
    v_job          public.apocrypha_job;
    v_retry        boolean;
    v_cancel       boolean;
    v_backoff_sec  integer;
BEGIN
    v_node := public.apocrypha_require_worker(p_node_id, p_node_token, false);

    -- Network-safe idempotency for a terminal acknowledgement that was lost.
    SELECT a.* INTO v_attempt
    FROM public.apocrypha_job_attempt AS a
    JOIN public.apocrypha_job AS j ON j.id = a.job_id
    WHERE a.id = p_attempt_id
      AND a.job_id = p_job_id
      AND a.worker_node_id = p_node_id;

    IF FOUND THEN
        SELECT * INTO v_job
        FROM public.apocrypha_job AS j
        WHERE j.id = p_job_id;
        IF v_attempt.status IN ('failed', 'abandoned', 'cancelled') THEN
            RETURN v_job;
        END IF;
    END IF;

    v_attempt := public.apocrypha_require_lease(
        p_node_id, p_node_token, p_job_id, p_attempt_id,
        p_lease_epoch, p_lease_token, false
    );

    SELECT * INTO v_job
    FROM public.apocrypha_job AS j
    WHERE j.id = p_job_id
    FOR UPDATE;

    v_cancel := v_job.status = 'cancel_requested';
    v_retry := coalesce(p_retryable, false)
        AND NOT v_cancel
        AND v_job.attempt_count < v_job.max_attempts;
    v_backoff_sec := least(300, power(2, least(v_job.attempt_count, 8))::integer);

    UPDATE public.apocrypha_job_attempt
    SET status = CASE WHEN v_cancel THEN 'cancelled' ELSE 'failed' END,
        started_at = coalesce(started_at, now()),
        last_heartbeat_at = now(),
        finished_at = now(),
        failure_code = left(coalesce(nullif(btrim(p_error_code), ''), 'WORKER_FAILURE'), 120),
        failure_detail = left(coalesce(p_error_detail, ''), 4096),
        metrics = metrics || coalesce(p_metrics, '{}'::jsonb)
    WHERE id = p_attempt_id;

    IF v_cancel THEN
        UPDATE public.apocrypha_job
        SET status = 'cancelled',
            current_attempt_id = NULL,
            completed_at = now(),
            error_code = 'CANCELLED_BY_REQUEST',
            error_detail = left(coalesce(p_error_detail, 'Worker acknowledged cancellation'), 4096)
        WHERE id = p_job_id
        RETURNING * INTO v_job;
    ELSIF v_retry THEN
        UPDATE public.apocrypha_job
        SET status = 'queued',
            current_attempt_id = NULL,
            available_at = now() + make_interval(secs => v_backoff_sec),
            error_code = left(coalesce(nullif(btrim(p_error_code), ''), 'WORKER_FAILURE'), 120),
            error_detail = left(coalesce(p_error_detail, ''), 4096)
        WHERE id = p_job_id
        RETURNING * INTO v_job;
    ELSE
        UPDATE public.apocrypha_job
        SET status = 'failed',
            current_attempt_id = NULL,
            completed_at = now(),
            error_code = left(coalesce(nullif(btrim(p_error_code), ''), 'WORKER_FAILURE'), 120),
            error_detail = left(coalesce(p_error_detail, ''), 4096)
        WHERE id = p_job_id
        RETURNING * INTO v_job;
    END IF;

    UPDATE public.apocrypha_worker_node SET last_seen_at = now() WHERE id = p_node_id;

    PERFORM public.apocrypha_record_job_event(
        p_job_id, p_attempt_id, 'job.attempt_failed', 'unexpected_fired', 'error',
        'worker.fail', true,
        jsonb_build_object(
            'error_code', v_job.error_code,
            'retryable', v_retry,
            'cancelled', v_cancel,
            'attempt_no', v_attempt.attempt_no
        )
    );

    IF v_cancel THEN
        PERFORM public.apocrypha_record_job_event(
            p_job_id, p_attempt_id, 'job.cancelled', 'expected_fired', 'info',
            'worker.fail', false, jsonb_build_object('acknowledged_by_worker', true)
        );
    ELSIF v_retry THEN
        PERFORM public.apocrypha_record_job_event(
            p_job_id, p_attempt_id, 'job.retry_scheduled', 'expected_fired', 'warning',
            'control_plane.retry', false,
            jsonb_build_object('available_at', v_job.available_at, 'backoff_seconds', v_backoff_sec)
        );
    ELSE
        PERFORM public.apocrypha_record_job_event(
            p_job_id, p_attempt_id, 'job.failed', 'unexpected_fired', 'error',
            'control_plane.fail', true,
            jsonb_build_object('error_code', v_job.error_code, 'attempts', v_job.attempt_count)
        );
    END IF;

    RETURN v_job;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_cancel_job(
    p_job_id uuid,
    p_requested_by_principal_id uuid,
    p_reason text DEFAULT NULL
)
RETURNS public.apocrypha_job
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_job       public.apocrypha_job;
    v_requester public.apocrypha_principal;
    v_old_status text;
BEGIN
    SELECT * INTO v_job
    FROM public.apocrypha_job AS j
    WHERE j.id = p_job_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'Apocrypha job not found' USING ERRCODE = 'P0002';
    END IF;

    SELECT * INTO v_requester
    FROM public.apocrypha_principal AS p
    WHERE p.id = p_requested_by_principal_id
      AND p.tenant_id = v_job.tenant_id
      AND p.status = 'active';

    IF NOT FOUND OR NOT (
        v_requester.id = v_job.owner_principal_id
        OR v_requester.principal_kind = 'owner'
    ) THEN
        RAISE EXCEPTION 'principal may not cancel this job' USING ERRCODE = '42501';
    END IF;

    IF v_job.status IN ('succeeded', 'failed', 'cancelled', 'cancel_requested') THEN
        RETURN v_job;
    END IF;

    v_old_status := v_job.status;
    IF v_job.status = 'queued' THEN
        UPDATE public.apocrypha_job
        SET status = 'cancelled',
            completed_at = now(),
            cancel_requested_at = now(),
            error_code = 'CANCELLED_BY_REQUEST',
            error_detail = left(coalesce(p_reason, 'Cancelled before execution'), 4096)
        WHERE id = p_job_id
        RETURNING * INTO v_job;
    ELSE
        UPDATE public.apocrypha_job
        SET status = 'cancel_requested',
            cancel_requested_at = now(),
            error_code = 'CANCEL_REQUESTED',
            error_detail = left(coalesce(p_reason, 'Cancellation requested'), 4096)
        WHERE id = p_job_id
        RETURNING * INTO v_job;
    END IF;

    PERFORM public.apocrypha_record_job_event(
        p_job_id, v_job.current_attempt_id,
        CASE WHEN v_job.status = 'cancelled' THEN 'job.cancelled' ELSE 'job.cancel_requested' END,
        'expected_fired', 'info', 'control_plane.cancel', false,
        jsonb_build_object(
            'requested_by_principal_id', p_requested_by_principal_id,
            'previous_status', v_old_status,
            'reason', left(coalesce(p_reason, ''), 500)
        )
    );

    RETURN v_job;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_reap(p_limit integer DEFAULT 100)
RETURNS TABLE (job_id uuid, attempt_id uuid, requeued boolean)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_attempt      public.apocrypha_job_attempt;
    v_job          public.apocrypha_job;
    v_requeued     boolean;
    v_backoff_sec  integer;
BEGIN
    IF p_limit NOT BETWEEN 1 AND 1000 THEN
        RAISE EXCEPTION 'reaper limit must be between 1 and 1000' USING ERRCODE = '23514';
    END IF;

    FOR v_attempt IN
        SELECT a.*
        FROM public.apocrypha_job_attempt AS a
        JOIN public.apocrypha_job AS j
          ON j.id = a.job_id AND j.current_attempt_id = a.id
        WHERE a.status IN ('leased', 'running')
          AND a.lease_expires_at <= now()
          AND j.status IN ('leased', 'running', 'cancel_requested')
        ORDER BY a.lease_expires_at, a.id
        FOR UPDATE OF a SKIP LOCKED
        LIMIT p_limit
    LOOP
        SELECT * INTO v_job
        FROM public.apocrypha_job AS j
        WHERE j.id = v_attempt.job_id
          AND j.current_attempt_id = v_attempt.id
        FOR UPDATE;

        IF NOT FOUND THEN
            CONTINUE;
        END IF;

        UPDATE public.apocrypha_job_attempt
        SET status = 'abandoned',
            finished_at = now(),
            failure_code = 'LEASE_EXPIRED',
            failure_detail = 'Worker lease expired before terminal acknowledgement'
        WHERE id = v_attempt.id;

        v_requeued := v_job.status <> 'cancel_requested'
            AND v_job.attempt_count < v_job.max_attempts;
        v_backoff_sec := least(300, power(2, least(v_job.attempt_count, 8))::integer);

        IF v_job.status = 'cancel_requested' THEN
            UPDATE public.apocrypha_job
            SET status = 'cancelled',
                current_attempt_id = NULL,
                completed_at = now(),
                error_code = 'CANCELLED_AFTER_LEASE_EXPIRY',
                error_detail = 'Cancellation completed after worker lease expired'
            WHERE id = v_job.id
            RETURNING * INTO v_job;
        ELSIF v_requeued THEN
            UPDATE public.apocrypha_job
            SET status = 'queued',
                current_attempt_id = NULL,
                available_at = now() + make_interval(secs => v_backoff_sec),
                error_code = 'LEASE_EXPIRED',
                error_detail = 'Worker lease expired; retry scheduled'
            WHERE id = v_job.id
            RETURNING * INTO v_job;
        ELSE
            UPDATE public.apocrypha_job
            SET status = 'failed',
                current_attempt_id = NULL,
                completed_at = now(),
                error_code = 'LEASE_EXPIRED',
                error_detail = 'Worker lease expired and retry budget was exhausted'
            WHERE id = v_job.id
            RETURNING * INTO v_job;
        END IF;

        PERFORM public.apocrypha_record_job_event(
            v_job.id, v_attempt.id, 'job.lease_expired',
            'expected_missed', 'error', 'control_plane.reaper', true,
            jsonb_build_object(
                'worker_node_id', v_attempt.worker_node_id,
                'attempt_no', v_attempt.attempt_no,
                'lease_epoch', v_attempt.lease_epoch,
                'requeued', v_requeued,
                'result_status', v_job.status
            )
        );

        job_id := v_job.id;
        attempt_id := v_attempt.id;
        requeued := v_requeued;
        RETURN NEXT;
    END LOOP;
END;
$$;

-- ─── Immutable entitlement accounting ────────────────────────────────

CREATE OR REPLACE FUNCTION public.apocrypha_record_entitlement_entry(
    p_tenant_id uuid,
    p_principal_id uuid,
    p_job_id uuid,
    p_account_key text,
    p_unit text,
    p_entry_kind text,
    p_delta bigint,
    p_provider_cost_microunits bigint,
    p_reference_key text,
    p_metadata jsonb DEFAULT '{}'::jsonb
)
RETURNS public.apocrypha_entitlement_ledger
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_existing  public.apocrypha_entitlement_ledger;
    v_entry     public.apocrypha_entitlement_ledger;
    v_balance   bigint;
    v_lock_key  text;
BEGIN
    v_lock_key := p_tenant_id::text || ':' || p_principal_id::text || ':'
        || p_account_key || ':' || p_unit;
    PERFORM pg_advisory_xact_lock(hashtextextended(v_lock_key, 0));

    SELECT * INTO v_existing
    FROM public.apocrypha_entitlement_ledger AS l
    WHERE l.tenant_id = p_tenant_id
      AND l.principal_id = p_principal_id
      AND l.account_key = p_account_key
      AND l.unit = p_unit
      AND l.reference_key = p_reference_key;

    IF FOUND THEN
        IF v_existing.delta <> p_delta
           OR v_existing.entry_kind <> p_entry_kind
           OR v_existing.provider_cost_microunits <> coalesce(p_provider_cost_microunits, 0)
           OR v_existing.job_id IS DISTINCT FROM p_job_id THEN
            RAISE EXCEPTION 'entitlement reference is already bound to a different entry'
                USING ERRCODE = '23505';
        END IF;
        RETURN v_existing;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM public.apocrypha_principal AS p
        WHERE p.id = p_principal_id AND p.tenant_id = p_tenant_id
    ) THEN
        RAISE EXCEPTION 'entitlement principal scope not found' USING ERRCODE = '23503';
    END IF;
    IF p_job_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM public.apocrypha_job AS j
        WHERE j.id = p_job_id
          AND j.tenant_id = p_tenant_id
          AND j.owner_principal_id = p_principal_id
    ) THEN
        RAISE EXCEPTION 'entitlement job must belong to the same principal scope'
            USING ERRCODE = '23503';
    END IF;

    SELECT coalesce(sum(l.delta), 0) + p_delta INTO v_balance
    FROM public.apocrypha_entitlement_ledger AS l
    WHERE l.tenant_id = p_tenant_id
      AND l.principal_id = p_principal_id
      AND l.account_key = p_account_key
      AND l.unit = p_unit;

    INSERT INTO public.apocrypha_entitlement_ledger (
        tenant_id, principal_id, job_id, account_key, unit, entry_kind,
        delta, balance_after, provider_cost_microunits, reference_key, metadata
    ) VALUES (
        p_tenant_id, p_principal_id, p_job_id, p_account_key, p_unit,
        p_entry_kind, p_delta, v_balance, coalesce(p_provider_cost_microunits, 0),
        p_reference_key, coalesce(p_metadata, '{}'::jsonb)
    ) RETURNING * INTO v_entry;

    RETURN v_entry;
END;
$$;

-- ─── Alert dispatch without recursive alert generation ───────────────

CREATE OR REPLACE FUNCTION public.apocrypha_claim_alerts(
    p_dispatcher text,
    p_limit integer DEFAULT 20,
    p_lease_seconds integer DEFAULT 60
)
RETURNS TABLE (
    outbox_id uuid,
    job_id uuid,
    event_id bigint,
    channel text,
    severity text,
    payload jsonb,
    attempt_no integer,
    locked_until timestamptz
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_limit integer;
    v_lease integer;
BEGIN
    IF char_length(btrim(coalesce(p_dispatcher, ''))) NOT BETWEEN 1 AND 120 THEN
        RAISE EXCEPTION 'dispatcher must contain 1-120 characters' USING ERRCODE = '23514';
    END IF;
    v_limit := least(100, greatest(1, coalesce(p_limit, 20)));
    v_lease := least(600, greatest(15, coalesce(p_lease_seconds, 60)));

    -- A dispatcher that vanished after claiming an alert is an expected event
    -- that failed to fire. Preserve that attempt in delivery history before
    -- making it available again; do not create an alert about the alert.
    INSERT INTO public.apocrypha_alert_delivery (
        outbox_id, attempt_no, dispatcher, outcome, response_detail
    )
    SELECT
        o.id,
        o.attempt_count,
        o.locked_by,
        CASE WHEN o.attempt_count < 8 THEN 'retry' ELSE 'dead_letter' END,
        'Dispatcher lease expired without a delivery receipt'
    FROM public.apocrypha_alert_outbox AS o
    WHERE o.status = 'delivering' AND o.locked_until <= now()
    ON CONFLICT ON CONSTRAINT apocrypha_alert_delivery_attempt_unique DO NOTHING;

    UPDATE public.apocrypha_alert_outbox AS o
    SET status = 'pending',
        locked_by = NULL,
        locked_until = NULL,
        available_at = now(),
        last_error = 'Dispatcher lease expired without a delivery receipt'
    WHERE o.status = 'delivering'
      AND o.locked_until <= now()
      AND o.attempt_count < 8;

    UPDATE public.apocrypha_alert_outbox AS o
    SET status = 'dead_letter',
        locked_by = NULL,
        locked_until = NULL,
        last_error = 'Dispatcher lease expired and retry budget was exhausted'
    WHERE o.status = 'delivering'
      AND o.locked_until <= now()
      AND o.attempt_count >= 8;

    RETURN QUERY
    WITH picked AS (
        SELECT o.id
        FROM public.apocrypha_alert_outbox AS o
        WHERE o.status = 'pending' AND o.available_at <= now()
        ORDER BY
            CASE o.severity WHEN 'critical' THEN 0 WHEN 'error' THEN 1 ELSE 2 END,
            o.available_at,
            o.created_at
        FOR UPDATE OF o SKIP LOCKED
        LIMIT v_limit
    ), claimed AS (
        UPDATE public.apocrypha_alert_outbox AS o
        SET status = 'delivering',
            attempt_count = o.attempt_count + 1,
            locked_by = btrim(p_dispatcher),
            locked_until = now() + make_interval(secs => v_lease)
        FROM picked
        WHERE o.id = picked.id
        RETURNING o.*
    )
    SELECT
        c.id, c.job_id, c.event_id, c.channel, c.severity, c.payload,
        c.attempt_count, c.locked_until
    FROM claimed AS c;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_record_alert_delivery(
    p_outbox_id uuid,
    p_dispatcher text,
    p_delivered boolean,
    p_retryable boolean,
    p_response_code integer DEFAULT NULL,
    p_response_detail text DEFAULT NULL,
    p_receipt jsonb DEFAULT '{}'::jsonb
)
RETURNS public.apocrypha_alert_outbox
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_outbox      public.apocrypha_alert_outbox;
    v_outcome     text;
    v_backoff_sec integer;
BEGIN
    SELECT * INTO v_outbox
    FROM public.apocrypha_alert_outbox AS o
    WHERE o.id = p_outbox_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'alert outbox row not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_outbox.status = 'delivered' THEN
        RETURN v_outbox;
    END IF;
    IF v_outbox.status <> 'delivering'
       OR v_outbox.locked_by IS DISTINCT FROM btrim(p_dispatcher)
       OR v_outbox.locked_until <= now() THEN
        RAISE EXCEPTION 'alert dispatcher lease is stale or invalid' USING ERRCODE = '40001';
    END IF;

    v_outcome := CASE
        WHEN p_delivered THEN 'delivered'
        WHEN coalesce(p_retryable, false) AND v_outbox.attempt_count < 8 THEN 'retry'
        ELSE 'dead_letter'
    END;

    INSERT INTO public.apocrypha_alert_delivery (
        outbox_id, attempt_no, dispatcher, outcome, response_code,
        response_detail, receipt
    ) VALUES (
        p_outbox_id, v_outbox.attempt_count, btrim(p_dispatcher), v_outcome,
        p_response_code, left(coalesce(p_response_detail, ''), 4096),
        coalesce(p_receipt, '{}'::jsonb)
    )
    ON CONFLICT (outbox_id, attempt_no) DO NOTHING;

    IF v_outcome = 'delivered' THEN
        UPDATE public.apocrypha_alert_outbox
        SET status = 'delivered',
            delivered_at = now(),
            locked_by = NULL,
            locked_until = NULL,
            last_error = NULL
        WHERE id = p_outbox_id
        RETURNING * INTO v_outbox;
    ELSIF v_outcome = 'retry' THEN
        v_backoff_sec := least(3600, power(2, least(v_outbox.attempt_count, 12))::integer);
        UPDATE public.apocrypha_alert_outbox
        SET status = 'pending',
            available_at = now() + make_interval(secs => v_backoff_sec),
            locked_by = NULL,
            locked_until = NULL,
            last_error = left(coalesce(p_response_detail, 'delivery failed'), 4096)
        WHERE id = p_outbox_id
        RETURNING * INTO v_outbox;
    ELSE
        UPDATE public.apocrypha_alert_outbox
        SET status = 'dead_letter',
            locked_by = NULL,
            locked_until = NULL,
            last_error = left(coalesce(p_response_detail, 'delivery failed permanently'), 4096)
        WHERE id = p_outbox_id
        RETURNING * INTO v_outbox;
    END IF;

    -- Deliberately no apocrypha_record_job_event call here. Delivery failures
    -- are observable in apocrypha_alert_delivery without generating alerts
    -- about alerts.
    RETURN v_outbox;
END;
$$;

-- ─── Read authorization helpers and row-level security ───────────────

CREATE OR REPLACE FUNCTION public.apocrypha_can_read_tenant(p_tenant_id uuid)
RETURNS boolean
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
    SELECT auth.uid() IS NOT NULL AND EXISTS (
        SELECT 1
        FROM public.apocrypha_principal AS p
        WHERE p.tenant_id = p_tenant_id
          AND p.auth_user_id = auth.uid()
          AND p.status = 'active'
    );
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_can_read_scope(
    p_tenant_id uuid,
    p_owner_principal_id uuid
)
RETURNS boolean
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
    SELECT auth.uid() IS NOT NULL AND (
        EXISTS (
            SELECT 1
            FROM public.apocrypha_principal AS own
            WHERE own.id = p_owner_principal_id
              AND own.tenant_id = p_tenant_id
              AND own.auth_user_id = auth.uid()
              AND own.status = 'active'
        )
        OR EXISTS (
            SELECT 1
            FROM public.apocrypha_principal AS owner
            WHERE owner.tenant_id = p_tenant_id
              AND owner.principal_kind = 'owner'
              AND owner.auth_user_id = auth.uid()
              AND owner.status = 'active'
        )
    );
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_can_read_job(p_job_id uuid)
RETURNS boolean
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
    SELECT EXISTS (
        SELECT 1
        FROM public.apocrypha_job AS j
        WHERE j.id = p_job_id
          AND public.apocrypha_can_read_scope(j.tenant_id, j.owner_principal_id)
    );
$$;

ALTER TABLE public.apocrypha_tenant ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_principal ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_worker_node ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_job ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_job_attempt ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_job_chunk ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_job_snapshot ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_job_revision ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_job_event ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_entitlement_ledger ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_alert_outbox ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_alert_delivery ENABLE ROW LEVEL SECURITY;

CREATE POLICY apocrypha_tenant_read_member
    ON public.apocrypha_tenant FOR SELECT TO authenticated
    USING (public.apocrypha_can_read_tenant(id));

CREATE POLICY apocrypha_principal_read_self_or_owner
    ON public.apocrypha_principal FOR SELECT TO authenticated
    USING (public.apocrypha_can_read_scope(tenant_id, id));

CREATE POLICY apocrypha_job_read_self_or_tenant_owner
    ON public.apocrypha_job FOR SELECT TO authenticated
    USING (public.apocrypha_can_read_scope(tenant_id, owner_principal_id));

-- Attempt fences include credential hashes and remain server-only.

CREATE POLICY apocrypha_job_chunk_read_job_owner
    ON public.apocrypha_job_chunk FOR SELECT TO authenticated
    USING (public.apocrypha_can_read_job(job_id));

CREATE POLICY apocrypha_job_snapshot_read_job_owner
    ON public.apocrypha_job_snapshot FOR SELECT TO authenticated
    USING (public.apocrypha_can_read_job(job_id));

CREATE POLICY apocrypha_job_revision_read_job_owner
    ON public.apocrypha_job_revision FOR SELECT TO authenticated
    USING (public.apocrypha_can_read_job(job_id));

CREATE POLICY apocrypha_job_event_read_job_owner
    ON public.apocrypha_job_event FOR SELECT TO authenticated
    USING (public.apocrypha_can_read_job(job_id));

CREATE POLICY apocrypha_entitlement_read_account_owner
    ON public.apocrypha_entitlement_ledger FOR SELECT TO authenticated
    USING (public.apocrypha_can_read_scope(tenant_id, principal_id));

-- No authenticated policies exist for worker identities, attempts, alert
-- outbox, or delivery receipts. No authenticated table has a write policy.

-- ─── Least-privilege table grants ────────────────────────────────────

REVOKE ALL ON TABLE
    public.apocrypha_tenant,
    public.apocrypha_principal,
    public.apocrypha_worker_node,
    public.apocrypha_job,
    public.apocrypha_job_attempt,
    public.apocrypha_job_chunk,
    public.apocrypha_job_snapshot,
    public.apocrypha_job_revision,
    public.apocrypha_job_event,
    public.apocrypha_entitlement_ledger,
    public.apocrypha_alert_outbox,
    public.apocrypha_alert_delivery
FROM PUBLIC, anon, authenticated;

GRANT SELECT (id, slug, display_name, status, created_at, updated_at)
    ON public.apocrypha_tenant TO authenticated;
GRANT SELECT (id, tenant_id, auth_user_id, principal_kind, display_name, status, created_at, updated_at)
    ON public.apocrypha_principal TO authenticated;
GRANT SELECT ON public.apocrypha_job TO authenticated;
GRANT SELECT ON public.apocrypha_job_chunk TO authenticated;
GRANT SELECT ON public.apocrypha_job_snapshot TO authenticated;
GRANT SELECT ON public.apocrypha_job_revision TO authenticated;
GRANT SELECT ON public.apocrypha_job_event TO authenticated;
GRANT SELECT ON public.apocrypha_entitlement_ledger TO authenticated;

GRANT SELECT, INSERT, UPDATE, DELETE ON TABLE
    public.apocrypha_tenant,
    public.apocrypha_principal,
    public.apocrypha_worker_node,
    public.apocrypha_job,
    public.apocrypha_job_attempt,
    public.apocrypha_alert_outbox
TO service_role;

GRANT SELECT, INSERT, DELETE ON TABLE
    public.apocrypha_job_chunk,
    public.apocrypha_job_snapshot,
    public.apocrypha_job_revision,
    public.apocrypha_job_event,
    public.apocrypha_entitlement_ledger,
    public.apocrypha_alert_delivery
TO service_role;

GRANT USAGE, SELECT ON SEQUENCE
    public.apocrypha_job_chunk_id_seq,
    public.apocrypha_job_event_id_seq
TO service_role;

-- ─── Function execution boundaries ──────────────────────────────────

REVOKE EXECUTE ON FUNCTION public.apocrypha_touch_updated_at() FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_reject_historical_update() FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_guard_job_update() FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_guard_attempt_update() FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_sha256(text) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_event_is_alertable(text, text, text, text, boolean) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_record_job_event(uuid, uuid, text, text, text, text, boolean, jsonb, bigint) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_require_worker(uuid, text, boolean) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_require_lease(uuid, text, uuid, uuid, bigint, text, boolean) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_ensure_owner_principal(text, text, uuid) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_issue_worker_token(text, text, text[], uuid, smallint, jsonb, jsonb) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_rotate_worker_token(uuid) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_revoke_worker_node(uuid, text) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_enqueue_job(uuid, uuid, text, text, jsonb, text, text, text, text, text, text, text, smallint, smallint, timestamptz, uuid, text) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_claim_job(uuid, text, text, integer) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_renew_lease(uuid, text, uuid, uuid, bigint, text, integer) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_append_chunk(uuid, text, uuid, uuid, bigint, text, integer, text, text, jsonb, integer, text, jsonb) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_complete_job(uuid, text, uuid, uuid, bigint, text, text, text, jsonb, jsonb) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_fail_job(uuid, text, uuid, uuid, bigint, text, text, text, boolean, jsonb) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_cancel_job(uuid, uuid, text) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_reap(integer) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_record_entitlement_entry(uuid, uuid, uuid, text, text, text, bigint, bigint, text, jsonb) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_claim_alerts(text, integer, integer) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_record_alert_delivery(uuid, text, boolean, boolean, integer, text, jsonb) FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_can_read_tenant(uuid) FROM PUBLIC, anon;
REVOKE EXECUTE ON FUNCTION public.apocrypha_can_read_scope(uuid, uuid) FROM PUBLIC, anon;
REVOKE EXECUTE ON FUNCTION public.apocrypha_can_read_job(uuid) FROM PUBLIC, anon;

GRANT EXECUTE ON FUNCTION public.apocrypha_can_read_tenant(uuid) TO authenticated, service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_can_read_scope(uuid, uuid) TO authenticated, service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_can_read_job(uuid) TO authenticated, service_role;

GRANT EXECUTE ON FUNCTION public.apocrypha_ensure_owner_principal(text, text, uuid) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_issue_worker_token(text, text, text[], uuid, smallint, jsonb, jsonb) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_rotate_worker_token(uuid) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_revoke_worker_node(uuid, text) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_enqueue_job(uuid, uuid, text, text, jsonb, text, text, text, text, text, text, text, smallint, smallint, timestamptz, uuid, text) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_claim_job(uuid, text, text, integer) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_renew_lease(uuid, text, uuid, uuid, bigint, text, integer) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_append_chunk(uuid, text, uuid, uuid, bigint, text, integer, text, text, jsonb, integer, text, jsonb) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_complete_job(uuid, text, uuid, uuid, bigint, text, text, text, jsonb, jsonb) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_fail_job(uuid, text, uuid, uuid, bigint, text, text, text, boolean, jsonb) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_cancel_job(uuid, uuid, text) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_reap(integer) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_record_entitlement_entry(uuid, uuid, uuid, text, text, text, bigint, bigint, text, jsonb) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_claim_alerts(text, integer, integer) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_record_alert_delivery(uuid, text, boolean, boolean, integer, text, jsonb) TO service_role;

-- Internal helpers remain executable only by the migration owner/service role.
GRANT EXECUTE ON FUNCTION public.apocrypha_sha256(text) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_event_is_alertable(text, text, text, text, boolean) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_record_job_event(uuid, uuid, text, text, text, text, boolean, jsonb, bigint) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_require_worker(uuid, text, boolean) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_require_lease(uuid, text, uuid, uuid, bigint, text, boolean) TO service_role;

COMMENT ON FUNCTION public.apocrypha_claim_job(uuid, text, text, integer) IS
    'Atomically leases one fair-queue job with FOR UPDATE SKIP LOCKED. Same-node claim-key replay returns the exact encrypted-at-rest lease token after node authentication.';
COMMENT ON FUNCTION public.apocrypha_append_chunk(uuid, text, uuid, uuid, bigint, text, integer, text, text, jsonb, integer, text, jsonb) IS
    'Fenced, idempotent chunk append with optional immutable resumable snapshot.';
COMMENT ON FUNCTION public.apocrypha_complete_job(uuid, text, uuid, uuid, bigint, text, text, text, jsonb, jsonb) IS
    'Fenced terminal completion that publishes an immutable revision and remains idempotent after a lost acknowledgement.';
COMMENT ON FUNCTION public.apocrypha_reap(integer) IS
    'Abandons expired fenced attempts and atomically requeues, fails, or completes cancellation according to retry budget.';
