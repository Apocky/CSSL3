// /admin/cognition · live cognitive cockpit for Apocrypha
//
// Visualizes the substrate's continuous activity (swarm ticks · dream cycles ·
// tool calls · chat turns) in real-time SVG ; surfaces interactive triggers
// (Dream now). Designed per Apocky's directive : "live cognitive visualization
// of Apocrypha's data streams/learning/thoughts based on real rich telemetry".

import type { NextPage } from 'next';
import { useState } from 'react';

import AdminLayout from '../../components/AdminLayout';
import { CognitionView } from '../../components/apocrypha/CognitionView';

const Cognition: NextPage = () => {
  const [adminAuthorized, setAdminAuthorized] = useState(false);
  return (
    <AdminLayout title="∞ Cognition" onAdminCheck={(c) => setAdminAuthorized(c.authorized)}>
      {adminAuthorized ? (
        <div style={{ height: 'calc(100dvh - 140px)', minHeight: 540 }}>
          <CognitionView />
        </div>
      ) : (
        <div style={{ padding: '2rem', color: '#a0a0b0' }}>
          <p>Cognition cockpit requires admin authentication.</p>
        </div>
      )}
    </AdminLayout>
  );
};

export default Cognition;
