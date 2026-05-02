// apocky.com/docs/sovereignty

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import CodeBlock from '@/components/CodeBlock';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

const Page: NextPage = () => {
  return (
    <DocsLayout
      activeSlug="sovereignty"
      title="Sovereignty Model · Apocky Docs"
      description="The cap-based sovereignty model — what data leaves your machine (nothing without your cap), how to grant or revoke a cap, and the audit trail you can inspect at any time."
    >
      <h1 className="docs-h1">Sovereignty Model</h1>
      <p className="docs-blurb">§ Caps · revocation · audit · what data leaves the machine.</p>

      <h2 className="docs-h2">§ The single sentence</h2>
      <Callout kind="success" title="Default-deny, always">
        Nothing leaves your machine without an explicit sovereign-cap that you grant. The default is{' '}
        <strong>deny-all</strong>. Caps are revocable at any time, instantly, without restarting the engine.
      </Callout>

      <h2 className="docs-h2">§ What is a cap</h2>
      <p className="docs-p">
        A <strong>cap</strong> is a structural permission carried as a Σ-mask bit. The mask is checked at every
        boundary where data could leave a process or a Home. If the cap is missing, the operation returns the
        <code className="docs-ic"> sovereign-cap-missing</code> u32 status code (in the 128..255 reserved range — see{' '}
        <a href="/docs/cssl-ffi" style={{ color: '#7dd3fc' }}>FFI conventions</a>) and nothing is sent.
      </p>
      <p className="docs-p">
        Caps have one and only one mode of operation: explicit grant. There is no "remember this choice" check
        box. There is no "trusted services" allow-list that grows on its own. Every grant is a deliberate act,
        every revocation is instant, and every grant is auditable.
      </p>

      <h2 className="docs-h2">§ What runs locally</h2>
      <p className="docs-p">
        The default LoA process does <strong>everything</strong> on your machine:
      </p>
      <ul className="docs-ul">
        <li>The intent-router (every chat-panel keystroke)</li>
        <li>The render pipeline (wgpu local)</li>
        <li>The procgen pipeline (substrate-grown content)</li>
        <li>The KAN classifiers (five swap-points, all in-process)</li>
        <li>The audit log (writes to <code className="docs-ic">logs/loa_runtime.log</code> next to the binary)</li>
      </ul>
      <p className="docs-p">
        None of these require a network. You can run LoA airgapped for as long as you want and it loses zero
        functionality.
      </p>

      <h2 className="docs-h2">§ What requires a cap</h2>
      <p className="docs-p">
        Three classes of operation cross machine boundaries; each requires a distinct cap:
      </p>
      <table className="docs-table">
        <thead>
          <tr>
            <th>Cap</th>
            <th>What it permits</th>
            <th>Status</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td><code className="docs-ic">cap.update</code></td>
            <td>Fetching binary updates from apocky.com</td>
            <td><span style={{ color: '#9aa0a6' }}>○ pull-only · user-initiated</span></td>
          </tr>
          <tr>
            <td><code className="docs-ic">cap.mycelium</code></td>
            <td>Cross-Home signal exchange · multiplayer features</td>
            <td><span style={{ color: '#fbbf24' }}>◐ in progress</span></td>
          </tr>
          <tr>
            <td><code className="docs-ic">cap.akashic</code></td>
            <td>Federated long-term memory · opt-in cross-instance learning</td>
            <td><span style={{ color: '#9aa0a6' }}>○ design-stage</span></td>
          </tr>
          <tr>
            <td><code className="docs-ic">cap.llm-bridge</code></td>
            <td>Routing intents to a Stage-2 LLM · for the chat panel</td>
            <td><span style={{ color: '#fbbf24' }}>◐ in progress</span></td>
          </tr>
        </tbody>
      </table>

      <Callout kind="warn" title="No telemetry cap">
        There is no telemetry cap. There is no anonymous-aggregate cap. There is no "help us improve the
        product by sharing usage data" cap. We do not collect those. The cap surface is intentionally small.
      </Callout>

      <h2 className="docs-h2">§ How a cap is granted</h2>
      <p className="docs-p">
        Caps live in <code className="docs-ic">~/.loa-secrets/caps.toml</code> — a plain-text file you control.
        Granting a cap is editing the file and re-launching (or, when the cap supports hot-reload, sending the
        engine a <code className="docs-ic">cap.refresh</code> intent). Revoking is deleting the line.
      </p>

      <CodeBlock lang="cssl" caption="~/.loa-secrets/caps.toml (example)">{`# Each cap on its own line. Order does not matter.
# Lines starting with # are comments.
# To revoke: delete or comment-out the line.

cap.update      = "v0.1.0"          # pin the version pull is allowed for
# cap.mycelium  = "discoverable"    # commented-out · cross-Home off
# cap.llm-bridge = "stage-2"        # commented-out · LLM routing off`}</CodeBlock>

      <h2 className="docs-h2">§ The audit trail</h2>
      <p className="docs-p">
        Every cap-gated operation emits an audit row. The rows live in
        <code className="docs-ic"> logs/loa_runtime.log</code> and additionally in the in-memory recent-ring exposed by
        the MCP <code className="docs-ic">audit.recent</code> tool. There is no separate audit destination; the file
        you can inspect with <code className="docs-ic">notepad</code> is the canonical record.
      </p>

      <h2 className="docs-h2">§ The sovereign-bypass record</h2>
      <p className="docs-p">
        When a system is forced (in test infrastructure, in a corner-case retry path) to act as if a cap were
        granted that was not, the bypass is <strong>recorded explicitly</strong> in the audit trail with the
        rationale. Sovereign-bypass-recorded is a substrate pattern (see the substrate-evolution memory notes);
        it is not a backdoor, it is a forensic surface.
      </p>

      <h2 className="docs-h2">§ Cross-reference</h2>
      <ul className="docs-ul">
        <li><a href="/docs/substrate" style={{ color: '#7dd3fc' }}>Substrate primitives — Σ-mask explanation</a></li>
        <li><a href="/docs/mycelium" style={{ color: '#7dd3fc' }}>How mycelium uses cap.mycelium</a></li>
        <li><a href="/transparency" style={{ color: '#7dd3fc' }}>Apocky-wide transparency surface</a></li>
        <li><a href="/legal/privacy" style={{ color: '#7dd3fc' }}>Privacy policy · the legal companion to this page</a></li>
      </ul>

      <PrevNextNav slug="sovereignty" />
    </DocsLayout>
  );
};

export default Page;
