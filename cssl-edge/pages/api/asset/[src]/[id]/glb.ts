// cssl-edge · /api/asset/<src>/<id>/glb
// Cached binary proxy stub for a specific asset's .glb payload.
// Real impl : verify license → fetch upstream → cache in Supabase Storage → stream.
// Stage-0 : returns JSON descriptor since no real binary cache exists yet.

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, stubEnvelope } from '@/lib/response';

interface GlbStubResponse {
  src: string;
  id: string;
  format: 'glb';
  cached: false;
  upstream_hint: string | null;
  served_by: string;
  ts: string;
  stub: true;
  todo: string;
}

interface GlbError {
  error: string;
  served_by: string;
  ts: string;
}

const UPSTREAM_TEMPLATES: Record<string, (id: string) => string> = {
  polyhaven: (id) => `https://dl.polyhaven.org/file/ph-assets/Models/glb/4k/${id}.glb`,
  kenney: (id) => `https://kenney.nl/assets/${id}`,
  quaternius: (id) => `https://quaternius.com/packs/${id}.html`,
  sketchfab: (id) => `https://sketchfab.com/models/${id}/embed`,
};

function pickUpstream(src: string, id: string): string | null {
  const fn = UPSTREAM_TEMPLATES[src];
  return fn ? fn(id) : null;
}

export default function handler(
  req: NextApiRequest,
  res: NextApiResponse<GlbStubResponse | GlbError>
): void {
  const srcRaw = req.query['src'];
  const idRaw = req.query['id'];

  const src = (Array.isArray(srcRaw) ? srcRaw[0] : srcRaw) ?? '';
  const id = (Array.isArray(idRaw) ? idRaw[0] : idRaw) ?? '';

  logHit('asset.glb', { src, id, method: req.method ?? 'GET' });

  if (req.method !== 'GET') {
    const stub = stubEnvelope('GET only');
    res.setHeader('Allow', 'GET');
    res.status(405).json({
      error: 'Method Not Allowed — GET only',
      served_by: stub.served_by,
      ts: stub.ts,
    });
    return;
  }

  if (src.length === 0 || id.length === 0) {
    const stub = stubEnvelope('require non-empty src + id path-params');
    res.status(400).json({
      error: 'Bad Request — src and id path-params required',
      served_by: stub.served_by,
      ts: stub.ts,
    });
    return;
  }

  const upstream = pickUpstream(src, id);
  const stub = stubEnvelope(
    'verify license · fetch upstream · cache via Supabase Storage · stream binary'
  );

  res.status(200).json({
    src,
    id,
    format: 'glb',
    cached: false,
    upstream_hint: upstream,
    served_by: stub.served_by,
    ts: stub.ts,
    stub: true,
    todo: stub.todo,
  });
}
