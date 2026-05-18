// /admin/chat · primary Apocrypha chat surface.
//
// Replaces the legacy Lazarus-Workbench at this route (deleted per D043 absorption).
// Uses the same ChatThread component as /admin/apocrypha/chat for a single source-of-truth.

import type { NextPage } from 'next';
import { useState } from 'react';

import AdminLayout from '../../components/AdminLayout';
import { ChatThread } from '../../components/apocrypha/ChatThread';

const ChatPage: NextPage = () => {
  const [adminAuthorized, setAdminAuthorized] = useState(false);

  return (
    <AdminLayout title="Chat" onAdminCheck={(c) => setAdminAuthorized(c.authorized)}>
      {adminAuthorized ? (
        <div style={{ height: 'calc(100dvh - 120px)', minHeight: 480 }}>
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

export default ChatPage;
