// cssl-edge · lib/sovereign.ts
// Sovereign-bypass header detection for cap-gated routes.
// Sovereign-cap = a single 64-bit constant identifying the operator. When the
// caller asserts `sovereign:true` AND presents the matching header, the cap
// gate is bypassed. Without the header, the sovereign flag is ignored.

// Legacy development/test fixture. Production authorization never accepts it.
// A production bypass must be an independently provisioned high-entropy secret.
export const SOVEREIGN_CAP_HEX = '0xCAFEBABEDEADBEEF';

// Header name (lower-cased — Headers API normalizes anyway).
export const SOVEREIGN_HEADER_NAME = 'x-loa-sovereign-cap';

function configuredSovereignCap(): string | null {
  const configured = process.env.LOA_SOVEREIGN_CAP_HEX?.trim();
  if (configured && configured.length >= 32 && configured.length <= 256) return configured;
  return process.env.NODE_ENV === 'production' ? null : SOVEREIGN_CAP_HEX;
}

function matchesConfiguredCap(raw: string): boolean {
  const expected = configuredSovereignCap();
  if (!expected || raw.length !== expected.length) return false;
  let mismatch = 0;
  for (let index = 0; index < expected.length; index += 1) {
    mismatch |= raw.charCodeAt(index) ^ expected.charCodeAt(index);
  }
  return mismatch === 0;
}

// Inspect a `Headers` instance for the sovereign-cap header. Returns true ONLY
// when the caller passes `sovereignFlag === true` AND the header value matches
// `SOVEREIGN_CAP_HEX` exactly (case-insensitive on the hex digits).
export function isSovereignHeader(hdrs: Headers, sovereignFlag?: boolean): boolean {
  if (sovereignFlag !== true) return false;
  const raw = hdrs.get(SOVEREIGN_HEADER_NAME);
  if (raw === null) return false;
  return matchesConfiguredCap(raw);
}

// Pages-router compat : Next.js NextApiRequest carries `headers` as a plain
// `IncomingHttpHeaders` (record of string|string[]|undefined). This helper
// adapts that shape to the same boolean predicate for the pages-router routes.
export function isSovereignFromIncoming(
  hdrs: Record<string, string | string[] | undefined>,
  sovereignFlag?: boolean
): boolean {
  if (sovereignFlag !== true) return false;
  const raw = hdrs[SOVEREIGN_HEADER_NAME];
  const v = Array.isArray(raw) ? raw[0] : raw;
  if (typeof v !== 'string') return false;
  return matchesConfiguredCap(v);
}
