import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';

import { PUBLIC_SURFACE_EDGES, PUBLIC_SURFACE_NODES } from '../lib/public-surface-graph';

const outputPath = resolve(process.cwd(), 'public', 'constellation.json');
const checkOnly = process.argv.includes('--check');

const projection = {
  schema: 'apocky.public-surface.v1',
  schema_href: '/schemas/public-surface.v1.json',
  canonical_origin: 'https://www.apocky.com',
  generated_from: 'lib/public-surface-graph.ts',
  nodes: PUBLIC_SURFACE_NODES.map((node) => ({
    id: node.id,
    title: node.title,
    summary: node.summary,
    href: node.href,
    external: node.external,
    availability: node.availability,
    kind: node.kind,
    axes: node.axes,
    coordinates: node.coordinates,
  })),
  edges: PUBLIC_SURFACE_EDGES,
};

const rendered = `${JSON.stringify(projection, null, 2)}\n`;

async function main(): Promise<void> {
  if (checkOnly) {
    const current = await readFile(outputPath, 'utf8').catch(() => '');
    if (current !== rendered) {
      throw new Error('public/constellation.json is stale; run npm run snapshot:public-surface');
    }
    console.log(`public surface projection current · ${projection.nodes.length} nodes · ${projection.edges.length} edges`);
    return;
  }

  await writeFile(outputPath, rendered, 'utf8');
  console.log(`public surface projection written · ${projection.nodes.length} nodes · ${projection.edges.length} edges`);
}

void main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : 'public surface projection failed');
  process.exitCode = 1;
});
