// cssl-edge · /api/asset/recommend
// Public recommendation stub for the marketplace gallery's "recommended for you"
// section. Returns a deterministic, license-filtered list of asset suggestions
// keyed off (player_id + seed_features) so the same input always produces the
// same recommendations within a deploy.
//
// Auth :
//   - Public endpoint (no cap required) · simple rate-limit via x-loa-rl header
//   - Heavy upstream calls deliberately omitted in stage-0
//
// Query :
//   - GET ?player_id=<string>&seed_features=<base64-json>&limit=<int>
//   - 200 : envelope({ recommendations: AssetSummary[], reason: string })
//   - 400 : missing/bad params
//   - 429 : x-loa-rl: deny header presented (test hook)

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope } from '@/lib/response';
import { logEvent, auditEvent } from '@/lib/audit';

export interface AssetSummary {
  asset_id: string;
  source: string;
  name: string;
  license: string;
  license_short: string;
  score: number;
  why: string;
}

interface RecommendOk {
  served_by: string;
  ts: string;
  recommendations: AssetSummary[];
  reason: string;
  player_id: string;
  total: number;
}

interface RecommendError {
  error: string;
  served_by: string;
  ts: string;
}

// Default + max limit on recommendations.
const DEFAULT_LIMIT = 24;
const MAX_LIMIT = 64;

// Permitted licenses (MUST match marketplace gallery defaults : CC0 + CC-BY-4.0).
const PERMITTED: ReadonlySet<string> = new Set<string>(['cc0', 'cc-by', 'cc-by-4.0']);

// 24-row stub catalog · all CC0 or CC-BY-4.0. Mirror of /api/asset/search shape +
// added score + why fields. License `cc-by` is normalized to `cc-by-4.0` form for
// badge display consistency with /marketplace/index.tsx.
const STUB_CATALOG: ReadonlyArray<Omit<AssetSummary, 'score' | 'why'>> = [
  // src=polyhaven · CC0
  { asset_id: 'polyhaven--wooden_cabin_01', source: 'polyhaven', name: 'Wooden Cabin 01', license: 'cc0', license_short: 'CC0' },
  { asset_id: 'polyhaven--rock_pile_03', source: 'polyhaven', name: 'Rock Pile 03', license: 'cc0', license_short: 'CC0' },
  { asset_id: 'polyhaven--forest_grass_02', source: 'polyhaven', name: 'Forest Grass 02', license: 'cc0', license_short: 'CC0' },
  { asset_id: 'polyhaven--firepit_old', source: 'polyhaven', name: 'Old Firepit', license: 'cc0', license_short: 'CC0' },
  // src=kenney · CC0
  { asset_id: 'kenney--cabin-survival', source: 'kenney', name: 'Cabin (Survival Kit)', license: 'cc0', license_short: 'CC0' },
  { asset_id: 'kenney--ui-pack-rpg', source: 'kenney', name: 'UI Pack (RPG)', license: 'cc0', license_short: 'CC0' },
  { asset_id: 'kenney--medieval-pack', source: 'kenney', name: 'Medieval Asset Pack', license: 'cc0', license_short: 'CC0' },
  { asset_id: 'kenney--space-kit', source: 'kenney', name: 'Space Kit', license: 'cc0', license_short: 'CC0' },
  // src=quaternius · CC0
  { asset_id: 'quaternius--lowpoly-cabins', source: 'quaternius', name: 'Lowpoly Cabin Pack', license: 'cc0', license_short: 'CC0' },
  { asset_id: 'quaternius--ultimate-creatures', source: 'quaternius', name: 'Ultimate Creatures Pack', license: 'cc0', license_short: 'CC0' },
  { asset_id: 'quaternius--lowpoly-trees', source: 'quaternius', name: 'Lowpoly Trees Pack', license: 'cc0', license_short: 'CC0' },
  { asset_id: 'quaternius--character-pack', source: 'quaternius', name: 'Character Pack', license: 'cc0', license_short: 'CC0' },
  // src=opengameart · CC-BY-4.0
  { asset_id: 'opengameart--wisp-companion', source: 'opengameart', name: 'Wisp Companion Sprite', license: 'cc-by-4.0', license_short: 'CC-BY-4.0' },
  { asset_id: 'opengameart--ambient-pad', source: 'opengameart', name: 'Ambient Pad Loop', license: 'cc-by-4.0', license_short: 'CC-BY-4.0' },
  { asset_id: 'opengameart--door-creak', source: 'opengameart', name: 'Door Creak SFX', license: 'cc-by-4.0', license_short: 'CC-BY-4.0' },
  { asset_id: 'opengameart--torch-flicker', source: 'opengameart', name: 'Torch Flicker Loop', license: 'cc-by-4.0', license_short: 'CC-BY-4.0' },
  // src=poly-pizza · CC0 + CC-BY
  { asset_id: 'poly-pizza--lantern', source: 'poly-pizza', name: 'Wrought Iron Lantern', license: 'cc0', license_short: 'CC0' },
  { asset_id: 'poly-pizza--treasure-chest', source: 'poly-pizza', name: 'Treasure Chest', license: 'cc0', license_short: 'CC0' },
  { asset_id: 'poly-pizza--mushroom-cluster', source: 'poly-pizza', name: 'Mushroom Cluster', license: 'cc-by-4.0', license_short: 'CC-BY-4.0' },
  { asset_id: 'poly-pizza--ancient-pillar', source: 'poly-pizza', name: 'Ancient Pillar', license: 'cc-by-4.0', license_short: 'CC-BY-4.0' },
  // src=ambient-cg · CC0
  { asset_id: 'ambient-cg--moss-rock', source: 'ambient-cg', name: 'Moss Rock Material', license: 'cc0', license_short: 'CC0' },
  { asset_id: 'ambient-cg--cobblestone', source: 'ambient-cg', name: 'Cobblestone Material', license: 'cc0', license_short: 'CC0' },
  { asset_id: 'ambient-cg--worn-bark', source: 'ambient-cg', name: 'Worn Bark Material', license: 'cc0', license_short: 'CC0' },
  { asset_id: 'ambient-cg--snow-deep', source: 'ambient-cg', name: 'Deep Snow Material', license: 'cc0', license_short: 'CC0' },
  // Bonus rows for >24 limit testing.
  { asset_id: 'polyhaven--leaves_oak_winter', source: 'polyhaven', name: 'Oak Leaves (Winter)', license: 'cc0', license_short: 'CC0' },
  { asset_id: 'kenney--platformer-pack', source: 'kenney', name: 'Platformer Pack', license: 'cc0', license_short: 'CC0' },
];

