// cssl-edge · lib/response.ts
// Standard response envelope for all /api/* endpoints.
// Every response carries `served_by` + `ts` so client code can attribute + trace.

export interface BaseEnvelope {
  served_by: string;
  ts: string;
}

export interface StubEnvelope extends BaseEnvelope {
  stub: true;
  todo: string;
}

const SERVED_BY = process.env.EDGE_SERVED_BY ?? 'cssl-edge';

export function envelope(): BaseEnvelope {
  return {
    served_by: SERVED_BY,
    ts: new Date().toISOString(),
  };
}

export function stubEnvelope(todo: string): StubEnvelope {
  return {
    ...envelope(),
    stub: true,
    todo,
  };
}

// Capture commit SHA — Vercel injects VERCEL_GIT_COMMIT_SHA at build/runtime.
// Falls back to local-git-sha env or 'unknown'.
export function commitSha(): string {
  return (
    process.env.VERCEL_GIT_COMMIT_SHA ??
    process.env.GIT_COMMIT_SHA ??
    process.env.COMMIT_SHA ??
    'unknown'
  );
}

// Resolve client cap header. Defaults to "sovereign" — flag for trace.
export function resolveCap(raw: string | string[] | undefined): 'sovereign' | 'none' {
  const v = Array.isArray(raw) ? raw[0] : raw;
  if (v === 'none') return 'none';
  return 'sovereign';
}

// Console-log shape consumed by Vercel runtime logs. Keep keys terse for cost.
export function logHit(route: string, extra: Record<string, unknown> = {}): void {
  // eslint-disable-next-line no-console
  console.log(JSON.stringify({ evt: 'hit', route, ts: new Date().toISOString(), ...extra }));
}
