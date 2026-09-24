export type RoomName = 'lobby' | 'owner';
export type RoomKind = 'utterance' | 'thought' | 'recall' | 'presence' | 'system';

export interface RoomEventView {
  readonly id: number;
  readonly room: RoomName;
  readonly author: string;
  readonly kind: RoomKind;
  readonly body: string;
  readonly meta: Record<string, unknown>;
  readonly created_at: string;
  /** Set on an optimistic row the server has not confirmed yet. */
  readonly pending?: boolean;
}

export interface PresenceView {
  readonly state: string;
  readonly at: string;
}

export function authorLabel(author: string): string {
  if (author === 'apocrypha') return 'Apocrypha';
  if (author === 'apocky') return 'Apocky';
  if (author.startsWith('guest:')) return `guest ${author.slice(6, 10)}`;
  return author;
}
