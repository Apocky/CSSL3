-- =====================================================================
-- § LAZARUS · autonomous LoA v14 coding runner control plane
-- =====================================================================
-- Tables:
--   lazarus_runner       local runner heartbeat + capabilities
--   lazarus_task         admin-created work queue
--   lazarus_run          one leased execution attempt
--   lazarus_event        append-only run event stream
--   lazarus_approval     hard gates for destructive/cost/PRIME-adjacent ops
--   lazarus_artifact     diffs, logs, screenshots, traces
--   lazarus_fleet_config model/privacy/budget routing configuration
--
-- cssl-edge accesses this surface with SUPABASE_SERVICE_ROLE_KEY only.
-- Browser clients must go through /api/admin/lazarus/*.
-- =====================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS public.lazarus_runner (
    id              text        PRIMARY KEY,
    label           text        NOT NULL,
    status          text        NOT NULL DEFAULT 'online'
                              CHECK (status IN ('online','offline','revoked')),
    capabilities    text[]      NOT NULL DEFAULT ARRAY[]::text[],
    current_run_id  text,
    last_seen_at    timestamptz NOT NULL DEFAULT now(),
    registered_at   timestamptz NOT NULL DEFAULT now(),
    metadata        jsonb       NOT NULL DEFAULT '{}'::jsonb,
    CONSTRAINT lazarus_runner_id_shape CHECK (id ~ '^[A-Za-z0-9_.:-]{1,96}$')
);

CREATE TABLE IF NOT EXISTS public.lazarus_task (
    id                  text        PRIMARY KEY,
    title               text        NOT NULL CHECK (char_length(title) BETWEEN 3 AND 160),
    prompt              text        NOT NULL CHECK (char_length(prompt) BETWEEN 8 AND 65536),
    repo_path           text        NOT NULL,
    model_mode          text        NOT NULL
                                      CHECK (model_mode IN ('deepseek-v4-pro','deepseek-v4-flash','reviewer','stub-safe')),
    cost_ceiling_usd    numeric(10,4) NOT NULL DEFAULT 2.0000 CHECK (cost_ceiling_usd >= 0),
    sensorium_enabled   boolean     NOT NULL DEFAULT true,
    playtest_enabled    boolean     NOT NULL DEFAULT false,
    status              text        NOT NULL DEFAULT 'queued'
                                      CHECK (status IN ('queued','leased','running','blocked','completed','failed','cancelled')),
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    leased_by           text        REFERENCES public.lazarus_runner(id) ON DELETE SET NULL,
    metadata            jsonb       NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS lazarus_task_status_created_idx
    ON public.lazarus_task (status, created_at);

CREATE TABLE IF NOT EXISTS public.lazarus_run (
    id                  text        PRIMARY KEY,
    task_id             text        NOT NULL REFERENCES public.lazarus_task(id) ON DELETE CASCADE,
    runner_id           text        NOT NULL REFERENCES public.lazarus_runner(id) ON DELETE CASCADE,
    status              text        NOT NULL
                                      CHECK (status IN ('leased','running','blocked','completed','failed','cancelled')),
    model_mode          text        NOT NULL
                                      CHECK (model_mode IN ('deepseek-v4-pro','deepseek-v4-flash','reviewer','stub-safe')),
    started_at          timestamptz NOT NULL DEFAULT now(),
    finished_at         timestamptz,
    summary             text,
    cost_usd_estimate   numeric(10,4) NOT NULL DEFAULT 0 CHECK (cost_usd_estimate >= 0),
    metadata            jsonb       NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS lazarus_run_task_idx
    ON public.lazarus_run (task_id, started_at DESC);
CREATE INDEX IF NOT EXISTS lazarus_run_runner_status_idx
    ON public.lazarus_run (runner_id, status, started_at DESC);

ALTER TABLE public.lazarus_runner
    DROP CONSTRAINT IF EXISTS lazarus_runner_current_run_fk;
ALTER TABLE public.lazarus_runner
    ADD CONSTRAINT lazarus_runner_current_run_fk
    FOREIGN KEY (current_run_id) REFERENCES public.lazarus_run(id) ON DELETE SET NULL;

CREATE TABLE IF NOT EXISTS public.lazarus_event (
    id          bigserial   PRIMARY KEY,
    run_id      text        NOT NULL REFERENCES public.lazarus_run(id) ON DELETE CASCADE,
    ts          timestamptz NOT NULL DEFAULT now(),
    level       text        NOT NULL DEFAULT 'info'
                            CHECK (level IN ('info','warn','error','debug')),
    kind        text        NOT NULL CHECK (char_length(kind) BETWEEN 1 AND 96),
    message     text        NOT NULL CHECK (char_length(message) BETWEEN 1 AND 4096),
    payload     jsonb       NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS lazarus_event_run_id_idx
    ON public.lazarus_event (run_id, id);

CREATE TABLE IF NOT EXISTS public.lazarus_approval (
    id            text        PRIMARY KEY,
    run_id        text        NOT NULL REFERENCES public.lazarus_run(id) ON DELETE CASCADE,
    gate          text        NOT NULL CHECK (gate IN (
        'git.push',
        'git.destructive',
        'fs.bulk_delete',
        'network.unknown_egress',
        'cost.overrun',
        'mneme.standing_write',
        'system.driver_or_setting',
        'prime.sigma.capability_sensitive',
        'hardware.mutation'
    )),
    status        text        NOT NULL DEFAULT 'pending'
                              CHECK (status IN ('pending','approved','denied','expired')),
    requested_at  timestamptz NOT NULL DEFAULT now(),
    decided_at    timestamptz,
    decided_by    text,
    reason        text        NOT NULL,
    payload       jsonb       NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS lazarus_approval_status_idx
    ON public.lazarus_approval (status, requested_at DESC);

CREATE TABLE IF NOT EXISTS public.lazarus_artifact (
    id          text        PRIMARY KEY,
    run_id      text        NOT NULL REFERENCES public.lazarus_run(id) ON DELETE CASCADE,
    kind        text        NOT NULL CHECK (kind IN ('diff','log','screenshot','trace','report')),
    uri         text        NOT NULL,
    sha256      text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    metadata    jsonb       NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS lazarus_artifact_run_idx
    ON public.lazarus_artifact (run_id, created_at DESC);

CREATE TABLE IF NOT EXISTS public.lazarus_fleet_config (
    id                    text        PRIMARY KEY,
    privacy_class         text        NOT NULL
                                      CHECK (privacy_class IN ('local-only','secret-ok','external-ok')),
    default_model_mode    text        NOT NULL
                                      CHECK (default_model_mode IN ('deepseek-v4-pro','deepseek-v4-flash','reviewer','stub-safe')),
    max_cost_usd_per_run  numeric(10,4) NOT NULL DEFAULT 2.0000 CHECK (max_cost_usd_per_run >= 0),
    review_required       boolean     NOT NULL DEFAULT true,
    updated_at            timestamptz NOT NULL DEFAULT now(),
    metadata              jsonb       NOT NULL DEFAULT '{}'::jsonb
);

INSERT INTO public.lazarus_fleet_config (
    id,
    privacy_class,
    default_model_mode,
    max_cost_usd_per_run,
    review_required,
    metadata
) VALUES (
    'default',
    'secret-ok',
    'deepseek-v4-pro',
    2.0000,
    true,
    '{"reviewer":"cross-vendor","workspace":"LoA v14"}'::jsonb
) ON CONFLICT (id) DO NOTHING;

CREATE OR REPLACE FUNCTION public.lazarus_touch_updated_at()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS lazarus_task_touch_updated_at ON public.lazarus_task;
CREATE TRIGGER lazarus_task_touch_updated_at
BEFORE UPDATE ON public.lazarus_task
FOR EACH ROW EXECUTE FUNCTION public.lazarus_touch_updated_at();

ALTER TABLE public.lazarus_runner       ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.lazarus_task         ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.lazarus_run          ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.lazarus_event        ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.lazarus_approval     ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.lazarus_artifact     ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.lazarus_fleet_config ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS lazarus_runner_service_all ON public.lazarus_runner;
CREATE POLICY lazarus_runner_service_all ON public.lazarus_runner
    FOR ALL TO service_role USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS lazarus_task_service_all ON public.lazarus_task;
CREATE POLICY lazarus_task_service_all ON public.lazarus_task
    FOR ALL TO service_role USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS lazarus_run_service_all ON public.lazarus_run;
CREATE POLICY lazarus_run_service_all ON public.lazarus_run
    FOR ALL TO service_role USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS lazarus_event_service_all ON public.lazarus_event;
CREATE POLICY lazarus_event_service_all ON public.lazarus_event
    FOR ALL TO service_role USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS lazarus_approval_service_all ON public.lazarus_approval;
CREATE POLICY lazarus_approval_service_all ON public.lazarus_approval
    FOR ALL TO service_role USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS lazarus_artifact_service_all ON public.lazarus_artifact;
CREATE POLICY lazarus_artifact_service_all ON public.lazarus_artifact
    FOR ALL TO service_role USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS lazarus_fleet_service_all ON public.lazarus_fleet_config;
CREATE POLICY lazarus_fleet_service_all ON public.lazarus_fleet_config
    FOR ALL TO service_role USING (true) WITH CHECK (true);
