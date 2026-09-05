export type AccountOperation = 'turn' | 'sessions' | 'status';
export type AccountDiagnosticStage = 'request' | 'authentication' | 'configuration' | 'transport' | 'history' | 'reply' | 'faculty' | 'complete' | 'client';
const entry = (stage: AccountDiagnosticStage, reason: string) => ({ stage, reason });
export const ACCOUNT_DIAGNOSTIC_CODES = {
  ACCOUNT_OK: entry('complete', 'The connection was verified.'),
  ACCOUNT_METHOD_DENIED: entry('request', 'This request method is unavailable.'),
  ACCOUNT_ORIGIN_DENIED: entry('request', 'Open Apocrypha from apocky.com or its official app.'),
  ACCOUNT_CONTENT_TYPE_REQUIRED: entry('request', 'This request format is unsupported. Refresh the page and retry.'),
  ACCOUNT_QUERY_INVALID: entry('request', 'This request has unsupported parameters.'),
  ACCOUNT_SESSION_INVALID: entry('request', 'Choose a valid conversation.'),
  ACCOUNT_TURN_INVALID: entry('request', 'Enter a message and start a valid conversation.'),
  ACCOUNT_REQUEST_INVALID: entry('request', 'This request could not be validated.'),
  ACCOUNT_SIGN_IN_REQUIRED: entry('authentication', 'Please sign in again to continue.'),
  ACCOUNT_SIGN_IN_UNAVAILABLE: entry('authentication', 'Sign-in could not be verified. Please retry.'),
  ACCOUNT_CONFIGURATION_UNAVAILABLE: entry('configuration', 'The account connection is not configured yet.'),
  ACCOUNT_CONFIGURATION_INVALID: entry('configuration', 'The account connection needs a configuration repair.'),
  ACCOUNT_SERVICE_UNAVAILABLE: entry('transport', 'Apocrypha could not be reached. Please try again shortly.'),
  ACCOUNT_RESPONSE_TIMEOUT: entry('transport', 'Apocrypha did not respond in time. Refresh the conversation before retrying.'),
  ACCOUNT_UPSTREAM_UNVERIFIED: entry('transport', 'The account service did not return a verified response.'),
  ACCOUNT_RESPONSE_EMPTY: entry('transport', 'The account service returned an empty response.'),
  ACCOUNT_RESPONSE_TOO_LARGE: entry('transport', 'The account service response exceeded the safe size limit.'),
  ACCOUNT_RESPONSE_INVALID: entry('transport', 'The account service response could not be read safely.'),
  ACCOUNT_RESPONSE_SCOPE_MISMATCH: entry('transport', 'The response could not be verified for this account.'),
  ACCOUNT_SESSION_NOT_FOUND: entry('history', 'This conversation is no longer available.'),
  ACCOUNT_HISTORY_UNVERIFIED: entry('history', 'This conversation could not be read safely. Please refresh it.'),
  ACCOUNT_TURN_UNVERIFIED: entry('reply', 'The reply could not be confirmed. Refresh this conversation before trying again.'),
  ACCOUNT_STATUS_UNVERIFIED: entry('transport', 'The connection status could not be verified.'),
  ACCOUNT_FACULTY_UNREADY: entry('faculty', 'Apocrypha’s response service is not ready yet.'),
  ACCOUNT_RATE_LIMITED: entry('request', 'Please wait a moment before sending again.'),
  ACCOUNT_WAIT_STOPPED: entry('client', 'Stopped waiting. Apocrypha may still finish; refresh the conversation to check.'),
} as const;
export type AccountDiagnosticCode = keyof typeof ACCOUNT_DIAGNOSTIC_CODES;
export interface AccountDiagnostic {
  schema_version: 'apocky.mobile.diagnostic.v1'; time: string; operation: AccountOperation;
  status: number; code: AccountDiagnosticCode; stage: AccountDiagnosticStage;
  duration_ms: number | null; trace_id: string | null;
}
const UUID = /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/i;
const row = (value: unknown): Record<string, unknown> | null => value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null;
export function accountDiagnosticCode(value: unknown, fallback: AccountDiagnosticCode = 'ACCOUNT_SERVICE_UNAVAILABLE'): AccountDiagnosticCode {
  return typeof value === 'string' && Object.hasOwn(ACCOUNT_DIAGNOSTIC_CODES, value) ? value as AccountDiagnosticCode : fallback;
}
export function accountDiagnostic(input: {
  operation: AccountOperation; status: number; code: unknown; time?: unknown; duration_ms?: unknown; trace_id?: unknown;
}): AccountDiagnostic {
  const code = accountDiagnosticCode(input.code);
  const time = typeof input.time === 'string' && /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(input.time) && Number.isFinite(Date.parse(input.time)) ? input.time : new Date().toISOString();
  return { schema_version: 'apocky.mobile.diagnostic.v1', time,
    operation: ['turn', 'sessions', 'status'].includes(input.operation) ? input.operation : 'status',
    status: Number.isSafeInteger(input.status) && input.status >= 0 && input.status <= 599 ? input.status : 0,
    code, stage: ACCOUNT_DIAGNOSTIC_CODES[code].stage,
    duration_ms: typeof input.duration_ms === 'number' && Number.isSafeInteger(input.duration_ms) && input.duration_ms >= 0 && input.duration_ms <= 3_600_000 ? input.duration_ms : null,
    trace_id: typeof input.trace_id === 'string' && UUID.test(input.trace_id) ? input.trace_id : null };
}
export const accountDiagnosticReason = (value: AccountDiagnostic) => ACCOUNT_DIAGNOSTIC_CODES[value.code].reason;
export function accountDiagnosticText(value: AccountDiagnostic): string {
  return JSON.stringify(accountDiagnostic(value), null, 2);
}
export function diagnosticForAccount(binding: { account: string; value: AccountDiagnostic } | null, subject: string | null): AccountDiagnostic | null {
  return binding && subject && binding.account === subject ? binding.value : null;
}
export function diagnosticFromBody(value: unknown, operation: AccountOperation, status: number, trace: string | null, fallback?: AccountDiagnosticCode): AccountDiagnostic {
  const body = row(value); const details = row(body?.diagnostic);
  const defaultCode = fallback ?? (status === 401 ? 'ACCOUNT_SIGN_IN_REQUIRED' : status === 404 ? 'ACCOUNT_SESSION_NOT_FOUND' : status === 429 ? 'ACCOUNT_RATE_LIMITED' : 'ACCOUNT_SERVICE_UNAVAILABLE');
  const verifiedLive = operation === 'status' && status === 200 && body?.schema_version === 'apocky.mobile.status.v1' && body?.status === 'live';
  const code = verifiedLive ? 'ACCOUNT_OK' : accountDiagnosticCode(details?.code ?? body?.code, defaultCode);
  return accountDiagnostic({ operation, status, code: code === 'ACCOUNT_OK' && !verifiedLive ? defaultCode : code,
    time: details?.time, duration_ms: details?.duration_ms, trace_id: trace });
}
export async function readAccountDiagnostic(response: Response, operation: AccountOperation, fallback?: AccountDiagnosticCode): Promise<AccountDiagnostic> {
  let value: unknown; let timer: ReturnType<typeof setTimeout> | undefined;
  const reader = response.body?.getReader();
  try {
    if (reader && response.headers.get('content-type')?.split(';', 1)[0]?.trim() === 'application/json') {
      value = await Promise.race([
        (async () => {
          const chunks: Uint8Array[] = []; let bytes = 0;
          for (;;) { const next = await reader.read(); if (next.done) break; bytes += next.value.byteLength; if (bytes > 8192) return null; chunks.push(next.value); }
          const merged = new Uint8Array(bytes); let offset = 0;
          for (const chunk of chunks) { merged.set(chunk, offset); offset += chunk.byteLength; }
          return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(merged)) as unknown;
        })(),
        new Promise<null>(resolve => { timer = setTimeout(() => resolve(null), 5000); }),
      ]);
    }
  } catch { /* ○ response.unreadable → fixed projection */ }
  finally { if (timer) clearTimeout(timer); if (reader) { void reader.cancel().catch(() => undefined); } }
  return diagnosticFromBody(value, operation, response.status, response.headers.get('x-apocky-trace-id'), fallback);
}
