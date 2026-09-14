import { useState } from 'react';
import styles from './WorkConsole.module.css';
import { workApi, type ArbiterStatus } from '@/lib/work/client';

/**
 * The GPU is single-tenant on this host, so the Work tab has to say plainly which lane holds it
 * and let the operator move it. Taking the card stops the public Chat engine, so the control
 * names that consequence rather than reading as a harmless toggle.
 */
export function EngineControl({
  arbiter, onChange, onNotice,
}: {
  readonly arbiter: ArbiterStatus;
  readonly onChange: (next: ArbiterStatus) => void;
  readonly onNotice: (message: string) => void;
}): JSX.Element | null {
  const [busy, setBusy] = useState(false);
  if (arbiter.mode === 'off') return null;

  const act = async (take: boolean, force = false): Promise<void> => {
    setBusy(true);
    onNotice(take
      ? 'Handing the GPU to Work. The chat engine stops and the coding model loads — this takes a few minutes.'
      : 'Returning the GPU to Chat.');
    try {
      onChange(take ? await workApi.acquire(force) : await workApi.release());
      onNotice(take ? 'Work holds the GPU.' : 'Chat holds the GPU.');
    } catch (error) {
      onNotice(error instanceof Error ? error.message : 'The handover failed.');
    } finally {
      setBusy(false);
    }
  };

  const holder = arbiter.resident === 'work' ? 'Work' : arbiter.resident === 'chat' ? 'Chat' : 'nobody';
  const busyNow = busy || arbiter.handoverInFlight;

  return (
    <div className={styles.status} role="group" aria-label="GPU assignment">
      <span className={`${styles.dot} ${arbiter.resident === 'work' ? styles.dotOk : arbiter.resident === 'chat' ? styles.dotBusy : styles.dotBad}`} />
      GPU: {holder}
      {busyNow ? <span> · moving…</span> : arbiter.resident === 'work' ? (
        <button type="button" className={styles.newTask} style={{ width: 'auto', padding: '2px 8px' }} onClick={() => { void act(false); }}>
          give back to Chat
        </button>
      ) : (
        <button
          type="button"
          className={styles.newTask}
          style={{ width: 'auto', padding: '2px 8px' }}
          onClick={() => { void act(true); }}
          title={arbiter.chatBusy ? 'Chat is mid-job; the handover will wait for it to finish.' : 'Stops the chat engine and loads the coding model.'}
        >
          take for Work
        </button>
      )}
    </div>
  );
}
