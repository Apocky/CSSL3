import { DEFAULT_PRESET, presetById, type PresetId } from './sampling';

// Per-device reading and composing preferences for the Apocrypha room.
//
// These are display choices and nothing more. Nothing here changes what the model is, what it may
// do, what it can reach, or who you are — that is decided on the server and is not negotiable from
// a browser. The settings panel says so, and this module is deliberately incapable of carrying
// anything else: the shape below is closed, and unknown keys read back as defaults.
//
// Stored per device rather than per account on purpose. "Text is too small" is a fact about the
// screen in front of you, not about your identity, and syncing it would carry a phone's choice onto
// a desktop.

const KEY = 'apx.chat.prefs.v1';

export type TextSize = 'normal' | 'large';

export interface ChatPrefs {
  /** Enter sends. Off means Enter starts a new line and Ctrl/Cmd+Enter sends. */
  readonly enterSends: boolean;
  readonly textSize: TextSize;
  /** Owner lanes only: show the tool and run trace beneath each answer. */
  readonly showTrace: boolean;
  /** Honour reduced motion even where the OS does not report it. */
  readonly calmMotion: boolean;
  /**
   * Temperament. ONE engine answers everything now, so this is what makes a code question behave
   * differently from a conversation -- not a different model. Persisted like the rest: a choice you
   * made once should not reset every turn.
   */
  readonly preset: PresetId;
}

export const DEFAULT_CHAT_PREFS: ChatPrefs = {
  enterSends: true,
  textSize: 'normal',
  showTrace: false,
  calmMotion: false,
  preset: DEFAULT_PRESET,
};

function coerce(raw: unknown): ChatPrefs {
  if (!raw || typeof raw !== 'object') return DEFAULT_CHAT_PREFS;
  const value = raw as Record<string, unknown>;
  return {
    enterSends: typeof value.enterSends === 'boolean' ? value.enterSends : DEFAULT_CHAT_PREFS.enterSends,
    textSize: value.textSize === 'large' ? 'large' : 'normal',
    showTrace: typeof value.showTrace === 'boolean' ? value.showTrace : DEFAULT_CHAT_PREFS.showTrace,
    calmMotion: typeof value.calmMotion === 'boolean' ? value.calmMotion : DEFAULT_CHAT_PREFS.calmMotion,
    // An unknown or absent preset is the default, never an error -- stored prefs predate this field.
    preset: presetById(typeof value.preset === 'string' ? value.preset : null).id,
  };
}

// The default for enter-sends is not universal: on a hardware keyboard Enter-to-send is the
// expected chat idiom, but on a soft keyboard the Return key is how you start a paragraph, and
// defaulting to send there posts half a thought. Pointer type is the honest signal -- a phone
// reports coarse, a laptop fine -- and it is only ever a DEFAULT: a stored choice always wins, and
// the setting stays in the panel either way.
function defaultPrefs(): ChatPrefs {
  try {
    const coarse = typeof window !== 'undefined'
      && typeof window.matchMedia === 'function'
      && window.matchMedia('(pointer: coarse)').matches;
    return coarse ? { ...DEFAULT_CHAT_PREFS, enterSends: false } : DEFAULT_CHAT_PREFS;
  } catch {
    return DEFAULT_CHAT_PREFS;
  }
}

export function readChatPrefs(storage?: Pick<Storage, 'getItem'>): ChatPrefs {
  try {
    const store = storage ?? window.localStorage;
    const raw = store.getItem(KEY);
    return raw ? coerce(JSON.parse(raw)) : defaultPrefs();
  } catch {
    // Private windows, blocked storage, corrupt JSON. A preference that cannot be read is a
    // default, never an error the reader has to deal with.
    return DEFAULT_CHAT_PREFS;
  }
}

export function writeChatPrefs(prefs: ChatPrefs, storage?: Pick<Storage, 'setItem'>): void {
  try {
    const store = storage ?? window.localStorage;
    store.setItem(KEY, JSON.stringify(prefs));
  } catch { /* storage unavailable; the choice still applies for this page view */ }
}

/** Does this keypress mean "send"? */
export function isSendKey(
  event: Pick<KeyboardEvent, 'key' | 'shiftKey' | 'ctrlKey' | 'metaKey'>,
  prefs: ChatPrefs,
): boolean {
  if (event.key !== 'Enter') return false;
  // Ctrl/Cmd+Enter sends under BOTH settings. Someone who has turned Enter off still expects the
  // universal shortcut to work, and someone who has left it on loses nothing by it also working.
  if (event.ctrlKey || event.metaKey) return true;
  if (!prefs.enterSends) return false;
  return !event.shiftKey;
}

/** Plain-language transcript, for copying a conversation out. */
export function transcriptOf(
  messages: ReadonlyArray<{ role: 'user' | 'apocrypha'; text: string; at: Date }>,
): string {
  return messages
    .map((message) => `${message.role === 'user' ? 'You' : 'Apocrypha'}:\n${message.text}`)
    .join('\n\n');
}
