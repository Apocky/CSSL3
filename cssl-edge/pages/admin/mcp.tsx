// /admin/mcp · invoke any of the 118 LoA-MCP-tools from your phone
// Routes through bridge → desktop LoA.exe MCP-server :3001 → response back

import type { NextPage } from 'next';
import { useState } from 'react';
import AdminLayout from '../../components/AdminLayout';

const COMMON_TOOLS = [
  { name: 'render.snapshot_png', desc: 'Capture current frame · returns PNG-base64' },
  { name: 'world.spawn_gltf', desc: 'Spawn GLTF mesh into world · cap-gated' },
  { name: 'sense.framebuffer', desc: 'Query render-pass framebuffer state' },
  { name: 'intent.translate', desc: 'Run text → typed-intent classifier' },
  { name: 'attestation.empty_session_text', desc: 'Render empty SessionAttestation' },
  { name: 'audit.summarize_dir', desc: 'Summarize logs/ directory' },
  { name: 'coder.list_pending', desc: 'List pending AST-edits' },
  { name: 'sigma_chain.head', desc: 'Σ-Chain TIER-2 latest event-id' },
];

const Mcp: NextPage = () => {
  const [tool, setTool] = useState('');
  const [args, setArgs] = useState('{}');
  const [result, setResult] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  async function invoke(e: React.FormEvent) {
    e.preventDefault();
    if (!tool || submitting) return;
    setSubmitting(true);
    setResult(null);
    try {
      const parsed = JSON.parse(args || '{}');
      const res = await fetch('/api/admin/bridge?action=send', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          target: 'mcp',
          text: JSON.stringify({ tool, arguments: parsed }),
        }),
      });
      const json = await res.json();
      setResult(typeof json.response === 'string' ? json.response : JSON.stringify(json, null, 2));
    } catch (err) {
      setResult(`✗ error : ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <AdminLayout title="⊑ MCP Tool Invoker">
      <p style={{ color: '#7a7a8c', fontSize: '0.82rem', marginTop: 0, marginBottom: '1.5rem' }}>
        Invoke any of LoA.exe's 118 MCP tools from your phone · routes via bridge → desktop :3001 · response back
      </p>

      <form onSubmit={invoke} style={{ display: 'grid', gap: '0.75rem', marginBottom: '1.5rem' }}>
        <div>
          <label style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.15em', color: '#7a7a8c' }}>
            tool
          </label>
          <input
            type="text"
            value={tool}
            onChange={(e) => setTool(e.target.value)}
            placeholder="e.g. render.snapshot_png"
            list="common-tools"
            style={{
              width: '100%',
              marginTop: 4,
              padding: '0.7rem 0.85rem',
              background: 'rgba(20, 20, 30, 0.7)',
              border: '1px solid #2a2a3a',
              borderRadius: 4,
              color: '#7dd3fc',
              fontFamily: 'inherit',
              fontSize: '0.92rem',
              outline: 'none',
              minHeight: 44,
            }}
          />
          <datalist id="common-tools">
            {COMMON_TOOLS.map((t) => (
              <option key={t.name} value={t.name} />
            ))}
          </datalist>
        </div>

        <div>
          <label style={{ fontSize: '0.7rem', textTransform: 'uppercase', letterSpacing: '0.15em', color: '#7a7a8c' }}>
            arguments (JSON)
          </label>
          <textarea
            value={args}
            onChange={(e) => setArgs(e.target.value)}
            rows={4}
            style={{
              width: '100%',
              marginTop: 4,
              padding: '0.7rem 0.85rem',
              background: 'rgba(20, 20, 30, 0.7)',
              border: '1px solid #2a2a3a',
              borderRadius: 4,
              color: '#cdd6e4',
              fontFamily: 'inherit',
              fontSize: '0.85rem',
              outline: 'none',
              resize: 'vertical',
            }}
          />
        </div>

        <button
          type="submit"
          disabled={submitting || !tool}
          style={{
            padding: '0.75rem',
            background: 'linear-gradient(135deg, #c084fc 0%, #7dd3fc 100%)',
            color: '#0a0a0f',
            fontWeight: 700,
            border: 'none',
            borderRadius: 4,
            cursor: submitting || !tool ? 'not-allowed' : 'pointer',
            opacity: submitting || !tool ? 0.5 : 1,
            fontSize: '0.92rem',
            minHeight: 44,
            fontFamily: 'inherit',
          }}
        >
          {submitting ? '◐ invoking…' : '→ invoke'}
        </button>
      </form>

      {/* COMMON TOOLS LIST */}
      <h2 style={{ fontSize: '0.65rem', textTransform: 'uppercase', letterSpacing: '0.18em', color: '#7a7a8c' }}>
        § Common tools
      </h2>
      <div style={{ display: 'grid', gap: '0.4rem', marginBottom: '1.5rem' }}>
        {COMMON_TOOLS.map((t) => (
          <button
            key={t.name}
            type="button"
            onClick={() => setTool(t.name)}
            style={{
              padding: '0.6rem 0.85rem',
              background: 'rgba(20, 20, 30, 0.4)',
              border: '1px solid #1f1f2a',
              borderRadius: 4,
              color: 'inherit',
              cursor: 'pointer',
              textAlign: 'left',
              fontFamily: 'inherit',
              minHeight: 44,
            }}
          >
            <code style={{ color: '#7dd3fc', fontSize: '0.82rem' }}>{t.name}</code>
            <div style={{ color: '#7a7a8c', fontSize: '0.75rem', marginTop: 2 }}>{t.desc}</div>
          </button>
        ))}
      </div>

      {/* RESULT */}
      {result !== null && (
        <section>
          <h2 style={{ fontSize: '0.65rem', textTransform: 'uppercase', letterSpacing: '0.18em', color: '#7a7a8c' }}>
            § Response
          </h2>
          <pre
            style={{
              background: 'rgba(10, 10, 16, 0.6)',
              border: '1px solid #1f1f2a',
              borderRadius: 4,
              padding: '0.85rem',
              fontSize: '0.78rem',
              color: '#cdd6e4',
              overflowX: 'auto',
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-word',
            }}
          >
            {result}
          </pre>
        </section>
      )}
    </AdminLayout>
  );
};

export default Mcp;
