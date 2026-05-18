// ApiKeyManager — controls-face widget : mint + list + revoke API keys.
//
// Bound to /api/admin/apocrypha/keys (proxied to Apocrypha /api/v1/keys).
// The plaintext key is shown ONCE at mint-time then hidden ; user copies it then.

import { useCallback, useEffect, useState } from 'react';

import {
  type ApiKeyInfo,
  type CreateKeyResponse,
  createKey,
  listKeys,
  revokeKey,
} from '../../lib/apocrypha/client';

export function ApiKeyManager() {
  const [keys, setKeys] = useState<ApiKeyInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [newLabel, setNewLabel] = useState('');
  const [newPrincipal, setNewPrincipal] = useState('api:');
  const [justMinted, setJustMinted] = useState<CreateKeyResponse | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await listKeys();
      setKeys(list);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function handleCreate() {
    const label = newLabel.trim();
    const principal = newPrincipal.trim();
    if (label.length < 1 || !principal.includes(':')) return;
    setError(null);
    try {
      const minted = await createKey(label, principal);
      setJustMinted(minted);
      setNewLabel('');
      setNewPrincipal('api:');
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function handleRevoke(keyId: string) {
    if (!window.confirm(`Revoke key ${keyId.slice(0, 8)}…?`)) return;
    setError(null);
    try {
      await revokeKey(keyId);
      await refresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <div style={{
      display: 'flex',
      flexDirection: 'column',
      gap: '0.75rem',
      color: '#cdd6e4',
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
      padding: '0.6rem',
    }}>
      <div style={{ fontSize: '0.85rem' }}>§ API keys (argon2id-hashed at rest ; D025)</div>

      {error && (
        <div style={{ color: '#ff8888', fontSize: '0.85rem' }}>error : {error}</div>
      )}

      {justMinted && (
        <div style={{
          padding: '0.5rem 0.7rem',
          border: '1px solid #ffaa55',
          borderRadius: 6,
          background: 'rgba(60, 40, 10, 0.4)',
          display: 'flex',
          flexDirection: 'column',
          gap: '0.3rem',
        }}>
          <div style={{ fontSize: '0.85rem', color: '#ffaa55' }}>
            ⚠ Plaintext key shown ONCE — copy now :
          </div>
          <code style={{
            fontSize: '0.82rem',
            wordBreak: 'break-all',
            padding: '0.4rem',
            background: '#0a0a10',
            borderRadius: 3,
          }}>
            {justMinted.plaintext}
          </code>
          <div style={{ display: 'flex', gap: '0.5rem' }}>
            <button
              onClick={() => void navigator.clipboard.writeText(justMinted.plaintext)}
              style={btnStyle}
            >
              copy
            </button>
            <button onClick={() => setJustMinted(null)} style={btnStyle}>
              dismiss
            </button>
          </div>
        </div>
      )}

      <div style={{
        display: 'grid',
        gridTemplateColumns: '1fr 1fr auto',
        gap: '0.5rem',
        padding: '0.5rem',
        border: '1px solid #2a2a3a',
        borderRadius: 6,
        alignItems: 'center',
      }}>
        <input
          value={newLabel}
          onChange={(e) => setNewLabel(e.target.value)}
          placeholder="label"
          style={inputStyle}
        />
        <input
          value={newPrincipal}
          onChange={(e) => setNewPrincipal(e.target.value)}
          placeholder="principal (kind:id)"
          style={inputStyle}
        />
        <button onClick={() => void handleCreate()} style={btnStyle}>
          mint
        </button>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: '0.25rem' }}>
        {loading && <div style={{ color: '#7a7a8c', fontSize: '0.82rem' }}>loading …</div>}
        {!loading && keys.length === 0 && (
          <div style={{ color: '#7a7a8c', fontSize: '0.82rem' }}>no keys minted yet</div>
        )}
        {keys.map((k) => (
          <div key={k.key_id} style={{
            display: 'grid',
            gridTemplateColumns: '1fr 1.5fr 1fr auto',
            gap: '0.5rem',
            padding: '0.4rem 0.5rem',
            border: '1px solid #1a1a26',
            borderRadius: 4,
            fontSize: '0.78rem',
            alignItems: 'center',
            opacity: k.revoked ? 0.4 : 1,
          }}>
            <span>{k.label}</span>
            <span style={{ color: '#9aa8c0' }}>{k.principal}</span>
            <span style={{ color: '#7a7a8c', fontSize: '0.72rem' }}>
              {k.key_id.slice(0, 12)}…
            </span>
            {k.revoked ? (
              <span style={{ color: '#aa6060' }}>revoked</span>
            ) : (
              <button onClick={() => void handleRevoke(k.key_id)} style={btnStyleDanger}>
                revoke
              </button>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

const inputStyle: React.CSSProperties = {
  background: '#0a0a10',
  color: '#cdd6e4',
  border: '1px solid #2a2a3a',
  padding: '0.35rem 0.5rem',
  borderRadius: 3,
  fontFamily: 'inherit',
  fontSize: '0.85rem',
};

const btnStyle: React.CSSProperties = {
  padding: '0.35rem 0.7rem',
  background: 'linear-gradient(135deg, #ffaa55 0%, #c084fc 100%)',
  color: '#0a0a10',
  border: 0,
  borderRadius: 3,
  cursor: 'pointer',
  fontWeight: 600,
  fontFamily: 'inherit',
  fontSize: '0.78rem',
};

const btnStyleDanger: React.CSSProperties = {
  padding: '0.3rem 0.6rem',
  background: 'transparent',
  color: '#ff8888',
  border: '1px solid #aa6060',
  borderRadius: 3,
  cursor: 'pointer',
  fontFamily: 'inherit',
  fontSize: '0.75rem',
};
