import { PUBLIC_SURFACE_NODES, type PublicSurfaceNode } from './public-surface-graph';

export const DIRECTORY_GROUPS = ['Tools', 'Words & ideas', 'Stories & writing', 'Explore', 'Help & support'] as const;
export type DirectoryGroup = (typeof DIRECTORY_GROUPS)[number];

const GROUP_IDS: Readonly<Record<DirectoryGroup, readonly string[]>> = {
  Tools: ['tools', 'spellcraft', 'sigils', 'spellbook', 'apocrypha', 'apps', 'chaos-tarot', 'oracle', 'memory-tools'],
  'Words & ideas': ['words', 'conversations', 'divination', 'omnoid', 'theory-of-everything', 'cssl', 'cslv3'],
  'Stories & writing': ['codex-apockalypsis', 'akashic-records'],
  Explore: ['home', 'atlas', 'start', 'quests', 'labyrinth', 'clearing', 'showcase', 'now', 'labs', 'infinity-engine'],
  'Help & support': ['documentation', 'principles', 'status', 'membership', 'support', 'ko-fi', 'patreon'],
};

export function directoryGroup(node: PublicSurfaceNode): DirectoryGroup {
  return DIRECTORY_GROUPS.find(group => GROUP_IDS[group].includes(node.id)) ?? 'Explore';
}

export const DIRECTORY_NODES = PUBLIC_SURFACE_NODES.filter(node => node.id !== 'home');

export function findDirectoryItems(query: string, group?: DirectoryGroup): readonly PublicSurfaceNode[] {
  const terms = query.toLocaleLowerCase().trim().split(/\s+/).filter(Boolean);
  return DIRECTORY_NODES.filter(node => {
    if (group && directoryGroup(node) !== group) return false;
    const text = `${node.title} ${node.shortTitle} ${node.summary} ${(node.keywords ?? []).join(' ')} ${node.action} ${node.id} ${directoryGroup(node)}`.toLocaleLowerCase();
    return terms.every(term => text.includes(term));
  });
}
