import { useState } from 'react';
import styles from './WorkConsole.module.css';
import { workApi, type ArbiterStatus } from '@/lib/work/client';

/**
 * One engine serves both lanes, so this is a MODEL switch, not a handover.
 *
 * Chat does not go down when Work is selected -- it is answered by the coding model instead, on
 * the same endpoint. The only thing that changes is which model is loaded, and the only cost is
 * the ~75 s swap plus a change of voice on the chat side. Nothing switches back on its own.
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
      ? 'Loading the coding model. Chat keeps working and will be answered by it too. About 75 seconds.'
      : 'Loading the chat model back. The Work lane will use it until you switch again.');
    try {
      onChange(take ? await workApi.acquire(force) : await workApi.release());
      onNotice(take ? 'Coding model loaded — serving both lanes.' : 'Chat model loaded — serving both lanes.');
    } catch (error) {
      onNotice(error instanceof Error ? error.message : 'The handover failed.');
    } finally {
      setBusy(false);
    }
  };

  const holder = arbiter.resident === 'work' ? 'coder' : arbiter.resident === 'chat' ? 'chat' : 'none';
  const busyNow = busy || arbiter.handoverInFlight;

  return (
    <div className={styles.status} role="group" aria-label="Loaded model">
      <span className={`${styles.dot} ${arbiter.resident === 'work' ? styles.dotOk : arbiter.resident === 'chat' ? styles.dotBusy : styles.dotBad}`} />
      model: {holder}
      {busyNow ? <span> · moving…</span> : arbiter.resident === 'work' ? (
        <button type="button" className={styles.newTask} style={{ width: 'auto', padding: '2px 8px' }} onClick={() => { void act(false); }}>
          use chat model
        </button>
      ) : (
        <button
          type="button"
          className={styles.newTask}
          style={{ width: 'auto', padding: '2px 8px' }}
          onClick={() => { void act(true); }}
          title={arbiter.chatBusy ? 'Chat is mid-job; the swap waits for it to finish.' : 'Swaps the loaded model. Chat stays up and is answered by the coder too.'}
        >
          use coding model
        </button>
      )}
    </div>
  );
}
