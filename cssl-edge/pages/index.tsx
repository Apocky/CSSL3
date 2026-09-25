// cssl-edge · pages/index.tsx
// apocky.com's front door is the living room (owner decisions 2026-09-24 and 2026-09-25): the room
// is the one Apocrypha chat surface, and / renders it. The former hub lives at /hub. The OAuth
// callback still lands on / (the registered redirect), so it is consumed here before the room
// takes over; a `next` path is honoured, otherwise the visitor stays in the room.

import { useEffect, useState } from 'react';
import Room from '../components/room/Room';
import { useSiteSession } from '../components/hub/SiteSession';
import { consumeAuthCallbackFromLocation, readAuthCallbackParams } from '../lib/auth-callback';
import { normalizeAuthReturnPath } from '../lib/auth-return';

export default function Home(): JSX.Element {
  const [authNotice, setAuthNotice] = useState<string | null>(null);
  const { refresh } = useSiteSession();

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const callbackParams = readAuthCallbackParams(location.search, location.hash);
      if (!callbackParams.hasCallback) return;
      const returnTo = normalizeAuthReturnPath(new URLSearchParams(location.search).get('next'), '');
      setAuthNotice('Finishing your sign-in…');
      const callbackResult = await consumeAuthCallbackFromLocation();
      if (cancelled) return;
      if (callbackResult.ok) {
        if (returnTo) { location.replace(returnTo); return; }
        setAuthNotice(null);
        await refresh();
      } else {
        setAuthNotice(`Sign-in failed: ${callbackResult.reason ?? 'please try again'}`);
      }
    })();
    return () => { cancelled = true; };
  }, [refresh]);

  return <>
    {authNotice ? <p role="status" style={{ position: 'fixed', top: 'calc(8px + env(safe-area-inset-top, 0px))', left: '50%', transform: 'translateX(-50%)', zIndex: 20, background: '#111524', color: '#f3f3fa', border: '1px solid #a9b5ff40', borderRadius: 12, padding: '6px 14px', fontSize: 14 }}>{authNotice}</p> : null}
    <Room />
  </>;
}
