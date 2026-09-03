import assert from 'node:assert/strict';

import { filterPublicGlossary, PUBLIC_GLOSSARY_SYMBOLS, PUBLIC_GLOSSARY_TERMS } from '../../lib/public-glossary';
import {
  filterPublicSurfaceNodes,
  findPublicSurfaceNodeForPath,
  getExternalRelayNodes,
  getPublicSurfaceNode,
  getPublicSurfaceRelations,
  PUBLIC_SURFACE_AXES,
  PUBLIC_SURFACE_EDGES,
  PUBLIC_SURFACE_NODES,
  type PublicSurfaceId,
} from '../../lib/public-surface-graph';

assert.equal(PUBLIC_SURFACE_NODES.length, 29, 'the public graph must project every ratified top-level capability');
assert.equal(PUBLIC_SURFACE_AXES.length, 4);

const ids = PUBLIC_SURFACE_NODES.map((node) => node.id);
const hrefs = PUBLIC_SURFACE_NODES.map((node) => node.href);
assert.equal(new Set(ids).size, ids.length, 'public node IDs must be unique');
assert.equal(new Set(hrefs).size, hrefs.length, 'public node destinations must be unique');

for (const node of PUBLIC_SURFACE_NODES) {
  assert.ok(node.axes.length > 0, `${node.id} must declare at least one primary dimension`);
  assert.equal(Object.keys(node.coordinates).length, PUBLIC_SURFACE_AXES.length, `${node.id} must explain all four dimensions`);
  assert.doesNotMatch(node.href, /\/shawn(?:\/|$)/i, `${node.id} must not expose candidate-only material`);
}

for (const edge of PUBLIC_SURFACE_EDGES) {
  assert.ok(getPublicSurfaceNode(edge.source), `edge source ${edge.source} must exist`);
  assert.ok(getPublicSurfaceNode(edge.target), `edge target ${edge.target} must exist`);
  assert.notEqual(edge.source, edge.target, 'self-edges add no public navigation information');
}
assert.equal(
  new Set(PUBLIC_SURFACE_EDGES.map((edge) => `${edge.source}->${edge.target}`)).size,
  PUBLIC_SURFACE_EDGES.length,
  'explicit graph edges must not be duplicated',
);

const allowedExternalHosts = new Set(['chaos-tarot.com', 'cssl.dev', 'ko-fi.com', 'www.patreon.com']);
for (const relay of getExternalRelayNodes()) {
  assert.equal(relay.availability, 'external_public');
  assert.ok(allowedExternalHosts.has(new URL(relay.href).hostname), `${relay.href} is not an approved external relay`);
}

assert.equal(getPublicSurfaceNode('membership')?.availability, 'public');
assert.equal(getPublicSurfaceNode('start')?.kind, 'orientation');
assert.equal(getPublicSurfaceNode('quests')?.kind, 'orientation');
assert.equal(getPublicSurfaceNode('status')?.kind, 'reference');
assert.equal(getPublicSurfaceNode('now')?.kind, 'orientation');
assert.equal(getPublicSurfaceNode('labs')?.kind, 'orientation');
assert.equal(getPublicSurfaceNode('memory-tools')?.availability, 'public');
assert.equal(getPublicSurfaceNode('infinity-engine')?.availability, 'design_study');
assert.equal(getPublicSurfaceNode('divination')?.kind, 'reference');
assert.equal(getPublicSurfaceNode('theory-of-everything')?.kind, 'cosmology');
assert.equal(getPublicSurfaceNode('clearing')?.availability, 'public_read_account_write');
assert.equal(getPublicSurfaceNode('ko-fi')?.href, 'https://ko-fi.com/oneinfinity');
assert.equal(getPublicSurfaceNode('patreon')?.href, 'https://www.patreon.com/0ne1nfinity');
assert.equal(getPublicSurfaceNode('cssl')?.href, '/docs/cssl-language');
assert.equal(getPublicSurfaceNode('cssl')?.external, false);
assert.equal(getPublicSurfaceNode('cslv3')?.href, '/words#symbols');
assert.equal(getPublicSurfaceNode('cslv3')?.external, false);
assert.deepEqual(getExternalRelayNodes().map((node) => node.id), ['chaos-tarot', 'ko-fi', 'patreon']);
assert.equal(findPublicSurfaceNodeForPath('/akashic-records/example-record?view=reader')?.id, 'akashic-records');
assert.equal(findPublicSurfaceNodeForPath('/atlas?axis=Meaning')?.id, 'atlas');
assert.equal(findPublicSurfaceNodeForPath('/shawn'), undefined);

assert.deepEqual(filterPublicSurfaceNodes({ query: 'atmosphere' }).map((node) => node.id), ['chaos-tarot']);
assert.ok(filterPublicSurfaceNodes({ axis: 'Time' }).every((node) => node.axes.includes('Time')));
assert.ok(filterPublicSurfaceNodes({ availability: 'design_study' }).every((node) => node.availability === 'design_study'));

const visited = new Set<PublicSurfaceId>(['atlas']);
const frontier: PublicSurfaceId[] = ['atlas'];
while (frontier.length > 0) {
  const current = frontier.shift();
  if (!current) break;
  for (const relation of getPublicSurfaceRelations(current)) {
    if (!visited.has(relation.neighbor.id)) {
      visited.add(relation.neighbor.id);
      frontier.push(relation.neighbor.id);
    }
  }
}
assert.deepEqual([...visited].sort(), [...ids].sort(), 'every public node must be reachable from the Atlas');

assert.equal(PUBLIC_GLOSSARY_TERMS.length, 20);
assert.equal(PUBLIC_GLOSSARY_SYMBOLS.length, 10);
assert.equal(new Set(PUBLIC_GLOSSARY_TERMS.map((term) => term.id)).size, PUBLIC_GLOSSARY_TERMS.length);
assert.equal(new Set(PUBLIC_GLOSSARY_SYMBOLS.map((symbol) => symbol.id)).size, PUBLIC_GLOSSARY_SYMBOLS.length);
assert.deepEqual(filterPublicGlossary('consent').terms.map((term) => term.id), ['consent']);
assert.deepEqual(filterPublicGlossary('section').symbols.map((symbol) => symbol.id), ['section']);

console.log('public surface graph is connected, explicit, public-only, and backed by one shared glossary');
