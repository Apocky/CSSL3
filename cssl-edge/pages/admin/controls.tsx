// /admin/controls · kill-switch + API keys + consent (operator-tier dangerous controls).
// Moved from /admin/apocrypha/controls per Apocky nav-cleanup.

import type { NextPage } from 'next';
import { useCallback, useState } from 'react';

import AdminLayout from '../../components/AdminLayout';
import { ApiKeyManager } from '../../components/apocrypha/ApiKeyManager';
import { authFetch } from '../../lib/browser-auth';

const NUCLEAR_TOKEN = 'I-UNDERSTAND-THIS-IS-IRREVERSIBLE';

const Controls: NextPage = () => {
  const [adminAuthorized, setAdminAuthorized] = useState(false);
  const [killBusy, setKillBusy] = useState(false);
  const [killResult, setKillResult] = useState<string | null>(null);
  const [killReason, setKillReason] = useState('manual via cockpit');
  const [confirmText, setConfirmText] = useState('');

  const triggerKillSwitch = useCallback(async () => {
    if (confirmText !== NUCLEAR_TOKEN) {
      setKillResult('refused : confirm-token mismatch');
      return;
    }
    setKillBusy(true);
    setKillResult(null);
    try {
      const r = await authFetch('/api/admin/apocrypha/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          text: `Invoke state.kill_switch with reason="${killReason}" `
              + `and confirm_token="${NUCLEAR_TOKEN}".`,
        }),
      });
      const json = await r.json();
      setKillResult(JSON.stringify(json, null, 2));
    } catch (err) {
      setKillResult('error : ' + (err instanceof Error ? err.message : String(err)));
    } finally {
      setKillBusy(false);
    }
  }, [confirmText, killReason]);

  return (
    <AdminLayout title="☢ Controls" onAdminCheck={(c) => setAdminAuthorized(c.authorized)}>
      {adminAuthorized ? (
        <div style={{
          display: 'flex',
          flexDirection: 'column',
          gap: '1.5rem',
          color: '#cdd6e4',
        }}>
          <section
            title="Sets state/kill-switch file ; halts swarm within ≤1ms (spec 09 I-05). Confirm-token required."
            style={{
              border: '1px solid #aa6060',
              borderRadius: 6,
              padding: '0.75rem 1rem',
              background: 'rgba(60, 20, 20, 0.2)',
            }}>
            <h2 style={{ marginTop: 0, fontSize: '1rem', color: '#ff8888' }}>
              ☢ Kill-switch (NUCLEAR · spec 09 I-05)
            </h2>
            <p style={{ fontSize: '0.85rem', color: '#cdd6e4' }}>
              Sets <code>state/kill-switch</code> within ≤1ms ; halts the swarm. Requires
              exact confirm-token : <code>{NUCLEAR_TOKEN}</code>.
            </p>
            <div style={{ display: 'flex', gap: '0.5rem', flexDirection: 'column' }}>
              <input
                value={killReason}
                onChange={(e) => setKillReason(e.target.value)}
                placeholder="reason"
                title="Recorded in the kill-switch file for the post-mortem"
                style={inputStyle}
              />
              <input
                value={confirmText}
                onChange={(e) => setConfirmText(e.target.value)}
                placeholder={`type ${NUCLEAR_TOKEN}`}
                title="Must match exactly ; case-sensitive ; intentional friction"
                style={inputStyle}
              />
              <button
                onClick={() => void triggerKillSwitch()}
                disabled={killBusy || confirmText !== NUCLEAR_TOKEN}
                title={confirmText === NUCLEAR_TOKEN
                  ? 'Will halt Apocrypha within 1ms'
                  : 'Type the confirm-token to enable'}
                style={{
                  ...btnStyle,
                  background: confirmText === NUCLEAR_TOKEN ? '#aa3030' : '#444',
                  color: '#fff',
                  cursor: confirmText === NUCLEAR_TOKEN ? 'pointer' : 'not-allowed',
                }}>
                {killBusy ? '…' : 'TRIGGER KILL-SWITCH'}
              </button>
            </div>
            {killResult && (
              <pre style={{
                marginTop: '0.6rem',
                padding: '0.5rem',
                background: '#0a0a10',
                border: '1px solid #2a2a3a',
                borderRadius: 3,
                fontSize: '0.75rem',
                whiteSpace: 'pre-wrap',
              }}>
                {killResult}
              </pre>
            )}
          </section>

          <section
            title="argon2id-hashed API keys ; principal-bound ; mint/list/revoke"
            style={{
              border: '1px solid #2a2a3a',
              borderRadius: 6,
              background: 'rgba(10, 10, 16, 0.4)',
            }}>
            <ApiKeyManager />
          </section>
        </div>
      ) : (
        <div style={{ padding: '2rem', color: '#a0a0b0' }}>
          <p>Controls require admin authentication.</p>
        </div>
      )}
    </AdminLayout>
  );
};

const inputStyle: React.CSSProperties = {
  background: '#0a0a10',
  color: '#cdd6e4',
  border: '1px solid #2a2a3a',
  padding: '0.4rem 0.6rem',
  borderRadius: 3,
  fontFamily: 'inherit',
  fontSize: '0.85rem',
};

const btnStyle: React.CSSProperties = {
  padding: '0.5rem 1rem',
  border: 0,
  borderRadius: 3,
  fontWeight: 600,
  fontFamily: 'inherit',
  fontSize: '0.9rem',
};

export default Controls;
