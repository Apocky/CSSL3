// cssl-edge · lib/sovereign.ts
// Sovereign-bypass header detection for cap-gated routes.
// Sovereign-cap = a single 64-bit constant identifying the operator. When the
// caller asserts `sovereign:true` AND presents the matching header, the cap
// gate is bypassed. Without the header, the sovereign flag is ignored.

// Stage-0 sentinel value. Real impl will rotate this via env-var injection.
export const SOVEREIGN_CAP_HEX = '0xCAFEBABEDEADBEEF';

// Header name (lower-cased — Headers API normalizes anyway).
export const SOVEREIGN_HEADER_NAME = 'x-loa-sovereign-cap';

// Inspect a `Headers` instance for the sovereign-cap header. Returns true ONLY
// when the caller passes `sovereignFlag === true` AND the header value matches
// `SOVEREIGN_CAP_HEX` exactly (case-insensitive on the hex digits).
export function isSovereignHeader(hdrs: Headers, sovereignFlag?: boolean): boolean {
  if (sovereignFlag !== true) return false;
  const raw = hdrs.get(SOVEREIGN_HEADER_NAME);
  if (raw === null) return false;
  return raw.toLowerCase() === SOVEREIGN_CAP_HEX.toLowerCase();
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
  return v.toLowerCase() === SOVEREIGN_CAP_HEX.toLowerCase();
}
