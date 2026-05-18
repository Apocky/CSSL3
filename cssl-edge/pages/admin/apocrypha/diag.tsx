// apocky.com/admin/apocrypha/diag · Diagnostics face
// Per HANDOFF_v10 § TRACK-A A4 (FACE-2 = diag ; live tool-call timeline).

import type { NextPage } from 'next';
import { useState } from 'react';

import AdminLayout from '../../../components/AdminLayout';
import { ToolCallTimeline } from '../../../components/apocrypha/ToolCallTimeline';

const ApocryphaDiag: NextPage = () => {
  const [adminAuthorized, setAdminAuthorized] = useState(false);

  return (
    <AdminLayout title="Apocrypha · Diag" onAdminCheck={(c) => setAdminAuthorized(c.authorized)}>
      {adminAuthorized ? (
        <div style={{ height: 'calc(100dvh - 120px)' }}>
          <ToolCallTimeline />
        </div>
      ) : (
        <div style={{ padding: '2rem', color: '#a0a0b0' }}>
          <p>Diagnostics require admin authentication.</p>
        </div>
      )}
    </AdminLayout>
  );
};

export default ApocryphaDiag;
