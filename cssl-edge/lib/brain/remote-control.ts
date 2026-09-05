import type { NextApiRequest, NextApiResponse } from 'next';
import { createHash } from 'node:crypto';
import { getAdminAuthorization } from '../admin-auth';
import { fetchBridge } from '../bridge/queue';
import { validBridgeCodePath } from '../bridge/crypto';
import { ACCOUNT_UUID, CONVERSATION_UUID } from '../mobile/account-grant';
import { createServerTrace, emitOperationalTelemetry } from '../telemetry/server';
import { setBrainPrivateHeaders } from './owner';

type Control = { action: 'status' } | { action: 'read' | 'rollback'; operation_id: string }
  | { action: 'run'; operation_id: string; objective: string; allowed_paths: string[] };
const SHA = /^[0-9a-f]{64}$/;
function record(value: unknown): value is Record<string, unknown> { return value !== null && typeof value === 'object' && !Array.isArray(value); }
function control(value: unknown): Control | null {
  if (!record(value)) return null;
  const keys = Object.keys(value).sort().join(',');
  if (value.action === 'status' && keys === 'action') return value as Control;
  if (typeof value.operation_id !== 'string' || !CONVERSATION_UUID.test(value.operation_id)) return null;
  if (typeof value.action === 'string' && ['read', 'rollback'].includes(value.action) && keys === 'action,operation_id') return value as Control;
  if (value.action !== 'run' || keys !== 'action,allowed_paths,objective,operation_id'
    || typeof value.objective !== 'string' || !value.objective || value.objective !== value.objective.trim()
    || [...value.objective].length > 32_768 || !Array.isArray(value.allowed_paths)
    || value.allowed_paths.length < 1 || value.allowed_paths.length > 32
    || value.allowed_paths.some(path => !validBridgeCodePath(path))
    || new Set(value.allowed_paths).size !== value.allowed_paths.length
    || JSON.stringify([...value.allowed_paths].sort()) !== JSON.stringify(value.allowed_paths)) return null;
  return value as Control;
}

function sha(value: unknown): value is string { return typeof value === 'string' && SHA.test(value); }
function nullableSha(value: unknown): boolean { return value === null || sha(value); }
function timestamp(value: unknown): boolean { return typeof value === 'string' && value.length <= 40 && /^\d{4}-\d\d-\d\dT/.test(value) && Number.isFinite(Date.parse(value)); }
function hash(value: Record<string, string>): string { return createHash('sha256').update(JSON.stringify(Object.fromEntries(Object.keys(value).sort().map(key => [key, value[key]])))).digest('hex'); }
function verifiedProjection(value: unknown, response: Response, action: Control): Record<string, unknown> | null {
  if (!record(value) || !sha(value.principal_ref) || !sha(value.privacy_partition_ref) || !sha(value.workspace_ref)) return null;
  const partition = hash({ schema_version: 'apocv4.runtime-auth.v1', privacy_partition: 'owner:apocky' });
  const binding = hash({ schema_version: 'apocv4.runtime-auth.v1', principal_ref: value.principal_ref, privacy_partition_ref: partition });
  if (response.headers.get('x-apocv4-auth-mode') !== 'STRICT_REGISTRY'
    || !sha(response.headers.get('x-apocv4-auth-registry-ref'))
    || response.headers.get('x-apocv4-principal-ref') !== value.principal_ref
    || response.headers.get('x-apocv4-privacy-partition-ref') !== partition
    || value.privacy_partition_ref !== partition || response.headers.get('x-apocv4-binding-ref') !== binding) return null;
  const identity = { schema_version: value.schema_version, workspace_ref: value.workspace_ref,
    principal_ref: value.principal_ref, privacy_partition_ref: value.privacy_partition_ref };
  if (action.action === 'status') {
    if (value.schema_version !== 'apocv4.remote-code.capabilities.v1' || typeof value.enabled !== 'boolean'
      || value.execution !== 'DIRECT_RUN' || typeof value.enforcement !== 'string' || !['EFFECT_GATEWAY_ENFORCE', 'UNAVAILABLE'].includes(value.enforcement)
      || !Array.isArray(value.actions) || JSON.stringify(value.actions) !== JSON.stringify(value.enabled ? ['run', 'read', 'rollback'] : [])
      || (value.enabled && value.enforcement !== 'EFFECT_GATEWAY_ENFORCE') || !sha(value.test_command_digest)
      || !record(value.limits) || value.limits.objective_chars !== 32768 || value.limits.allowed_paths !== 32) return null;
    return { ...identity, enabled: value.enabled, execution: value.execution, enforcement: value.enforcement,
      actions: value.actions, test_command_digest: value.test_command_digest, limits: { objective_chars: 32768, allowed_paths: 32 } };
  }
  if (value.schema_version !== 'apocv4.remote-code.operation.v1' || value.operation_id !== action.operation_id
    || !sha(value.input_digest) || !timestamp(value.created_at) || !timestamp(value.updated_at)
    || typeof value.state !== 'string' || !['PROMOTED', 'PROMOTION_ABORTED', 'EXECUTION_ROLLED_BACK', 'SOURCE_DRIFT_BEFORE_ADMISSION', 'ADMISSION_REFUSED', 'FAILED', 'INDETERMINATE', 'ROLLED_BACK'].includes(value.state)
    || !record(value.result)) return null;
  const result = value.result;
  const digests = ['terminal_event_digest', 'promotion_event_digest', 'rollback_event_digest', 'request_digest', 'proposal_digest'];
  if (digests.some(key => !nullableSha(result[key])) || !Array.isArray(result.changed_paths)
    || result.changed_paths.length > 32 || result.changed_paths.some(path => !validBridgeCodePath(path))) return null;
  const test = result.test;
  if (test !== null && (!record(test) || typeof test.passed !== 'boolean' || typeof test.timed_out !== 'boolean'
    || (test.exit_code !== null && !Number.isSafeInteger(test.exit_code)) || !sha(test.receipt_digest))) return null;
  return { ...identity, operation_id: value.operation_id, input_digest: value.input_digest, state: value.state,
    created_at: value.created_at, updated_at: value.updated_at, result: {
      ...Object.fromEntries(digests.map(key => [key, result[key]])), changed_paths: result.changed_paths,
      test: record(test) ? { passed: test.passed, exit_code: test.exit_code, timed_out: test.timed_out, receipt_digest: test.receipt_digest } : null } };
}

