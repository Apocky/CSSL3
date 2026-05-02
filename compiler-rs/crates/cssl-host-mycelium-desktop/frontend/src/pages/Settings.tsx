// § Settings.tsx — Mode A/B/C selector + API-key paste + Ollama-endpoint
//   + sandbox-paths + cap-grants table.
// § per spec/grand-vision/23 § SETTINGS-PANE.
// § T11-W17-C : Anthropic key persisted to ~/.loa-secrets/anthropic.env
//   via host-side commands ; the plaintext NEVER lives in this component's
//   state — only the masked indicator from the host.
import { useEffect, useState } from "react";
import * as ipc from "../lib/ipc";
import type { AppConfig, LlmMode, ToolName } from "../lib/types";
import { TOOL_NAMES } from "../lib/types";

export default function Settings() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  // § T11-W17-C : ephemeral input — cleared immediately after save so the
  //   plaintext does not persist in React state. The masked indicator is
  //   the canonical UI source-of-truth for "is a key configured".
  const [apiKey, setApiKey] = useState("");
  const [keyMasked, setKeyMasked] = useState<string | null>(null);
  const [keySaving, setKeySaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedMsg, setSavedMsg] = useState<string | null>(null);

  useEffect(() => {
    void ipc.getConfig().then((resp) => {
      if (resp.type === "config") setConfig(resp.config);
      else if (resp.type === "error") setError(resp.message);
    });
    void ipc.loadAnthropicKeyMasked().then((resp) => {
      if (resp.type === "anthropic_key_masked") setKeyMasked(resp.masked);
    });
  }, []);

  async function onSave() {
    if (!config) return;
    setError(null);
    const resp = await ipc.updateConfig(config);
    if (resp.type === "config_updated") {
      setSavedMsg("Config saved.");
    } else if (resp.type === "error") {
      setError(resp.message);
    }
  }

  async function onSaveAnthropicKey() {
    const key = apiKey.trim();
    if (!key) return;
    setError(null);
    setKeySaving(true);
    const resp = await ipc.saveAnthropicKey(key);
    setKeySaving(false);
    // § Always wipe the input so the plaintext doesn't linger in React state.
    setApiKey("");
    if (resp.type === "anthropic_key_saved") {
      setKeyMasked(resp.masked);
      setSavedMsg("Anthropic API key saved.");
    } else if (resp.type === "error") {
      setError(resp.message);
    }
  }

  async function onGrant(tool: ToolName, mode: "auto" | "require_approval") {
    const resp = await ipc.grantCap(tool, mode);
    if (resp.type === "error") setError(resp.message);
  }

  async function onRevoke(tool: ToolName) {
    const resp = await ipc.revokeCap(tool);
    if (resp.type === "error") setError(resp.message);
  }

  if (!config) {
    return <div>Loading config...</div>;
  }

  return (
    <div>
      <h2 style={{ marginTop: 0 }}>Settings</h2>
      {error && (
        <div style={{ color: "var(--error)" }}>Error: {error}</div>
      )}
      {savedMsg && <div style={{ color: "var(--success)" }}>{savedMsg}</div>}

      <fieldset style={{ marginBottom: "var(--space-md)" }}>
        <legend>LLM Mode</legend>
        <select
          value={config.llm.mode}
          onChange={(e) => {
            const mode = e.target.value as LlmMode;
            setConfig({ ...config, llm: { ...config.llm, mode } });
          }}
        >
          <option value="external_anthropic">Mode-A · Anthropic API (richest)</option>
          <option value="local_ollama">Mode-B · Local Ollama (zero-cost)</option>
          <option value="substrate_only">Mode-C · Substrate-only (always-on)</option>
        </select>
      </fieldset>

      <fieldset style={{ marginBottom: "var(--space-md)" }} data-testid="anthropic-key-section">
        <legend>Anthropic API key</legend>
        <div
          style={{
            color: keyMasked ? "var(--success)" : "var(--fg-muted)",
            marginBottom: "var(--space-sm)",
            fontSize: 13,
          }}
          data-testid="anthropic-key-status"
        >
          {keyMasked
            ? `✓ configured (${keyMasked})`
            : "✗ not configured"}
        </div>
        <div style={{ display: "flex", gap: "var(--space-sm)" }}>
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="sk-ant-..."
            style={{ flex: 1 }}
            autoComplete="off"
            spellCheck={false}
            data-testid="anthropic-key-input"
          />
          <button
            onClick={onSaveAnthropicKey}
            disabled={keySaving || apiKey.trim().length === 0}
            className="primary"
            data-testid="anthropic-key-save"
          >
            {keySaving ? "Saving..." : "Save"}
          </button>
        </div>
        <p
          style={{
            color: "var(--fg-muted)",
            fontSize: 12,
            marginTop: "var(--space-sm)",
          }}
        >
          Stored locally at <code>~/.loa-secrets/anthropic.env</code>. Never
          sent anywhere except <code>api.anthropic.com</code>.
        </p>
      </fieldset>

      <fieldset style={{ marginBottom: "var(--space-md)" }}>
        <legend>Ollama endpoint</legend>
        <input
          value={config.llm.ollama_endpoint}
          onChange={(e) =>
            setConfig({
              ...config,
              llm: { ...config.llm, ollama_endpoint: e.target.value },
            })
          }
          style={{ width: "100%" }}
        />
      </fieldset>

      <fieldset style={{ marginBottom: "var(--space-md)" }}>
        <legend>Sandbox paths (one per line)</legend>
        <textarea
          rows={4}
          value={config.sandbox_paths.join("\n")}
          onChange={(e) =>
            setConfig({
              ...config,
              sandbox_paths: e.target.value
                .split("\n")
                .map((s) => s.trim())
                .filter(Boolean),
            })
          }
          style={{ width: "100%" }}
        />
      </fieldset>

      <fieldset style={{ marginBottom: "var(--space-md)" }}>
        <legend>Cap grants</legend>
        <table style={{ width: "100%", borderCollapse: "collapse" }}>
          <thead>
            <tr>
              <th style={{ textAlign: "left" }}>Tool</th>
              <th>Auto</th>
              <th>Require</th>
              <th>Revoke</th>
            </tr>
          </thead>
          <tbody>
            {TOOL_NAMES.map((t) => (
              <tr key={t}>
                <td>{t}</td>
                <td>
                  <button onClick={() => onGrant(t, "auto")}>auto</button>
                </td>
                <td>
                  <button onClick={() => onGrant(t, "require_approval")}>req</button>
                </td>
                <td>
                  <button onClick={() => onRevoke(t)}>revoke</button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </fieldset>

      <button onClick={onSave} className="primary">
        Save config
      </button>
    </div>
  );
}
