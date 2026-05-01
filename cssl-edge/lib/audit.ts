// cssl-edge · lib/audit.ts
// Structured audit-event logging for cap-gated routes.
// Vercel captures stdout per-invocation, so a single JSON-line per event is
// sufficient for downstream log aggregation (Vercel Logs, Datadog, etc.).

export type AuditStatus = 'ok' | 'denied' | 'error';

export interface AuditEvent {
  ts_iso: string;
  kind: string;
  cap_used: number;
  sovereign_used: boolean;
  status: AuditStatus;
  extra?: Record<string, unknown>;
}

// Emit a single JSON-line per event. Keep keys terse so log volume is bounded.
export function logEvent(ev: AuditEvent): void {
  // eslint-disable-next-line no-console
  console.log(
    JSON.stringify({
      evt: 'audit',
      ts: ev.ts_iso,
      kind: ev.kind,
      cap: ev.cap_used,
      sovereign: ev.sovereign_used,
      status: ev.status,
      ...(ev.extra ?? {}),
    })
  );
}

// Construct a fresh AuditEvent with current timestamp + supplied fields.
// Convenience for handlers — equivalent to manually building the object.
export function auditEvent(
  kind: string,
  capUsed: number,
  sovereignUsed: boolean,
  status: AuditStatus,
  extra?: Record<string, unknown>
): AuditEvent {
  return {
    ts_iso: new Date().toISOString(),
    kind,
    cap_used: capUsed,
    sovereign_used: sovereignUsed,
    status,
    ...(extra !== undefined ? { extra } : {}),
  };
}

// Standard 403 envelope helper. Builds an AuditEvent with status='denied'
// AND returns the matching HTTP-status + body in one shot — handlers can
// `const d = deny(reason); res.status(d.status).json(d.body); return;`.
export function deny(reason: string, capUsed = 0): { status: number; body: AuditEvent } {
  const body = auditEvent('access.denied', capUsed, false, 'denied', { reason });
  return { status: 403, body };
}
