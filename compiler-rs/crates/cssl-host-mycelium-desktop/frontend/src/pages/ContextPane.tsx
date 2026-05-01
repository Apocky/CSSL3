// § ContextPane.tsx — top-K substrate-docs loaded + token budget viz +
//   cost counter. per spec/grand-vision/23 § AGENT-LOOP.
import { useEffect, useState } from "react";
import * as ipc from "../lib/ipc";
import type { AppConfig } from "../lib/types";

export default function ContextPane() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [docCount, setDocCount] = useState<number | null>(null);

  useEffect(() => {
    void ipc.getConfig().then((resp) => {
      if (resp.type === "config") setConfig(resp.config);
    });
    void ipc.getSubstrateDocCount().then((resp) => {
      if (resp.type === "substrate_doc_count") setDocCount(resp.count);
    });
  }, []);

  if (!config) return <div>Loading...</div>;

  const budget = config.context_token_budget;
  const topK = config.knowledge_top_k;

  return (
    <div>
      <h2 style={{ marginTop: 0 }}>Context</h2>
      <div style={{ marginBottom: "var(--space-md)" }}>
        <div>Top-K substrate docs per turn: <strong>{topK}</strong></div>
        <div>Embedded substrate docs: <strong>{docCount ?? "..."}</strong></div>
        <div>Context token budget: <strong>{budget.toLocaleString()}</strong></div>
      </div>

      <div style={{ marginBottom: "var(--space-md)" }}>
        <div style={{ color: "var(--fg-muted)", fontSize: 12 }}>
          Token budget visualization (canon · always-loaded vs fetched · top-K)
        </div>
        <div
          style={{
            display: "flex",
            height: 16,
            border: "1px solid var(--border-subtle)",
            borderRadius: "var(--radius-sm)",
            overflow: "hidden",
          }}
        >
          <div
            style={{
              width: "30%",
              background: "var(--accent-purple)",
              opacity: 0.8,
            }}
            title="Canon (PRIME-DIRECTIVE + CLAUDE.md + MEMORY.md)"
          />
          <div
            style={{
              width: "50%",
              background: "var(--accent-cyan)",
              opacity: 0.6,
            }}
            title={`Top-${topK} substrate hits`}
          />
          <div
            style={{
              flex: 1,
              background: "var(--bg-surface)",
            }}
            title="Reserve / user-prompt"
          />
        </div>
      </div>

      <div>
        <div style={{ color: "var(--fg-muted)", fontSize: 12 }}>
          Per-turn cost counter (Mode-C is free ; Mode-A varies)
        </div>
        <div>
          Estimated next-turn cost: <strong>$0.0000</strong> (Mode-C)
        </div>
      </div>
    </div>
  );
}
