import { useCallback, useEffect, useState } from 'react';
import { ipc } from './lib/ipc.ts';
import { EMPTY_VIEW, type View } from './lib/view.ts';
import { SignIn } from './pages/SignIn.tsx';
import { Chat } from './pages/Chat.tsx';

export function App() {
  const [view, setView] = useState<View>(EMPTY_VIEW);
  const [busy, setBusy] = useState(true);

  // One flow at a time. The Rust side serialises anyway; refusing here keeps
  // the window from queueing a second request the person did not intend.
  const act = useCallback(
    async (job: () => Promise<View>) => {
      if (busy) return;
      setBusy(true);
      try {
        setView(await job());
      } catch (error) {
        setView((current) => ({
          ...current,
          notice: error instanceof Error ? error.message : String(error),
        }));
      } finally {
        setBusy(false);
      }
    },
    [busy],
  );

  useEffect(() => {
    let cancelled = false;
    setBusy(true);
    ipc
      .bootstrap()
      .then((next) => {
        if (!cancelled) setView(next);
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setView({
            ...EMPTY_VIEW,
            notice: error instanceof Error ? error.message : String(error),
          });
        }
      })
      .finally(() => {
        if (!cancelled) setBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="app">
      {view.signed_in ? (
        <Chat view={view} busy={busy} act={act} />
      ) : (
        <SignIn view={view} busy={busy} act={act} />
      )}
    </div>
  );
}

export type Act = (job: () => Promise<View>) => Promise<void>;
