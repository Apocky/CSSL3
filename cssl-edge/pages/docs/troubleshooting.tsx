// apocky.com/docs/troubleshooting

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import CodeBlock from '@/components/CodeBlock';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

const Page: NextPage = () => {
  return (
    <DocsLayout
      activeSlug="troubleshooting"
      title="Troubleshooting · Apocky Docs"
      description="Fixes for common Labyrinth of Apocalypse issues — black screen, missing fonts, snapshot directory permissions, log locations, and how to file a bug."
    >
      <h1 className="docs-h1">Troubleshooting</h1>
      <p className="docs-blurb">§ Common issues · log locations · how to file a useful bug.</p>

      <h2 className="docs-h2">§ The window opens then immediately closes</h2>
      <p className="docs-p">
        Almost always a renderer initialization failure. Run from a terminal so you can see the message:
      </p>
      <CodeBlock lang="bash" caption="Capture stderr">{`# PowerShell
.\\LoA.exe 2>&1 | Tee-Object -FilePath last_run.log

# cmd
LoA.exe > last_run.log 2>&1`}</CodeBlock>
      <p className="docs-p">
        Then check the last-run log alongside <code className="docs-ic">logs/loa_runtime.log</code> for the
        renderer startup banner. The banner names the wgpu adapter selected and the surface format. If the
        banner is missing, your GPU driver is the most likely cause.
      </p>

      <h2 className="docs-h2">§ Black screen but the window is alive</h2>
      <ul className="docs-ul">
        <li>Press <span className="docs-kbd">F1</span> to force the default render mode.</li>
        <li>Press <span className="docs-kbd">F11</span> to toggle borderless fullscreen — sometimes the swapchain re-initializes.</li>
        <li>Try moving with <span className="docs-kbd">W</span> to confirm input is live.</li>
        <li>If movement works but you still see black, try <span className="docs-kbd">F2</span> (wireframe). If wireframe is visible, the issue is shader-side; file a bug with the GPU model.</li>
      </ul>

      <h2 className="docs-h2">§ Chat panel won't focus</h2>
      <ul className="docs-ul">
        <li>Confirm the window is focused (click into it). Background windows do not receive keyboard input.</li>
        <li>Check that the keyboard layout is the one you expect. The <span className="docs-kbd">/</span> key is the literal "/" character.</li>
        <li>If you are in pause state (<span className="docs-kbd">Tab</span>/<span className="docs-kbd">Esc</span>), unpause first.</li>
      </ul>

      <h2 className="docs-h2">§ "Intent classified as Unknown"</h2>
      <p className="docs-p">
        The classifier is deterministic and conservative — it falls through to{' '}
        <code className="docs-ic">Intent::Unknown</code> when no rule matches. The chat scrollback will echo the
        normalized input so you can see what the classifier saw. Common fixes:
      </p>
      <ul className="docs-ul">
        <li>Use a verb the rule-table knows · see <a href="/docs/intents" style={{ color: '#7dd3fc' }}>/docs/intents</a> for the full vocabulary.</li>
        <li>Spell room or material names from the alias tables (e.g. <code className="docs-ic">color</code>, <code className="docs-ic">brass</code>, <code className="docs-ic">qr</code>).</li>
        <li>Try a simpler phrasing: <code className="docs-ic">spawn cube at 5 5 5</code> instead of <code className="docs-ic">could you place a cube at 5 5 5 please</code>.</li>
      </ul>

      <h2 className="docs-h2">§ Snapshots don't appear</h2>
      <p className="docs-p">
        F9 (burst) and F12 (single screenshot) write into <code className="docs-ic">snapshots/</code> next to{' '}
        <code className="docs-ic">LoA.exe</code>. The directory is created on first capture. If you placed the
        binary in a write-protected location (e.g. <code className="docs-ic">C:\Program Files\</code>), capture
        will fail silently. Move the binary to a folder you own.
      </p>

      <Callout kind="note" title="No admin elevation">
        LoA never requests admin elevation. If Windows is asking for elevation, that is not us — close the
        prompt and check that the binary is the one you downloaded.
      </Callout>

      <h2 className="docs-h2">§ Log locations</h2>
      <table className="docs-table">
        <thead>
          <tr><th>File</th><th>Purpose</th></tr>
        </thead>
        <tbody>
          <tr><td><code className="docs-ic">logs/loa_runtime.log</code></td><td>Engine startup + per-frame anomalies + audit emissions</td></tr>
          <tr><td><code className="docs-ic">logs/intent_recent.jsonl</code></td><td>The last RECENT_INTENT_CAP (16) intent dispatches · same as <code className="docs-ic">intent.recent</code> MCP</td></tr>
          <tr><td><code className="docs-ic">snapshots/snap_&lt;frame&gt;_&lt;ts_ms&gt;.png</code></td><td>Captures from F9 / F12 / <code className="docs-ic">snapshot</code> intent</td></tr>
          <tr><td><code className="docs-ic">~/.loa-secrets/caps.toml</code></td><td>Cap configuration · sovereignty surface</td></tr>
        </tbody>
      </table>

      <h2 className="docs-h2">§ The KAN classifier seems wrong</h2>
      <p className="docs-p">
        Stage-1 (KAN classifier) drops in alongside Stage-0 (the deterministic keyword classifier). Stage-0
        always-fallback means the deterministic rule is the floor; KAN can only steer, not override. If you
        suspect KAN is making a poor choice in a procgen swap-point, you can disable it with:
      </p>
      <CodeBlock lang="bash" caption="Disable KAN steering">{`# Set this env-var before launch · KAN nudge becomes a no-op,
# stage-0 fallback is exclusive. The engine still emits the
# attempted-classify audit row so you can see what KAN would have done.
$env:LOA_DISABLE_KAN_NUDGE = "1"
.\\LoA.exe`}</CodeBlock>

      <h2 className="docs-h2">§ How to file a useful bug</h2>
      <p className="docs-p">
        Open an issue at <a href="https://github.com/Apocky" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>github.com/Apocky</a>{' '}
        with:
      </p>
      <ol className="docs-ol">
        <li>The version (visible in the title bar; also at the top of <code className="docs-ic">logs/loa_runtime.log</code>).</li>
        <li>OS version + GPU model (Windows + the wgpu adapter line from the log).</li>
        <li>Steps to reproduce, including any chat-panel inputs you typed.</li>
        <li>The last ~50 lines of <code className="docs-ic">logs/loa_runtime.log</code>.</li>
        <li>(Optional) the relevant snapshot if visual.</li>
      </ol>
      <p className="docs-p">
        Logs and snapshots never leave your machine unless you attach them to the issue yourself.
      </p>

      <PrevNextNav slug="troubleshooting" />
    </DocsLayout>
  );
};

export default Page;
