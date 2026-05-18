// apocky.com/admin/apocrypha/chat · Apocrypha chat face
// Per HANDOFF_v10 § TRACK-A A4. Wraps ChatThread in AdminLayout w/ admin gate.

import type { NextPage } from 'next';
import { useState } from 'react';

import AdminLayout from '../../../components/AdminLayout';
import { ChatThread } from '../../../components/apocrypha/ChatThread';

const ApocryphaChat: NextPage = () => {
  const [adminAuthorized, setAdminAuthorized] = useState(false);

  return (
    <AdminLayout title="Apocrypha · Chat" onAdminCheck={(c) => setAdminAuthorized(c.authorized)}>
      {adminAuthorized ? (
        <div style={{ height: 'calc(100dvh - 120px)' }}>
          <ChatThread />
        </div>
      ) : (
        <div style={{ padding: '2rem', color: '#a0a0b0' }}>
          <p>Chat requires admin authentication.</p>
        </div>
      )}
    </AdminLayout>
  );
};

export default ApocryphaChat;
