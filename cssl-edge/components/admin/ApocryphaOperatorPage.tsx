import { useRef, useState } from 'react';
import Link from 'next/link';
import AdminLayout from '../../components/AdminLayout';
import { authFetch } from '../../lib/browser-auth';

type Data = Record<string, unknown>;
type User = { id: string; email: string | null; created_at: string; last_sign_in_at: string | null };
type Session = { session_id: string; title: string; message_count: number };
function object(value: unknown): value is Data { return value !== null && typeof value === 'object' && !Array.isArray(value); }

function Inspector() {
  const [purpose, setPurpose] = useState('debugging');
  const [users, setUsers] = useState<User[]>([]);
  const [page, setPage] = useState(1);
  const [subject, setSubject] = useState('');
  const [sessions, setSessions] = useState<Session[]>([]);
  const [conversation, setConversation] = useState<Data | null>(null);
  const [aggregate, setAggregate] = useState<Data | null>(null);
  const [details, setDetails] = useState<Data | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [enabled, setEnabled] = useState(false);
  const [objective, setObjective] = useState('');
  const [paths, setPaths] = useState('');
  const [operation, setOperation] = useState('');
  const uncertain = useRef(false);
  const revision = useRef(0);

  async function request(path: string, body: Data): Promise<Data | null> {
    setBusy(true); setError('');
    try {
      const response = await authFetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body), cache: 'no-store' });
      const value: unknown = await response.json();
      if (!response.ok || !object(value)) throw new Error(object(value) && typeof value.code === 'string' ? value.code : 'OPERATOR_RESPONSE_UNAVAILABLE');
      return value;
    } catch (failure) { setError(failure instanceof Error ? failure.message : 'OPERATOR_RESPONSE_UNAVAILABLE'); return null; }
    finally { setBusy(false); }
  }
  async function loadUsers(next: number) {
    const value = await request('/api/admin/apocrypha/inspect', { action: 'users', purpose, page: next });
    if (value && object(value.result) && Array.isArray(value.result.users)) { setUsers(value.result.users as User[]); setPage(next); }
  }
  async function selectUser(id: string) {
    const epoch = ++revision.current;
    setSubject(id); setSessions([]); setConversation(null);
    const value = await request('/api/admin/apocrypha/inspect', { action: 'sessions', purpose, subject: id });
    if (epoch === revision.current && value && object(value.result) && Array.isArray(value.result.sessions)) setSessions(value.result.sessions as Session[]);
  }
  async function readSession(sessionId: string) {
    const epoch = ++revision.current; setConversation(null);
    const value = await request('/api/admin/apocrypha/inspect', { action: 'session', purpose, subject, session_id: sessionId });
    if (epoch === revision.current && value && object(value.result) && object(value.result.session)) setConversation(value.result.session);
  }
  async function control(action: 'status' | 'run' | 'read' | 'rollback') {
    let id = operation;
    if (action === 'run') {
      if (!id) { id = crypto.randomUUID(); setOperation(id); }
      uncertain.current = true;
    }
    const value = await request('/api/brain/control', { action,
      ...(action === 'status' ? {} : { operation_id: id }),
      ...(action === 'run' ? { objective: objective.trim(), allowed_paths: [...new Set(paths.split('\n').map(p => p.trim()).filter(Boolean))].sort() } : {}) });
    if (value) {
      setDetails(value);
      if (action === 'status') setEnabled(value.enabled === true);
      else uncertain.current = value.state === 'INDETERMINATE';
    }
  }
  const messages = conversation && Array.isArray(conversation.messages) ? conversation.messages.filter(object) : [];
  return <div className="inspection">
    <p>Inspect selected accounts for debugging and research, and control your connected desktop.</p>
    <p><Link href="/brain">Private Brain</Link> · <Link href="/admin/logs">Logs and traces</Link> · <Link href="/admin/mcp">Game MCP tools</Link></p>
    <section aria-labelledby="accounts-title">
      <h2 id="accounts-title">Account inspection</h2>
      <label>Purpose<select value={purpose} onChange={event => setPurpose(event.target.value)}><option value="debugging">Debugging</option><option value="research">Research and development</option></select></label>
      <button disabled={busy} onClick={() => void loadUsers(page)}>Load accounts</button>
      <button disabled={busy} onClick={() => void request('/api/admin/apocrypha/inspect', { action: 'aggregate', purpose }).then(value => {
        if (value && object(value.result)) setAggregate(value.result);
      })}>Load aggregate data</button>
      <p>Access is recorded. Account data is visible here to authorized operators.</p>
      {aggregate && <details open><summary>Aggregate account and operational data</summary><pre>{JSON.stringify(aggregate, null, 2)}</pre></details>}
      {users.length > 0 && <><ul className="choices">{users.map(user => <li key={user.id}><button disabled={busy} onClick={() => void selectUser(user.id)} aria-pressed={subject === user.id}>{user.email ?? user.id}</button><small>Joined {user.created_at.slice(0, 10)} · Last sign-in {user.last_sign_in_at?.slice(0, 10) ?? 'none'}</small></li>)}</ul>
        <button disabled={busy || page === 1} onClick={() => void loadUsers(page - 1)}>Previous accounts</button><span> Page {page} </span><button disabled={busy || users.length < 50} onClick={() => void loadUsers(page + 1)}>Next accounts</button></>}
      <label>Selected account ID<input value={subject} onChange={event => { ++revision.current; setSubject(event.target.value); setSessions([]); setConversation(null); }} spellCheck={false} /></label>
      <button disabled={busy || !subject} onClick={() => void selectUser(subject)}>Read conversation list</button>
      <ul className="choices">{sessions.map(session => <li key={session.session_id}><button disabled={busy} onClick={() => void readSession(session.session_id)}>{session.title || 'Conversation'} · {session.message_count} messages</button></li>)}</ul>
      {conversation && <article><h3>{String(conversation.title ?? 'Conversation')}</h3>{messages.map((message, index) => <div className="message" key={index}><strong>{String(message.role)}</strong><p>{String(message.content)}</p></div>)}{conversation.events_truncated === true && <p>Showing the most recent messages.</p>}</article>}
    </section>
    <section aria-labelledby="control-title">
      <h2 id="control-title">Desktop actions</h2>
      <button disabled={busy} onClick={() => void control('status')}>Check desktop capabilities</button>
      <p>{enabled ? 'The desktop reports that code actions are enabled.' : 'Check the desktop before running an action.'}</p>
      <label>Task<textarea value={objective} disabled={uncertain.current} onChange={event => setObjective(event.target.value)} placeholder="Describe the change to make in the desktop workspace." /></label>
      <label>Files the task may change<textarea value={paths} disabled={uncertain.current} onChange={event => setPaths(event.target.value)} placeholder="One path per line, relative to the desktop workspace." /></label>
      <label>Operation ID<input value={operation} onChange={event => setOperation(event.target.value)} spellCheck={false} /></label>
      <div className="actions"><button disabled={busy || !enabled || !objective.trim() || !paths.trim() || uncertain.current} onClick={() => void control('run')}>Run task</button>
        <button disabled={busy || !operation} onClick={() => void control('read')}>Read result</button>
        <button disabled={busy || !operation || details?.state !== 'PROMOTED'} onClick={() => void control('rollback')}>Undo this task</button>
        <button disabled={busy || uncertain.current} onClick={() => { setOperation(''); setDetails(null); }}>New task</button></div>
      {uncertain.current && <p>The result is not confirmed. Keep this operation ID and read its result before starting another task.</p>}
      {details && <details open><summary>Desktop result and diagnostics</summary><pre>{JSON.stringify(details, null, 2)}</pre></details>}
    </section>
    <p aria-live="polite">{busy ? 'Working…' : ''}</p>{error && <p role="alert">{error}</p>}
    <style jsx>{`
      .inspection{max-width:980px;margin:0 auto}section{border:1px solid #37314a;border-radius:16px;padding:20px;margin:24px 0;background:#11101b}
      h2{font-size:1.25rem}label{display:block;margin:16px 0 8px}input,textarea,select{display:block;width:100%;margin-top:7px;padding:12px;background:#080912;color:#f2efff;border:1px solid #57516f;border-radius:8px}
      textarea{min-height:100px;resize:vertical}button{padding:11px 14px;margin:5px 6px 5px 0;border:1px solid #686080;border-radius:9px;background:#24203c;color:#f2efff;cursor:pointer}button:disabled{opacity:.5;cursor:default}button[aria-pressed=true]{border-color:#66dcf7}
      .choices{list-style:none;padding:0}.choices li{margin:10px 0}.choices small{display:block;color:#bcb6cf}.message{padding:14px 0;border-bottom:1px solid #37314a}.message p{white-space:pre-wrap;overflow-wrap:anywhere}pre{max-height:460px;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere}a{color:#83dff2}p{line-height:1.6}.actions{display:flex;flex-wrap:wrap}
    `}</style>
  </div>;
}
export default function ApocryphaOperatorPage() { return <AdminLayout title="Apocrypha operations"><Inspector /></AdminLayout>; }
