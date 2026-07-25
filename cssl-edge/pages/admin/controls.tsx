// /admin/controls · truthful V2 authority boundary.

import type { NextPage } from 'next';
import { useState } from 'react';

import AdminLayout from '../../components/AdminLayout';

const Controls: NextPage = () => {
  const [adminAuthorized, setAdminAuthorized] = useState(false);

  return (
    <AdminLayout title="Controls" onAdminCheck={(check) => setAdminAuthorized(check.authorized)}>
      {adminAuthorized ? (
        <section style={{
          maxWidth: 760,
          border: '1px solid #37364a',
          borderRadius: 12,
          padding: '1rem 1.15rem',
          color: '#d5d5e2',
          background: 'rgba(14, 14, 22, 0.82)',
        }}>
          <h2 style={{ margin: '0 0 0.65rem', fontSize: '1rem', color: '#f0edf9' }}>
            No V2 effect control is exposed
          </h2>
          <p style={{ margin: '0 0 0.75rem', lineHeight: 1.65 }}>
            This frontend cannot stop, restart, mutate, or issue tool effects to the V2 body.
            A chat message is never represented as an operational command.
          </p>
          <p style={{ margin: 0, color: '#9997aa', lineHeight: 1.65 }}>
            The predecessor kill-switch and API-key controls are retired. A future control may
            appear only after a dedicated V2 authority contract, receipt, rollback path, and
            end-to-end verification exist.
          </p>
        </section>
      ) : (
        <p style={{ padding: '2rem', color: '#a0a0b0' }}>Controls require owner authentication.</p>
      )}
    </AdminLayout>
  );
};

export default Controls;
