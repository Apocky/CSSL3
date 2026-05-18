// apocky.com/admin/apocrypha · primary chat surface (formerly the cockpit landing).
//
// Per the polish-pass : /admin/apocrypha now defaults to the modern chat experience.
// The OrganRack + StatusBar + NavRail cockpit shell moved to /admin/apocrypha/cockpit
// for users who want the per-organ telemetry view.
//
// Links to the other three faces (cockpit / diag / controls) live in the header bar.

import type { NextPage } from 'next';
import Link from 'next/link';
import { useState } from 'react';

import AdminLayout from '../../../components/AdminLayout';
import { ChatThread } from '../../../components/apocrypha/ChatThread';

const FACE_LINKS: ReadonlyArray<{ href: string; label: string }> = [
  { href: '/admin/apocrypha/cockpit', label: '§ Cockpit' },
  { href: '/admin/apocrypha/diag', label: '⌬ Diag' },
  { href: '/admin/apocrypha/controls', label: '☢ Controls' },
];

const Apocrypha: NextPage = () => {
  const [adminAuthorized, setAdminAuthorized] = useState(false);

  return (
    <AdminLayout
      title="Apocrypha"
      onAdminCheck={(check) => setAdminAuthorized(check.authorized)}
    >
      {adminAuthorized ? (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 0 }}>
          <nav style={{
            display: 'flex',
            gap: '0.4rem',
            padding: '0 0 0.5rem 0',
            fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
          }}>
            {FACE_LINKS.map((f) => (
              <Link key={f.href} href={f.href} style={{
                padding: '0.3rem 0.6rem',
                border: '1px solid #2a2a3a',
                borderRadius: 4,
                color: '#9aa0a6',
                fontSize: '0.78rem',
                textDecoration: 'none',
                background: 'rgba(15, 15, 22, 0.5)',
              }}>
                {f.label}
              </Link>
            ))}
          </nav>
          <div style={{ height: 'calc(100dvh - 160px)', minHeight: 480 }}>
            <ChatThread />
          </div>
        </div>
      ) : (
        <div style={{ padding: '2rem', color: '#a0a0b0' }}>
          <p>Apocrypha requires admin authentication.</p>
        </div>
      )}
    </AdminLayout>
  );
};

export default Apocrypha;