export function createRemoteControlHandler(dependencies: {
  authorize?: typeof getAdminAuthorization; bridge?: typeof fetchBridge;
  audit?: (req: NextApiRequest, actor: string, action: Control) => Promise<boolean>;
} = {}) {
  return async (req: NextApiRequest, res: NextApiResponse): Promise<void> => {
    setBrainPrivateHeaders(res); res.setHeader('Allow', 'POST');
    const fail = (status: number, code: string) => { res.status(status).json({ code, error: 'The desktop action could not be completed. Keep the operation ID to check its result.' }); };
    if (req.method !== 'POST') { fail(405, 'CONTROL_METHOD_DENIED'); return; }
    const origin = req.headers.origin;
    if (origin !== 'https://www.apocky.com' && !(process.env.NODE_ENV !== 'production' && typeof origin === 'string'
      && /^http:\/\/(?:localhost|127\.0\.0\.1):[0-9]{1,5}$/.test(origin) && new URL(origin).host === req.headers.host)) {
      fail(403, 'CONTROL_ORIGIN_DENIED'); return;
    }
    if (req.headers['content-type']?.split(';', 1)[0]?.trim() !== 'application/json') { fail(415, 'CONTROL_JSON_REQUIRED'); return; }
    let authorization;
    try { authorization = await (dependencies.authorize ?? getAdminAuthorization)(req); }
    catch { fail(503, 'CONTROL_AUTH_UNAVAILABLE'); return; }
    if (!authorization.authorized || !authorization.user) { fail(authorization.user ? 403 : 401, 'CONTROL_OWNER_REQUIRED'); return; }
    const action = Object.keys(req.query).length === 0 ? control(req.body) : null;
    if (!action) { fail(400, 'CONTROL_REQUEST_INVALID'); return; }
    const subject = process.env.APOCRYPHA_BRIDGE_OWNER_USER_ID ?? '';
    if (!ACCOUNT_UUID.test(subject)) { fail(503, 'CONTROL_BRIDGE_UNCONFIGURED'); return; }
    try {
      const audit = dependencies.audit ?? (async (request, actor, command) => {
        const receipt = await emitOperationalTelemetry({ trace: createServerTrace(request),
          kind: 'operator.desktop.control', source: 'remote-control', plane: 'effect', severity: 'info',
          outcome: 'accepted', authority: 'VERIFIED_ADMIN', attributes: { actor_id: actor, action: command.action,
            ...(command.action === 'status' ? {} : { operation_id: command.operation_id }) } });
        return receipt.persisted;
      });
      if (!await audit(req, authorization.user.id, action)) { fail(503, 'CONTROL_AUDIT_UNAVAILABLE'); return; }
      const target = action.action === 'status' ? '/v1/code/capabilities'
        : action.action === 'read' ? `/v1/code/operations?operation_id=${action.operation_id}`
        : action.action === 'rollback' ? '/v1/code/operations/rollback' : '/v1/code/operations';
      const body = action.action === 'run' ? { operation_id: action.operation_id, objective: action.objective, allowed_paths: action.allowed_paths }
        : action.action === 'rollback' ? { operation_id: action.operation_id } : null;
      const response = await (dependencies.bridge ?? fetchBridge)({ channel: 'owner', subject, target,
        method: body === null ? 'GET' : 'POST', body: Buffer.from(body === null ? '' : JSON.stringify(body)),
        signal: AbortSignal.timeout(280_000) });
      const raw = await response.text();
      if (Buffer.byteLength(raw) > 262_144) { fail(502, 'CONTROL_RESPONSE_TOO_LARGE'); return; }
      const value: unknown = JSON.parse(raw);
      if (!response.ok) {
        const code = record(value) && typeof value.error === 'string' && /^[a-z0-9_]{1,80}$/.test(value.error)
          ? value.error.toUpperCase() : 'CONTROL_UPSTREAM_FAILED';
        fail([400, 403, 404, 409, 503].includes(response.status) ? response.status : 502, code); return;
      }
      const projection = verifiedProjection(value, response, action);
      if (!projection) {
        fail(502, 'CONTROL_RESPONSE_UNVERIFIED'); return;
      }
      res.status(200).json(projection);
    } catch { fail(503, 'CONTROL_RESULT_UNAVAILABLE'); }
  };
}
