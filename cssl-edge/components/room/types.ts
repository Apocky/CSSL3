export type RoomName = 'lobby' | 'owner';
export type RoomKind = 'utterance' | 'thought' | 'recall' | 'presence' | 'system';
export type EngineLane = 'local' | 'flagship';
export type RoomTool = 'image' | 'web';

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

/** A turn still being answered: its job is queued or streaming. */
export interface LiveTurnView {
  readonly job_id: string;
  readonly reply_to: number;
  readonly lane: EngineLane;
  readonly status: string;
  readonly text: string;
  readonly thinking: boolean;
}

export interface ViewerView {
  readonly owner: boolean;
  readonly kind: 'owner' | 'member' | 'guest';
  readonly signed_in: boolean;
  readonly author: string | null;
  readonly premium: boolean;
  readonly premium_ready: boolean;
}

export interface PendingAttachment {
  readonly key: string;
  readonly name: string;
  readonly mime: string;
  readonly id: string | null;
  readonly state: 'uploading' | 'ready' | 'failed';
  readonly error?: string;
}

export function authorLabel(author: string): string {
  if (author === 'apocrypha') return 'Apocrypha';
  if (author === 'apocky') return 'Apocky';
  if (author.startsWith('guest:')) return `guest ${author.slice(6, 10)}`;
  if (author.startsWith('member:')) return `member ${author.slice(7, 11)}`;
  return author;
}

export function modelLabel(meta: Record<string, unknown>): string | null {
  const model = typeof meta.model === 'string' ? meta.model : '';
  if (meta.engine_lane === 'flagship') return 'Apocrypha+';
  if (model !== '') return 'Local';
  return null;
}
