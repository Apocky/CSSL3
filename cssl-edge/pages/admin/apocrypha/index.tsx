import { FormEvent, useEffect, useState } from 'react';
import AdminLayout from '../../../components/AdminLayout';

type State = { ok: boolean; state: string; reason?: string; body?: Record<string, unknown> };

export default function ApocryphaCockpit() {
  const [status, setStatus] = useState<State | null>(null);
  const [message, setMessage] = useState('');
  const [reply, setReply] = useState<Record<string, unknown> | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = () => fetch('/api/admin/apocrypha/status').then((r) => r.json()).then(setStatus).catch(() => setStatus({ ok: false, state: 'degraded', reason: 'proxy_unreachable' }));
  useEffect(() => { refresh(); }, []);

  async function send(event: FormEvent) {
    event.preventDefault();
    if (!message.trim() || busy) return;
    setBusy(true); setReply(null);
    try {
      const response = await fetch('/api/admin/apocrypha/chat', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ message }) });
      setReply(await response.json());
    } catch { setReply({ ok: false, state: 'degraded', reason: 'proxy_unreachable' }); }
    finally { setBusy(false); }
  }

  return (
    <AdminLayout title="§ Apocrypha Cockpit">
      <p style={{ color: '#7a7a8c', fontSize: '0.82rem' }}>Authenticated organ console · upstream state is explicit · degraded means no hidden fallback.</p>
      <section style={{ marginTop: '1.5rem', padding: '1rem', border: '1px solid #29293a', borderRadius: 8, background: 'rgba(20,20,30,.55)' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div><div style={{ color: '#7a7a8c', fontSize: '.7rem', textTransform: 'uppercase', letterSpacing: '.15em' }}>Apocrypha runtime</div><div style={{ color: status?.state === 'live' ? '#34d399' : '#fbbf24', fontSize: '1.1rem', marginTop: 6 }}>{status?.state ?? 'checking'}</div></div>
          <button onClick={refresh} style={{ minHeight: 44, padding: '0 .9rem', background: '#161622', color: '#cdd6e4', border: '1px solid #39394b', borderRadius: 6 }}>Refresh</button>
        </div>
        {status?.reason && <p style={{ color: '#fbbf24', fontSize: '.82rem' }}>{status.reason}</p>}
        {status?.body && <pre style={{ overflowX: 'auto', color: '#9ca3af', fontSize: '.75rem' }}>{JSON.stringify(status.body, null, 2)}</pre>}
      </section>
      <section style={{ marginTop: '1rem', padding: '1rem', border: '1px solid #29293a', borderRadius: 8, background: 'rgba(20,20,30,.55)' }}>
        <h2 style={{ fontSize: '.7rem', textTransform: 'uppercase', letterSpacing: '.15em', color: '#7a7a8c' }}>Chat with the organism</h2>
        <form onSubmit={send}>
          <textarea aria-label="Message Apocrypha" value={message} onChange={(e) => setMessage(e.target.value)} rows={5} maxLength={20000} placeholder="Send a message…" style={{ width: '100%', marginTop: 8, padding: '.8rem', color: '#e6e6f0', background: '#0b0b12', border: '1px solid #39394b', borderRadius: 6, resize: 'vertical' }} />
          <button disabled={busy || !message.trim()} style={{ marginTop: 8, minHeight: 44, padding: '0 1rem', background: busy ? '#29293a' : '#6d5dfc', color: 'white', border: 0, borderRadius: 6 }}>{busy ? 'Thinking…' : 'Send'}</button>
        </form>
        {reply && <pre style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere', marginTop: 16, color: '#cdd6e4', fontSize: '.82rem' }}>{JSON.stringify(reply, null, 2)}</pre>}
      </section>
    </AdminLayout>
  );
}
