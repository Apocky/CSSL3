// One-job Direct SMS executor. Vercel cron uses GET; authenticated manual
// execution may use POST. Missing SMS configuration is an explicit 503 state,
// never a stub success.

import { randomUUID } from 'node:crypto';

import type { NextApiRequest, NextApiResponse } from 'next';

import { readSmsConfiguration, type SmsConfigurationState } from '@/lib/apocrypha/sms/config';
import { createSmsStore, type SmsStore } from '@/lib/apocrypha/sms/store';
import { sendTwilioMessage } from '@/lib/apocrypha/sms/twilio';
import {
  runSmsWorkerOnce,
  type SmsWorkerDependencies,
  type SmsWorkerResult,
} from '@/lib/apocrypha/sms/worker';
import {
  emitCronAudit,
  isCronAuthorized,
  reject401,
  type CronExecution,
} from '@/lib/cron-auth';
import { envelope, logHit } from '@/lib/response';

type CronAuthorization = ReturnType<typeof isCronAuthorized>;

export interface ApocryphaSmsCronDependencies {
  authorize(req: NextApiRequest): CronAuthorization;
  readConfiguration(): SmsConfigurationState;
  createStore(): SmsStore;
  run(workerId: string, deps: SmsWorkerDependencies): Promise<SmsWorkerResult>;
  workerId(): string;
  audit(execution: CronExecution): Promise<void>;
}

interface SmsCronResponse {
  ok: boolean;
  job: 'apocrypha-sms';
  configured: boolean;
  state: SmsWorkerResult['state'] | 'disabled' | 'unavailable';
  processed: 0 | 1;
  error?: string;
  missing?: string[];
  provider_status?: 'accepted' | 'scheduled' | 'queued';
  served_by: string;
  ts: string;
}

function defaults(): ApocryphaSmsCronDependencies {
  return {
    authorize: isCronAuthorized,
    readConfiguration: () => readSmsConfiguration(process.env),
    createStore: () => createSmsStore(process.env),
    run: runSmsWorkerOnce,
    workerId: () => `sms-cron:${randomUUID()}`,
    audit: emitCronAudit,
  };
}

function audit(
  deps: ApocryphaSmsCronDependencies,
  startedAtMs: number,
  via: CronAuthorization['via'],
  status: CronExecution['status'],
  rows: number,
  notes: string,
): void {
  void deps.audit({
    job_name: 'apocrypha-sms',
    started_at: new Date(startedAtMs).toISOString(),
    finished_at: new Date().toISOString(),
    duration_ms: Date.now() - startedAtMs,
    status,
    rows_processed: rows,
    retry_count: 0,
    via,
    notes,
  });
}

export function createApocryphaSmsCronHandler(
  overrides: Partial<ApocryphaSmsCronDependencies> = {},
): (req: NextApiRequest, res: NextApiResponse<SmsCronResponse>) => Promise<void> {
  const deps: ApocryphaSmsCronDependencies = { ...defaults(), ...overrides };
  return async function apocryphaSmsCronHandler(
    req: NextApiRequest,
    res: NextApiResponse<SmsCronResponse>,
  ): Promise<void> {
    logHit('cron.apocrypha-sms', { method: req.method ?? 'GET' });
    res.setHeader('Cache-Control', 'private, no-store, no-cache, must-revalidate, max-age=0');
    const startedAtMs = Date.now();

    if (req.method !== 'GET' && req.method !== 'POST') {
      res.setHeader('Allow', 'GET, POST');
      const responseEnvelope = envelope();
      res.status(405).json({
        ok: false,
        job: 'apocrypha-sms',
        configured: false,
        state: 'unavailable',
        processed: 0,
        error: 'method_not_allowed',
        served_by: responseEnvelope.served_by,
        ts: responseEnvelope.ts,
      });
      return;
    }

    const authorization = deps.authorize(req);
    if (!authorization.ok) {
      reject401(res, authorization.reason ?? 'auth-failed');
      return;
    }

    const configuration = deps.readConfiguration();
    if (!configuration.configured) {
      audit(deps, startedAtMs, authorization.via, 'skip', 0, 'sms_not_configured');
      const responseEnvelope = envelope();
      res.status(503).json({
        ok: false,
        job: 'apocrypha-sms',
        configured: false,
        state: 'disabled',
        processed: 0,
        error: 'sms_not_configured',
        missing: configuration.missing,
        served_by: responseEnvelope.served_by,
        ts: responseEnvelope.ts,
      });
      return;
    }

    let store: SmsStore;
    try {
      store = deps.createStore();
    } catch {
      audit(deps, startedAtMs, authorization.via, 'fail', 0, 'sms_persistence_unavailable');
      const responseEnvelope = envelope();
      res.status(503).json({
        ok: false,
        job: 'apocrypha-sms',
        configured: true,
        state: 'unavailable',
        processed: 0,
        error: 'sms_persistence_unavailable',
        served_by: responseEnvelope.served_by,
        ts: responseEnvelope.ts,
      });
      return;
    }

    let result: SmsWorkerResult;
    try {
      result = await deps.run(deps.workerId(), {
        store,
        config: configuration.config,
        provider: {
          send: (message) => sendTwilioMessage(
            configuration.config.provider,
            configuration.config.policy,
            message,
          ),
        },
      });
    } catch {
      audit(deps, startedAtMs, authorization.via, 'fail', 0, 'sms_worker_unavailable');
      const responseEnvelope = envelope();
      res.status(503).json({
        ok: false,
        job: 'apocrypha-sms',
        configured: true,
        state: 'unavailable',
        processed: 0,
        error: 'sms_worker_unavailable',
        served_by: responseEnvelope.served_by,
        ts: responseEnvelope.ts,
      });
      return;
    }

    const successful = result.state === 'idle'
      || result.state === 'sent'
      || result.state === 'not_dispatched';
    const note = result.state === 'failed' || result.state === 'uncertain'
      ? result.errorCode
      : result.state;
    audit(
      deps,
      startedAtMs,
      authorization.via,
      successful ? 'ok' : result.state === 'uncertain' ? 'partial' : 'fail',
      result.processed,
      note,
    );
    const responseEnvelope = envelope();
    res.status(200).json({
      ok: successful,
      job: 'apocrypha-sms',
      configured: true,
      state: result.state,
      processed: result.processed,
      ...(result.state === 'failed' || result.state === 'uncertain'
        ? { error: result.errorCode }
        : {}),
      ...(result.state === 'sent' ? { provider_status: result.providerStatus } : {}),
      served_by: responseEnvelope.served_by,
      ts: responseEnvelope.ts,
    });
  };
}

export default createApocryphaSmsCronHandler();
