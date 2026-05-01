// cssl-edge · lib/license_filter.ts
// Shared license-filter logic for /api/asset/* endpoints.
// Mission : permit only freely-redistributable licenses by default.
// Closed licenses are rejected at search-time AND at proxy-time.

export type License =
  | 'cc0'
  | 'cc-by'
  | 'cc-by-sa'
  | 'public-domain'
  | 'mit'
  | 'apache-2.0'
  | 'gpl-3.0'
  | 'unknown';

// Whitelist : safe-to-redistribute-and-modify licenses for stage-0.
// 'unknown' is REJECTED — must be explicitly tagged.
export const PERMITTED_LICENSES: ReadonlySet<License> = new Set<License>([
  'cc0',
  'cc-by',
  'cc-by-sa',
  'public-domain',
  'mit',
  'apache-2.0',
]);

// Restrictive licenses we explicitly know about but reject by default.
// 'gpl-3.0' is permitted in PERMITTED for source-code; for ASSETS we treat it
// conservatively because GPL on a model can entangle the consuming game.
// Keep the two sets distinct.
export const PERMITTED_FOR_ASSETS: ReadonlySet<License> = new Set<License>([
  'cc0',
  'cc-by',
  'cc-by-sa',
  'public-domain',
]);

export interface LicenseCheck {
  ok: boolean;
  license: License;
  reason?: string;
}

export function normalizeLicense(raw: string | null | undefined): License {
  if (!raw) return 'unknown';
  const s = raw.toLowerCase().trim().replace(/\s+/g, '-');

  if (s === 'cc0' || s === 'creative-commons-zero' || s === 'cc-zero') return 'cc0';
  if (s === 'cc-by' || s === 'cc-by-4.0' || s === 'cc-by-3.0') return 'cc-by';
  if (s === 'cc-by-sa' || s === 'cc-by-sa-4.0') return 'cc-by-sa';
  if (s === 'public-domain' || s === 'pd') return 'public-domain';
  if (s === 'mit') return 'mit';
  if (s === 'apache-2.0' || s === 'apache2' || s === 'apache') return 'apache-2.0';
  if (s === 'gpl-3.0' || s === 'gpl3' || s === 'gpl-v3') return 'gpl-3.0';
  return 'unknown';
}

export function checkAssetLicense(raw: string | null | undefined): LicenseCheck {
  const license = normalizeLicense(raw);
  if (PERMITTED_FOR_ASSETS.has(license)) {
    return { ok: true, license };
  }
  return {
    ok: false,
    license,
    reason: `License '${license}' not in asset-permit list (permitted: ${Array.from(PERMITTED_FOR_ASSETS).join(', ')})`,
  };
}

// Filter a list of asset records by license. Returns only permitted entries.
export function filterByLicense<T extends { license: string }>(items: readonly T[]): T[] {
  return items.filter((item) => checkAssetLicense(item.license).ok);
}
