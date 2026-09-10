import {
  parseSpellbook,
  serializeSpellbook,
  type SpellbookEntry,
} from './spellcraft';

export const LOCAL_SPELLBOOK_KEY = 'apocky.symbolic-spellbook.v1';

export type LocalSpellbookResult =
  | { readonly status: 'ready'; readonly entries: readonly SpellbookEntry[] }
  | { readonly status: 'unavailable' | 'rejected'; readonly code: 'SPELLBOOK_LOCAL_UNAVAILABLE' | 'SPELLBOOK_SCHEMA_REJECTED'; readonly message: string };

type ReadableStorage = Pick<Storage, 'getItem'>;
type WritableStorage = Pick<Storage, 'setItem'>;

export function readLocalSpellbook(storage: ReadableStorage): LocalSpellbookResult {
  try {
    const serialized = storage.getItem(LOCAL_SPELLBOOK_KEY);
    if (!serialized) return { status: 'ready', entries: [] };
    const parsed = parseSpellbook(serialized);
    if (!parsed.valid || !parsed.payload) {
      return {
        status: 'rejected',
        code: 'SPELLBOOK_SCHEMA_REJECTED',
        message: 'The saved collection failed its version or integrity checks. It was not loaded or changed.',
      };
    }
    return { status: 'ready', entries: parsed.payload.entries };
  } catch {
    return {
      status: 'unavailable',
      code: 'SPELLBOOK_LOCAL_UNAVAILABLE',
      message: 'Browser storage is unavailable. Exported files and the symbolic tools remain usable.',
    };
  }
}
export function writeLocalSpellbook(storage: WritableStorage, entries: readonly SpellbookEntry[]): LocalSpellbookResult {
  try {
    storage.setItem(LOCAL_SPELLBOOK_KEY, serializeSpellbook(entries));
    return { status: 'ready', entries };
  } catch {
    return {
      status: 'unavailable',
      code: 'SPELLBOOK_LOCAL_UNAVAILABLE',
      message: 'The collection could not be written. Nothing else was changed; export or free browser storage and try again.',
    };
  }
}

export function addLocalSpellbookEntry(storage: ReadableStorage & WritableStorage, entry: SpellbookEntry): LocalSpellbookResult {
  const current = readLocalSpellbook(storage);
  if (current.status !== 'ready') return current;
  const withoutDuplicate = current.entries.filter((candidate) => candidate.entryId !== entry.entryId);
  return writeLocalSpellbook(storage, [entry, ...withoutDuplicate]);
}
