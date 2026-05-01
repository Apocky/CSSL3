// § Logs.tsx — audit-event list + Σ-Chain TIER-1 view + revert buttons.
// § Stage-0 stub : audit events surfaced via session-history.
//   Wave-C2 wires real audit-port → IPC stream + revert-window UI.
import { useEffect, useState } from "react";
import * as ipc from "../lib/ipc";
import type { TurnSummary } from "../lib/types";

export default function Logs() {
  const [history, setHistory] = useState<TurnSummary[]>([]);

  useEffect(() => {
    void ipc.getHistory(50).then((resp) => {
      if (resp.type === "history") setHistory(resp.turns);
    });
  }, []);

  function refresh() {
    void ipc.getHistory(50).then((resp) => {
      if (resp.type === "history") setHistory(resp.turns);
    });
  }

  return (
    <div>
      <h2 style={{ marginTop: 0 }}>Logs</h2>
      <p style={{ color: "var(--fg-muted)" }}>
        Audit events + turn history. Σ-Chain TIER-1 (local-only) per
        spec/grand-vision/14.
      </p>
      <button onClick={refresh}>Refresh</button>
      <table style={{ width: "100%", marginTop: "var(--space-md)", borderCollapse: "collapse" }}>
        <thead>
          <tr>
            <th style={{ textAlign: "left" }}>Turn</th>
            <th style={{ textAlign: "left" }}>Input</th>
            <th style={{ textAlign: "left" }}>Reply</th>
            <th>ms</th>
            <th>Revert</th>
          </tr>
        </thead>
        <tbody>
          {history.map((t) => (
            <tr key={t.turn_id} style={{ borderTop: "1px solid var(--border-subtle)" }}>
              <td>{t.turn_id}</td>
              <td style={{ fontSize: 12, color: "var(--fg-secondary)" }}>
                {t.user_input_preview}
              </td>
              <td style={{ fontSize: 12, color: "var(--fg-secondary)" }}>
                {t.reply_preview}
              </td>
              <td>{t.elapsed_ms}</td>
              <td>
                <button disabled title="Revert window expired (stage-0 stub)">
                  ↶
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {history.length === 0 && (
        <div style={{ color: "var(--fg-muted)", marginTop: "var(--space-md)" }}>
          No turns yet. Send a chat message to populate the audit log.
        </div>
      )}
    </div>
  );
}