// Cheap deterministic hash : FNV-1a 32-bit. Stable across Node versions.
function fnv1a(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i += 1) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

function readQueryParam(
  q: Record<string, string | string[] | undefined>,
  key: string
): string | undefined {
  const v = q[key];
  if (Array.isArray(v)) return v[0];
  return v;
}

// Build deterministic recommendations from (player_id, seed_features).
// Same inputs → same scores → same ordering. CC0 + CC-BY-4.0 only.
export function buildRecommendations(
  player_id: string,
  seedFeaturesJson: string,
  limit: number
): { recs: AssetSummary[]; reason: string } {
  const baseHash = fnv1a(`${player_id}::${seedFeaturesJson}`);
  // Filter catalog by license · build a candidate list.
  const candidates = STUB_CATALOG.filter((row) => PERMITTED.has(row.license.toLowerCase()));

  // Score each candidate · score derived from (asset_id-hash XOR baseHash).
  // Result is a stable float in [0, 1).
  const scored: AssetSummary[] = candidates.map((row) => {
    const localHash = fnv1a(row.asset_id);
    const combined = (localHash ^ baseHash) >>> 0;
    const score = combined / 0x100000000; // 32-bit normalize
    const why = `Matches your ${row.source} preference signal (score=${score.toFixed(3)})`;
    return {
      asset_id: row.asset_id,
      source: row.source,
      name: row.name,
      license: row.license,
      license_short: row.license_short,
      score,
      why,
    };
  });

  // Stable descending sort by score · ties broken by asset_id for determinism.
  scored.sort((a, b) => {
    if (b.score !== a.score) return b.score - a.score;
    return a.asset_id.localeCompare(b.asset_id);
  });

  const truncated = scored.slice(0, Math.min(limit, scored.length));
  const reason = `Recommendations seeded from player_id + ${seedFeaturesJson.length}-byte feature vector`;
  return { recs: truncated, reason };
}

export default function handler(
  req: NextApiRequest,
  res: NextApiResponse<RecommendOk | RecommendError>
): void {
  logHit('asset.recommend', { method: req.method ?? 'GET' });

  if (req.method !== 'GET') {
    const env = envelope();
    res.setHeader('Allow', 'GET');
    res.status(405).json({
      error: 'Method Not Allowed — GET ?player_id=&seed_features=&limit=',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  // Simple rate-limit hook : header `x-loa-rl: deny` emulates a 429 path so
  // tests + monitoring can exercise the throttle branch without hitting a real
  // limiter.
  const rlHdr = req.headers['x-loa-rl'];
  const rlValue = Array.isArray(rlHdr) ? rlHdr[0] : rlHdr;
  if (rlValue === 'deny') {
    const env = envelope();
    res.setHeader('Retry-After', '5');
    res.status(429).json({
      error: 'Too Many Requests — backoff',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const q = req.query as Record<string, string | string[] | undefined>;
  const player_id = readQueryParam(q, 'player_id') ?? 'anonymous';
  const seedFeaturesB64 = readQueryParam(q, 'seed_features') ?? '';
  const limitRaw = readQueryParam(q, 'limit');
  const limitParsed = limitRaw !== undefined ? parseInt(limitRaw, 10) : DEFAULT_LIMIT;
  const limit = Math.max(
    1,
    Math.min(Number.isFinite(limitParsed) ? limitParsed : DEFAULT_LIMIT, MAX_LIMIT)
  );

  // Decode seed-features (best-effort) · we don't actually USE the features in
  // stage-0 beyond hashing them, but we still validate base64 → JSON shape so
  // future model-callers can swap implementations.
  let seedJson = '';
  if (seedFeaturesB64.length > 0) {
    try {
      seedJson = Buffer.from(seedFeaturesB64, 'base64').toString('utf-8');
      // Optional shape-check : must JSON-parse to an object.
      JSON.parse(seedJson);
    } catch {
      seedJson = ''; // fall back to empty seed → deterministic baseline ordering
    }
  }

  const { recs, reason } = buildRecommendations(player_id, seedJson, limit);

  logEvent(
    auditEvent('asset.recommend', 0, false, 'ok', {
      player_id,
      limit,
      returned: recs.length,
    })
  );

  const env = envelope();
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    recommendations: recs,
    reason,
    player_id,
    total: recs.length,
  });
}
