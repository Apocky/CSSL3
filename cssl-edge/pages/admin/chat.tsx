// /admin/chat · the owner's way into the room.
//
// Same component as /apocrypha — there is one chat interface. What this route adds is the admin
// shell around it and the owner lane, which carries the tool trace and the ability to stop a run.

import type { NextPage } from 'next';
import { useMemo, useState } from 'react';

import AdminLayout from '../../components/AdminLayout';
import ApocryphaChat from '../../components/apocrypha/ApocryphaChat';
import { ownerLane } from '../../lib/apocrypha/chat-lanes';
import { authFetch } from '../../lib/browser-auth';

const ChatPage: NextPage = () => {
  const [adminAuthorized, setAdminAuthorized] = useState(false);
  const lane = useMemo(() => ownerLane(authFetch), []);

  return (
    <AdminLayout title="Chat" onAdminCheck={(c) => setAdminAuthorized(c.authorized)}>
      {adminAuthorized ? (
        <div style={{ height: 'calc(100dvh - 120px)', minHeight: 480 }}>
          <ApocryphaChat lane={lane} signedIn height="100%" />
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
