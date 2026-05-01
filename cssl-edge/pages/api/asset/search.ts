// cssl-edge · /api/asset/search?q=...&license=...
// Free-asset upstream proxy stub. Real impl will fan out to Sketchfab + Polyhaven
// + Quaternius + Kenney + OpenGameArt and license-filter the merged results.

import type { NextApiRequest, NextApiResponse } from 'next';
import { filterByLicense, normalizeLicense } from '@/lib/license_filter';
import { logHit, stubEnvelope } from '@/lib/response';

interface AssetResult {
  src: string;
  id: string;
  name: string;
  license: string;
  format: string;
  url: string;
  preview_url: string;
}

interface SearchResponse {
  results: AssetResult[];
  total: number;
  query: string;
  license_filter: string | null;
  served_by: string;
  ts: string;
  stub: true;
  todo: string;
}

interface SearchError {
  error: string;
  served_by: string;
  ts: string;
}

// Hardcoded stub catalog — real catalog will be fetched + cached via Supabase.
const STUB_CATALOG: readonly AssetResult[] = [
  {
    src: 'polyhaven',
    id: 'wooden_cabin_01',
    name: 'Wooden Cabin 01',
    license: 'cc0',
    format: 'glb',
    url: 'https://polyhaven.com/a/wooden_cabin_01',
    preview_url: 'https://cdn.polyhaven.com/asset_img/primary/wooden_cabin_01.png',
  },
  {
    src: 'kenney',
    id: 'cabin-survival-kit',
    name: 'Cabin (Survival Kit)',
    license: 'cc0',
    format: 'glb',
    url: 'https://kenney.nl/assets/survival-kit',
    preview_url: 'https://kenney.nl/media/pages/assets/survival-kit/cover.png',
  },
  {
    src: 'quaternius',
    id: 'lowpoly-cabin-pack',
    name: 'Lowpoly Cabin Pack',
    license: 'cc0',
    format: 'gltf',
    url: 'https://quaternius.com/packs/lowpoly-cabins.html',
    preview_url: 'https://quaternius.com/img/cabins.png',
  },
];

export default function handler(
  req: NextApiRequest,
  res: NextApiResponse<SearchResponse | SearchError>
): void {
  logHit('asset.search', { method: req.method ?? 'GET' });

  if (req.method !== 'GET') {
    const stub = stubEnvelope('GET only');
    res.setHeader('Allow', 'GET');
    res.status(405).json({
      error: 'Method Not Allowed — GET ?q=...&license=...',
      served_by: stub.served_by,
      ts: stub.ts,
    });
    return;
  }

  const qParam = req.query['q'];
  const licenseParam = req.query['license'];

  const q = (Array.isArray(qParam) ? qParam[0] : qParam) ?? '';
  const licenseRaw = (Array.isArray(licenseParam) ? licenseParam[0] : licenseParam) ?? null;

  // Stage-0 : return all stub-catalog entries (already cc0). Light query-substring filter
  // so smoke-tests can exercise the path.
  let results: AssetResult[] = q.length === 0
    ? [...STUB_CATALOG]
    : STUB_CATALOG.filter((r) => r.name.toLowerCase().includes(q.toLowerCase()));

  // Always run license filter so misconfigured upstream entries are rejected.
  results = filterByLicense(results);

  // If client asked for a specific license, narrow further.
  if (licenseRaw) {
    const wanted = normalizeLicense(licenseRaw);
    results = results.filter((r) => normalizeLicense(r.license) === wanted);
  }

  const stub = stubEnvelope('fan-out to upstream catalogs · cache via Supabase · paginate');
  res.status(200).json({
    results,
    total: results.length,
    query: q,
    license_filter: licenseRaw,
    served_by: stub.served_by,
    ts: stub.ts,
    stub: true,
    todo: stub.todo,
  });
}
