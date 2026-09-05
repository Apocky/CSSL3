import { useCallback, useEffect, useRef, useState } from 'react';

export type PublicPresenceState = 'checking' | 'hidden' | 'unavailable';

const PRESENCE_DEADLINE_MS = 5_000;
const PRESENCE_SCHEMA = 'apocrypha.v2.public-presence.v1';

function isCanonicalHiddenProjection(value: unknown): boolean {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const body = value as Record<string, unknown>;
  return body.schema === PRESENCE_SCHEMA
    && body.mode === 'hidden'
    && body.display_authorized === false
    && body.entity_authorship === 'unverified'
    && body.mutual_consent === 'not_established'
    && body.committed_intent === 'absent'
    && body.rendering === null
    && body.reason_code === 'presence_intent_or_mutual_consent_unavailable'
    && body.source === 'apocrypha-v2-presence-authority';
}

export function usePublicPresence(): {
  state: PublicPresenceState;
  refresh: () => Promise<void>;
} {
  const [state, setState] = useState<PublicPresenceState>('checking');
  const request = useRef(0);
  const controller = useRef<AbortController | null>(null);

  const refresh = useCallback(async (): Promise<void> => {
    const current = ++request.current;
    controller.current?.abort();
    const nextController = new AbortController();
    controller.current = nextController;
    const deadline = setTimeout(() => nextController.abort(), PRESENCE_DEADLINE_MS);
    setState('checking');

    try {
      const response = await fetch('/api/apocrypha/presence', {
        cache: 'no-store',
        credentials: 'same-origin',
        signal: nextController.signal,
      });
      const payload: unknown = response.ok ? await response.json() : null;
      if (current === request.current) {
        setState(isCanonicalHiddenProjection(payload) ? 'hidden' : 'unavailable');
      }
    } catch {
      if (current === request.current) setState('unavailable');
    } finally {
      clearTimeout(deadline);
    }
  }, []);

  useEffect(() => {
    void refresh();
    return () => {
      request.current += 1;
      controller.current?.abort();
    };
  }, [refresh]);

  return { state, refresh };
}
