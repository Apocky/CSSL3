import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { ipc } from './lib/ipc.ts';
import { EMPTY_VIEW, TURN_DELTA_EVENT, type Live, type View } from './lib/view.ts';
import { SignIn } from './pages/SignIn.tsx';
import { Chat } from './pages/Chat.tsx';

export function App() {
  const [view, setView] = useState<View>(EMPTY_VIEW);
  const [busy, setBusy] = useState(true);
  const [live, setLive] = useState<Live | null>(null);
  // Fragments can arrive faster than React re-renders, so they are appended to
  // a buffer and flushed on a frame; nothing is dropped and nothing thrashes.
  const buffer = useRef('');
  const frame = useRef<number | null>(null);

  useEffect(() => {
    const subscription = listen<string>(TURN_DELTA_EVENT, (event) => {
      buffer.current += event.payload;
      if (frame.current !== null) return;
      frame.current = requestAnimationFrame(() => {
        frame.current = null;
        const pending = buffer.current;
        buffer.current = '';
        if (!pending) return;
        setLive((current) => (current ? { ...current, text: current.text + pending } : current));
      });
    });
    return () => {
      void subscription.then((stop) => stop());
      if (frame.current !== null) cancelAnimationFrame(frame.current);
    };
  }, []);

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

  const sendMessage = useCallback(
    async (text: string): Promise<boolean> => {
      if (busy) return false;
      buffer.current = '';
      setLive({ text: '', startedAt: Date.now() });
      setBusy(true);
      let accepted = true;
      try {
        const next = await ipc.send(text);
        // The controller hands the text back when the service refused it, so
        // the person does not lose what they wrote.
        accepted = next.messages.some((message) => message.role === 'user' && message.content === text.trim());
        setView(next);
      } catch (error) {
        accepted = false;
        setView((current) => ({
          ...current,
          notice: error instanceof Error ? error.message : String(error),
        }));
      } finally {
        setBusy(false);
        setLive(null);
      }
      return accepted;
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
        <Chat view={view} busy={busy} live={live} act={act} sendMessage={sendMessage} />
      ) : (
        <SignIn view={view} busy={busy} act={act} />
      )}
    </div>
  );
}

export type Act = (job: () => Promise<View>) => Promise<void>;
export type SendMessage = (text: string) => Promise<boolean>;
