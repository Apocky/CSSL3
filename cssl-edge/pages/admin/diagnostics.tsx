// /admin/diagnostics · live tool-call timeline + recent conversations.
//
// Combines the previous /admin/apocrypha/diag (ToolCallTimeline polling) with a
// new "Recent Conversations" pane sourced from /api/v1/conversations. Mobile-
// friendly bottom-nav target per AdminLayout.

import type { NextPage } from 'next';
import { useEffect, useState } from 'react';

import AdminLayout from '../../components/AdminLayout';
import { ToolCallTimeline } from '../../components/apocrypha/ToolCallTimeline';
import { authFetch } from '../../lib/browser-auth';

interface ConvRow {
  id: number;
  title: string | null;
  user_principal: string;
  last_active_iso: string;
}

const Diagnostics: NextPage = () => {
  const [adminAuthorized, setAdminAuthorized] = useState(false);
  const [convs, setConvs] = useState<ConvRow[]>([]);

  useEffect(() => {
    if (!adminAuthorized) return;
    const fetchConvs = async () => {
      try {
        const r = await authFetch('/api/admin/apocrypha/conversations');
        const j = await r.json();
        setConvs(j.data?.conversations ?? []);
      } catch {
        // silent ; left column shows empty state
      }
    };
    void fetchConvs();
    const t = setInterval(() => void fetchConvs(), 5000);
    return () => clearInterval(t);
  }, [adminAuthorized]);

  return (
    <AdminLayout title="⌬ Diagnostics" onAdminCheck={(c) => setAdminAuthorized(c.authorized)}>
      {adminAuthorized ? (
        <div style={{
          display: 'grid',
          gridTemplateColumns: 'minmax(0, 1fr) 280px',
          gap: '1rem',
          height: 'calc(100dvh - 140px)',
          minHeight: 480,
        }}>
          <section style={{
            border: '1px solid #1f1f2a',
            borderRadius: 6,
            background: 'rgba(10, 10, 16, 0.5)',
            overflow: 'hidden',
          }}>
            <ToolCallTimeline />
          </section>

          <aside title="Recent conversations · click to open in chat" style={{
            border: '1px solid #1f1f2a',
            borderRadius: 6,
            background: 'rgba(15, 15, 22, 0.5)',
            padding: '0.6rem',
            overflowY: 'auto',
          }}>
            <div style={{
              fontSize: '0.78rem',
              color: '#a78bfa',
              marginBottom: '0.6rem',
              textTransform: 'uppercase',
              letterSpacing: '0.1em',
            }}>
              § Recent conversations
            </div>
            {convs.length === 0 && (
              <div style={{ fontSize: '0.78rem', color: '#7a7a8c' }}>
                no conversations yet · use /admin/chat to start one
              </div>
            )}
            {convs.map((c) => (
              <a key={c.id}
                 href={`/admin/chat?conv=${c.id}`}
                 title={`Conv ${c.id} · ${c.user_principal} · last active ${c.last_active_iso}`}
                 style={{
                   display: 'block',
                   padding: '0.45rem 0.55rem',
                   marginBottom: 2,
                   borderRadius: 4,
                   background: 'rgba(20, 20, 30, 0.5)',
                   border: '1px solid #1a1a26',
                   color: '#cdd6e4',
                   fontSize: '0.78rem',
                 }}>
                <div style={{ fontWeight: 500 }}>
                  {c.title || `Conversation #${c.id}`}
                </div>
                <div style={{ fontSize: '0.7rem', color: '#5a5a6a', marginTop: 2 }}>
                  {new Date(c.last_active_iso).toLocaleString()}
                </div>
              </a>
            ))}
          </aside>
        </div>
      ) : (
        <div style={{ padding: '2rem', color: '#a0a0b0' }}>
          <p>Diagnostics require admin authentication.</p>
        </div>
      )}
    </AdminLayout>
  );
};

export default Diagnostics;
