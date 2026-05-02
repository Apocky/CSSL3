// § Chat.tsx — message list + streaming reply + slash-command handling.
// § per spec/grand-vision/23 § CHAT-PANE.
import { useState } from "react";
import * as ipc from "../lib/ipc";
import { parseSlash, SLASH_HELP } from "../lib/slash";
import type { IpcResponse } from "../lib/types";

interface Msg {
  role: "user" | "assistant" | "system";
  content: string;
  turnId?: number;
  elapsedMs?: number;
  tools?: string[];
}

export default function Chat() {
  const [input, setInput] = useState("");
  const [messages, setMessages] = useState<Msg[]>([]);
  const [busy, setBusy] = useState(false);
  const [showHelp, setShowHelp] = useState(false);

  async function onSend() {
    const text = input.trim();
    if (!text || busy) return;
    const slash = parseSlash(text);

    // Slash-command short-circuits.
    if (slash.kind === "clear") {
      setMessages([]);
      setInput("");
      return;
    }
    if (slash.kind === "help") {
      setShowHelp((v) => !v);
      setInput("");
      return;
    }
    if (slash.kind === "cancel") {
      setBusy(false);
      await ipc.cancel();
      setInput("");
      return;
    }
    if (slash.kind === "settings") {
      // The shell catches this in the nav state — tell user to click Settings.
      pushSystem("Switch to the Settings pane via the left-nav.");
      setInput("");
      return;
    }
    if (slash.kind === "revoke_all") {
      await ipc.revokeAllSovereignCaps();
      pushSystem("All sovereign caps revoked. Cap-mode set to paranoid.");
      setInput("");
      return;
    }
    if (slash.kind === "spec") {
      const resp = await ipc.querySubstrate(slash.query, 5);
      pushSystem(formatSpecHits(resp));
      setInput("");
      return;
    }
    if (slash.kind === "unknown") {
      pushSystem(`Unknown slash-command: ${slash.raw}`);
      setInput("");
      return;
    }

    // Plain message — send to agent-loop.
    const userMsg: Msg = { role: "user", content: text };
    setMessages((prev) => [...prev, userMsg]);
    setInput("");
    setBusy(true);
    const resp = await ipc.sendMessage(text);
    setBusy(false);
    if (resp.type === "message_reply") {
      setMessages((prev) => [
        ...prev,
        {
          role: "assistant",
          content: resp.content,
          turnId: resp.turn_id,
          elapsedMs: resp.elapsed_ms,
          tools: resp.tool_calls.map((t) => t.tool),
        },
      ]);
    } else if (resp.type === "error") {
      pushSystem(`Error [${resp.code}]: ${resp.message}`);
    }
  }

  function pushSystem(content: string) {
    setMessages((prev) => [...prev, { role: "system", content }]);
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <h2 style={{ margin: 0, marginBottom: "var(--space-md)" }}>Chat</h2>
      {showHelp && (
        <div
          style={{
            background: "var(--bg-surface)",
            border: "1px solid var(--border-subtle)",
            padding: "var(--space-md)",
            marginBottom: "var(--space-md)",
            borderRadius: "var(--radius-sm)",
          }}
        >
          {SLASH_HELP.map((h) => (
            <div key={h.cmd}>
              <code>{h.cmd}</code> — {h.desc}
            </div>
          ))}
        </div>
      )}
      <div style={{ flex: 1, overflowY: "auto", marginBottom: "var(--space-md)" }}>
        {messages.map((m, i) => (
          <div
            key={i}
            style={{
              padding: "var(--space-sm)",
              marginBottom: "var(--space-sm)",
              borderLeft: `3px solid ${
                m.role === "user"
                  ? "var(--accent-cyan)"
                  : m.role === "assistant"
                    ? "var(--accent-purple)"
                    : "var(--fg-muted)"
              }`,
              background: "var(--bg-surface)",
            }}
          >
            <div style={{ color: "var(--fg-muted)", fontSize: 11 }}>
              {m.role}
              {m.turnId !== undefined && <> · turn {m.turnId}</>}
              {m.elapsedMs !== undefined && <> · {m.elapsedMs}ms</>}
            </div>
            <div style={{ whiteSpace: "pre-wrap" }}>{m.content}</div>
            {m.tools && m.tools.length > 0 && (
              <details style={{ marginTop: "var(--space-xs)", color: "var(--fg-secondary)" }}>
                <summary>tools: {m.tools.length}</summary>
                <ul>
                  {m.tools.map((t, j) => (
                    <li key={j}>{t}</li>
                  ))}
                </ul>
              </details>
            )}
          </div>
        ))}
      </div>
      <div style={{ display: "flex", gap: "var(--space-sm)" }}>
        <textarea
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void onSend();
            }
          }}
          placeholder="Send a message · /help for commands"
          rows={3}
          style={{ flex: 1, resize: "vertical" }}
        />
        <button onClick={onSend} disabled={busy} className="primary">
          {busy ? "..." : "Send"}
        </button>
      </div>
    </div>
  );
}

function formatSpecHits(resp: IpcResponse): string {
  if (resp.type !== "substrate_matches") {
    return `Unexpected response: ${resp.type}`;
  }
  if (resp.hits.length === 0) {
    return "No spec matches found.";
  }
  return resp.hits
    .map((h) => `${h.score.toFixed(3)} · ${h.doc_name}`)
    .join("\n");
}
