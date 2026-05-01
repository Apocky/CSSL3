// § Tauri-IPC TypeScript wrapper. Single dispatch entry-point matches the
//   Rust `handle_command` signature ; every UI action goes through `send`.
// § PRIME-DIRECTIVE : the IPC contract is identical between feature-on and
//   feature-off builds, so this file works against both real Tauri runtime
//   and the mock-bridge used in vitest.

import type { IpcCommand, IpcResponse, AppConfig, ToolName } from "./types";

/// Lazy-resolve the Tauri `invoke` so vitest can run without Tauri loaded.
type TauriInvoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

let cachedInvoke: TauriInvoke | null = null;

async function getInvoke(): Promise<TauriInvoke> {
  if (cachedInvoke) return cachedInvoke;
  // Dynamic import so vitest can stub this without `@tauri-apps/api`
  // installed in the harness.
  try {
    const mod = await import("@tauri-apps/api/core");
    cachedInvoke = mod.invoke as TauriInvoke;
    return cachedInvoke;
  } catch {
    // In test / non-Tauri environments, fall back to the registered mock.
    if (mockInvoke) {
      cachedInvoke = mockInvoke;
      return mockInvoke;
    }
    throw new Error(
      "Tauri runtime not available — register a mock with `__setMockInvoke` for tests"
    );
  }
}

let mockInvoke: TauriInvoke | null = null;

/**
 * Register a mock invoke for tests. Pass `null` to clear.
 */
export function __setMockInvoke(fn: TauriInvoke | null): void {
  mockInvoke = fn;
  cachedInvoke = fn;
}

/**
 * Send an IPC command and await the typed response. The Rust side wraps
 * `handle_command` ; the response is always one of the `IpcResponse`
 * variants (including `error`).
 */
export async function send(command: IpcCommand): Promise<IpcResponse> {
  const invoke = await getInvoke();
  const raw = await invoke("ipc_dispatch", { command });
  return raw as IpcResponse;
}

/* ─────────────── convenience wrappers ─────────────── */

export async function startSession(): Promise<IpcResponse> {
  return send({ type: "start_session" });
}

export async function sendMessage(content: string): Promise<IpcResponse> {
  return send({ type: "send_message", content });
}

export async function cancel(): Promise<IpcResponse> {
  return send({ type: "cancel" });
}

export async function getHistory(limit: number): Promise<IpcResponse> {
  return send({ type: "get_history", limit });
}

export async function grantCap(
  tool: ToolName,
  mode: "auto" | "require_approval"
): Promise<IpcResponse> {
  return send({ type: "grant_cap", tool, mode });
}

export async function revokeCap(tool: ToolName): Promise<IpcResponse> {
  return send({ type: "revoke_cap", tool });
}

export async function revokeAllSovereignCaps(): Promise<IpcResponse> {
  return send({ type: "revoke_all_sovereign_caps" });
}

export async function querySubstrate(
  query: string,
  topK: number
): Promise<IpcResponse> {
  return send({ type: "query_substrate", query, top_k: topK });
}

export async function getConfig(): Promise<IpcResponse> {
  return send({ type: "get_config" });
}

export async function updateConfig(config: AppConfig): Promise<IpcResponse> {
  return send({ type: "update_config", config });
}

export async function getSubstrateDocCount(): Promise<IpcResponse> {
  return send({ type: "get_substrate_doc_count" });
}

/* ─────────────── response narrowing helpers ─────────────── */

export function isError(
  resp: IpcResponse
): resp is Extract<IpcResponse, { type: "error" }> {
  return resp.type === "error";
}

export function expectMessageReply(
  resp: IpcResponse
): Extract<IpcResponse, { type: "message_reply" }> {
  if (resp.type !== "message_reply") {
    throw new Error(`Expected message_reply, got ${resp.type}`);
  }
  return resp;
}
