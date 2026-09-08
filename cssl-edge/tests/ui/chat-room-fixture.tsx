// § Actual chat components + actual CSS ; only boundary substitutions @ preview server
import { createRoot } from 'react-dom/client';
import { useState } from 'react';
import AccountChat from '../../components/apocrypha/AccountChat';
import BrainExperience from '../../components/brain/BrainExperience';
import { FeedbackProvider } from '../../components/ui/Feedback';
import '../../styles/apocky-system.css';
import '../../styles/apocky-redesign.css';
import { mode, pendingFixture, prepareFixture } from './chat-room-boundaries';

function FixtureControls(): JSX.Element {
  const [inspection, setInspection] = useState('');
  async function inspect(): Promise<void> {
    const fixture = (window as Window & { chatRoomFixture?: { inspect: () => Promise<unknown> } }).chatRoomFixture;
    try { setInspection(fixture ? JSON.stringify(await fixture.inspect(), null, 2) : 'Fixture inspector is unavailable.'); }
    catch { setInspection('Fixture inspection failed. No service request was made.'); }
  }
  return <details style={{ position: 'fixed', bottom: 4, right: 4, zIndex: 1000, maxWidth: 'min(340px, calc(100vw - 8px))', background: '#101322', color: '#eef2ff', border: '1px solid #526070', borderRadius: 8, padding: '4px 8px', font: '12px/1.4 system-ui' }}>
      <summary style={{ minHeight: 24, cursor: 'pointer' }}>Local UI fixture · {mode}{pendingFixture ? ' · pending' : ''}</summary>
      <p>Synthetic data. Owner vault is mocked; account pending storage uses the real browser journal on this local origin. No production connection.</p>
      <nav aria-label="Fixture scenarios" style={{ display: 'flex', gap: 10, flexWrap: 'wrap' }}>
        <a href="/?mode=account&pending=0">Account</a><a href="/?mode=account&pending=1">Account pending</a><a href="/?mode=owner&pending=0">Owner</a><a href="/?mode=owner&pending=1">Owner pending</a>
      </nav>
      <button type="button" style={{ minHeight: 44, marginTop: 12, padding: '8px 12px' }} onClick={() => { void inspect(); }}>Inspect UI test state</button>
      {inspection ? <pre role="status" aria-label="UI fixture state" style={{ maxHeight: '50dvh', overflow: 'auto', whiteSpace: 'pre-wrap', overflowWrap: 'anywhere', font: '12px/1.5 monospace' }}>{inspection}</pre> : null}
    </details>;
}

async function start(): Promise<void> {
  await prepareFixture();
  const element = document.getElementById('root'); if (!element) throw new Error('Fixture root missing.');
  createRoot(element).render(<FeedbackProvider>
    <FixtureControls />
    {mode === 'owner' ? <BrainExperience serverAccess="owner" /> : <AccountChat />}
  </FeedbackProvider>);
}
void start().catch(error => { const element = document.getElementById('root'); if (element) element.textContent = 'Fixture failed: ' + String(error); });
