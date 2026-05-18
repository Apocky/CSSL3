// apocky.com/admin/apocrypha/cockpit · per-organ telemetry view (moved from /admin/apocrypha)
//
// 4-zone cockpit shell : StatusBar + NavRail + organ-pane + OrganRack. The
// per-organ panes were Phase-2 placeholders ; the chat-face landing moved to
// /admin/apocrypha. This route preserves the cockpit view for users who want it.

import type { NextPage } from 'next';
import { useState } from 'react';

import AdminLayout from '../../../components/AdminLayout';
import { CockpitShell } from '../../../components/apocrypha/CockpitShell';

const ApocryphaCockpit: NextPage = () => {
  const [adminAuthorized, setAdminAuthorized] = useState(false);

  return (
    <AdminLayout
      title="Apocrypha · Cockpit"
      onAdminCheck={(check) => setAdminAuthorized(check.authorized)}
    >
      {adminAuthorized ? (
        <CockpitShell />
      ) : (
        <div style={{ padding: '2rem', color: '#a0a0b0' }}>
          <p>Cockpit requires admin authentication.</p>
        </div>
      )}
    </AdminLayout>
  );
};

export default ApocryphaCockpit;
