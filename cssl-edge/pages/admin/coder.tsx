// /admin/coder · pending Coder-runtime AST-edits awaiting approval
// Approve/revert from phone · 30-second-revert-window honored

import type { NextPage } from 'next';
import { useEffect, useState } from 'react';
import AdminLayout from '../../components/AdminLayout';

interface PendingEdit {
  edit_id: string;
  proposed_at: string;
  target_file: string;
  diff_summary: string;
  cap_required: string;
  status: 'pending' | 'approved' | 'reverted';
}

const Coder: NextPage = () => {
  const [edits, setEdits] = useState<PendingEdit[] | null>(null);
  const [stub, setStub] = useState<boolean>(false);

  useEffect(() => {
    fetch('/api/admin/coder/pending')
      .then((r) => r.json())
      .then((j) => {
        setEdits(j.edits ?? []);
        setStub(!!j.stub);
      })
      .catch(() => {
        setEdits([]);
        setStub(true);
      });
  }, []);

  return (
    <AdminLayout title="W! Pending Coder Edits">
      <p style={{ color: '#7a7a8c', fontSize: '0.82rem', marginTop: 0, marginBottom: '1.5rem' }}>
        Approve or revert AST-edits proposed by Mycelium-Desktop or LoA.exe Coder · 30-second-revert-window honored ·
        sovereign-cap-required for substrate-edits
      </p>

      {stub && (
        <div
          style={{
            padding: '1rem 1.25rem',
            background: 'rgba(251, 191, 36, 0.1)',
            border: '1px solid rgba(251, 191, 36, 0.4)',
            borderRadius: 6,
            marginBottom: '1.5rem',
            fontSize: '0.85rem',
            color: '#fbbf24',
          }}
        >
          <strong>⚠ stub-mode</strong>
          <p style={{ margin: '0.4rem 0 0' }}>
            Pending Coder-edits queue lives in cssl-host-coder-runtime (W8-F1 ✓) · admin-bridge to it activates
            when Apocky-Hub Supabase is online + desktop runs LoA.exe or Mycelium.
          </p>
        </div>
      )}

      {!stub && (!edits || edits.length === 0) && (
        <div
          style={{
            padding: '2rem 1rem',
            textAlign: 'center',
            color: '#7a7a8c',
            background: 'rgba(20, 20, 30, 0.4)',
            border: '1px solid #1f1f2a',
            borderRadius: 6,
          }}
        >
          § no pending edits · queue empty
        </div>
      )}

      {edits && edits.length > 0 && (
        <div style={{ display: 'grid', gap: '0.6rem' }}>
          {edits.map((e) => (
            <article
              key={e.edit_id}
              style={{
                padding: '1rem 1.1rem',
                background: 'rgba(20, 20, 30, 0.5)',
                border: '1px solid #1f1f2a',
                borderRadius: 6,
              }}
            >
              <code style={{ fontSize: '0.78rem', color: '#fbbf24', marginBottom: 4, display: 'block' }}>
                {e.target_file}
              </code>
              <p style={{ fontSize: '0.85rem', color: '#cdd6e4', margin: '0.4rem 0' }}>{e.diff_summary}</p>
              <div style={{ fontSize: '0.7rem', color: '#7a7a8c', marginBottom: 8 }}>
                cap : {e.cap_required} · proposed : {new Date(e.proposed_at).toLocaleString()}
              </div>
              <div style={{ display: 'flex', gap: '0.4rem' }}>
                <button
                  type="button"
                  style={{
                    flex: 1,
                    padding: '0.6rem',
                    background: 'rgba(52, 211, 153, 0.15)',
                    border: '1px solid rgba(52, 211, 153, 0.4)',
                    color: '#34d399',
                    borderRadius: 4,
                    fontSize: '0.85rem',
                    fontFamily: 'inherit',
                    cursor: 'pointer',
                    minHeight: 44,
                  }}
                >
                  ✓ approve
                </button>
                <button
                  type="button"
                  style={{
                    flex: 1,
                    padding: '0.6rem',
                    background: 'rgba(248, 113, 113, 0.1)',
                    border: '1px solid rgba(248, 113, 113, 0.3)',
                    color: '#f87171',
                    borderRadius: 4,
                    fontSize: '0.85rem',
                    fontFamily: 'inherit',
                    cursor: 'pointer',
                    minHeight: 44,
                  }}
                >
                  ✗ revert
                </button>
              </div>
            </article>
          ))}
        </div>
      )}
    </AdminLayout>
  );
};

export default Coder;
